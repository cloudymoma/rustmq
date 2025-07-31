use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

use crate::error::{RustMqError, Result};
use crate::types::*;
use crate::storage::traits::{WriteAheadLog, ObjectStorage, Cache};
use crate::replication::traits::ReplicationManager;
use crate::network::traits::NetworkHandler;

/// High-level message broker core that orchestrates produce/consume operations
pub struct MessageBrokerCore<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    wal: Arc<W>,
    object_storage: Arc<O>,
    cache: Arc<C>,
    replication_manager: Arc<R>,
    network_handler: Arc<N>,
    partitions: Arc<RwLock<HashMap<TopicPartition, PartitionMetadata>>>,
    broker_id: BrokerId,
}

#[derive(Debug, Clone)]
pub struct PartitionMetadata {
    pub leader_epoch: u64,
    pub high_watermark: Offset,
    pub is_leader: bool,
    pub replicas: Vec<BrokerId>,
    pub in_sync_replicas: Vec<BrokerId>,
}

/// High-level producer API
#[async_trait]
pub trait Producer {
    /// Send a single record to a topic-partition
    async fn send(&self, record: ProduceRecord) -> Result<ProduceResult>;
    
    /// Send a batch of records to a topic-partition
    async fn send_batch(&self, records: Vec<ProduceRecord>) -> Result<Vec<ProduceResult>>;
    
    /// Flush any pending records
    async fn flush(&self) -> Result<()>;
}

/// High-level consumer API
#[async_trait]
pub trait Consumer {
    /// Subscribe to topics
    async fn subscribe(&mut self, topics: Vec<TopicName>) -> Result<()>;
    
    /// Poll for records with timeout
    async fn poll(&mut self, timeout_ms: u32) -> Result<Vec<ConsumeRecord>>;
    
    /// Commit offsets
    async fn commit_offsets(&mut self, offsets: HashMap<TopicPartition, Offset>) -> Result<()>;
    
    /// Seek to specific offset
    async fn seek(&mut self, topic_partition: TopicPartition, offset: Offset) -> Result<()>;
}

/// Record for producing
#[derive(Debug, Clone)]
pub struct ProduceRecord {
    pub topic: TopicName,
    pub partition: Option<PartitionId>,
    pub key: Option<Vec<u8>>,
    pub value: Vec<u8>,
    pub headers: Vec<Header>,
    pub acks: AcknowledgmentLevel,
    pub timeout_ms: u32,
}

/// Result of produce operation
#[derive(Debug, Clone)]
pub struct ProduceResult {
    pub topic_partition: TopicPartition,
    pub offset: Offset,
    pub timestamp: i64,
}

/// Record for consuming
#[derive(Debug, Clone)]
pub struct ConsumeRecord {
    pub topic_partition: TopicPartition,
    pub offset: Offset,
    pub key: Option<Vec<u8>>,
    pub value: Vec<u8>,
    pub headers: Vec<Header>,
    pub timestamp: i64,
}

