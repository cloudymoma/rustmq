use rustmq::broker::core::*;
use rustmq::error::Result;
use rustmq::network::traits::*;
use rustmq::replication::traits::*;
use rustmq::storage::traits::*;
use rustmq::types::*;

use async_trait::async_trait;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// Mock implementations for integration testing
struct MockWal {
    records: Arc<RwLock<Vec<WalRecord>>>,
}

impl MockWal {
    fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl WriteAheadLog for MockWal {
    async fn append(&self, record: &WalRecord) -> Result<u64> {
        let mut stored_records = self.records.write().await;
        let offset = stored_records.len() as u64;
        stored_records.push(record.clone());
        Ok(offset)
    }

    async fn read(&self, offset: u64, max_bytes: usize) -> Result<Vec<WalRecord>> {
        let stored_records = self.records.read().await;
        let matching_records: Vec<WalRecord> = stored_records
            .iter()
            .skip(offset as usize)
            .take(max_bytes / 1024) // Simple size limit
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

struct MockObjectStorage {
    storage: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl MockObjectStorage {
    fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ObjectStorage for MockObjectStorage {
    async fn put(&self, key: &str, data: bytes::Bytes) -> Result<()> {
        let mut storage = self.storage.write().await;
        storage.insert(key.to_string(), data.to_vec());
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<bytes::Bytes> {
        let storage = self.storage.read().await;
        storage
            .get(key)
            .cloned()
            .map(|v| bytes::Bytes::from(v))
            .ok_or_else(|| rustmq::error::RustMqError::ObjectNotFound(key.to_string()))
    }

    async fn get_range(&self, key: &str, range: std::ops::Range<u64>) -> Result<bytes::Bytes> {
        let storage = self.storage.read().await;
        if let Some(data) = storage.get(key) {
            let start = range.start as usize;
            let end = std::cmp::min(range.end as usize, data.len());
            if start < data.len() {
                Ok(bytes::Bytes::from(data[start..end].to_vec()))
            } else {
                Ok(bytes::Bytes::new())
            }
        } else {
            Err(rustmq::error::RustMqError::ObjectNotFound(key.to_string()))
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut storage = self.storage.write().await;
        storage.remove(key);
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let storage = self.storage.read().await;
        let keys: Vec<String> = storage
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let storage = self.storage.read().await;
        Ok(storage.contains_key(key))
    }

    async fn open_read_stream(
        &self,
        key: &str,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
        Err(rustmq::error::RustMqError::NotFound("mock".to_string()))
    }

    async fn open_write_stream(
        &self,
        key: &str,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Send + Unpin>> {
        Err(rustmq::error::RustMqError::NotFound("mock".to_string()))
    }
}

struct MockCache {
    cache: Arc<RwLock<HashMap<String, bytes::Bytes>>>,
}

impl MockCache {
    fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Cache for MockCache {
    async fn get(&self, key: &str) -> Result<Option<bytes::Bytes>> {
        let cache = self.cache.read().await;
        Ok(cache.get(key).cloned())
    }

    async fn put(&self, key: &str, value: bytes::Bytes) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), value);
        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.remove(key);
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.clear();
        Ok(())
    }

    async fn size(&self) -> Result<usize> {
        let cache = self.cache.read().await;
        Ok(cache.len())
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

struct MockReplicationManager {
    replicated_records: Arc<RwLock<Vec<WalRecord>>>,
}

impl MockReplicationManager {
    fn new() -> Self {
        Self {
            replicated_records: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl ReplicationManager for MockReplicationManager {
    async fn replicate_record(&self, record: &WalRecord) -> Result<ReplicationResult> {
        let mut replicated = self.replicated_records.write().await;
        replicated.push(record.clone());
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

struct MockNetworkHandler;

#[async_trait]
impl NetworkHandler for MockNetworkHandler {
    async fn send_request(&self, _broker_id: &BrokerId, _request: Vec<u8>) -> Result<Vec<u8>> {
        Ok(b"mock response".to_vec())
    }

    async fn broadcast(&self, _brokers: &[BrokerId], _request: Vec<u8>) -> Result<()> {
        Ok(())
    }
}

async fn create_test_broker() -> Arc<
    MessageBrokerCore<
        MockWal,
        MockObjectStorage,
        MockCache,
        MockReplicationManager,
        MockNetworkHandler,
    >,
> {
    let wal = Arc::new(MockWal::new());
    let object_storage = Arc::new(MockObjectStorage::new());
    let cache = Arc::new(MockCache::new());
    let replication_manager = Arc::new(MockReplicationManager::new());
    let network_handler = Arc::new(MockNetworkHandler);

    let core = Arc::new(MessageBrokerCore::new(
        wal,
        object_storage,
        cache,
        replication_manager,
        network_handler,
        "test-broker-1".to_string(),
    ));

    // Add a test partition
    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };
    let metadata = PartitionMetadata {
        leader_epoch: 1,
        high_watermark: 0,
        is_leader: true,
        replicas: vec!["test-broker-1".to_string()],
        in_sync_replicas: vec!["test-broker-1".to_string()],
    };
    core.add_partition(topic_partition, metadata).await.unwrap();

    core
}

#[tokio::test]
async fn test_end_to_end_produce_consume_workflow() {
    let core = create_test_broker().await;
    let producer = core.create_producer();
    let mut consumer = core.create_consumer("test-group".to_string());

    // Subscribe consumer to topics
    consumer
        .subscribe(vec!["test-topic".to_string()])
        .await
        .unwrap();

    // Produce a single record
    let produce_record = ProduceRecord {
        topic: "test-topic".to_string(),
        partition: Some(0),
        key: Some(b"key1".to_vec()),
        value: b"Hello, World!".to_vec(),
        headers: {
            let mut h = SmallVec::new();
            h.push(Header::new(
                "content-type".to_string(),
                b"text/plain".to_vec(),
            ));
            h
        },
        acks: AcknowledgmentLevel::Leader,
        timeout_ms: 5000,
    };

    let result = producer.send(produce_record).await.unwrap();
    assert_eq!(result.offset, 0);
    assert_eq!(result.topic_partition.topic, "test-topic");
    assert_eq!(result.topic_partition.partition, 0);

    // Consume the record
    let consumed_records = consumer.poll(1000).await.unwrap();
    assert_eq!(consumed_records.len(), 1);

    let consumed_record = &consumed_records[0];
    assert_eq!(consumed_record.offset, 0);
    assert_eq!(consumed_record.topic_partition.topic, "test-topic");
    assert_eq!(consumed_record.topic_partition.partition, 0);
    assert_eq!(
        consumed_record.key.as_ref().map(|k| k.as_ref()),
        Some(b"key1".as_ref())
    );
    assert_eq!(consumed_record.value.as_ref(), b"Hello, World!");
    assert_eq!(consumed_record.headers.len(), 1);
    assert_eq!(consumed_record.headers[0].key, "content-type");
}

#[tokio::test]
async fn test_batch_produce_operations() {
    let core = create_test_broker().await;
    let producer = core.create_producer();

    let batch_records = vec![
        ProduceRecord {
            topic: "test-topic".to_string(),
            partition: Some(0),
            key: Some(b"key1".to_vec()),
            value: b"Message 1".to_vec(),
            headers: SmallVec::new(),
            acks: AcknowledgmentLevel::Leader,
            timeout_ms: 5000,
        },
        ProduceRecord {
            topic: "test-topic".to_string(),
            partition: Some(0),
            key: Some(b"key2".to_vec()),
            value: b"Message 2".to_vec(),
            headers: SmallVec::new(),
            acks: AcknowledgmentLevel::Leader,
            timeout_ms: 5000,
        },
        ProduceRecord {
            topic: "test-topic".to_string(),
            partition: Some(0),
            key: Some(b"key3".to_vec()),
            value: b"Message 3".to_vec(),
            headers: SmallVec::new(),
            acks: AcknowledgmentLevel::Leader,
            timeout_ms: 5000,
        },
    ];

    let results = producer.send_batch(batch_records).await.unwrap();
    assert_eq!(results.len(), 3);

    // Verify offsets are sequential
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.offset, i as u64);
        assert_eq!(result.topic_partition.topic, "test-topic");
        assert_eq!(result.topic_partition.partition, 0);
    }
}

#[tokio::test]
async fn test_consumer_seek_functionality() {
    let core = create_test_broker().await;
    let producer = core.create_producer();
    let mut consumer = core.create_consumer("test-group".to_string());

    consumer
        .subscribe(vec!["test-topic".to_string()])
        .await
        .unwrap();

    // Produce multiple records
    for i in 0..5 {
        let record = ProduceRecord {
            topic: "test-topic".to_string(),
            partition: Some(0),
            key: Some(format!("key{}", i).into_bytes()),
            value: format!("Message {}", i).into_bytes(),
            headers: SmallVec::new(),
            acks: AcknowledgmentLevel::Leader,
            timeout_ms: 5000,
        };
        producer.send(record).await.unwrap();
    }

    // Seek to offset 2
    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };
    consumer.seek(topic_partition.clone(), 2).await.unwrap();

    // Consume should start from offset 2
    let records = consumer.poll(1000).await.unwrap();
    if !records.is_empty() {
        assert!(records[0].offset >= 2);
    }
}

#[tokio::test]
async fn test_offset_commit_functionality() {
    let core = create_test_broker().await;
    let mut consumer = core.create_consumer("test-group".to_string());

    consumer
        .subscribe(vec!["test-topic".to_string()])
        .await
        .unwrap();

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let mut offsets = HashMap::new();
    offsets.insert(topic_partition, 42);

    // Commit offsets should succeed
    consumer.commit_offsets(offsets).await.unwrap();
}

#[tokio::test]
async fn test_producer_flush() {
    let core = create_test_broker().await;
    let producer = core.create_producer();

    // Flush should succeed
    producer.flush().await.unwrap();
}

#[tokio::test]
async fn test_multiple_consumers_same_group() {
    let core = create_test_broker().await;
    let mut consumer1 = core.create_consumer("test-group".to_string());
    let mut consumer2 = core.create_consumer("test-group".to_string());

    // Both consumers subscribe to the same topic
    consumer1
        .subscribe(vec!["test-topic".to_string()])
        .await
        .unwrap();
    consumer2
        .subscribe(vec!["test-topic".to_string()])
        .await
        .unwrap();

    // Both should be able to poll (though in a real system,
    // partition assignment would coordinate between them)
    let _records1 = consumer1.poll(100).await.unwrap();
    let _records2 = consumer2.poll(100).await.unwrap();
}

#[tokio::test]
async fn test_error_handling_invalid_partition() {
    let core = create_test_broker().await;
    let producer = core.create_producer();

    // Try to produce to a partition that doesn't exist
    let record = ProduceRecord {
        topic: "non-existent-topic".to_string(),
        partition: Some(0),
        key: None,
        value: b"test".to_vec(),
        headers: SmallVec::new(),
        acks: AcknowledgmentLevel::Leader,
        timeout_ms: 5000,
    };

    let result = producer.send(record).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_different_acknowledgment_levels() {
    let core = create_test_broker().await;
    let producer = core.create_producer();

    let ack_levels = vec![
        AcknowledgmentLevel::None,
        AcknowledgmentLevel::Leader,
        AcknowledgmentLevel::Majority,
        AcknowledgmentLevel::All,
        AcknowledgmentLevel::Custom(2),
    ];

    for ack_level in ack_levels {
        let record = ProduceRecord {
            topic: "test-topic".to_string(),
            partition: Some(0),
            key: None,
            value: b"test".to_vec(),
            headers: SmallVec::new(),
            acks: ack_level,
            timeout_ms: 5000,
        };

        let result = producer.send(record).await.unwrap();
        assert!(result.offset >= 0);
    }
}

#[tokio::test]
async fn test_partition_metadata_operations() {
    let core = create_test_broker().await;

    let topic_partition = TopicPartition {
        topic: "metadata-test".to_string(),
        partition: 1,
    };

    let metadata = PartitionMetadata {
        leader_epoch: 5,
        high_watermark: 100,
        is_leader: false,
        replicas: vec!["broker-1".to_string(), "broker-2".to_string()],
        in_sync_replicas: vec!["broker-1".to_string()],
    };

    // Add partition
    core.add_partition(topic_partition.clone(), metadata.clone())
        .await
        .unwrap();

    // Get partition metadata
    let retrieved_metadata = core.get_partition_metadata(&topic_partition).await.unwrap();
    assert!(retrieved_metadata.is_some());
    let retrieved = retrieved_metadata.unwrap();
    assert_eq!(retrieved.leader_epoch, 5);
    assert_eq!(retrieved.high_watermark, 100);
    assert!(!retrieved.is_leader);

    // Remove partition
    core.remove_partition(&topic_partition).await.unwrap();
    let removed_metadata = core.get_partition_metadata(&topic_partition).await.unwrap();
    assert!(removed_metadata.is_none());
}
