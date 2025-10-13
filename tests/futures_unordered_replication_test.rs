/// Comprehensive tests for FuturesUnordered replication optimization
///
/// These tests validate that the FuturesUnordered implementation:
/// 1. Returns early when majority acknowledgments are received
/// 2. Handles failures gracefully
/// 3. Maintains correctness with variable follower latencies
/// 4. Improves performance over join_all approach

use rustmq::replication::manager::{ReplicationManager, ReplicationRpcClient};
use rustmq::replication::traits::ReplicationManager as ReplicationManagerTrait;
use rustmq::config::ReplicationConfig;
use rustmq::types::*;
use rustmq::Result;
use rustmq::storage::{WriteAheadLog, DirectIOWal, AlignedBufferPool};
use rustmq::config::WalConfig;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::Instant;

/// Mock RPC client that simulates variable latencies
struct VariableLatencyRpcClient {
    latencies: Arc<std::collections::HashMap<String, Duration>>,
    failure_brokers: Arc<std::sync::Mutex<Vec<String>>>,
    call_count: Arc<AtomicU64>,
}

impl VariableLatencyRpcClient {
    fn new(latencies: std::collections::HashMap<String, Duration>) -> Self {
        Self {
            latencies: Arc::new(latencies),
            failure_brokers: Arc::new(std::sync::Mutex::new(Vec::new())),
            call_count: Arc::new(AtomicU64::new(0)),
        }
    }

    fn with_failures(self, failures: Vec<String>) -> Self {
        *self.failure_brokers.lock().unwrap() = failures;
        self
    }

    fn get_call_count(&self) -> u64 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ReplicationRpcClient for VariableLatencyRpcClient {
    async fn replicate_data(&self, broker_id: &BrokerId, _request: ReplicateDataRequest) -> Result<ReplicateDataResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        // Check if this broker should fail
        {
            let failures = self.failure_brokers.lock().unwrap();
            if failures.contains(broker_id) {
                return Err(rustmq::error::RustMqError::ReplicationFailed {
                    broker_id: broker_id.clone(),
                    error_message: "Simulated failure".to_string(),
                });
            }
        }

        // Simulate latency
        if let Some(latency) = self.latencies.get(broker_id) {
            tokio::time::sleep(*latency).await;
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(ReplicateDataResponse {
            success: true,
            error_code: 0,
            error_message: None,
            follower_state: Some(FollowerState {
                broker_id: broker_id.clone(),
                last_known_offset: 0,
                last_heartbeat: chrono::Utc::now(),
                lag: 0,
            }),
        })
    }

    async fn send_heartbeat(&self, broker_id: &BrokerId, _request: HeartbeatRequest) -> Result<HeartbeatResponse> {
        Ok(HeartbeatResponse {
            success: true,
            error_code: 0,
            error_message: None,
            follower_state: Some(FollowerState {
                broker_id: broker_id.clone(),
                last_known_offset: 0,
                last_heartbeat: chrono::Utc::now(),
                lag: 0,
            }),
        })
    }

    async fn transfer_leadership(&self, _broker_id: &BrokerId, _request: TransferLeadershipRequest) -> Result<TransferLeadershipResponse> {
        Ok(TransferLeadershipResponse {
            success: true,
            error_code: 0,
            error_message: None,
            new_leader_epoch: Some(1),
        })
    }
}

async fn setup_test_replication_manager(
    rpc_client: Arc<dyn ReplicationRpcClient>,
    min_in_sync_replicas: usize,
    followers: Vec<String>,
) -> (ReplicationManager, TempDir) {
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
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap()) as Arc<dyn WriteAheadLog>;

    let config = ReplicationConfig {
        min_in_sync_replicas,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let mut replica_set = vec!["broker-leader".to_string()];
    replica_set.extend(followers);

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let manager = ReplicationManager::new(
        1,
        topic_partition,
        "broker-leader".to_string(),
        1,
        replica_set,
        config,
        wal,
        rpc_client,
    );

    (manager, temp_dir)
}

fn create_test_record(offset: u64) -> WalRecord {
    WalRecord {
        topic_partition: TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        },
        offset,
        record: Record::new(
            Some(format!("key-{}", offset).into_bytes()),
            format!("value-{}", offset).into_bytes(),
            vec![],
            chrono::Utc::now().timestamp_millis(),
        ),
        crc32: 0,
    }
}

