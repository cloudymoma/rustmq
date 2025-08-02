use rustmq::replication::{ReplicationManager, FollowerReplicationHandler};
use rustmq::replication::manager::MockReplicationRpcClient;
use rustmq::replication::traits::ReplicationManager as ReplicationManagerTrait;
use rustmq::storage::{DirectIOWal, AlignedBufferPool};
use rustmq::config::{ReplicationConfig, WalConfig};
use rustmq::types::*;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_replication_manager_basic_replication() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let config = ReplicationConfig {
        min_in_sync_replicas: 2,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let replica_set = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let replication_manager = ReplicationManager::new(
        1,
        topic_partition.clone(),
        "broker-1".to_string(),
        1,
        replica_set,
        config,
        wal,
        rpc_client,
    );

    // Test record replication
    let record = WalRecord {
        topic_partition: topic_partition.clone(),
        offset: 0,
        record: Record {
            key: Some(b"key1".to_vec()),
            value: b"value1".to_vec(),
            headers: vec![],
            timestamp: chrono::Utc::now().timestamp_millis(),
        },
        crc32: 0,
    };

    let result = replication_manager.replicate_record(record).await.unwrap();
    assert_eq!(result.offset, 0);
    assert!(matches!(result.durability, DurabilityLevel::Durable));

    // Test follower management
    replication_manager.add_follower("broker-4".to_string()).await.unwrap();
    let states = replication_manager.get_follower_states().await.unwrap();
    assert!(states.len() >= 1); // Should have at least the newly added follower

    // Test high watermark
    let hwm = replication_manager.get_high_watermark().await.unwrap();
    assert!(hwm >= 0);
}

#[tokio::test]
async fn test_replication_manager_leader_epoch_handling() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let config = ReplicationConfig {
        min_in_sync_replicas: 1,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let replica_set = vec!["broker-1".to_string()];
    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let replication_manager = ReplicationManager::new(
        1,
        topic_partition,
        "broker-1".to_string(),
        5, // Initial epoch
        replica_set,
        config,
        wal,
        rpc_client,
    );

    // Test initial epoch
    assert_eq!(replication_manager.get_leader_epoch(), 5);

    // Test epoch validation
    let validation_result = replication_manager.validate_leader_epoch(3);
    assert!(validation_result.is_err()); // Should reject stale epoch

    let validation_result = replication_manager.validate_leader_epoch(5);
    assert!(validation_result.is_ok()); // Should accept current epoch

    let validation_result = replication_manager.validate_leader_epoch(7);
    assert!(validation_result.is_ok()); // Should accept higher epoch

    // Test epoch update
    replication_manager.update_leader_epoch(10);
    assert_eq!(replication_manager.get_leader_epoch(), 10);
}

#[tokio::test]
async fn test_replication_manager_high_watermark_calculation() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let config = ReplicationConfig {
        min_in_sync_replicas: 3,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let replica_set = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let replication_manager = ReplicationManager::new(
        1,
        topic_partition,
        "broker-1".to_string(),
        1,
        replica_set,
        config,
        wal,
        rpc_client,
    );

    // Simulate different follower states
    let follower_states = vec![
        FollowerState {
            broker_id: "broker-2".to_string(),
            last_known_offset: 95,
            last_heartbeat: chrono::Utc::now(),
            lag: 5,
        },
        FollowerState {
            broker_id: "broker-3".to_string(),
            last_known_offset: 98,
            last_heartbeat: chrono::Utc::now(),
            lag: 2,
        },
    ];

    for state in follower_states {
        replication_manager.update_follower_state(state).await;
    }

    // Test that ISR tracking works
    let isr = replication_manager.get_in_sync_replicas();
    assert!(isr.len() <= 2); // Should include only in-sync followers

    // Test high watermark updates
    let initial_hwm = replication_manager.get_high_watermark().await.unwrap();
    replication_manager.update_high_watermark(50).await.unwrap();
    let updated_hwm = replication_manager.get_high_watermark().await.unwrap();
    assert_eq!(updated_hwm, 50);
}

#[tokio::test]
async fn test_follower_replication_handler_epoch_validation() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let handler = FollowerReplicationHandler::new(
        topic_partition.clone(),
        5, // Current epoch
        Some("leader-1".to_string()),
        wal,
        "follower-1".to_string(),
    );

    // Test accepting current epoch
    let current_request = ReplicateDataRequest {
        leader_epoch: 5,
        topic_partition: topic_partition.clone(),
        records: vec![],
        leader_id: "leader-1".to_string(),
    };

    let response = handler.handle_replicate_data(current_request).await.unwrap();
    assert!(response.success);

    // Test rejecting stale epoch
    let stale_request = ReplicateDataRequest {
        leader_epoch: 3,
        topic_partition: topic_partition.clone(),
        records: vec![],
        leader_id: "old-leader".to_string(),
    };

    let response = handler.handle_replicate_data(stale_request).await.unwrap();
    assert!(!response.success);
    assert_eq!(response.error_code, 1001);

    // Test accepting higher epoch
    let new_request = ReplicateDataRequest {
        leader_epoch: 7,
        topic_partition: topic_partition.clone(),
        records: vec![],
        leader_id: "new-leader".to_string(),
    };

    let response = handler.handle_replicate_data(new_request).await.unwrap();
    assert!(response.success);
    assert_eq!(handler.get_leader_epoch(), 7);
    assert_eq!(handler.get_current_leader(), Some("new-leader".to_string()));
}

