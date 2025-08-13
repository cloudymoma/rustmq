// Optimized DirectIOWal using the async file abstraction for performance
use crate::{Result, config::WalConfig, storage::traits::*, types::*};
use super::async_file::{AsyncWalFile, AsyncWalFileFactory, PlatformCapabilities};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot};
use bytes::Bytes;

// Write commands for the optimized file task using the async file abstraction
#[derive(Debug)]
enum OptimizedWriteCommand {
    Write {
        data: Vec<u8>,
        file_offset: u64,
        response: oneshot::Sender<Result<Vec<u8>>>, // Returns buffer for reuse
    },
    Sync {
        response: oneshot::Sender<Result<()>>,
    },
    Read {
        file_offset: u64,
        size: usize,
        response: oneshot::Sender<Result<Vec<u8>>>,
    },
    Shutdown,
}

pub struct OptimizedDirectIOWal {
    // Channel to send commands to the dedicated file task
    write_tx: mpsc::UnboundedSender<OptimizedWriteCommand>,
    
    buffer_pool: Arc<dyn BufferPool>,
    current_offset: Arc<AtomicU64>,
    current_file_offset: Arc<AtomicU64>,
    config: Arc<RwLock<WalConfig>>,
    segments: Arc<RwLock<Vec<WalSegmentMetadata>>>,
    current_segment_start_time: Arc<RwLock<Instant>>,
    current_segment_size: Arc<AtomicU64>,
    current_segment_start_offset: Arc<AtomicU64>,
    upload_callbacks: Arc<RwLock<Vec<Box<dyn Fn(u64, u64) + Send + Sync>>>>,
    upload_in_progress: Arc<AtomicBool>,
    segment_tracking_lock: Arc<Mutex<()>>,
    
    // Platform capabilities for metrics and debugging
    platform_capabilities: PlatformCapabilities,
    backend_type: String,
}

#[derive(Debug, Clone)]
struct WalSegmentMetadata {
    start_offset: u64,
    end_offset: u64,
    file_offset: u64,
    size_bytes: u64,
    created_at: Instant,
}

