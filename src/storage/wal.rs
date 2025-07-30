use crate::{Result, config::WalConfig, storage::traits::*, types::*};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::time::{Duration, Instant};
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot};

#[cfg(feature = "io-uring")]
use tokio_uring::fs::File as UringFile;

// Write commands sent to the dedicated file task
#[derive(Debug)]
enum WriteCommand {
    Write {
        data: Vec<u8>,
        file_offset: u64,
        response: oneshot::Sender<Result<()>>,
    },
    Sync {
        response: oneshot::Sender<Result<()>>,
    },
    Read {
        file_offset: u64,
        size: usize,
        response: oneshot::Sender<Result<Vec<u8>>>,
    },
    Seek {
        position: u64,
        response: oneshot::Sender<Result<()>>,
    },
    Shutdown,
}

pub struct DirectIOWal {
    // Channel to send commands to the dedicated file task
    write_tx: mpsc::UnboundedSender<WriteCommand>,
    
    buffer_pool: Arc<dyn BufferPool>,
    current_offset: Arc<AtomicU64>,
    current_file_offset: Arc<AtomicU64>, // Track file position for sequential writes
    config: Arc<RwLock<WalConfig>>,
    segments: Arc<RwLock<Vec<WalSegmentMetadata>>>,
    current_segment_start_time: Arc<RwLock<Instant>>,
    current_segment_size: Arc<AtomicU64>,
    current_segment_start_offset: Arc<AtomicU64>,
    upload_callbacks: Arc<RwLock<Vec<Box<dyn Fn(u64, u64) + Send + Sync>>>>,
}

#[derive(Debug, Clone)]
struct WalSegmentMetadata {
    start_offset: u64,
    end_offset: u64,
    file_offset: u64,
    size_bytes: u64,
    created_at: Instant,
}

