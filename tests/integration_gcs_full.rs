mod common;

use bytes::Bytes;
use object_store::ObjectStore;
use object_store::memory::InMemory;
use rustmq::config::*;
use rustmq::error::RustMqError;
use rustmq::storage::cache::*;
use rustmq::storage::cloud_storage::CloudObjectStorage;
use rustmq::storage::object_storage::UploadManagerImpl;
use rustmq::storage::tiered::*;
use rustmq::storage::traits::ObjectStorage;
use rustmq::storage::wal::DirectIOWal;
use rustmq::storage::{AlignedBufferPool, WalSegment};
use rustmq::types::{Record, TopicPartition, WalRecord};
use std::sync::Arc;
use tempfile::TempDir;

use common::gcs::GcsTestConfig;

async fn setup_tiered_engine() -> (Arc<TieredStorageEngine>, Arc<dyn ObjectStore>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    
    let (store, bucket, service_account_path) = match GcsTestConfig::load() {
        Some(config) => {
            let mut builder = object_store::gcp::GoogleCloudStorageBuilder::from_env()
                .with_bucket_name(&config.bucket_name);
            if let Some(creds) = config.credentials_path.as_deref().filter(|s| !s.is_empty()) {
                builder = builder.with_service_account_path(creds);
            }
            match builder.build() {
                Ok(gcs) => {
                    let gcs_arc = Arc::new(gcs) as Arc<dyn ObjectStore>;
                    // Verify access before proceeding
                    match gcs_arc.list_with_delimiter(None).await {
                        Ok(_) => (gcs_arc, config.bucket_name, config.credentials_path),
                        Err(e) => {
                            println!("GCS ping failed: {}. Falling back to InMemory.", e);
                            (Arc::new(InMemory::new()) as Arc<dyn ObjectStore>, "test".to_string(), None)
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to build GCS store: {}. Falling back to InMemory.", e);
                    (Arc::new(InMemory::new()) as Arc<dyn ObjectStore>, "test".to_string(), None)
                }
            }
        }
        None => {
            (Arc::new(InMemory::new()) as Arc<dyn ObjectStore>, "test".to_string(), None)
        }
    };

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
        write_cache_size_bytes: 1024 * 1024,
        read_cache_size_bytes: 1024 * 1024,
        eviction_policy: EvictionPolicy::Lru,
    };

    let storage_config = ObjectStorageConfig {
        storage_type: StorageType::Gcs,
        bucket,
        region: "local".to_string(),
        endpoint: "".to_string(),
        access_key: None,
        secret_key: None,
        service_account_path,
        multipart_threshold: 10 * 1024 * 1024,
        max_concurrent_uploads: 4,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let cache_manager = Arc::new(CacheManager::new(&cache_config));
    let object_storage = Arc::new(CloudObjectStorage::new(store.clone()));
    let upload_manager = Arc::new(UploadManagerImpl::new(
        object_storage.clone(),
        storage_config,
    ));

    let engine =
        TieredStorageEngine::new(wal, cache_manager, object_storage.clone(), upload_manager);

    (Arc::new(engine), store, temp_dir)
}

#[tokio::test]
async fn test_gcs_full_write_path() {
    let (engine, store, _dir) = setup_tiered_engine().await;
    let topic_partition = TopicPartition {
        topic: "write-path".to_string(),
        partition: 0,
    };

    let record = Record::new(
        Some(b"k".to_vec()),
        b"v".to_vec(),
        vec![],
        chrono::Utc::now().timestamp_millis(),
    );
    let offset = engine
        .append(topic_partition.clone(), record)
        .await
        .unwrap();

    // Simulate segment upload
    let wal_record = WalRecord {
        topic_partition: topic_partition.clone(),
        offset,
        record: Record::new(Some(b"k".to_vec()), b"v".to_vec(), vec![], 0),
        crc32: 0,
    };
    let data = bincode::serialize(&vec![wal_record]).unwrap();
    let segment = WalSegment {
        topic_partition: topic_partition.clone(),
        start_offset: offset,
        end_offset: offset + 1,
        size_bytes: data.len() as u64,
        data: Bytes::from(data),
    };

    let key = engine
        .upload_manager()
        .upload_segment(segment)
        .await
        .unwrap();

    // Verify it exists in store
    let path = object_store::path::Path::from(key);
    store.head(&path).await.unwrap();
}

#[tokio::test]
async fn test_read_path_gcs_to_cache() {
    let (engine, store, _dir) = setup_tiered_engine().await;
    let topic_partition = TopicPartition {
        topic: "read-path".to_string(),
        partition: 0,
    };

    let offset = 100;
    let wal_record = WalRecord {
        topic_partition: topic_partition.clone(),
        offset,
        record: Record::new(Some(b"key".to_vec()), b"val".to_vec(), vec![], 12345),
        crc32: 0,
    };

    // Wrap in Vec since TieredStorageEngine::read expects Vec<WalRecord> serialized
    let data = bincode::serialize(&vec![wal_record]).unwrap();

    // Compress data since storage type is Gcs
    let compressed_data = lz4_flex::compress_prepend_size(&data);

    // Manual setup in MetadataStore
    let object_key = "topics/read-path/0/100_101.seg";
    store
        .put(
            &object_store::path::Path::from(object_key),
            compressed_data.into(),
        )
        .await
        .unwrap();

    engine.metadata_store().add_segment(
        topic_partition.clone(),
        SegmentMetadata {
            start_offset: offset,
            end_offset: offset + 1,
            object_key: object_key.to_string(),
            size_bytes: 0, // Not used in read
            status: SegmentStatus::InObjectStore,
        },
    );

    // First read — triggers GCS download and populates cache
    let records = engine.read(&topic_partition, offset, 1024).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].value, b"val".to_vec());

    // Delete the object from the store so that only the cache can serve it
    store
        .delete(&object_store::path::Path::from(object_key))
        .await
        .unwrap();

    // Second read — must hit cache since object is gone from store
    let cached_records = engine.read(&topic_partition, offset, 1024).await.unwrap();
    assert_eq!(cached_records.len(), 1, "Cache miss — read failed after object deletion");
    assert_eq!(cached_records[0].value, b"val".to_vec());
}