impl OptimizedDirectIOWal {
    pub async fn new(config: WalConfig, buffer_pool: Arc<dyn BufferPool>) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path).await?;
        let file_path = config.path.join("wal.log");

        // Create the optimal file backend for the current platform
        let factory = AsyncWalFileFactory::new();
        let platform_capabilities = factory.capabilities().clone();
        let file = factory.create_file(&file_path).await?;
        let backend_type = file.backend_type().to_string();
        
        tracing::info!(
            "Initialized WAL with {} backend on platform with io_uring_available={}",
            backend_type,
            platform_capabilities.io_uring_available
        );

        let initial_file_size = file.file_size().await?;

        // Create channel for communicating with the file task
        let (write_tx, write_rx) = mpsc::unbounded_channel();

        // Start the optimized file task with the appropriate runtime
        let flush_interval_ms = config.flush_interval_ms;
        let fsync_on_write = config.fsync_on_write;
        
        // Spawn the file task on the appropriate runtime
        // The backend_type tells us what the factory actually created
        #[cfg(all(target_os = "linux", feature = "io-uring"))]
        {
            if backend_type.starts_with("io-uring") {
                // We have an actual io_uring backend, try to spawn on io_uring runtime
                // This will work when we're in an io_uring context
                tokio_uring::spawn(Self::optimized_file_task(
                    file, write_rx, flush_interval_ms, fsync_on_write
                ));
                tracing::debug!("Spawned io_uring file task");
            } else {
                // Factory created tokio backend, use tokio spawn
                tokio::spawn(Self::optimized_file_task(
                    file, write_rx, flush_interval_ms, fsync_on_write
                ));
                tracing::debug!("Spawned tokio file task");
            }
        }
        
        #[cfg(not(all(target_os = "linux", feature = "io-uring")))]
        {
            tokio::spawn(Self::optimized_file_task(
                file, write_rx, flush_interval_ms, fsync_on_write
            ));
            tracing::debug!("Spawned tokio file task");
        }

        let mut wal = Self {
            write_tx,
            buffer_pool,
            current_offset: Arc::new(AtomicU64::new(0)),
            current_file_offset: Arc::new(AtomicU64::new(initial_file_size)),
            config: Arc::new(RwLock::new(config)),
            segments: Arc::new(RwLock::new(Vec::new())),
            current_segment_start_time: Arc::new(RwLock::new(Instant::now())),
            current_segment_size: Arc::new(AtomicU64::new(0)),
            current_segment_start_offset: Arc::new(AtomicU64::new(0)),
            upload_callbacks: Arc::new(RwLock::new(Vec::new())),
            upload_in_progress: Arc::new(AtomicBool::new(false)),
            segment_tracking_lock: Arc::new(Mutex::new(())),
            platform_capabilities,
            backend_type,
        };

        wal.recover().await?;
        wal.start_background_tasks().await?;
        Ok(wal)
    }

    // Optimized file task using the async file abstraction
    async fn optimized_file_task(
        file: Box<dyn AsyncWalFile>,
        mut rx: mpsc::UnboundedReceiver<OptimizedWriteCommand>,
        flush_interval_ms: u64,
        fsync_on_write: bool,
    ) {
        let mut flush_interval = tokio::time::interval(Duration::from_millis(flush_interval_ms));
        let mut needs_flush = false;
        
        tracing::info!("Started optimized WAL file task with {} backend", file.backend_type());
        
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(OptimizedWriteCommand::Write { data, file_offset, response }) => {
                            let result = async {
                                // Use the optimized async file interface
                                let returned_buffer = file.write_at(data, file_offset).await?;
                                
                                if fsync_on_write {
                                    file.sync_all().await?;
                                } else {
                                    needs_flush = true;
                                }
                                
                                Ok(returned_buffer)
                            }.await;
                            
                            let _ = response.send(result);
                        },
                        Some(OptimizedWriteCommand::Sync { response }) => {
                            let result = file.sync_all().await.map_err(Into::into);
                            needs_flush = false;
                            let _ = response.send(result);
                        },
                        Some(OptimizedWriteCommand::Read { file_offset, size, response }) => {
                            let result = file.read_at(file_offset, size).await.map_err(Into::into);
                            let _ = response.send(result);
                        },
                        Some(OptimizedWriteCommand::Shutdown) | None => {
                            // Perform final flush before shutdown
                            if needs_flush {
                                let _ = file.sync_all().await;
                            }
                            break;
                        }
                    }
                },
                _ = flush_interval.tick(), if !fsync_on_write && needs_flush => {
                    if let Err(e) = file.sync_all().await {
                        tracing::error!("Periodic flush failed with {} backend: {}", file.backend_type(), e);
                    } else {
                        needs_flush = false;
                        tracing::debug!("Periodic flush completed with {} backend", file.backend_type());
                    }
                }
            }
        }
        
        tracing::info!("Optimized WAL file task ({}) shutting down", file.backend_type());
    }

    async fn start_background_tasks(&self) -> Result<()> {
        self.start_upload_monitor_task().await?;
        Ok(())
    }

    async fn start_upload_monitor_task(&self) -> Result<()> {
        let config = self.config.clone();
        let current_segment_start_time = self.current_segment_start_time.clone();
        let current_segment_size = self.current_segment_size.clone();
        let current_segment_start_offset = self.current_segment_start_offset.clone();
        let upload_callbacks = self.upload_callbacks.clone();
        let current_offset = self.current_offset.clone();
        let upload_in_progress = self.upload_in_progress.clone();
        let segment_tracking_lock = self.segment_tracking_lock.clone();

        tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                check_interval.tick().await;
                let cfg = config.read().clone();
                let segment_start_time = *current_segment_start_time.read();
                let segment_size = current_segment_size.load(Ordering::SeqCst);
                
                let should_upload = segment_size >= cfg.segment_size_bytes ||
                    segment_start_time.elapsed() >= Duration::from_millis(cfg.upload_interval_ms);
                
                if should_upload && segment_size > 0 {
                    if upload_in_progress.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        let (start_offset, end_offset) = {
                            let _guard = segment_tracking_lock.lock().unwrap();
                            let end_offset = current_offset.load(Ordering::SeqCst);
                            let start_offset = current_segment_start_offset.load(Ordering::SeqCst);
                            
                            current_segment_size.store(0, Ordering::SeqCst);
                            current_segment_start_offset.store(end_offset, Ordering::SeqCst);
                            *current_segment_start_time.write() = Instant::now();
                            
                            (start_offset, end_offset)
                        };
                        
                        let callbacks = upload_callbacks.read();
                        for callback in callbacks.iter() {
                            callback(start_offset, end_offset);
                        }
                        
                        upload_in_progress.store(false, Ordering::SeqCst);
                    }
                }
            }
        });
        
        Ok(())
    }

    pub fn register_upload_callback<F>(&self, callback: F) 
    where 
        F: Fn(u64, u64) + Send + Sync + 'static 
    {
        self.upload_callbacks.write().push(Box::new(callback));
    }

    pub async fn update_config(&self, new_config: WalConfig) -> Result<()> {
        let mut config = self.config.write();
        *config = new_config;
        Ok(())
    }

    /// Get platform capabilities and backend information
    pub fn get_platform_info(&self) -> (PlatformCapabilities, &str) {
        (self.platform_capabilities.clone(), &self.backend_type)
    }

    async fn recover(&mut self) -> Result<()> {
        const RECOVERY_BUFFER_SIZE: usize = 4 * 1024 * 1024; // 4MB buffer

        let file_size = self.current_file_offset.load(Ordering::SeqCst);
        if file_size == 0 {
            return Ok(());
        }

        let mut logical_offset = 0u64;
        let mut file_offset = 0u64;

        while file_offset < file_size {
            let bytes_to_read = (file_size - file_offset).min(RECOVERY_BUFFER_SIZE as u64) as usize;
            
            let (tx, rx) = oneshot::channel();
            self.write_tx.send(OptimizedWriteCommand::Read {
                file_offset,
                size: bytes_to_read,
                response: tx,
            }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
            
            let buffer = rx.await
                .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;

            let mut buffer_pos = 0;
            while buffer_pos < buffer.len() {
                if let Ok(record_size) = self.read_record_size(&buffer[buffer_pos..]) {
                    if buffer_pos + record_size as usize > buffer.len() {
                        break;
                    }

                    let segment_meta = WalSegmentMetadata {
                        start_offset: logical_offset,
                        end_offset: logical_offset + 1,
                        file_offset: file_offset + buffer_pos as u64,
                        size_bytes: record_size,
                        created_at: Instant::now(),
                    };

                    self.segments.write().push(segment_meta);
                    logical_offset += 1;
                    buffer_pos += record_size as usize;
                } else {
                    break;
                }
            }
            file_offset += buffer_pos as u64;
        }

        self.current_offset.store(logical_offset, Ordering::SeqCst);
        
        {
            let _guard = self.segment_tracking_lock.lock().unwrap();
            self.current_segment_start_offset.store(logical_offset, Ordering::SeqCst);
            self.current_segment_size.store(0, Ordering::SeqCst);
        }

        Ok(())
    }

    fn read_record_size(&self, buffer: &[u8]) -> Result<u64> {
        if buffer.len() < 8 {
            return Err(crate::error::RustMqError::Wal("Buffer too small for record size".to_string()));
        }
        
        let size = u64::from_le_bytes([
            buffer[0], buffer[1], buffer[2], buffer[3],
            buffer[4], buffer[5], buffer[6], buffer[7],
        ]);
        
        Ok(size)
    }

    async fn write_with_optimized_io(&self, data: &[u8]) -> Result<(u64, Vec<u8>)> {
        let file_offset = self.current_file_offset.fetch_add(data.len() as u64, Ordering::SeqCst);
        
        let (tx, rx) = oneshot::channel();
        self.write_tx.send(OptimizedWriteCommand::Write {
            data: data.to_vec(),
            file_offset,
            response: tx,
        }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
        
        let returned_buffer = rx.await
            .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
        
        Ok((file_offset, returned_buffer))
    }

    async fn write_with_buffer(&self, buffer: Vec<u8>) -> Result<(u64, Vec<u8>)> {
        let file_offset = self.current_file_offset.fetch_add(buffer.len() as u64, Ordering::SeqCst);
        
        let (tx, rx) = oneshot::channel();
        self.write_tx.send(OptimizedWriteCommand::Write {
            data: buffer, // Direct ownership transfer - eliminates data.to_vec() allocation
            file_offset,
            response: tx,
        }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
        
        let returned_buffer = rx.await
            .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
        
        Ok((file_offset, returned_buffer))
    }
}

#[async_trait]
impl WriteAheadLog for OptimizedDirectIOWal {
    async fn append(&self, record: WalRecord) -> Result<u64> {
        let serialized = bincode::serialize(&record)?;
        let record_size = serialized.len() as u64;
        let total_size = serialized.len() + 8;
        
        // Get buffer from pool and write directly into it
        let mut buffer = self.buffer_pool.get_aligned_buffer(total_size)?;
        buffer.clear(); // Ensure buffer is empty
        buffer.extend_from_slice(&record_size.to_le_bytes());
        buffer.extend_from_slice(&serialized);
        
        // Use optimized method that takes ownership to avoid data.to_vec()
        let (file_offset, returned_buffer) = self.write_with_buffer(buffer).await?;
        let logical_offset = self.current_offset.fetch_add(1, Ordering::SeqCst);

        // Return the buffer to pool for reuse
        self.buffer_pool.return_buffer(returned_buffer);

        let segment_meta = WalSegmentMetadata {
            start_offset: logical_offset,
            end_offset: logical_offset + 1,
            file_offset,
            size_bytes: total_size as u64,
            created_at: Instant::now(),
        };

        self.segments.write().push(segment_meta);
        
        {
            let _guard = self.segment_tracking_lock.lock().unwrap();
            self.current_segment_size.fetch_add(total_size as u64, Ordering::SeqCst);
        }

        Ok(logical_offset)
    }

    async fn read(&self, offset: u64, max_bytes: usize) -> Result<Vec<WalRecord>> {
        let segments = {
            let segments_lock = self.segments.read();
            segments_lock.clone()
        };
        let mut records = Vec::new();
        let mut bytes_read = 0;

        for segment in segments.iter() {
            if segment.start_offset <= offset && offset < segment.end_offset {
                let (tx, rx) = oneshot::channel();
                self.write_tx.send(OptimizedWriteCommand::Read {
                    file_offset: segment.file_offset,
                    size: 8,
                    response: tx,
                }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
                
                let size_buffer = rx.await
                    .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
                
                let record_size = u64::from_le_bytes([
                    size_buffer[0], size_buffer[1], size_buffer[2], size_buffer[3],
                    size_buffer[4], size_buffer[5], size_buffer[6], size_buffer[7],
                ]) as usize;
                
                if bytes_read + record_size > max_bytes {
                    break;
                }

                let (tx, rx) = oneshot::channel();
                self.write_tx.send(OptimizedWriteCommand::Read {
                    file_offset: segment.file_offset + 8,
                    size: record_size,
                    response: tx,
                }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
                
                let record_buffer = rx.await
                    .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
                
                let record: WalRecord = bincode::deserialize(&record_buffer)?;
                records.push(record);
                bytes_read += record_size;
            }
        }

        Ok(records)
    }

    async fn read_range(&self, start_offset: u64, end_offset: u64) -> Result<Vec<WalRecord>> {
        let segments = {
            let segments_lock = self.segments.read();
            segments_lock.clone()
        };
        let mut records = Vec::new();

        for segment in segments.iter() {
            if segment.end_offset <= start_offset || segment.start_offset >= end_offset {
                continue;
            }

            let mut file_offset = segment.file_offset;
            
            while file_offset < segment.file_offset + segment.size_bytes {
                let (tx, rx) = oneshot::channel();
                self.write_tx.send(OptimizedWriteCommand::Read {
                    file_offset,
                    size: 8,
                    response: tx,
                }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
                
                let size_buffer = rx.await
                    .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
                
                let record_size = u64::from_le_bytes([
                    size_buffer[0], size_buffer[1], size_buffer[2], size_buffer[3],
                    size_buffer[4], size_buffer[5], size_buffer[6], size_buffer[7],
                ]) as usize;

                let (tx, rx) = oneshot::channel();
                self.write_tx.send(OptimizedWriteCommand::Read {
                    file_offset: file_offset + 8,
                    size: record_size,
                    response: tx,
                }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
                
                let record_buffer = rx.await
                    .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
                
                let record: WalRecord = bincode::deserialize(&record_buffer)?;
                
                if record.offset >= end_offset {
                    break;
                }
                
                if record.offset >= start_offset {
                    records.push(record);
                }
                
                file_offset += 8 + record_size as u64;
            }
        }

        Ok(records)
    }

    async fn sync(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.write_tx.send(OptimizedWriteCommand::Sync {
            response: tx,
        }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
        
        rx.await
            .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
        
        Ok(())
    }

    async fn truncate(&self, offset: u64) -> Result<()> {
        let mut segments = self.segments.write();
        segments.retain(|seg| seg.start_offset < offset);
        self.current_offset.store(offset, Ordering::SeqCst);
        
        {
            let _guard = self.segment_tracking_lock.lock().unwrap();
            self.current_segment_start_offset.store(offset, Ordering::SeqCst);
            self.current_segment_size.store(0, Ordering::SeqCst);
        }
        
        Ok(())
    }

    async fn get_end_offset(&self) -> Result<u64> {
        Ok(self.current_offset.load(Ordering::SeqCst))
    }

    fn register_upload_callback(&self, callback: Box<dyn Fn(u64, u64) + Send + Sync>) {
        self.upload_callbacks.write().push(callback);
    }
}

impl Drop for OptimizedDirectIOWal {
    fn drop(&mut self) {
        let _ = self.write_tx.send(OptimizedWriteCommand::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AlignedBufferPool;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_optimized_wal_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = OptimizedDirectIOWal::new(config, buffer_pool).await.unwrap();

        // Check platform information
        let (capabilities, backend_type) = wal.get_platform_info();
        println!("Using backend: {} with io_uring_available: {}", 
                backend_type, capabilities.io_uring_available);

        let record = WalRecord {
            topic_partition: TopicPartition {
                topic: "test-topic".to_string(),
                partition: 0,
            },
            offset: 0,
            record: Record::new(
                Some(b"key1".to_vec()),
                b"value1".to_vec(),
                vec![],
                chrono::Utc::now().timestamp_millis(),
            ),
            crc32: 0,
        };

        let offset = wal.append(record.clone()).await.unwrap();
        assert_eq!(offset, 0);

        let records = wal.read(0, 1024).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].topic_partition.topic, "test-topic");
    }

    #[tokio::test]
    async fn test_backend_selection() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = OptimizedDirectIOWal::new(config, buffer_pool).await.unwrap();

        let (capabilities, backend_type) = wal.get_platform_info();
        
        // Verify that backend selection is working
        #[cfg(all(target_os = "linux", feature = "io-uring"))]
        {
            // In test environments, the factory defaults to tokio backend for safety
            // even when io_uring is available, unless explicitly enabled
            if capabilities.io_uring_available {
                // During tests, we expect tokio backend due to runtime detection
                assert!(backend_type == "tokio-fs" || backend_type.starts_with("io-uring"));
            } else {
                assert_eq!(backend_type, "tokio-fs");
            }
        }
        
        #[cfg(not(all(target_os = "linux", feature = "io-uring")))]
        {
            assert_eq!(backend_type, "tokio-fs");
            assert!(!capabilities.io_uring_available);
        }
        
        println!("Selected backend: {} (io_uring available: {})", 
                backend_type, capabilities.io_uring_available);
    }

    #[tokio::test]
    async fn test_performance_characteristics() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 20));
        let wal = OptimizedDirectIOWal::new(config, buffer_pool).await.unwrap();

        let (_, backend_type) = wal.get_platform_info();
        let start_time = Instant::now();
        
        // Write 100 records to test performance
        for i in 0..100 {
            let record = WalRecord {
                topic_partition: TopicPartition {
                    topic: "perf-test".to_string(),
                    partition: 0,
                },
                offset: i,
                record: Record::new(
                    Some(format!("key{}", i).into_bytes()),
                    format!("value{}", i).into_bytes(),
                    vec![],
                    chrono::Utc::now().timestamp_millis(),
                ),
                crc32: 0,
            };
            wal.append(record).await.unwrap();
        }
        
        let write_duration = start_time.elapsed();
        wal.sync().await.unwrap();
        let total_duration = start_time.elapsed();
        
        println!("Backend: {} - Write time: {:?}, Total time: {:?}", 
                backend_type, write_duration, total_duration);
        
        // Verify all records were written
        assert_eq!(wal.get_end_offset().await.unwrap(), 100);
        
        // Performance should be reasonable regardless of backend
        assert!(total_duration.as_millis() < 5000, "WAL operations took too long: {:?}", total_duration);
    }
}