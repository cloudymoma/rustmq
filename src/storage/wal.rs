use crate::{Result, config::WalConfig, storage::traits::*, types::*};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use parking_lot::RwLock;

#[cfg(feature = "io-uring")]
use tokio_uring::fs::File as UringFile;

pub struct DirectIOWal {
    #[cfg(feature = "io-uring")]
    file: Option<Arc<tokio::sync::Mutex<UringFile>>>,
    #[cfg(not(feature = "io-uring"))]
    file: Arc<tokio::sync::Mutex<File>>,
    buffer_pool: Arc<dyn BufferPool>,
    current_offset: AtomicU64,
    config: WalConfig,
    segments: Arc<RwLock<Vec<WalSegmentMetadata>>>,
}

#[derive(Debug, Clone)]
struct WalSegmentMetadata {
    start_offset: u64,
    end_offset: u64,
    file_offset: u64,
    size_bytes: u64,
}

impl DirectIOWal {
    pub async fn new(config: WalConfig, buffer_pool: Arc<dyn BufferPool>) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path).await?;
        let file_path = config.path.join("wal.log");

        #[cfg(feature = "io-uring")]
        let file = {
            if tokio_uring::start().is_ok() {
                Some(Arc::new(tokio::sync::Mutex::new(UringFile::open(&file_path).await?)))
            } else {
                None
            }
        };

        #[cfg(not(feature = "io-uring"))]
        let file = Arc::new(tokio::sync::Mutex::new(
            tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .open(&file_path)
                .await?
        ));

        let mut wal = Self {
            file,
            buffer_pool,
            current_offset: AtomicU64::new(0),
            config,
            segments: Arc::new(RwLock::new(Vec::new())),
        };

        wal.recover().await?;
        Ok(wal)
    }

    async fn recover(&mut self) -> Result<()> {
        #[cfg(not(feature = "io-uring"))]
        {
            let mut file = self.file.lock().await;
            let file_size = file.metadata().await?.len();
            if file_size > 0 {
                let mut buffer = vec![0u8; file_size as usize];
                file.seek(SeekFrom::Start(0)).await?;
                file.read_exact(&mut buffer).await?;
                
                let mut offset = 0u64;
                let mut file_offset = 0u64;
                
                while file_offset < file_size {
                    if let Ok(record_size) = self.read_record_size(&buffer[file_offset as usize..]) {
                        if file_offset + record_size > file_size {
                            break;
                        }
                        
                        let segment_meta = WalSegmentMetadata {
                            start_offset: offset,
                            end_offset: offset + 1,
                            file_offset,
                            size_bytes: record_size,
                        };
                        
                        self.segments.write().push(segment_meta);
                        offset += 1;
                        file_offset += record_size;
                    } else {
                        break;
                    }
                }
                
                self.current_offset.store(offset, Ordering::SeqCst);
            }
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

    async fn write_with_direct_io(&self, data: &[u8]) -> Result<u64> {
        #[cfg(feature = "io-uring")]
        if let Some(ref uring_file) = self.file {
            let offset = self.current_offset.load(Ordering::SeqCst);
            let (result, _buf) = uring_file.write_at(data, offset).await;
            result?;
            return Ok(offset);
        }

        #[cfg(not(feature = "io-uring"))]
        {
            use tokio::io::AsyncWriteExt;
            let offset = self.current_offset.load(Ordering::SeqCst);
            let mut file = self.file.lock().await;
            file.seek(SeekFrom::Start(offset)).await?;
            file.write_all(data).await?;
            
            if self.config.fsync_on_write {
                file.sync_data().await?;
            }
            
            Ok(offset)
        }
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
        };

        self.segments.write().push(segment_meta);

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
                #[cfg(not(feature = "io-uring"))]
                {
                    let mut file = self.file.lock().await;
                    file.seek(SeekFrom::Start(segment.file_offset)).await?;
                    
                    let mut size_buffer = [0u8; 8];
                    file.read_exact(&mut size_buffer).await?;
                    let record_size = u64::from_le_bytes(size_buffer) as usize;
                    
                    if bytes_read + record_size > max_bytes {
                        break;
                    }

                    let mut record_buffer = vec![0u8; record_size];
                    file.read_exact(&mut record_buffer).await?;
                    
                    let record: WalRecord = bincode::deserialize(&record_buffer)?;
                    records.push(record);
                    bytes_read += record_size;
                }
            }
        }

        Ok(records)
    }

    async fn sync(&self) -> Result<()> {
        #[cfg(not(feature = "io-uring"))]
        {
            let file = self.file.lock().await;
            file.sync_data().await?;
        }
        Ok(())
    }

    async fn truncate(&self, offset: u64) -> Result<()> {
        let mut segments = self.segments.write();
        segments.retain(|seg| seg.start_offset < offset);
        self.current_offset.store(offset, Ordering::SeqCst);
        Ok(())
    }

    async fn get_end_offset(&self) -> Result<u64> {
        Ok(self.current_offset.load(Ordering::SeqCst))
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
}