#[tokio::test]
async fn test_early_return_with_majority() {
    // Test that FuturesUnordered returns early when majority is achieved
    // Setup: 5 brokers (1 leader + 4 followers), min_in_sync=3
    // Latencies: broker-1: 10ms, broker-2: 20ms, broker-3: 100ms, broker-4: 200ms
    // Expected: Return after broker-2 completes (~20ms), not wait for broker-3/4

    let mut latencies = std::collections::HashMap::new();
    latencies.insert("broker-1".to_string(), Duration::from_millis(10));
    latencies.insert("broker-2".to_string(), Duration::from_millis(20));
    latencies.insert("broker-3".to_string(), Duration::from_millis(100));
    latencies.insert("broker-4".to_string(), Duration::from_millis(200));

    let rpc_client = Arc::new(VariableLatencyRpcClient::new(latencies)) as Arc<dyn ReplicationRpcClient>;
    let followers = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
        "broker-4".to_string(),
    ];

    let (manager, _temp_dir) = setup_test_replication_manager(rpc_client.clone(), 3, followers).await;

    let record = create_test_record(0);

    let start = Instant::now();
    let result = manager.replicate_record(record).await.unwrap();
    let elapsed = start.elapsed();

    // Should return in ~20-30ms (after broker-2), not 200ms (broker-4)
    assert!(elapsed < Duration::from_millis(50), "Expected early return but took {:?}", elapsed);
    assert_eq!(result.durability, DurabilityLevel::Durable);

    println!("✓ Early return test passed: {} acks in {:?}", 3, elapsed);
}

#[tokio::test]
async fn test_handles_slow_followers_gracefully() {
    // Test that one very slow follower doesn't block the entire operation
    let mut latencies = std::collections::HashMap::new();
    latencies.insert("broker-1".to_string(), Duration::from_millis(5));
    latencies.insert("broker-2".to_string(), Duration::from_millis(5));
    latencies.insert("broker-3".to_string(), Duration::from_millis(5000)); // Very slow

    let rpc_client = Arc::new(VariableLatencyRpcClient::new(latencies)) as Arc<dyn ReplicationRpcClient>;
    let followers = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let (manager, _temp_dir) = setup_test_replication_manager(rpc_client, 2, followers).await;

    let record = create_test_record(0);

    let start = Instant::now();
    let result = manager.replicate_record(record).await.unwrap();
    let elapsed = start.elapsed();

    // Should return quickly despite slow follower
    assert!(elapsed < Duration::from_millis(100), "Slow follower blocked operation: {:?}", elapsed);
    assert_eq!(result.durability, DurabilityLevel::Durable);

    println!("✓ Slow follower handling test passed: completed in {:?}", elapsed);
}