impl<W, O, C, R, N> MessageBrokerCore<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    pub fn new(
        wal: Arc<W>,
        object_storage: Arc<O>,
        cache: Arc<C>,
        replication_manager: Arc<R>,
        network_handler: Arc<N>,
        broker_id: BrokerId,
    ) -> Self {
        Self {
            wal,
            object_storage,
            cache,
            replication_manager,
            network_handler,
            partitions: Arc::new(RwLock::new(HashMap::new())),
            broker_id,
        }
    }

    /// Create a new producer instance
    pub fn create_producer(&self) -> MessageProducer<W, O, C, R, N> {
        MessageProducer::new(self)
    }

    /// Create a new consumer instance
    pub fn create_consumer(&self, consumer_group: String) -> MessageConsumer<W, O, C, R, N> {
        MessageConsumer::new(self, consumer_group)
    }

    /// Add a partition to this broker
    pub async fn add_partition(
        &self,
        topic_partition: TopicPartition,
        metadata: PartitionMetadata,
    ) -> Result<()> {
        let mut partitions = self.partitions.write().await;
        partitions.insert(topic_partition, metadata);
        Ok(())
    }

    /// Remove a partition from this broker
    pub async fn remove_partition(&self, topic_partition: &TopicPartition) -> Result<()> {
        let mut partitions = self.partitions.write().await;
        partitions.remove(topic_partition);
        Ok(())
    }

    /// Get partition metadata
    pub async fn get_partition_metadata(
        &self,
        topic_partition: &TopicPartition,
    ) -> Result<Option<PartitionMetadata>> {
        let partitions = self.partitions.read().await;
        Ok(partitions.get(topic_partition).cloned())
    }

    /// Internal method to append records to WAL and replicate
    async fn append_records(
        &self,
        topic_partition: &TopicPartition,
        records: Vec<Record>,
        acks: AcknowledgmentLevel,
    ) -> Result<Vec<Offset>> {
        // Get partition metadata
        let metadata = self
            .get_partition_metadata(topic_partition)
            .await?
            .ok_or_else(|| RustMqError::PartitionNotFound(topic_partition.to_string()))?;

        if !metadata.is_leader {
            return Err(RustMqError::NotLeader(topic_partition.to_string()));
        }

        // Create WAL records with offsets
        let mut wal_records = Vec::new();
        let mut offsets = Vec::new();
        let base_offset = metadata.high_watermark;

        for (i, record) in records.into_iter().enumerate() {
            let offset = base_offset + i as u64;
            let wal_record = WalRecord {
                topic_partition: topic_partition.clone(),
                offset,
                record,
                crc32: 0, // Calculate actual CRC32
            };
            wal_records.push(wal_record);
            offsets.push(offset);
        }

        // Append to WAL
        for record in &wal_records {
            self.wal.append(record.clone()).await?;
        }

        // Replicate based on acknowledgment level
        match acks {
            AcknowledgmentLevel::None => {
                // Fire and forget - no replication wait
            }
            AcknowledgmentLevel::Leader => {
                // Leader acknowledgment only - records are already in WAL
            }
            AcknowledgmentLevel::Majority | AcknowledgmentLevel::All => {
                // Wait for replication
                for record in &wal_records {
                    self.replication_manager
                        .replicate_record(record.clone())
                        .await?;
                }
            }
            AcknowledgmentLevel::Custom(_n) => {
                for record in &wal_records {
                    self.replication_manager
                        .replicate_record(record.clone())
                        .await?;
                }
            }
        }

        // Update high watermark
        let mut partitions = self.partitions.write().await;
        if let Some(metadata) = partitions.get_mut(topic_partition) {
            metadata.high_watermark = base_offset + wal_records.len() as u64;
        }

        Ok(offsets)
    }

    /// Internal method to fetch records
    async fn fetch_records(
        &self,
        topic_partition: &TopicPartition,
        fetch_offset: Offset,
        max_bytes: u32,
    ) -> Result<Vec<Record>> {
        // Try cache first
        if let Ok(Some(cached_data)) = self
            .cache
            .get(&format!("{}:{}", topic_partition, fetch_offset))
            .await
        {
            if let Ok(cached_records) = bincode::deserialize::<Vec<Record>>(&cached_data) {
                return Ok(cached_records);
            }
        }

        // Try WAL for recent records
        match self
            .wal
            .read(fetch_offset, max_bytes as usize)
            .await
        {
            Ok(wal_records) => {
                let records: Vec<Record> = wal_records.into_iter().map(|wr| wr.record).collect();
                return Ok(records);
            }
            Err(RustMqError::OffsetOutOfRange(_)) => {
                // Fall through to object storage
            }
            Err(e) => return Err(e),
        }

        // Fetch from object storage
        let object_key = format!("{}/{}", topic_partition, fetch_offset);
        let data = self.object_storage.get(&object_key).await?;
        let records: Vec<Record> = bincode::deserialize(&data)?;

        // Cache the results
        let cache_key = format!("{}:{}", topic_partition, fetch_offset);
        let serialized_records = bincode::serialize(&records)?;
        let _ = self.cache.put(&cache_key, serialized_records.into()).await;

        Ok(records)
    }
}

