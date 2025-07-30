use crate::{Result, storage::*, types::*};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;

pub struct TieredStorageEngine {
    wal: Arc<dyn WriteAheadLog>,
    cache_manager: Arc<CacheManager>,
    object_storage: Arc<dyn ObjectStorage>,
    upload_manager: Arc<dyn UploadManager>,
    metadata_store: Arc<MetadataStore>,
    compaction_manager: Arc<dyn CompactionManager>,
}

pub struct MetadataStore {
    segments: Arc<RwLock<HashMap<TopicPartition, Vec<SegmentMetadata>>>>,
}

#[derive(Debug, Clone)]
pub struct SegmentMetadata {
    pub start_offset: u64,
    pub end_offset: u64,
    pub object_key: String,
    pub size_bytes: u64,
    pub status: SegmentStatus,
}

#[derive(Debug, Clone)]
pub enum SegmentStatus {
    InWal,
    Uploading,
    InObjectStore,
    Failed(String),
}

impl MetadataStore {
    pub fn new() -> Self {
        Self {
            segments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_segment(&self, topic_partition: TopicPartition, metadata: SegmentMetadata) {
        let mut segments = self.segments.write();
        segments
            .entry(topic_partition)
            .or_insert_with(Vec::new)
            .push(metadata);
    }

    pub fn get_segments(&self, topic_partition: &TopicPartition) -> Vec<SegmentMetadata> {
        let segments = self.segments.read();
        segments.get(topic_partition).cloned().unwrap_or_default()
    }

    pub fn update_segment_status(
        &self,
        topic_partition: &TopicPartition,
        start_offset: u64,
        status: SegmentStatus,
    ) -> Result<()> {
        let mut segments = self.segments.write();
        if let Some(topic_segments) = segments.get_mut(topic_partition) {
            for segment in topic_segments.iter_mut() {
                if segment.start_offset == start_offset {
                    segment.status = status;
                    return Ok(());
                }
            }
        }
        Err(crate::error::RustMqError::Storage(
            "Segment not found".to_string(),
        ))
    }
}

pub struct CompactionManagerImpl {
    object_storage: Arc<dyn ObjectStorage>,
    metadata_store: Arc<MetadataStore>,
}

impl CompactionManagerImpl {
    pub fn new(
        object_storage: Arc<dyn ObjectStorage>,
        metadata_store: Arc<MetadataStore>,
    ) -> Self {
        Self {
            object_storage,
            metadata_store,
        }
    }
}

#[async_trait]
impl CompactionManager for CompactionManagerImpl {
    async fn compact_segments(&self, segment_keys: Vec<String>) -> Result<String> {
        let mut combined_data = Vec::new();
        let mut start_offset = u64::MAX;
        let mut end_offset = 0u64;

        for key in &segment_keys {
            let segment_data = self.object_storage.get(key).await?;
            combined_data.extend_from_slice(&segment_data);

            let parts: Vec<&str> = key.split('_').collect();
            if parts.len() >= 2 {
                if let (Ok(s), Ok(e)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
                    start_offset = start_offset.min(s);
                    end_offset = end_offset.max(e);
                }
            }
        }

        let compacted_key = format!("compacted_{start_offset}_{end_offset}.seg");
        self.object_storage
            .put(&compacted_key, Bytes::from(combined_data))
            .await?;

        for key in segment_keys {
            self.object_storage.delete(&key).await?;
        }

        Ok(compacted_key)
    }

    async fn schedule_compaction(&self, topic_partition: TopicPartition) -> Result<()> {
        let segments = self.metadata_store.get_segments(&topic_partition);
        let small_segments: Vec<_> = segments
            .iter()
            .filter(|s| s.size_bytes < 10 * 1024 * 1024 && matches!(s.status, SegmentStatus::InObjectStore))
            .collect();

        if small_segments.len() >= 3 {
            let segment_keys: Vec<String> = small_segments
                .iter()
                .map(|s| s.object_key.clone())
                .collect();

            tokio::spawn({
                let compaction_manager = Arc::new(self.clone());
                async move {
                    if let Err(e) = compaction_manager.compact_segments(segment_keys).await {
                        tracing::error!("Compaction failed: {}", e);
                    }
                }
            });
        }

        Ok(())
    }

    async fn get_compaction_status(&self, _topic_partition: &TopicPartition) -> Result<CompactionStatus> {
        Ok(CompactionStatus::NotScheduled)
    }
}

impl Clone for CompactionManagerImpl {
    fn clone(&self) -> Self {
        Self {
            object_storage: self.object_storage.clone(),
            metadata_store: self.metadata_store.clone(),
        }
    }
}

impl TieredStorageEngine {
    pub fn new(
        wal: Arc<dyn WriteAheadLog>,
        cache_manager: Arc<CacheManager>,
        object_storage: Arc<dyn ObjectStorage>,
        upload_manager: Arc<dyn UploadManager>,
    ) -> Self {
        let metadata_store = Arc::new(MetadataStore::new());
        let compaction_manager = Arc::new(CompactionManagerImpl::new(
            object_storage.clone(),
            metadata_store.clone(),
        ));

        Self {
            wal,
            cache_manager,
            object_storage,
            upload_manager,
            metadata_store,
            compaction_manager,
        }
    }

    fn calculate_crc32(record: &Record) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        
        // Hash the record value
        hasher.update(&record.value);
        
        // Hash the key if present
        if let Some(ref key) = record.key {
            hasher.update(key);
        }
        
        // Hash headers
        for header in &record.headers {
            hasher.update(header.key.as_bytes());
            hasher.update(&header.value);
        }
        
        // Hash timestamp as bytes
        hasher.update(&record.timestamp.to_le_bytes());
        
        hasher.finalize()
    }

    pub async fn append(&self, topic_partition: TopicPartition, record: Record) -> Result<Offset> {
        let wal_record = WalRecord {
            topic_partition: topic_partition.clone(),
            offset: 0, // Will be set by WAL
            record: record.clone(),
            crc32: Self::calculate_crc32(&record),
        };

        let record_copy = wal_record.record.clone();
        let offset = self.wal.append(wal_record).await?;

        let cache_key = format!("{topic_partition}:{offset}");
        let serialized = bincode::serialize(&record_copy)?;
        self.cache_manager
            .cache_write(&cache_key, Bytes::from(serialized))
            .await?;

        Ok(offset)
    }

    pub async fn read(
        &self,
        topic_partition: &TopicPartition,
        offset: Offset,
        max_bytes: usize,
    ) -> Result<Vec<Record>> {
        let cache_key = format!("{topic_partition}:{offset}");

        if let Some(cached_data) = self.cache_manager.serve_read(&cache_key).await? {
            let record: Record = bincode::deserialize(&cached_data)?;
            return Ok(vec![record]);
        }

        let wal_records = self.wal.read(offset, max_bytes).await?;
        if !wal_records.is_empty() {
            let records: Vec<Record> = wal_records.into_iter().map(|r| r.record).collect();
            return Ok(records);
        }

        let segments = self.metadata_store.get_segments(topic_partition);
        for segment in segments {
            if segment.start_offset <= offset && offset < segment.end_offset {
                if matches!(segment.status, SegmentStatus::InObjectStore) {
                    let object_data = self.object_storage.get(&segment.object_key).await?;
                    self.cache_manager
                        .cache_read(&cache_key, object_data.clone())
                        .await?;

                    let segment = self.upload_manager.download_segment(&segment.object_key).await?;
                    let wal_records = bincode::deserialize::<Vec<WalRecord>>(&segment.data)?;
                    let records: Vec<Record> = wal_records.into_iter().map(|r| r.record).collect();
                    return Ok(records);
                }
            }
        }

        Ok(vec![])
    }

    pub async fn upload_segment(
        &self,
        topic_partition: TopicPartition,
        start_offset: u64,
        end_offset: u64,
    ) -> Result<()> {
        let wal_records = self.wal.read(start_offset, usize::MAX).await?;
        let relevant_records: Vec<WalRecord> = wal_records
            .into_iter()
            .filter(|r| r.offset >= start_offset && r.offset < end_offset)
            .collect();

        let serialized_data = bincode::serialize(&relevant_records)?;
        let segment = WalSegment {
            start_offset,
            end_offset,
            size_bytes: serialized_data.len() as u64,
            data: Bytes::from(serialized_data),
            topic_partition: topic_partition.clone(),
        };

        let metadata = SegmentMetadata {
            start_offset,
            end_offset,
            object_key: "".to_string(),
            size_bytes: segment.size_bytes,
            status: SegmentStatus::Uploading,
        };

        self.metadata_store.add_segment(topic_partition.clone(), metadata);

        let segment_clone = segment.clone();
        let object_key = self.upload_manager.upload_segment(segment).await?;

        self.metadata_store.update_segment_status(
            &topic_partition,
            start_offset,
            SegmentStatus::InObjectStore,
        )?;

        let updated_metadata = SegmentMetadata {
            start_offset,
            end_offset,
            object_key,
            size_bytes: segment_clone.size_bytes,
            status: SegmentStatus::InObjectStore,
        };

        self.metadata_store.add_segment(topic_partition.clone(), updated_metadata);

        self.compaction_manager.schedule_compaction(topic_partition).await?;

        Ok(())
    }

    pub async fn get_high_watermark(&self, _topic_partition: &TopicPartition) -> Result<Offset> {
        self.wal.get_end_offset().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{DirectIOWal, AlignedBufferPool, LocalObjectStorage, UploadManagerImpl};
    use crate::config::{WalConfig, CacheConfig, ObjectStorageConfig, StorageType};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_tiered_storage_engine() {
        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig {
            path: temp_dir.path().join("wal"),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let cache_config = CacheConfig {
            write_cache_size_bytes: 1024,
            read_cache_size_bytes: 1024,
            eviction_policy: crate::config::EvictionPolicy::Lru,
        };

        let storage_config = ObjectStorageConfig {
            storage_type: StorageType::Local {
                path: temp_dir.path().join("storage"),
            },
            bucket: "test".to_string(),
            region: "local".to_string(),
            endpoint: "".to_string(),
            access_key: None,
            secret_key: None,
            multipart_threshold: 1024,
            max_concurrent_uploads: 1,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap()) as Arc<dyn WriteAheadLog>;
        let cache_manager = Arc::new(CacheManager::new(&cache_config));
        let object_storage = Arc::new(
            LocalObjectStorage::new(temp_dir.path().join("storage")).unwrap()
        ) as Arc<dyn ObjectStorage>;
        let upload_manager = Arc::new(UploadManagerImpl::new(object_storage.clone(), storage_config)) as Arc<dyn UploadManager>;

        let storage_engine = TieredStorageEngine::new(
            wal,
            cache_manager,
            object_storage,
            upload_manager,
        );

        let topic_partition = TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };

        let record = Record {
            key: Some(b"key1".to_vec()),
            value: b"value1".to_vec(),
            headers: vec![],
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        let offset = storage_engine
            .append(topic_partition.clone(), record.clone())
            .await
            .unwrap();

        let records = storage_engine
            .read(&topic_partition, offset, 1024)
            .await
            .unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].value, b"value1");
    }
}