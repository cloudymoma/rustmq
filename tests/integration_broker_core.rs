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

// Mock upload manager: tiering is not exercised by these tests.
struct NoUpload;

#[async_trait]
impl UploadManager for NoUpload {
    async fn upload_segment(&self, _segment: WalSegment) -> Result<String> {
        unreachable!("tiering not exercised in these tests")
    }
    async fn download_segment(&self, _key: &str) -> Result<WalSegment> {
        unreachable!("tiering not exercised in these tests")
    }
    async fn verify_upload(&self, _key: &str, _expected: &[u8]) -> Result<bool> {
        Ok(true)
    }
}

/// Build a real [`PartitionStore`] backed by a segmented WAL in `dir`.
async fn make_store(dir: &std::path::Path) -> Arc<rustmq::storage::PartitionStore> {
    let cfg = rustmq::config::WalConfig {
        path: dir.to_path_buf(),
        capacity_bytes: 1 << 20,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };
    let wal: Arc<dyn rustmq::storage::SegmentedLog> =
        Arc::new(rustmq::storage::SegmentedWal::new(cfg).await.unwrap());
    let upload: Arc<dyn UploadManager> = Arc::new(NoUpload);
    let cache: Arc<dyn Cache> = Arc::new(rustmq::storage::LruCache::new(1 << 20));
    let cold_index = Arc::new(
        rustmq::storage::ColdIndexManifest::open(dir.join("cold.manifest"))
            .await
            .unwrap(),
    );
    rustmq::storage::PartitionStore::new(wal, upload, cache, 0, cold_index)
        .await
        .unwrap()
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

/// Returns the broker core plus the [`TempDir`] backing its WAL. The directory
/// must be kept alive for the lifetime of the store, so callers bind it.
async fn create_test_broker() -> (
    Arc<MessageBrokerCore<MockReplicationManager, MockNetworkHandler>>,
    tempfile::TempDir,
) {
    let dir = tempfile::TempDir::new().unwrap();
    let store = make_store(dir.path()).await;
    let replication_manager = Arc::new(MockReplicationManager::new());
    let network_handler = Arc::new(MockNetworkHandler);

    let core = Arc::new(MessageBrokerCore::new(
        store,
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

    (core, dir)
}

#[tokio::test]
async fn test_end_to_end_produce_consume_workflow() {
    let (core, _dir) = create_test_broker().await;
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
    let (core, _dir) = create_test_broker().await;
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
    let (core, _dir) = create_test_broker().await;
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
    let (core, _dir) = create_test_broker().await;
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
    let (core, _dir) = create_test_broker().await;
    let producer = core.create_producer();

    // Flush should succeed
    producer.flush().await.unwrap();
}

#[tokio::test]
async fn test_multiple_consumers_same_group() {
    let (core, _dir) = create_test_broker().await;
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
    let (core, _dir) = create_test_broker().await;
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
    let (core, _dir) = create_test_broker().await;
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
    let (core, _dir) = create_test_broker().await;

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