/// High-level producer implementation
pub struct MessageProducer<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    core: Arc<MessageBrokerCore<W, O, C, R, N>>,
}

impl<W, O, C, R, N> MessageProducer<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    fn new(core: &MessageBrokerCore<W, O, C, R, N>) -> Self {
        Self {
            core: Arc::new(core.clone()),
        }
    }
}

#[async_trait]
impl<W, O, C, R, N> Producer for MessageProducer<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    async fn send(&self, record: ProduceRecord) -> Result<ProduceResult> {
        let partition_id = record.partition.unwrap_or(0); // Simple partitioning
        let topic_partition = TopicPartition {
            topic: record.topic.clone(),
            partition: partition_id,
        };

        let wal_record = Record {
            key: record.key,
            value: record.value,
            headers: record.headers,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        let offsets = self
            .core
            .append_records(&topic_partition, vec![wal_record], record.acks)
            .await?;

        Ok(ProduceResult {
            topic_partition,
            offset: offsets[0],
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn send_batch(&self, records: Vec<ProduceRecord>) -> Result<Vec<ProduceResult>> {
        let mut results = Vec::new();

        // Group records by topic-partition
        let mut grouped_records: HashMap<TopicPartition, (Vec<Record>, AcknowledgmentLevel)> =
            HashMap::new();

        for record in records {
            let partition_id = record.partition.unwrap_or(0);
            let topic_partition = TopicPartition {
                topic: record.topic.clone(),
                partition: partition_id,
            };

            let wal_record = Record {
                key: record.key,
                value: record.value,
                headers: record.headers,
                timestamp: chrono::Utc::now().timestamp_millis(),
            };

            grouped_records
                .entry(topic_partition)
                .or_insert((Vec::new(), record.acks))
                .0
                .push(wal_record);
        }

        // Send each group
        for (topic_partition, (group_records, acks)) in grouped_records {
            let offsets = self
                .core
                .append_records(&topic_partition, group_records, acks)
                .await?;

            for offset in offsets {
                results.push(ProduceResult {
                    topic_partition: topic_partition.clone(),
                    offset,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                });
            }
        }

        Ok(results)
    }

    async fn flush(&self) -> Result<()> {
        self.core.wal.sync().await
    }
}

/// High-level consumer implementation
pub struct MessageConsumer<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    core: Arc<MessageBrokerCore<W, O, C, R, N>>,
    consumer_group: String,
    subscribed_topics: Vec<TopicName>,
    partition_offsets: HashMap<TopicPartition, Offset>,
}

impl<W, O, C, R, N> MessageConsumer<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    fn new(core: &MessageBrokerCore<W, O, C, R, N>, consumer_group: String) -> Self {
        Self {
            core: Arc::new(core.clone()),
            consumer_group,
            subscribed_topics: Vec::new(),
            partition_offsets: HashMap::new(),
        }
    }
}

#[async_trait]
impl<W, O, C, R, N> Consumer for MessageConsumer<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    async fn subscribe(&mut self, topics: Vec<TopicName>) -> Result<()> {
        self.subscribed_topics = topics;
        // Initialize offsets for all topic-partitions
        // This is a simplified implementation - in reality, we'd need
        // to discover partitions and coordinate with consumer group
        for topic in &self.subscribed_topics {
            let topic_partition = TopicPartition {
                topic: topic.clone(),
                partition: 0,
            };
            self.partition_offsets.insert(topic_partition, 0);
        }
        Ok(())
    }