#[tokio::test]
async fn test_segment_compaction_gcs() {
    let (engine, store, _dir) = setup_tiered_engine().await;
    let tp = TopicPartition {
        topic: "compact-test".to_string(),
        partition: 0,
    };

    // Build two segments with serialized WalRecord data (keys match parser: number_number)
    let records1 = vec![WalRecord {
        topic_partition: tp.clone(),
        offset: 100,
        record: Record::new(Some(b"key1".to_vec()), b"val1".to_vec(), vec![], 0),
        crc32: 0,
    }];
    let records2 = vec![WalRecord {
        topic_partition: tp.clone(),
        offset: 110,
        record: Record::new(Some(b"key2".to_vec()), b"val2".to_vec(), vec![], 0),
        crc32: 0,
    }];

    let data1 = bincode::serialize(&records1).unwrap();
    let data2 = bincode::serialize(&records2).unwrap();

    let key1 = "100_110";
    let key2 = "110_120";
    store
        .put(
            &object_store::path::Path::from(key1),
            Bytes::from(data1.clone()).into(),
        )
        .await
        .unwrap();
    store
        .put(
            &object_store::path::Path::from(key2),
            Bytes::from(data2.clone()).into(),
        )
        .await
        .unwrap();

    let compacted_key = engine
        .compaction_manager()
        .compact_segments(vec![key1.to_string(), key2.to_string()])
        .await
        .unwrap();
    assert!(
        compacted_key.contains("100_120"),
        "Compacted key {} should contain 100_120",
        compacted_key
    );

    // Verify compacted data is the concatenation of both segments
    let result = store
        .get(&object_store::path::Path::from(compacted_key.clone()))
        .await
        .unwrap();
    let bytes = result.bytes().await.unwrap();
    let mut expected = data1.clone();
    expected.extend_from_slice(&data2);
    assert_eq!(bytes.to_vec(), expected, "Compacted data should be byte-exact concatenation");

    // Verify source segments were deleted
    assert!(
        store
            .head(&object_store::path::Path::from(key1))
            .await
            .is_err(),
        "Source segment 1 should be deleted after compaction"
    );
    assert!(
        store
            .head(&object_store::path::Path::from(key2))
            .await
            .is_err(),
        "Source segment 2 should be deleted after compaction"
    );
}

#[tokio::test]
async fn test_gcs_failure_scenarios() {
    // NOTE: InMemory backend cannot simulate auth failures, quota exceeded,
    // or network timeouts. Those scenarios require integration tests with
    // real GCS or a fault-injecting mock (tracked in Phase 9).
    let (engine, _store, _dir) = setup_tiered_engine().await;

    // 1. NotFound — verify error variant is preserved, not opaque Storage(String)
    let res = engine.object_storage().get("nonexistent").await;
    assert!(matches!(res.unwrap_err(), RustMqError::NotFound(_)));

    // 2. Invalid key (leading slash) — rejected by validate_key()
    let res = engine
        .object_storage()
        .put("/invalid", Bytes::from("x"))
        .await;
    assert!(res.is_err());

    // 3. Invalid key (path traversal) — rejected by validate_key()
    let res = engine
        .object_storage()
        .put("../escape", Bytes::from("x"))
        .await;
    assert!(res.is_err());

    // 4. Invalid key (empty) — rejected by validate_key()
    let res = engine.object_storage().put("", Bytes::from("x")).await;
    assert!(res.is_err());
}
