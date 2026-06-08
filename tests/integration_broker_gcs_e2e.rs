use rustmq::broker::core::*;
use rustmq::config::*;
use rustmq::network::traits::NetworkHandler;
use rustmq::replication::traits::ReplicationManager;
use rustmq::storage::cache::LruCache;
use rustmq::storage::traits::*;
use rustmq::storage::{PartitionStore, SegmentedLog, SegmentedWal};
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

// Upload manager placeholder: the seal->upload->reclaim (tiering) loop is a later
// phase, so it is not exercised here. download_segment must not be reached by the
// hot read path this test verifies.
struct NoUpload;
#[async_trait::async_trait]
impl UploadManager for NoUpload {
    async fn upload_segment(&self, _segment: WalSegment) -> rustmq::error::Result<String> {
        unreachable!("tiering not exercised in this test")
    }
    async fn download_segment(&self, _object_key: &str) -> rustmq::error::Result<WalSegment> {
        unreachable!("tiering not exercised in this test")
    }
    async fn verify_upload(
        &self,
        _object_key: &str,
        _expected: &[u8],
    ) -> rustmq::error::Result<bool> {
        Ok(true)
    }
}

type TestCore = MessageBrokerCore<MockReplication, MockNetwork>;

async fn make_store(dir: &std::path::Path) -> Arc<PartitionStore> {
    let wal_config = WalConfig {
        path: dir.join("wal"),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };
    let wal: Arc<dyn SegmentedLog> = Arc::new(SegmentedWal::new(wal_config).await.unwrap());
    let upload: Arc<dyn UploadManager> = Arc::new(NoUpload);
    let cache: Arc<dyn Cache> = Arc::new(LruCache::new(1024 * 1024));
    let cold_index = Arc::new(
        rustmq::storage::ColdIndexManifest::open(dir.join("cold.manifest"))
            .await
            .unwrap(),
    );
    PartitionStore::new(wal, upload, cache, 0, cold_index)
        .await
        .unwrap()
}

/// End-to-end produce -> fetch over a real `PartitionStore`.
///
/// After the storage refactor, `MessageBrokerCore` reads and writes exclusively
/// through `PartitionStore`; the object-storage backend is no longer wired into the
/// broker directly. This test verifies the produce + partition-correct fetch path.
#[tokio::test]
async fn test_broker_gcs_e2e_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let store = make_store(temp_dir.path()).await;

    let core = Arc::new(TestCore::new(
        store,
        Arc::new(MockReplication),
        Arc::new(MockNetwork),
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

    // Produce a record.
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

    // Fetch it back through the partition-correct read path.
    let fetched = core.fetch_records(&tp, offset, 1024).await.unwrap();
    assert_eq!(fetched.len(), 1);
    assert_eq!(fetched[0].value, b"value".to_vec());
}
