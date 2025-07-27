use crate::{Result, storage::traits::*, config::*};
use async_trait::async_trait;
use bytes::Bytes;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncSeekExt};

pub struct LocalObjectStorage {
    base_path: PathBuf,
}

impl LocalObjectStorage {
    pub fn new(base_path: PathBuf) -> Result<Self> {
        Ok(Self { base_path })
    }

    fn key_to_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }
}

#[async_trait]
impl ObjectStorage for LocalObjectStorage {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        let path = self.key_to_path(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        let mut file = fs::File::create(&path).await?;
        file.write_all(&data).await?;
        file.sync_all().await?;
        
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        let path = self.key_to_path(key);
        let data = fs::read(&path).await?;
        Ok(Bytes::from(data))
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        let path = self.key_to_path(key);
        let mut file = fs::File::open(&path).await?;
        
        file.seek(tokio::io::SeekFrom::Start(range.start)).await?;
        let read_size = (range.end - range.start) as usize;
        let mut buffer = vec![0u8; read_size];
        file.read_exact(&mut buffer).await?;
        
        Ok(Bytes::from(buffer))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.key_to_path(key);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let prefix_path = self.key_to_path(prefix);
        let mut results = Vec::new();
        
        if prefix_path.is_dir() {
            let mut entries = fs::read_dir(&prefix_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                if let Some(name) = entry.file_name().to_str() {
                    results.push(format!("{}/{}", prefix, name));
                }
            }
        }
        
        Ok(results)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.key_to_path(key);
        Ok(path.exists())
    }
}

pub struct BandwidthLimiter {
    capacity: u64,
    tokens: Arc<std::sync::atomic::AtomicU64>,
    refill_rate: u64,
    last_refill: Arc<tokio::sync::Mutex<tokio::time::Instant>>,
}

impl BandwidthLimiter {
    pub fn new(capacity: u64, refill_rate: u64) -> Self {
        Self {
            capacity,
            tokens: Arc::new(std::sync::atomic::AtomicU64::new(capacity)),
            refill_rate,
            last_refill: Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now())),
        }
    }

    pub async fn acquire(&self, bytes: usize) -> Result<()> {
        use std::sync::atomic::Ordering;
        
        loop {
            let current_tokens = self.tokens.load(Ordering::SeqCst);
            if current_tokens >= bytes as u64 {
                match self.tokens.compare_exchange_weak(
                    current_tokens,
                    current_tokens - bytes as u64,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return Ok(()),
                    Err(_) => continue,
                }
            } else {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                self.refill_tokens().await;
            }
        }
    }

    async fn refill_tokens(&self) {
        use std::sync::atomic::Ordering;
        
        let mut last_refill = self.last_refill.lock().await;
        let now = tokio::time::Instant::now();
        let elapsed = now.duration_since(*last_refill);

        if elapsed >= tokio::time::Duration::from_millis(10) {
            let tokens_to_add = (elapsed.as_millis() as u64 * self.refill_rate) / 1000;
            let current = self.tokens.load(Ordering::SeqCst);
            let new_tokens = (current + tokens_to_add).min(self.capacity);
            self.tokens.store(new_tokens, Ordering::SeqCst);
            *last_refill = now;
        }
    }
}

pub struct UploadManagerImpl {
    storage: Arc<dyn ObjectStorage>,
    bandwidth_limiter: Arc<BandwidthLimiter>,
    config: ObjectStorageConfig,
}

impl UploadManagerImpl {
    pub fn new(
        storage: Arc<dyn ObjectStorage>,
        config: ObjectStorageConfig,
    ) -> Self {
        let bandwidth_limiter = Arc::new(BandwidthLimiter::new(
            100 * 1024 * 1024, // 100 MB/s capacity
            10 * 1024 * 1024,  // 10 MB/s refill rate
        ));

        Self {
            storage,
            bandwidth_limiter,
            config,
        }
    }

    async fn compress_segment(&self, segment: &WalSegment) -> Result<Bytes> {
        match self.config.storage_type {
            StorageType::S3 | StorageType::Gcs | StorageType::Azure => {
                let compressed = lz4_flex::compress_prepend_size(&segment.data);
                Ok(Bytes::from(compressed))
            }
            StorageType::Local { .. } => Ok(segment.data.clone()),
        }
    }

    async fn simple_upload(&self, data: &[u8], key: &str) -> Result<String> {
        self.storage.put(key, Bytes::copy_from_slice(data)).await?;
        Ok(key.to_string())
    }

    async fn multipart_upload(&self, data: &[u8], key: &str) -> Result<String> {
        let chunk_size = 5 * 1024 * 1024; // 5MB chunks
        let mut offset = 0;
        
        while offset < data.len() {
            let end = (offset + chunk_size).min(data.len());
            let chunk = &data[offset..end];
            let chunk_key = format!("{}.part{}", key, offset / chunk_size);
            
            self.storage.put(&chunk_key, Bytes::copy_from_slice(chunk)).await?;
            offset = end;
        }

        Ok(key.to_string())
    }
}

