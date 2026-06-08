use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{Result, RustMqError};
use crate::network::traits::NetworkHandler;
use crate::replication::traits::ReplicationManager;
use crate::storage::{PartitionStore, RecordLog};
use crate::types::*;

/// High-level message broker core that orchestrates produce/consume operations.
///
/// All durability and reads go through a single [`PartitionStore`], so produce and
/// fetch are partition-correct over the shared WAL. Replication and network handling
/// remain generic collaborators.
pub struct MessageBrokerCore<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    store: Arc<PartitionStore>,
    replication_manager: Arc<R>,
    #[allow(dead_code)]
    network_handler: Arc<N>,
    partitions: Arc<RwLock<HashMap<TopicPartition, PartitionMetadata>>>,
    broker_id: BrokerId,
    total_messages_processed: Arc<std::sync::atomic::AtomicU64>,
    pub group_coordinator: Arc<crate::consumer_group::coordinator::GroupCoordinatorManager>,
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
    pub headers: Headers, // SmallVec<[Header; 4]> - avoids heap alloc
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
    pub key: Option<Bytes>, // Changed from Vec<u8> for zero-copy
    pub value: Bytes,       // Changed from Vec<u8> for zero-copy
    pub headers: Headers,   // SmallVec<[Header; 4]> - avoids heap alloc
    pub timestamp: i64,
}

impl ConsumeRecord {
    /// Get key as Vec<u8> (copies data) - for backward compatibility
    pub fn key_as_vec(&self) -> Option<Vec<u8>> {
        self.key.as_ref().map(|k| k.to_vec())
    }

    /// Get value as Vec<u8> (copies data) - for backward compatibility
    pub fn value_as_vec(&self) -> Vec<u8> {
        self.value.to_vec()
    }
}