    async fn poll(&mut self, timeout_ms: u32) -> Result<Vec<ConsumeRecord>> {
        let mut records = Vec::new();

        for (topic_partition, current_offset) in &mut self.partition_offsets {
            match self
                .core
                .fetch_records(topic_partition, *current_offset, 1024 * 1024) // 1MB max
                .await
            {
                Ok(fetched_records) => {
                    for (i, record) in fetched_records.into_iter().enumerate() {
                        records.push(ConsumeRecord {
                            topic_partition: topic_partition.clone(),
                            offset: *current_offset + i as u64,
                            key: record.key,
                            value: record.value,
                            headers: record.headers,
                            timestamp: record.timestamp,
                        });
                    }
                    *current_offset += records.len() as u64;
                }
                Err(RustMqError::OffsetOutOfRange(_)) => {
                    // No more records available
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(records)
    }

    async fn commit_offsets(&mut self, offsets: HashMap<TopicPartition, Offset>) -> Result<()> {
        for (topic_partition, offset) in offsets {
            self.partition_offsets.insert(topic_partition, offset);
        }
        // In a real implementation, we'd persist these offsets
        Ok(())
    }

    async fn seek(&mut self, topic_partition: TopicPartition, offset: Offset) -> Result<()> {
        self.partition_offsets.insert(topic_partition, offset);
        Ok(())
    }
}

impl<W, O, C, R, N> Clone for MessageBrokerCore<W, O, C, R, N>
where
    W: WriteAheadLog + Send + Sync,
    O: ObjectStorage + Send + Sync,
    C: Cache + Send + Sync,
    R: ReplicationManager + Send + Sync,
    N: NetworkHandler + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            wal: Arc::clone(&self.wal),
            object_storage: Arc::clone(&self.object_storage),
            cache: Arc::clone(&self.cache),
            replication_manager: Arc::clone(&self.replication_manager),
            network_handler: Arc::clone(&self.network_handler),
            partitions: Arc::clone(&self.partitions),
            broker_id: self.broker_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Mock implementations for testing
    struct MockWal {
        records: Arc<RwLock<Vec<WalRecord>>>,
    }

    struct MockObjectStorage;
    struct MockCache;
    struct MockReplicationManager;
    struct MockNetworkHandler;

    #[async_trait]
    impl WriteAheadLog for MockWal {
        async fn append(&self, record: WalRecord) -> Result<u64> {
            let mut stored_records = self.records.write().await;
            let offset = stored_records.len() as u64;
            stored_records.push(record);
            Ok(offset)
        }

        async fn read(&self, offset: u64, max_bytes: usize) -> Result<Vec<WalRecord>> {
            let stored_records = self.records.read().await;
            let matching_records: Vec<WalRecord> = stored_records
                .iter()
                .skip(offset as usize)
                .take(max_bytes / 1024)
                .cloned()
                .collect();
            Ok(matching_records)
        }

        async fn read_range(&self, start_offset: u64, end_offset: u64) -> Result<Vec<WalRecord>> {
            let stored_records = self.records.read().await;
            let matching_records: Vec<WalRecord> = stored_records
                .iter()
                .skip(start_offset as usize)
                .take((end_offset - start_offset) as usize)
                .cloned()
                .collect();
            Ok(matching_records)
        }

        async fn sync(&self) -> Result<()> {
            Ok(())
        }

        async fn truncate(&self, offset: u64) -> Result<()> {
            let mut stored_records = self.records.write().await;
            stored_records.truncate(offset as usize);
            Ok(())
        }

        async fn get_end_offset(&self) -> Result<u64> {
            let stored_records = self.records.read().await;
            Ok(stored_records.len() as u64)
        }

        fn register_upload_callback(&self, _callback: Box<dyn Fn(u64, u64) + Send + Sync>) {
            // No-op for mock
        }
    }

    #[async_trait]
    impl ObjectStorage for MockObjectStorage {
        async fn put(&self, key: &str, data: bytes::Bytes) -> Result<()> {
            Ok(())
        }

        async fn get(&self, key: &str) -> Result<bytes::Bytes> {
            Ok(bytes::Bytes::new())
        }

        async fn get_range(&self, key: &str, range: std::ops::Range<u64>) -> Result<bytes::Bytes> {
            Ok(bytes::Bytes::new())
        }

        async fn delete(&self, key: &str) -> Result<()> {
            Ok(())
        }

        async fn list(&self, prefix: &str) -> Result<Vec<String>> {
            Ok(vec![])
        }

        async fn exists(&self, key: &str) -> Result<bool> {
            Ok(false)
        }

        async fn open_read_stream(&self, key: &str) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
            Err(RustMqError::NotFound("mock".to_string()))
        }

        async fn open_write_stream(&self, key: &str) -> Result<Box<dyn tokio::io::AsyncWrite + Send + Unpin>> {
            Err(RustMqError::NotFound("mock".to_string()))
        }
    }

    #[async_trait]
    impl Cache for MockCache {
        async fn get(&self, key: &str) -> Result<Option<bytes::Bytes>> {
            Ok(None)
        }

        async fn put(&self, key: &str, value: bytes::Bytes) -> Result<()> {
            Ok(())
        }

        async fn remove(&self, key: &str) -> Result<()> {
            Ok(())
        }

        async fn clear(&self) -> Result<()> {
            Ok(())
        }

        async fn size(&self) -> Result<usize> {
            Ok(0)
        }
    }

    #[async_trait]
    impl ReplicationManager for MockReplicationManager {
        async fn replicate_record(&self, record: WalRecord) -> Result<ReplicationResult> {
            Ok(ReplicationResult {
                offset: record.offset,
                durability: DurabilityLevel::Durable,
            })
        }

        async fn add_follower(&self, broker_id: BrokerId) -> Result<()> {
            Ok(())
        }

        async fn remove_follower(&self, broker_id: BrokerId) -> Result<()> {
            Ok(())
        }

        async fn get_follower_states(&self) -> Result<Vec<FollowerState>> {
            Ok(vec![])
        }

        async fn update_high_watermark(&self, offset: Offset) -> Result<()> {
            Ok(())
        }

        async fn get_high_watermark(&self) -> Result<Offset> {
            Ok(0)
        }
    }

    #[async_trait]
    impl NetworkHandler for MockNetworkHandler {
        async fn send_request(&self, _broker_id: &BrokerId, _request: Vec<u8>) -> Result<Vec<u8>> {
            Ok(vec![])
        }

        async fn broadcast(&self, _brokers: &[BrokerId], _request: Vec<u8>) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_producer_send() {
        let wal = Arc::new(MockWal {
            records: Arc::new(RwLock::new(Vec::new())),
        });
        let object_storage = Arc::new(MockObjectStorage);
        let cache = Arc::new(MockCache);
        let replication_manager = Arc::new(MockReplicationManager);
        let network_handler = Arc::new(MockNetworkHandler);

        let core = MessageBrokerCore::new(
            wal,
            object_storage,
            cache,
            replication_manager,
            network_handler,
            "broker-1".to_string(),
        );

        // Add a partition
        let topic_partition = TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };
        let metadata = PartitionMetadata {
            leader_epoch: 1,
            high_watermark: 0,
            is_leader: true,
            replicas: vec!["broker-1".to_string()],
            in_sync_replicas: vec!["broker-1".to_string()],
        };
        core.add_partition(topic_partition.clone(), metadata).await.unwrap();

        let producer = core.create_producer();

        let record = ProduceRecord {
            topic: "test-topic".to_string(),
            partition: Some(0),
            key: Some(b"key1".to_vec()),
            value: b"Hello, World!".to_vec(),
            headers: vec![],
            acks: AcknowledgmentLevel::Leader,
            timeout_ms: 5000,
        };

        let result = producer.send(record).await.unwrap();
        assert_eq!(result.topic_partition, topic_partition);
        assert_eq!(result.offset, 0);
    }

    #[tokio::test]
    async fn test_consumer_poll() {
        let wal = Arc::new(MockWal {
            records: Arc::new(RwLock::new(Vec::new())),
        });
        let object_storage = Arc::new(MockObjectStorage);
        let cache = Arc::new(MockCache);
        let replication_manager = Arc::new(MockReplicationManager);
        let network_handler = Arc::new(MockNetworkHandler);

        let core = MessageBrokerCore::new(
            wal,
            object_storage,
            cache,
            replication_manager,
            network_handler,
            "broker-1".to_string(),
        );

        let mut consumer = core.create_consumer("test-group".to_string());
        consumer.subscribe(vec!["test-topic".to_string()]).await.unwrap();

        // Poll should return empty when no records
        let records = consumer.poll(1000).await.unwrap();
        assert!(records.is_empty());
    }
}