#[async_trait]
impl UploadManager for UploadManagerImpl {
    async fn upload_segment(&self, segment: WalSegment) -> Result<String> {
        self.bandwidth_limiter.acquire(segment.size()).await?;

        let compressed = self.compress_segment(&segment).await?;
        let object_key = format!(
            "topics/{}/{}/{}_{}.seg",
            segment.topic_partition.topic,
            segment.topic_partition.partition,
            segment.start_offset,
            segment.end_offset
        );

        let result = if compressed.len() > self.config.multipart_threshold as usize {
            self.multipart_upload(&compressed, &object_key).await?
        } else {
            self.simple_upload(&compressed, &object_key).await?
        };

        self.verify_upload(&object_key, &compressed).await?;
        Ok(result)
    }

    async fn download_segment(&self, object_key: &str) -> Result<WalSegment> {
        let compressed_data = self.storage.get(object_key).await?;
        
        let data = match self.config.storage_type {
            StorageType::S3 | StorageType::Gcs | StorageType::Azure => {
                let decompressed = lz4_flex::decompress_size_prepended(&compressed_data)
                    .map_err(|e| crate::error::RustMqError::Storage(e.to_string()))?;
                Bytes::from(decompressed)
            }
            StorageType::Local { .. } => compressed_data,
        };

        let parts: Vec<&str> = object_key.split('/').collect();
        if parts.len() < 4 {
            return Err(crate::error::RustMqError::Storage(
                "Invalid object key format".to_string(),
            ));
        }

        let topic = parts[1].to_string();
        let partition = parts[2].parse::<u32>()
            .map_err(|_| crate::error::RustMqError::Storage("Invalid partition".to_string()))?;
        
        let filename = parts[3];
        let offset_parts: Vec<&str> = filename.split('_').collect();
        if offset_parts.len() < 2 {
            return Err(crate::error::RustMqError::Storage(
                "Invalid filename format".to_string(),
            ));
        }

        let start_offset = offset_parts[0].parse::<u64>()
            .map_err(|_| crate::error::RustMqError::Storage("Invalid start offset".to_string()))?;
        let end_offset = offset_parts[1].trim_end_matches(".seg").parse::<u64>()
            .map_err(|_| crate::error::RustMqError::Storage("Invalid end offset".to_string()))?;

        Ok(WalSegment {
            start_offset,
            end_offset,
            size_bytes: data.len() as u64,
            data,
            topic_partition: crate::types::TopicPartition { topic, partition },
        })
    }

    async fn verify_upload(&self, object_key: &str, expected_data: &[u8]) -> Result<bool> {
        let uploaded_data = self.storage.get(object_key).await?;
        Ok(uploaded_data.as_ref() == expected_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_object_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalObjectStorage::new(temp_dir.path().to_path_buf()).unwrap();

        let test_data = Bytes::from("test data");
        storage.put("test/key", test_data.clone()).await.unwrap();

        assert!(storage.exists("test/key").await.unwrap());
        let retrieved = storage.get("test/key").await.unwrap();
        assert_eq!(retrieved, test_data);

        storage.delete("test/key").await.unwrap();
        assert!(!storage.exists("test/key").await.unwrap());
    }

    #[tokio::test]
    async fn test_bandwidth_limiter() {
        let limiter = BandwidthLimiter::new(1000, 100); // 1000 tokens capacity, 100/s refill

        limiter.acquire(500).await.unwrap();
        limiter.acquire(500).await.unwrap();

        let start = tokio::time::Instant::now();
        limiter.acquire(100).await.unwrap(); // Should wait for refill
        let elapsed = start.elapsed();
        
        assert!(elapsed >= tokio::time::Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_upload_manager() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(LocalObjectStorage::new(temp_dir.path().to_path_buf()).unwrap());
        let config = ObjectStorageConfig {
            storage_type: StorageType::Local { path: temp_dir.path().to_path_buf() },
            bucket: "test".to_string(),
            region: "local".to_string(),
            endpoint: "".to_string(),
            access_key: None,
            secret_key: None,
            multipart_threshold: 1024,
            max_concurrent_uploads: 1,
        };

        let upload_manager = UploadManagerImpl::new(storage, config);

        let segment = WalSegment {
            start_offset: 0,
            end_offset: 100,
            size_bytes: 128,
            data: Bytes::from("test segment data"),
            topic_partition: crate::types::TopicPartition {
                topic: "test-topic".to_string(),
                partition: 0,
            },
        };

        let object_key = upload_manager.upload_segment(segment.clone()).await.unwrap();
        let downloaded = upload_manager.download_segment(&object_key).await.unwrap();

        assert_eq!(downloaded.topic_partition.topic, segment.topic_partition.topic);
        assert_eq!(downloaded.start_offset, segment.start_offset);
        assert_eq!(downloaded.end_offset, segment.end_offset);
    }
}