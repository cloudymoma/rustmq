mod common;

use bytes::Bytes;
use common::gcs::GcsTestConfig;
use object_store::ObjectStore;
use object_store::memory::InMemory;
use rustmq::broker::core::*;
use rustmq::config::*;
use rustmq::network::traits::NetworkHandler;
use rustmq::replication::traits::ReplicationManager;
use rustmq::storage::cache::LruCache;
use rustmq::storage::cloud_storage::CloudObjectStorage;
use rustmq::storage::traits::*;
use rustmq::storage::wal::DirectIOWal;
use rustmq::storage::{AlignedBufferPool, WalSegment};
use rustmq::types::*;
use std::sync::Arc;
use tempfile::TempDir;

// Mock Replication Manager
struct MockReplication;
#[async_trait::async_trait]
impl ReplicationManager for MockReplication {
    async fn replicate_record(
        &self,
        record: &WalRecord,
    ) -> rustmq::error::Result<ReplicationResult> {
        Ok(ReplicationResult {
            offset: record.offset,
            durability: DurabilityLevel::Durable,
        })
    }
    async fn add_follower(&self, _: BrokerId) -> rustmq::error::Result<()> {
        Ok(())
    }
    async fn remove_follower(&self, _: BrokerId) -> rustmq::error::Result<()> {
        Ok(())
    }
    async fn get_follower_states(&self) -> rustmq::error::Result<Vec<FollowerState>> {
        Ok(vec![])
    }
    async fn update_high_watermark(&self, _: Offset) -> rustmq::error::Result<()> {
        Ok(())
    }
    async fn get_high_watermark(&self) -> rustmq::error::Result<Offset> {
        Ok(0)
    }
}

// Mock Network Handler
struct MockNetwork;
#[async_trait::async_trait]
impl NetworkHandler for MockNetwork {
    async fn send_request(&self, _: &BrokerId, _: Vec<u8>) -> rustmq::error::Result<Vec<u8>> {
        Ok(vec![])
    }
    async fn broadcast(&self, _: &[BrokerId], _: Vec<u8>) -> rustmq::error::Result<()> {
        Ok(())
    }
}

type TestCore =
    MessageBrokerCore<DirectIOWal, CloudObjectStorage, LruCache, MockReplication, MockNetwork>;

#[tokio::test]
async fn test_broker_gcs_e2e_lifecycle() {
    let temp_dir = TempDir::new().unwrap();

    // 1. Setup Storage (Fallback to InMemory if no GCS config)
    let (store, bucket) = match GcsTestConfig::load() {
        Some(config) => {
            let mut builder = object_store::gcp::GoogleCloudStorageBuilder::from_env()
                .with_bucket_name(&config.bucket_name);
            if let Some(creds) = config.credentials_path.as_deref().filter(|s| !s.is_empty()) {
                builder = builder.with_service_account_path(creds);
            }
            match builder.build() {
                Ok(gcs) => (Arc::new(gcs) as Arc<dyn ObjectStore>, config.bucket_name),
                Err(_) => (
                    Arc::new(InMemory::new()) as Arc<dyn ObjectStore>,
                    "test".to_string(),
                ),
            }
        }
        None => (
            Arc::new(InMemory::new()) as Arc<dyn ObjectStore>,
            "test".to_string(),
        ),
    };

    let object_storage = Arc::new(CloudObjectStorage::new(store.clone()));

    // 2. Initialize Components
    let wal_config = WalConfig {
        path: temp_dir.path().join("wal"),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };
    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let cache = Arc::new(LruCache::new(1024 * 1024));
    let replication = Arc::new(MockReplication);
    let network = Arc::new(MockNetwork);

    let core = Arc::new(TestCore::new(
        wal.clone(),
        object_storage.clone(),
        cache.clone(),
        replication,
        network,
        "broker-1".to_string(),
    ));

    let tp = TopicPartition {
        topic: "e2e-topic".to_string(),
        partition: 0,
    };
    core.add_partition(
        tp.clone(),
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

    // 3. Produce Record
    let producer = core.create_producer();
    let record = ProduceRecord {
        topic: tp.topic.clone(),
        partition: Some(tp.partition),
        key: Some(b"key".to_vec()),
        value: b"value".to_vec(),
        headers: rustmq::types::Headers::new(),
        acks: AcknowledgmentLevel::Leader,
        timeout_ms: 5000,
    };
    let result = producer.send(record).await.unwrap();
    let offset = result.offset;

    // 4. Manually trigger "Tiered Storage" transition
    // We simulate what the background uploader would do.
    let wal_record = WalRecord {
        topic_partition: tp.clone(),
        offset,
        record: Record::new(Some(b"key".to_vec()), b"value".to_vec(), vec![], 0),
        crc32: 0,
    };
    let data = bincode::serialize(&vec![wal_record]).unwrap();
    let compressed = lz4_flex::compress_prepend_size(&data);

    // Use the exact key format expected by fetch_records
    let object_key = format!("{}/{}", tp, offset);
    object_storage
        .put(&object_key, Bytes::from(compressed))
        .await
        .unwrap();

    // 5. Clear Local State
    // Truncate WAL to force ObjectStorage read
    wal.truncate(0).await.unwrap();
    // Clear cache
    cache.clear().await.unwrap();

    // 6. Fetch Record (Should hit Object Storage / GCS)
    let fetched = core.fetch_records(&tp, offset, 1024).await.unwrap();
    assert_eq!(fetched.len(), 1);
    assert_eq!(fetched[0].value, b"value".to_vec());

    // 7. Verify Cache was warmed
    let cached = cache.get(&format!("{}:{}", tp, offset)).await.unwrap();
    assert!(cached.is_some());
}