impl<R, N> MessageBrokerCore<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    pub fn new(
        store: Arc<PartitionStore>,
        replication_manager: Arc<R>,
        network_handler: Arc<N>,
        broker_id: BrokerId,
    ) -> Self {
        let record_log: Arc<dyn RecordLog> = store.clone();
        let group_coordinator = Arc::new(
            crate::consumer_group::coordinator::GroupCoordinatorManager::new(
                record_log,
                crate::consumer_group::CONSUMER_OFFSETS_PARTITIONS,
            ),
        );

        Self {
            store,
            replication_manager,
            network_handler,
            partitions: Arc::new(RwLock::new(HashMap::new())),
            broker_id,
            total_messages_processed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            group_coordinator,
        }
    }

    /// Create a new producer instance
    pub fn create_producer(self: &Arc<Self>) -> MessageProducer<R, N> {
        MessageProducer::new(Arc::clone(self))
    }

    /// Create a new consumer instance
    pub fn create_consumer(self: &Arc<Self>, consumer_group: String) -> MessageConsumer<R, N> {
        MessageConsumer::new(Arc::clone(self), consumer_group)
    }

    /// Add a partition to this broker. The high watermark is advanced to any
    /// offset already recovered for this partition so produce never reuses offsets.
    pub async fn add_partition(
        &self,
        topic_partition: TopicPartition,
        mut metadata: PartitionMetadata,
    ) -> Result<()> {
        let recovered = self.store.next_offset(&topic_partition);
        metadata.high_watermark = metadata.high_watermark.max(recovered);
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

    pub fn get_total_messages_processed(&self) -> u64 {
        self.total_messages_processed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn get_high_watermarks(&self) -> Vec<(TopicPartition, u64)> {
        let mut results = Vec::new();
        let partitions = self.partitions.read().await;
        for (tp, meta) in partitions.iter() {
            if meta.is_leader {
                results.push((tp.clone(), meta.high_watermark));
            }
        }
        results
    }

    pub fn broker_id(&self) -> &BrokerId {
        &self.broker_id
    }

    pub async fn get_partitions_for_topic(&self, topic: &str) -> Vec<TopicPartition> {
        let partitions = self.partitions.read().await;
        partitions
            .keys()
            .filter(|tp| tp.topic == topic)
            .cloned()
            .collect()
    }

    pub async fn get_partition_counts(&self) -> (usize, usize) {
        let partitions = self.partitions.read().await;
        let total = partitions.len();
        let leader_count = partitions.values().filter(|p| p.is_leader).count();
        (total, leader_count)
    }

    pub async fn get_partition_metadata(
        &self,
        topic_partition: &TopicPartition,
    ) -> Result<Option<PartitionMetadata>> {
        let partitions = self.partitions.read().await;
        Ok(partitions.get(topic_partition).cloned())
    }

    /// Get reference to the partition store (for shutdown / metrics / tiering wiring).
    pub fn get_store(&self) -> &Arc<PartitionStore> {
        &self.store
    }

    /// Get reference to replication manager for shutdown operations
    pub fn get_replication_manager(&self) -> &Arc<R> {
        &self.replication_manager
    }

    /// Internal method to append records to the store and replicate
    async fn append_records(
        &self,
        topic_partition: &TopicPartition,
        records: Vec<Record>,
        acks: AcknowledgmentLevel,
    ) -> Result<Vec<Offset>> {
        let metadata = self
            .get_partition_metadata(topic_partition)
            .await?
            .ok_or_else(|| RustMqError::PartitionNotFound(topic_partition.to_string()))?;

        if !metadata.is_leader {
            return Err(RustMqError::NotLeader(topic_partition.to_string()));
        }

        // Assign per-partition offsets and frame records.
        let mut wal_records = Vec::new();
        let mut offsets = Vec::new();
        let base_offset = metadata.high_watermark;

        for (i, record) in records.into_iter().enumerate() {
            let offset = base_offset + i as u64;
            wal_records.push(WalRecord {
                topic_partition: topic_partition.clone(),
                offset,
                record,
                crc32: 0,
            });
            offsets.push(offset);
        }

        // Append+index every record through the store.
        for record in &wal_records {
            self.store.append(record).await?;
        }

        // Replicate based on acknowledgment level.
        match acks {
            AcknowledgmentLevel::None | AcknowledgmentLevel::Leader => {}
            AcknowledgmentLevel::Majority
            | AcknowledgmentLevel::All
            | AcknowledgmentLevel::Custom(_) => {
                for record in &wal_records {
                    self.replication_manager.replicate_record(record).await?;
                }
            }
        }

        // Update high watermark.
        let mut partitions = self.partitions.write().await;
        if let Some(metadata) = partitions.get_mut(topic_partition) {
            metadata.high_watermark = base_offset + wal_records.len() as u64;
        }

        self.total_messages_processed.fetch_add(
            wal_records.len() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        Ok(offsets)
    }

    /// Fetch records for a partition; resolution (cache → hot WAL → cold object) and
    /// partition scoping are handled by the store.
    pub async fn fetch_records(
        &self,
        topic_partition: &TopicPartition,
        fetch_offset: Offset,
        max_bytes: u32,
    ) -> Result<Vec<Record>> {
        let wal_records = self
            .store
            .read(topic_partition, fetch_offset, max_bytes as usize)
            .await?;
        let records: Vec<Record> = wal_records.into_iter().map(|wr| wr.record).collect();
        self.total_messages_processed
            .fetch_add(records.len() as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(records)
    }
}

/// High-level producer implementation
pub struct MessageProducer<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    core: Arc<MessageBrokerCore<R, N>>,
}

impl<R, N> MessageProducer<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    fn new(core: Arc<MessageBrokerCore<R, N>>) -> Self {
        Self { core }
    }
}

#[async_trait]
impl<R, N> Producer for MessageProducer<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    async fn send(&self, record: ProduceRecord) -> Result<ProduceResult> {
        let partition_id = record.partition.unwrap_or(0);
        let topic_partition = TopicPartition {
            topic: record.topic.clone(),
            partition: partition_id,
        };

        let wal_record = Record::with_headers(
            record.key.map(Bytes::from),
            Bytes::from(record.value),
            record.headers,
            chrono::Utc::now().timestamp_millis(),
        );

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

        let mut grouped_records: HashMap<TopicPartition, (Vec<Record>, AcknowledgmentLevel)> =
            HashMap::new();

        for record in records {
            let partition_id = record.partition.unwrap_or(0);
            let topic_partition = TopicPartition {
                topic: record.topic.clone(),
                partition: partition_id,
            };

            let wal_record = Record::with_headers(
                record.key.map(Bytes::from),
                Bytes::from(record.value),
                record.headers,
                chrono::Utc::now().timestamp_millis(),
            );

            grouped_records
                .entry(topic_partition)
                .or_insert((Vec::new(), record.acks))
                .0
                .push(wal_record);
        }

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
        self.core.store.sync().await
    }
}

/// High-level consumer implementation
pub struct MessageConsumer<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    core: Arc<MessageBrokerCore<R, N>>,
    #[allow(dead_code)]
    consumer_group: String,
    subscribed_topics: Vec<TopicName>,
    partition_offsets: HashMap<TopicPartition, Offset>,
}

impl<R, N> MessageConsumer<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    fn new(core: Arc<MessageBrokerCore<R, N>>, consumer_group: String) -> Self {
        Self {
            core,
            consumer_group,
            subscribed_topics: Vec::new(),
            partition_offsets: HashMap::new(),
        }
    }
}