#[tokio::test]
async fn test_follower_heartbeat_handling() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let handler = FollowerReplicationHandler::new(
        topic_partition.clone(),
        5,
        Some("leader-1".to_string()),
        wal,
        "follower-1".to_string(),
    );

    // Test valid heartbeat
    let heartbeat = HeartbeatRequest {
        leader_epoch: 5,
        leader_id: "leader-1".to_string(),
        topic_partition: topic_partition.clone(),
        high_watermark: 100,
    };

    let result = handler.handle_heartbeat(heartbeat).await;
    assert!(result.is_ok());
    
    let follower_state = result.unwrap();
    assert_eq!(follower_state.broker_id, "follower-1");

    // Test stale heartbeat
    let stale_heartbeat = HeartbeatRequest {
        leader_epoch: 3,
        leader_id: "old-leader".to_string(),
        topic_partition,
        high_watermark: 100,
    };

    let result = handler.handle_heartbeat(stale_heartbeat).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_replication_timeout_handling() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let config = ReplicationConfig {
        min_in_sync_replicas: 2,
        ack_timeout_ms: 100, // Very short timeout for testing
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let replica_set = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
    ];

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let replication_manager = ReplicationManager::new(
        1,
        topic_partition.clone(),
        "broker-1".to_string(),
        1,
        replica_set,
        config,
        wal,
        rpc_client,
    );

    // Test that replication completes within timeout
    let record = WalRecord {
        topic_partition,
        offset: 0,
        record: Record {
            key: Some(b"key1".to_vec()),
            value: b"value1".to_vec(),
            headers: vec![],
            timestamp: chrono::Utc::now().timestamp_millis(),
        },
        crc32: 0,
    };

    let result = timeout(
        Duration::from_millis(500),
        replication_manager.replicate_record(record)
    ).await;
    
    assert!(result.is_ok(), "Replication should complete within timeout");
    let replication_result = result.unwrap().unwrap();
    assert_eq!(replication_result.offset, 0);
}

#[tokio::test]
async fn test_concurrent_replication_operations() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let config = ReplicationConfig {
        min_in_sync_replicas: 1,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let replica_set = vec!["broker-1".to_string()];
    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let replication_manager = Arc::new(ReplicationManager::new(
        1,
        topic_partition.clone(),
        "broker-1".to_string(),
        1,
        replica_set,
        config,
        wal,
        rpc_client,
    ));

    // Test concurrent record replication
    let mut tasks = vec![];
    for i in 0..5 {
        let manager = replication_manager.clone();
        let tp = topic_partition.clone();
        tasks.push(tokio::spawn(async move {
            let record = WalRecord {
                topic_partition: tp,
                offset: i,
                record: Record {
                    key: Some(format!("key{}", i).into_bytes()),
                    value: format!("value{}", i).into_bytes(),
                    headers: vec![],
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                crc32: 0,
            };
            manager.replicate_record(record).await
        }));
    }

    let mut successful = 0;
    for task in tasks {
        let result = timeout(Duration::from_secs(1), task).await.unwrap();
        if result.is_ok() {
            successful += 1;
        }
    }

    assert_eq!(successful, 5, "All concurrent replications should succeed");
}

#[tokio::test]
async fn test_replication_follower_state_updates() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let config = ReplicationConfig {
        min_in_sync_replicas: 2,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let replica_set = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let replication_manager = ReplicationManager::new(
        1,
        topic_partition,
        "broker-1".to_string(),
        1,
        replica_set,
        config,
        wal,
        rpc_client,
    );

    // Add and remove followers
    replication_manager.add_follower("broker-4".to_string()).await.unwrap();
    replication_manager.add_follower("broker-5".to_string()).await.unwrap();

    let states = replication_manager.get_follower_states().await.unwrap();
    let initial_count = states.len();
    assert!(initial_count >= 2);

    // Remove a follower
    replication_manager.remove_follower("broker-4".to_string()).await.unwrap();

    let states = replication_manager.get_follower_states().await.unwrap();
    assert_eq!(states.len(), initial_count - 1);

    // Verify specific follower is gone
    assert!(!states.iter().any(|s| s.broker_id == "broker-4"));
}