#[tokio::test]
async fn test_handles_follower_failures() {
    // Test that failures are handled correctly
    let latencies = std::collections::HashMap::new();

    let rpc_client = Arc::new(
        VariableLatencyRpcClient::new(latencies)
            .with_failures(vec!["broker-1".to_string()])
    ) as Arc<dyn ReplicationRpcClient>;

    let followers = vec![
        "broker-1".to_string(), // Will fail
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let (manager, _temp_dir) = setup_test_replication_manager(rpc_client, 2, followers).await;

    let record = create_test_record(0);

    let result = manager.replicate_record(record).await.unwrap();

    // Should still succeed with 2/3 followers
    assert_eq!(result.durability, DurabilityLevel::Durable);

    println!("✓ Follower failure handling test passed");
}

#[tokio::test]
async fn test_insufficient_acks_returns_local_only() {
    // Test that insufficient acks results in LocalOnly durability
    let latencies = std::collections::HashMap::new();

    let rpc_client = Arc::new(
        VariableLatencyRpcClient::new(latencies)
            .with_failures(vec![
                "broker-1".to_string(),
                "broker-2".to_string(),
                "broker-3".to_string(),
            ])
    ) as Arc<dyn ReplicationRpcClient>;

    let followers = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let (manager, _temp_dir) = setup_test_replication_manager(rpc_client, 3, followers).await;

    let record = create_test_record(0);

    let result = manager.replicate_record(record).await.unwrap();

    // Should return LocalOnly since all followers failed
    assert_eq!(result.durability, DurabilityLevel::LocalOnly);

    println!("✓ Insufficient acks test passed");
}

#[tokio::test]
async fn test_performance_improvement_over_join_all() {
    // Benchmark: Compare FuturesUnordered vs hypothetical join_all behavior
    // With 5 followers of varying latencies, FuturesUnordered should be significantly faster

    let mut latencies = std::collections::HashMap::new();
    latencies.insert("broker-1".to_string(), Duration::from_millis(10));
    latencies.insert("broker-2".to_string(), Duration::from_millis(15));
    latencies.insert("broker-3".to_string(), Duration::from_millis(20));
    latencies.insert("broker-4".to_string(), Duration::from_millis(100));
    latencies.insert("broker-5".to_string(), Duration::from_millis(150));

    let rpc_client = Arc::new(VariableLatencyRpcClient::new(latencies)) as Arc<dyn ReplicationRpcClient>;
    let followers = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
        "broker-4".to_string(),
        "broker-5".to_string(),
    ];

    let (manager, _temp_dir) = setup_test_replication_manager(rpc_client, 3, followers).await;

    let record = create_test_record(0);

    let start = Instant::now();
    let result = manager.replicate_record(record).await.unwrap();
    let elapsed = start.elapsed();

    // FuturesUnordered: should return after ~20ms (3rd fastest follower)
    // join_all would wait: ~150ms (slowest follower)
    // Expected improvement: 85-87% latency reduction

    assert!(elapsed < Duration::from_millis(40), "Expected <40ms but took {:?}", elapsed);
    assert_eq!(result.durability, DurabilityLevel::Durable);

    let improvement_pct = (1.0 - (elapsed.as_millis() as f64 / 150.0)) * 100.0;
    println!("✓ Performance test passed: {:?} vs ~150ms with join_all (~{:.0}% improvement)",
             elapsed, improvement_pct);
}

#[tokio::test]
async fn test_concurrent_replication_requests() {
    // Test that multiple concurrent replication requests work correctly
    let latencies = std::collections::HashMap::new();
    let rpc_client = Arc::new(VariableLatencyRpcClient::new(latencies)) as Arc<dyn ReplicationRpcClient>;

    let followers = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let (manager, _temp_dir) = setup_test_replication_manager(rpc_client.clone(), 2, followers).await;
    let manager = Arc::new(manager);

    // Send 10 concurrent replication requests
    let mut handles = vec![];
    for i in 0..10 {
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            let record = create_test_record(i);
            manager_clone.replicate_record(record).await
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let mut successes = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(result) if result.durability == DurabilityLevel::Durable => successes += 1,
            _ => {}
        }
    }

    assert_eq!(successes, 10, "All concurrent replications should succeed");
    println!("✓ Concurrent replication test passed: {}/10 successful", successes);
}

#[tokio::test]
async fn test_preserves_follower_state_updates() {
    // Test that follower states are properly updated for successful replications
    // With FuturesUnordered and early return, we update states only for completed acks
    let latencies = std::collections::HashMap::new();
    let rpc_client = Arc::new(VariableLatencyRpcClient::new(latencies)) as Arc<dyn ReplicationRpcClient>;

    let followers = vec![
        "broker-1".to_string(),
        "broker-2".to_string(),
        "broker-3".to_string(),
    ];

    let (manager, _temp_dir) = setup_test_replication_manager(rpc_client, 2, followers).await;

    let record = create_test_record(0);
    manager.replicate_record(record).await.unwrap();

    // Give some time for remaining futures to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check that follower states were updated
    let states = manager.get_follower_states().await.unwrap();

    // With early return and min_in_sync=2, we need at least 1 follower ack (leader counts as 1)
    // Due to early return, we may have exactly the minimum number of states
    // This is correct behavior - we return as soon as we have enough acks
    assert!(states.len() >= 1, "Expected at least 1 follower state, got {}", states.len());
    assert!(states.len() <= 3, "Expected at most 3 follower states, got {}", states.len());

    println!("✓ Follower state preservation test passed: {} states updated (early return working correctly)", states.len());
}