#[async_trait]
impl<R, N> Consumer for MessageConsumer<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    async fn subscribe(&mut self, topics: Vec<TopicName>) -> Result<()> {
        self.subscribed_topics = topics;
        for topic in &self.subscribed_topics {
            let topic_partition = TopicPartition {
                topic: topic.clone(),
                partition: 0,
            };
            self.partition_offsets.insert(topic_partition, 0);
        }
        Ok(())
    }

    async fn poll(&mut self, _timeout_ms: u32) -> Result<Vec<ConsumeRecord>> {
        let mut records = Vec::new();

        for (topic_partition, current_offset) in &mut self.partition_offsets {
            match self
                .core
                .fetch_records(topic_partition, *current_offset, 1024 * 1024)
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
                Err(RustMqError::OffsetOutOfRange(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Ok(records)
    }

    async fn commit_offsets(&mut self, offsets: HashMap<TopicPartition, Offset>) -> Result<()> {
        for (topic_partition, offset) in offsets {
            self.partition_offsets.insert(topic_partition, offset);
        }
        Ok(())
    }

    async fn seek(&mut self, topic_partition: TopicPartition, offset: Offset) -> Result<()> {
        self.partition_offsets.insert(topic_partition, offset);
        Ok(())
    }
}

impl<R, N> Clone for MessageBrokerCore<R, N>
where
    R: ReplicationManager + Send + Sync + ?Sized,
    N: NetworkHandler + Send + Sync + ?Sized,
{
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            replication_manager: Arc::clone(&self.replication_manager),
            network_handler: Arc::clone(&self.network_handler),
            partitions: Arc::clone(&self.partitions),
            broker_id: self.broker_id.clone(),
            total_messages_processed: Arc::clone(&self.total_messages_processed),
            group_coordinator: Arc::clone(&self.group_coordinator),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WalConfig;
    use crate::storage::cache::LruCache;
    use crate::storage::traits::{Cache, UploadManager, WalSegment};
    use crate::storage::wal::{SegmentedLog, SegmentedWal};
    use tempfile::TempDir;

    struct MockReplicationManager;
    struct MockNetworkHandler;

    // Not exercised by these tests (no tiering).
    struct UnusedUploadManager;
    #[async_trait]
    impl UploadManager for UnusedUploadManager {
        async fn upload_segment(&self, _segment: WalSegment) -> Result<String> {
            panic!("upload not expected");
        }
        async fn download_segment(&self, _object_key: &str) -> Result<WalSegment> {
            panic!("download not expected");
        }
        async fn verify_upload(&self, _object_key: &str, _expected: &[u8]) -> Result<bool> {
            Ok(true)
        }
    }

    #[async_trait]
    impl ReplicationManager for MockReplicationManager {
        async fn replicate_record(&self, record: &WalRecord) -> Result<ReplicationResult> {
            Ok(ReplicationResult {
                offset: record.offset,
                durability: DurabilityLevel::Durable,
            })
        }
        async fn add_follower(&self, _broker_id: BrokerId) -> Result<()> {
            Ok(())
        }
        async fn remove_follower(&self, _broker_id: BrokerId) -> Result<()> {
            Ok(())
        }
        async fn get_follower_states(&self) -> Result<Vec<FollowerState>> {
            Ok(vec![])
        }
        async fn update_high_watermark(&self, _offset: Offset) -> Result<()> {
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

    async fn make_core(
        dir: &std::path::Path,
    ) -> Arc<MessageBrokerCore<MockReplicationManager, MockNetworkHandler>> {
        let config = WalConfig {
            path: dir.to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };
        let wal: Arc<dyn SegmentedLog> = Arc::new(SegmentedWal::new(config).await.unwrap());
        let upload: Arc<dyn UploadManager> = Arc::new(UnusedUploadManager);
        let cache: Arc<dyn Cache> = Arc::new(LruCache::new(1024 * 1024));
        let cold_index = Arc::new(
            crate::storage::ColdIndexManifest::open(dir.join("cold.manifest"))
                .await
                .unwrap(),
        );
        let store = PartitionStore::new(wal, upload, cache, 0, cold_index)
            .await
            .unwrap();
        Arc::new(MessageBrokerCore::new(
            store,
            Arc::new(MockReplicationManager),
            Arc::new(MockNetworkHandler),
            "broker-1".to_string(),
        ))
    }

    #[tokio::test]
    async fn test_producer_send() {
        let dir = TempDir::new().unwrap();
        let core = make_core(dir.path()).await;

        let topic_partition = TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };
        core.add_partition(
            topic_partition.clone(),
            PartitionMetadata {
                leader_epoch: 1,
                high_watermark: 0,
                is_leader: true,
                replicas: vec!["broker-1".to_string()],
                in_sync_replicas: vec!["broker-1".to_string()],
            },
        )
        .await
        .unwrap();

        let producer = core.create_producer();
        let record = ProduceRecord {
            topic: "test-topic".to_string(),
            partition: Some(0),
            key: Some(b"key1".to_vec()),
            value: b"Hello, World!".to_vec(),
            headers: smallvec::SmallVec::new(),
            acks: AcknowledgmentLevel::Leader,
            timeout_ms: 5000,
        };

        let result = producer.send(record).await.unwrap();
        assert_eq!(result.topic_partition, topic_partition);
        assert_eq!(result.offset, 0);

        // Round-trip: the produced record reads back from its own partition.
        let fetched = core
            .fetch_records(&topic_partition, 0, 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].value.as_ref(), b"Hello, World!");
    }

    #[tokio::test]
    async fn test_consumer_poll_empty() {
        let dir = TempDir::new().unwrap();
        let core = make_core(dir.path()).await;
        let mut consumer = core.create_consumer("test-group".to_string());
        consumer
            .subscribe(vec!["test-topic".to_string()])
            .await
            .unwrap();
        let records = consumer.poll(1000).await.unwrap();
        assert!(records.is_empty());
    }
}