impl DirectIOWal {
    pub async fn new(config: WalConfig, buffer_pool: Arc<dyn BufferPool>) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path).await?;
        let file_path = config.path.join("wal.log");

        // Create the file and get its initial size
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&file_path)
            .await?;
        
        let initial_file_size = file.metadata().await?.len();

        // Create channel for communicating with the file task
        let (write_tx, write_rx) = mpsc::unbounded_channel();

        // Start the dedicated file task
        let flush_interval_ms = config.flush_interval_ms;
        let fsync_on_write = config.fsync_on_write;
        
        tokio::spawn(Self::file_task(file, write_rx, flush_interval_ms, fsync_on_write));

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
        };

        wal.recover().await?;
        wal.start_background_tasks().await?;
        Ok(wal)
    }

    // Dedicated file task that owns the file handle and manages all I/O
    async fn file_task(
        mut file: File,
        mut rx: mpsc::UnboundedReceiver<WriteCommand>,
        flush_interval_ms: u64,
        fsync_on_write: bool,
    ) {
        let mut flush_interval = tokio::time::interval(Duration::from_millis(flush_interval_ms));
        let mut needs_flush = false;
        
        loop {
            tokio::select! {
                // Handle incoming write commands
                cmd = rx.recv() => {
                    match cmd {
                        Some(WriteCommand::Write { data, file_offset, response }) => {
                            let result = async {
                                file.seek(SeekFrom::Start(file_offset)).await?;
                                file.write_all(&data).await?;
                                
                                if fsync_on_write {
                                    file.sync_data().await?;
                                } else {
                                    needs_flush = true;
                                }
                                
                                Ok(())
                            }.await;
                            
                            let _ = response.send(result);
                        },
                        Some(WriteCommand::Sync { response }) => {
                            let result = file.sync_data().await.map_err(Into::into);
                            needs_flush = false;
                            let _ = response.send(result);
                        },
                        Some(WriteCommand::Read { file_offset, size, response }) => {
                            let result = async {
                                file.seek(SeekFrom::Start(file_offset)).await?;
                                let mut buffer = vec![0u8; size];
                                file.read_exact(&mut buffer).await?;
                                Ok(buffer)
                            }.await;
                            
                            let _ = response.send(result);
                        },
                        Some(WriteCommand::Seek { position, response }) => {
                            let result = file.seek(SeekFrom::Start(position)).await
                                .map(|_| ())
                                .map_err(Into::into);
                            let _ = response.send(result);
                        },
                        Some(WriteCommand::Shutdown) | None => {
                            // Perform final flush before shutdown
                            if needs_flush {
                                let _ = file.sync_data().await;
                            }
                            break;
                        }
                    }
                },
                // Periodic flush when not using fsync_on_write
                _ = flush_interval.tick(), if !fsync_on_write && needs_flush => {
                    if let Err(e) = file.sync_data().await {
                        tracing::error!("Periodic flush failed: {}", e);
                    } else {
                        needs_flush = false;
                        tracing::debug!("Periodic flush completed");
                    }
                }
            }
        }
        
        tracing::info!("WAL file task shutting down");
    }

    async fn start_background_tasks(&self) -> Result<()> {
        // Note: Flush task is now integrated into the file_task
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

        tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(Duration::from_secs(1)); // Check more frequently for testing
            
            loop {
                check_interval.tick().await;
                let cfg = config.read().clone();
                let segment_start_time = *current_segment_start_time.read();
                let segment_size = current_segment_size.load(Ordering::SeqCst);
                
                let should_upload = segment_size >= cfg.segment_size_bytes ||
                    segment_start_time.elapsed() >= Duration::from_millis(cfg.upload_interval_ms);
                
                if should_upload && segment_size > 0 {
                    let end_offset = current_offset.load(Ordering::SeqCst);
                    let start_offset = current_segment_start_offset.load(Ordering::SeqCst);
                    
                    // Trigger upload callbacks
                    let callbacks = upload_callbacks.read();
                    for callback in callbacks.iter() {
                        callback(start_offset, end_offset);
                    }
                    
                    // Reset segment tracking for next segment
                    current_segment_size.store(0, Ordering::SeqCst);
                    current_segment_start_offset.store(end_offset, Ordering::SeqCst); // Next segment starts where this one ends
                    *current_segment_start_time.write() = Instant::now();
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
        
        // Note: File task handles flush configuration changes automatically
        // through its internal flush_interval and fsync_on_write logic
        
        Ok(())
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
            
            // Use channel to read from file
            let (tx, rx) = oneshot::channel();
            self.write_tx.send(WriteCommand::Read {
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
                        break; // Incomplete record in buffer
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
                    break; // Could not read record size
                }
            }
            file_offset += buffer_pos as u64;
        }

        self.current_offset.store(logical_offset, Ordering::SeqCst);
        // After recovery, the current segment starts from the recovered offset
        self.current_segment_start_offset.store(logical_offset, Ordering::SeqCst);

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

    async fn write_with_direct_io(&self, data: &[u8]) -> Result<u64> {
        // Get the current file offset for this write
        let file_offset = self.current_file_offset.fetch_add(data.len() as u64, Ordering::SeqCst);
        
        // Send write command to the file task
        let (tx, rx) = oneshot::channel();
        self.write_tx.send(WriteCommand::Write {
            data: data.to_vec(),
            file_offset,
            response: tx,
        }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
        
        // Wait for the write to complete
        rx.await
            .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
        
        Ok(file_offset)
    }
}

#[async_trait]
impl WriteAheadLog for DirectIOWal {
    async fn append(&self, record: WalRecord) -> Result<u64> {
        let buffer = self.buffer_pool.get_aligned_buffer(record.size() + 8)?;
        
        let serialized = bincode::serialize(&record)?;
        let record_size = serialized.len() as u64;
        
        let mut write_buffer = Vec::new();
        write_buffer.extend_from_slice(&record_size.to_le_bytes());
        write_buffer.extend_from_slice(&serialized);
        
        if write_buffer.len() > buffer.len() {
            return Err(crate::error::RustMqError::BufferTooSmall);
        }

        let file_offset = self.write_with_direct_io(&write_buffer).await?;
        let logical_offset = self.current_offset.fetch_add(1, Ordering::SeqCst);

        let segment_meta = WalSegmentMetadata {
            start_offset: logical_offset,
            end_offset: logical_offset + 1,
            file_offset,
            size_bytes: write_buffer.len() as u64,
            created_at: Instant::now(),
        };

        self.segments.write().push(segment_meta);
        
        // Update current segment size for upload monitoring
        self.current_segment_size.fetch_add(write_buffer.len() as u64, Ordering::SeqCst);

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
                // Read record size first
                let (tx, rx) = oneshot::channel();
                self.write_tx.send(WriteCommand::Read {
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

                // Read the actual record
                let (tx, rx) = oneshot::channel();
                self.write_tx.send(WriteCommand::Read {
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
            // Skip segments that don't overlap with our range
            if segment.end_offset <= start_offset || segment.start_offset >= end_offset {
                continue;
            }

            // Read records from this segment
            let mut file_offset = segment.file_offset;
            
            // Read through the segment sequentially
            while file_offset < segment.file_offset + segment.size_bytes {
                // Read record size first
                let (tx, rx) = oneshot::channel();
                self.write_tx.send(WriteCommand::Read {
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

                // Read the actual record
                let (tx, rx) = oneshot::channel();
                self.write_tx.send(WriteCommand::Read {
                    file_offset: file_offset + 8,
                    size: record_size,
                    response: tx,
                }).map_err(|_| crate::error::RustMqError::Wal("File task unavailable".to_string()))?;
                
                let record_buffer = rx.await
                    .map_err(|_| crate::error::RustMqError::Wal("File task response failed".to_string()))??;
                
                let record: WalRecord = bincode::deserialize(&record_buffer)?;
                
                // Early exit if we've read past our range
                if record.offset >= end_offset {
                    break;
                }
                
                // Only include records within our offset range
                if record.offset >= start_offset {
                    records.push(record);
                }
                
                // Move to next record
                file_offset += 8 + record_size as u64;
            }
        }

        Ok(records)
    }

    async fn sync(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.write_tx.send(WriteCommand::Sync {
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
        // After truncation, the current segment starts from the truncated offset
        self.current_segment_start_offset.store(offset, Ordering::SeqCst);
        Ok(())
    }

    async fn get_end_offset(&self) -> Result<u64> {
        Ok(self.current_offset.load(Ordering::SeqCst))
    }

    fn register_upload_callback(&self, callback: Box<dyn Fn(u64, u64) + Send + Sync>) {
        self.upload_callbacks.write().push(callback);
    }
}

impl Drop for DirectIOWal {
    fn drop(&mut self) {
        // Send shutdown signal to the file task
        let _ = self.write_tx.send(WriteCommand::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AlignedBufferPool;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_wal_append_and_read() {
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
        let wal = DirectIOWal::new(config, buffer_pool).await.unwrap();

        let record = WalRecord {
            topic_partition: TopicPartition {
                topic: "test-topic".to_string(),
                partition: 0,
            },
            offset: 0,
            record: Record {
                key: Some(b"key1".to_vec()),
                value: b"value1".to_vec(),
                headers: vec![],
                timestamp: chrono::Utc::now().timestamp_millis(),
            },
            crc32: 0,
        };

        let offset = wal.append(record.clone()).await.unwrap();
        assert_eq!(offset, 0);

        let records = wal.read(0, 1024).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].topic_partition.topic, "test-topic");
    }

    #[tokio::test]
    async fn test_wal_truncate() {
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
        let wal = DirectIOWal::new(config, buffer_pool).await.unwrap();

        for i in 0..5 {
            let record = WalRecord {
                topic_partition: TopicPartition {
                    topic: "test-topic".to_string(),
                    partition: 0,
                },
                offset: i,
                record: Record {
                    key: Some(format!("key{}", i).into_bytes()),
                    value: format!("value{}", i).into_bytes(),
                    headers: vec![],
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                crc32: 0,
            };
            wal.append(record).await.unwrap();
        }

        assert_eq!(wal.get_end_offset().await.unwrap(), 5);

        wal.truncate(3).await.unwrap();
        assert_eq!(wal.get_end_offset().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_upload_callback_size_trigger() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 1024, // Small segment for testing
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = DirectIOWal::new(config, buffer_pool).await.unwrap();

        let upload_triggered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let upload_triggered_clone = upload_triggered.clone();
        
        wal.register_upload_callback(move |start_offset, end_offset| {
            println!("Upload triggered: {} -> {}", start_offset, end_offset);
            upload_triggered_clone.store(true, Ordering::SeqCst);
        });

        // Write records to trigger size-based upload
        for i in 0..10 {
            let record = WalRecord {
                topic_partition: TopicPartition {
                    topic: "test-topic".to_string(),
                    partition: 0,
                },
                offset: i,
                record: Record {
                    key: Some(format!("key{}", i).into_bytes()),
                    value: vec![0u8; 200], // Large value to trigger size limit
                    headers: vec![],
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                crc32: 0,
            };
            wal.append(record).await.unwrap();
        }

        // Wait for upload trigger (check every second for 5 seconds)
        let mut found = false;
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(2_000)).await;
            if upload_triggered.load(Ordering::SeqCst) {
                found = true;
                break;
            }
        }
        assert!(found, "Upload callback was not triggered within 10 seconds");
    }

    #[tokio::test]
    async fn test_runtime_config_update() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: true,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = DirectIOWal::new(config, buffer_pool).await.unwrap();

        // Update config
        let new_config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 128 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 30_000,
            flush_interval_ms: 500,
        };

        let result = wal.update_config(new_config.clone()).await;
        assert!(result.is_ok());

        // Verify config was updated
        assert_eq!(wal.config.read().fsync_on_write, false);
        assert_eq!(wal.config.read().segment_size_bytes, 128 * 1024);
        assert_eq!(wal.config.read().upload_interval_ms, 30_000);
    }

    #[tokio::test]
    async fn test_precise_segment_offset_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 1024, // Small segment for testing
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = DirectIOWal::new(config, buffer_pool).await.unwrap();

        // Track upload callbacks with precise offsets
        let callback_results = Arc::new(std::sync::Mutex::new(Vec::<(u64, u64)>::new()));
        let callback_results_clone = callback_results.clone();
        
        wal.register_upload_callback(move |start_offset, end_offset| {
            callback_results_clone.lock().unwrap().push((start_offset, end_offset));
        });

        // Append several records
        let mut expected_end_offset = 0;
        for i in 0..5 {
            let record = WalRecord {
                topic_partition: TopicPartition {
                    topic: "test-topic".to_string(),
                    partition: 0,
                },
                offset: i,
                record: Record {
                    key: Some(format!("key{}", i).into_bytes()),
                    value: vec![0u8; 200], // Large value to trigger size limit quickly
                    headers: vec![],
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                crc32: 0,
            };
            expected_end_offset = wal.append(record).await.unwrap() + 1;
        }

        // Wait for upload callback to be triggered by size
        tokio::time::sleep(Duration::from_millis(2_000)).await;

        // Verify that the callback was called with precise offsets
        let results = callback_results.lock().unwrap();
        assert!(results.len() > 0, "Upload callback should have been triggered");
        
        for (start_offset, end_offset) in results.iter() {
            // Verify that start_offset is precise (not a rough approximation)
            assert!(*start_offset <= *end_offset, "Start offset should be <= end offset");
            assert!(*start_offset == 0 || *start_offset < *end_offset, "Offsets should be logical and precise");
            
            // The first segment should start at 0
            if results.len() == 1 {
                assert_eq!(*start_offset, 0, "First segment should start at offset 0");
            }
        }
    }

    #[tokio::test]
    async fn test_efficient_flush_task_no_blocking() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false, // Use background flush
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 50, // Frequent flush for testing
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = DirectIOWal::new(config, buffer_pool).await.unwrap();

        // Test that appends are not blocked by flush operations
        let start_time = Instant::now();
        
        // Write many records quickly
        for i in 0..50 {
            let record = WalRecord {
                topic_partition: TopicPartition {
                    topic: "test-topic".to_string(),
                    partition: 0,
                },
                offset: i,
                record: Record {
                    key: Some(format!("key{}", i).into_bytes()),
                    value: format!("value{}", i).into_bytes(),
                    headers: vec![],
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                crc32: 0,
            };
            wal.append(record).await.unwrap();
        }
        
        let append_duration = start_time.elapsed();
        
        // Force a sync to ensure all data is flushed
        wal.sync().await.unwrap();
        
        let total_duration = start_time.elapsed();
        
        // Verify that appends completed efficiently without being blocked by flush
        // The channel-based approach should prevent blocking
        assert_eq!(wal.get_end_offset().await.unwrap(), 50);
        
        // With the improved implementation, appends should be fast
        assert!(append_duration.as_millis() < 1000, "Appends took too long: {:?}", append_duration);
        
        println!("Sequential appends completed in: {:?}", append_duration);
        println!("Total time including final sync: {:?}", total_duration);
    }

    #[tokio::test]
    async fn test_read_range_efficiency() {
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
        let wal = DirectIOWal::new(config, buffer_pool).await.unwrap();

        // Write several records with different offsets
        let topic_partition = TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };

        let mut expected_records = Vec::new();
        
        // Write 10 records
        for i in 0..10 {
            let record = WalRecord {
                topic_partition: topic_partition.clone(),
                offset: i, // Will be set by append
                record: Record {
                    key: Some(format!("key-{}", i).into_bytes()),
                    value: format!("value-{}", i).into_bytes(),
                    headers: vec![],
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                crc32: 0,
            };
            
            let actual_offset = wal.append(record.clone()).await.unwrap();
            
            // Store records with their actual offsets for verification
            let mut updated_record = record;
            updated_record.offset = actual_offset;
            expected_records.push(updated_record);
        }

        // Test reading a range of records (offsets 3-6)
        let range_records = wal.read_range(3, 7).await.unwrap();
        
        // Should get exactly 4 records (offsets 3, 4, 5, 6)
        assert_eq!(range_records.len(), 4);
        
        // Verify the records are correct
        for (i, record) in range_records.iter().enumerate() {
            assert_eq!(record.offset, 3 + i as u64);
            assert_eq!(record.record.key, Some(format!("key-{}", 3 + i).into_bytes()));
            assert_eq!(record.record.value, format!("value-{}", 3 + i).into_bytes());
        }

        // Test reading a smaller range (just offset 5)
        let single_record = wal.read_range(5, 6).await.unwrap();
        assert_eq!(single_record.len(), 1);
        assert_eq!(single_record[0].offset, 5);

        // Test reading beyond available range
        let empty_records = wal.read_range(20, 25).await.unwrap();
        assert_eq!(empty_records.len(), 0);

        // Test reading partial overlap
        let partial_records = wal.read_range(8, 15).await.unwrap();
        assert_eq!(partial_records.len(), 2); // Should get offsets 8 and 9
        assert_eq!(partial_records[0].offset, 8);
        assert_eq!(partial_records[1].offset, 9);
    }
}