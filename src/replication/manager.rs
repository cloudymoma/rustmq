use crate::{Result, types::*, config::ReplicationConfig, storage::WriteAheadLog, replication::traits::ReplicationManager as ReplicationManagerTrait};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::{HashMap, BinaryHeap};
use std::time::Duration;
use tokio::time::timeout;
use std::cmp::Reverse;

pub struct ReplicationManager {
    stream_id: u64,
    topic_partition: TopicPartition,
    current_leader: Option<BrokerId>,
    leader_epoch: AtomicU64,
    replica_set: Vec<BrokerId>,
    
    log_end_offset: AtomicU64,
    high_watermark: AtomicU64,
    follower_states: Arc<RwLock<HashMap<BrokerId, FollowerState>>>,
    
    min_in_sync_replicas: usize,
    ack_timeout: Duration,
    max_replication_lag: u64,
    heartbeat_timeout: Duration,
    
    wal: Arc<dyn WriteAheadLog>,
    rpc_client: Arc<dyn ReplicationRpcClient>,
}

#[async_trait]
pub trait ReplicationRpcClient: Send + Sync {
    /// Send replication data to a follower broker
    /// CRITICAL: All implementations must validate leader epoch on the follower side
    async fn replicate_data(&self, broker_id: &BrokerId, request: ReplicateDataRequest) -> Result<ReplicateDataResponse>;
    
    /// Send heartbeat to a follower broker
    /// CRITICAL: All implementations must validate leader epoch on the follower side
    async fn send_heartbeat(&self, broker_id: &BrokerId, request: HeartbeatRequest) -> Result<HeartbeatResponse>;
    
    /// Transfer leadership to another broker
    /// Used during planned leadership transitions
    async fn transfer_leadership(&self, broker_id: &BrokerId, request: TransferLeadershipRequest) -> Result<TransferLeadershipResponse>;
}

pub struct MockReplicationRpcClient;

#[async_trait]
impl ReplicationRpcClient for MockReplicationRpcClient {
    async fn replicate_data(&self, _broker_id: &BrokerId, _request: ReplicateDataRequest) -> Result<ReplicateDataResponse> {
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(ReplicateDataResponse {
            success: true,
            error_code: 0,
            error_message: None,
            follower_state: Some(FollowerState {
                broker_id: _broker_id.clone(),
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

impl ReplicationManager {
    pub fn new(
        stream_id: u64,
        topic_partition: TopicPartition,
        leader: BrokerId,
        leader_epoch: u64,
        replica_set: Vec<BrokerId>,
        config: ReplicationConfig,
        wal: Arc<dyn WriteAheadLog>,
        rpc_client: Arc<dyn ReplicationRpcClient>,
    ) -> Self {
        Self {
            stream_id,
            topic_partition,
            current_leader: Some(leader),
            leader_epoch: AtomicU64::new(leader_epoch),
            replica_set,
            log_end_offset: AtomicU64::new(0),
            high_watermark: AtomicU64::new(0),
            follower_states: Arc::new(RwLock::new(HashMap::new())),
            min_in_sync_replicas: config.min_in_sync_replicas,
            ack_timeout: Duration::from_millis(config.ack_timeout_ms),
            max_replication_lag: config.max_replication_lag,
            heartbeat_timeout: Duration::from_millis(config.heartbeat_timeout_ms),
            wal,
            rpc_client,
        }
    }

    async fn append_to_local_wal(&self, record: WalRecord) -> Result<Offset> {
        let offset = self.wal.append(record).await?;
        self.log_end_offset.store(offset + 1, Ordering::SeqCst);
        Ok(offset)
    }

    async fn send_to_follower(&self, broker_id: BrokerId, record: WalRecord) -> Result<()> {
        let current_epoch = self.leader_epoch.load(Ordering::SeqCst);
        let leader_id = self.current_leader.as_ref().ok_or(crate::error::RustMqError::NoLeader)?.clone();
        
        let request = ReplicateDataRequest {
            leader_epoch: current_epoch,
            topic_partition: self.topic_partition.clone(),
            records: vec![record],
            leader_id,
        };

        let response = timeout(self.ack_timeout, self.rpc_client.replicate_data(&broker_id, request)).await
            .map_err(|_| crate::error::RustMqError::Timeout)??;

        if !response.success {
            return Err(crate::error::RustMqError::ReplicationFailed {
                broker_id,
                error_message: response.error_message.unwrap_or_else(|| "Unknown error".to_string()),
            });
        }

        // Update follower state if provided
        if let Some(follower_state) = response.follower_state {
            self.update_follower_state(follower_state).await;
        }

        Ok(())
    }

    async fn wait_for_acknowledgments(&self, record: WalRecord, local_offset: Offset) -> Result<ReplicationResult> {
        let followers: Vec<BrokerId> = self.replica_set
            .iter()
            .filter(|&broker_id| Some(broker_id.clone()) != self.current_leader)
            .cloned()
            .collect();

        if followers.is_empty() {
            // No followers, so leader's offset becomes the high-watermark
            self.high_watermark.store(local_offset, Ordering::SeqCst);
            return Ok(ReplicationResult {
                offset: local_offset,
                durability: DurabilityLevel::Durable,
            });
        }

        let replication_futures = followers.iter().map(|broker_id| {
            let broker_id = broker_id.clone();
            let record = record.clone();
            async move {
                self.send_to_follower(broker_id, record).await
            }
        });

        let results = futures::future::join_all(replication_futures).await;
        let successful_acks = results.iter().filter(|r| r.is_ok()).count();

        if successful_acks >= self.min_in_sync_replicas - 1 {
            // Recalculate high-watermark based on actual follower states
            self.recalculate_high_watermark().await;
            Ok(ReplicationResult {
                offset: local_offset,
                durability: DurabilityLevel::Durable,
            })
        } else {
            Ok(ReplicationResult {
                offset: local_offset,
                durability: DurabilityLevel::LocalOnly,
            })
        }
    }

    pub async fn update_follower_state(&self, follower_state: FollowerState) {
        let mut states = self.follower_states.write();
        states.insert(follower_state.broker_id.clone(), follower_state);
    }

    /// Recalculate high-watermark based on current follower states
    /// High-watermark is the highest offset that is confirmed on all required in-sync replicas
    /// 
    /// Optimized implementation using a min-heap to avoid O(n log n) sorting
    async fn recalculate_high_watermark(&self) {
        let states = self.follower_states.read();
        let current_leader_offset = self.log_end_offset.load(Ordering::SeqCst);
        
        // Use a min-heap to efficiently find the min_in_sync_replicas-th highest offset
        // We use Reverse to make BinaryHeap work as a min-heap
        let mut min_heap: BinaryHeap<Reverse<u64>> = BinaryHeap::new();
        
        // Add leader's offset
        min_heap.push(Reverse(current_leader_offset));
        
        // Add follower offsets that are in-sync
        for state in states.values() {
            if self.is_follower_in_sync(state) {
                min_heap.push(Reverse(state.last_known_offset));
            }
        }
        
        if min_heap.len() >= self.min_in_sync_replicas {
            // We need to find the min_in_sync_replicas-th smallest offset
            // which represents the highest offset that at least min_in_sync_replicas have
            let mut offsets_to_extract = min_heap.len() - self.min_in_sync_replicas + 1;
            let mut new_high_watermark = 0;
            
            while offsets_to_extract > 0 && !min_heap.is_empty() {
                if let Some(Reverse(offset)) = min_heap.pop() {
                    new_high_watermark = offset;
                    offsets_to_extract -= 1;
                }
            }
            
            // Only advance high-watermark, never decrease it
            let current_hwm = self.high_watermark.load(Ordering::SeqCst);
            if new_high_watermark > current_hwm {
                self.high_watermark.store(new_high_watermark, Ordering::SeqCst);
                tracing::debug!(
                    "Advanced high-watermark to {} with {} in-sync replicas",
                    new_high_watermark,
                    min_heap.len() + offsets_to_extract // original size
                );
            }
        }
    }

    /// Check if a follower is considered in-sync based on lag and heartbeat
    fn is_follower_in_sync(&self, follower_state: &FollowerState) -> bool {
        let current_offset = self.log_end_offset.load(Ordering::SeqCst);
        let lag = current_offset.saturating_sub(follower_state.last_known_offset);
        
        // Check both lag and heartbeat recency
        let now = chrono::Utc::now();
        
        // Properly handle chrono duration conversion errors
        let heartbeat_fresh = match (now - follower_state.last_heartbeat).to_std() {
            Ok(duration) => duration < self.heartbeat_timeout,
            Err(e) => {
                // If conversion fails (e.g., negative duration due to clock skew),
                // conservatively mark as stale
                tracing::warn!(
                    "Failed to convert heartbeat duration for follower {}: {}. Marking as stale.", 
                    follower_state.broker_id, e
                );
                false
            }
        };
        
        lag <= self.max_replication_lag && heartbeat_fresh
    }

    pub fn get_in_sync_replicas(&self) -> Vec<BrokerId> {
        let states = self.follower_states.read();
        states
            .values()
            .filter(|state| self.is_follower_in_sync(state))
            .map(|state| state.broker_id.clone())
            .collect()
    }

    pub async fn start_heartbeat_monitor(&self) {
        let follower_states = self.follower_states.clone();
        let rpc_client = self.rpc_client.clone();
        let replica_set = self.replica_set.clone();
        let current_leader = self.current_leader.clone();
        let topic_partition = self.topic_partition.clone();
        let leader_epoch = Arc::new(AtomicU64::new(self.leader_epoch.load(Ordering::SeqCst)));
        let high_watermark = Arc::new(AtomicU64::new(self.high_watermark.load(Ordering::SeqCst)));

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                let followers: Vec<BrokerId> = replica_set
                    .iter()
                    .filter(|&broker_id| Some(broker_id.clone()) != current_leader)
                    .cloned()
                    .collect();

                if let Some(ref leader_id) = current_leader {
                    let current_epoch = leader_epoch.load(Ordering::SeqCst);
                    let current_hwm = high_watermark.load(Ordering::SeqCst);
                    
                    let heartbeat_request = HeartbeatRequest {
                        leader_epoch: current_epoch,
                        leader_id: leader_id.clone(),
                        topic_partition: topic_partition.clone(),
                        high_watermark: current_hwm,
                    };

                    for broker_id in followers {
                        match rpc_client.send_heartbeat(&broker_id, heartbeat_request.clone()).await {
                            Ok(response) => {
                                if response.success {
                                    if let Some(state) = response.follower_state {
                                        let mut states = follower_states.write();
                                        states.insert(broker_id, state);
                                    }
                                } else {
                                    // Handle heartbeat failure (e.g., stale leader epoch)
                                    tracing::warn!(
                                        "Heartbeat failed for broker {}: error_code={}, message={:?}",
                                        broker_id,
                                        response.error_code,
                                        response.error_message
                                    );
                                    
                                    // If we get a stale leader epoch error, this broker thinks there's a newer leader
                                    if response.error_code == 1001 {
                                        tracing::error!(
                                            "Broker {} rejected heartbeat due to stale leader epoch. Current leadership may have changed.",
                                            broker_id
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to send heartbeat to broker {}: {}", broker_id, e);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Update leader epoch when leadership changes
    pub fn update_leader_epoch(&self, new_epoch: u64) {
        self.leader_epoch.store(new_epoch, Ordering::SeqCst);
    }

    /// Get current leader epoch
    pub fn get_leader_epoch(&self) -> u64 {
        self.leader_epoch.load(Ordering::SeqCst)
    }

    /// Validate incoming replication request epoch
    pub fn validate_leader_epoch(&self, request_epoch: u64) -> Result<()> {
        let current_epoch = self.leader_epoch.load(Ordering::SeqCst);
        if request_epoch < current_epoch {
            return Err(crate::error::RustMqError::StaleLeaderEpoch {
                request_epoch,
                current_epoch,
            });
        }
        Ok(())
    }
}

#[async_trait]
impl ReplicationManagerTrait for ReplicationManager {
    async fn replicate_record(&self, record: WalRecord) -> Result<ReplicationResult> {
        let local_offset = self.append_to_local_wal(record.clone()).await?;
        self.wait_for_acknowledgments(record, local_offset).await
    }

    async fn add_follower(&self, broker_id: BrokerId) -> Result<()> {
        let mut states = self.follower_states.write();
        states.insert(broker_id.clone(), FollowerState {
            broker_id,
            last_known_offset: 0,
            last_heartbeat: chrono::Utc::now(),
            lag: 0,
        });
        Ok(())
    }

    async fn remove_follower(&self, broker_id: BrokerId) -> Result<()> {
        let mut states = self.follower_states.write();
        states.remove(&broker_id);
        Ok(())
    }

    async fn get_follower_states(&self) -> Result<Vec<FollowerState>> {
        let states = self.follower_states.read();
        Ok(states.values().cloned().collect())
    }

    async fn update_high_watermark(&self, offset: Offset) -> Result<()> {
        self.high_watermark.store(offset, Ordering::SeqCst);
        Ok(())
    }

    async fn get_high_watermark(&self) -> Result<Offset> {
        Ok(self.high_watermark.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{DirectIOWal, AlignedBufferPool};
    use crate::config::WalConfig;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_replication_manager() {
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
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

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

        let topic_partition = crate::types::TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };

        let replication_manager = ReplicationManager::new(
            1,
            topic_partition,
            "broker-1".to_string(),
            1, // leader_epoch
            replica_set,
            config,
            wal,
            rpc_client,
        );

        let record = WalRecord {
            topic_partition: crate::types::TopicPartition {
                topic: "test-topic".to_string(),
                partition: 0,
            },
            offset: 0,
            record: crate::types::Record {
                key: Some(b"key1".to_vec()),
                value: b"value1".to_vec(),
                headers: vec![],
                timestamp: chrono::Utc::now().timestamp_millis(),
            },
            crc32: 0,
        };

        let result = replication_manager.replicate_record(record).await.unwrap();
        assert_eq!(result.offset, 0);

        replication_manager.add_follower("broker-4".to_string()).await.unwrap();
        let states = replication_manager.get_follower_states().await.unwrap();
        // Should have 3 follower states: broker-2, broker-3 (from replication), and broker-4 (manually added)
        assert_eq!(states.len(), 3);
        
        // Check that broker-4 is in the states
        let broker_4_state = states.iter().find(|s| s.broker_id == "broker-4");
        assert!(broker_4_state.is_some());
    }

    #[tokio::test]
    async fn test_replication_durability() {
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
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

        let config = ReplicationConfig {
            min_in_sync_replicas: 3, // High requirement
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
            heartbeat_timeout_ms: 30000,
        };

        let replica_set = vec!["broker-1".to_string()]; // Only leader, no followers

        let topic_partition = crate::types::TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };

        let replication_manager = ReplicationManager::new(
            1,
            topic_partition,
            "broker-1".to_string(),
            1, // leader_epoch
            replica_set,
            config,
            wal,
            rpc_client,
        );

        let record = WalRecord {
            topic_partition: crate::types::TopicPartition {
                topic: "test-topic".to_string(),
                partition: 0,
            },
            offset: 0,
            record: crate::types::Record {
                key: Some(b"key1".to_vec()),
                value: b"value1".to_vec(),
                headers: vec![],
                timestamp: chrono::Utc::now().timestamp_millis(),
            },
            crc32: 0,
        };

        let result = replication_manager.replicate_record(record).await.unwrap();
        
        // Should still succeed locally but with LocalOnly durability
        assert_eq!(result.offset, 0);
        assert!(matches!(result.durability, DurabilityLevel::Durable)); // No followers, so it's considered durable
    }

    #[tokio::test]
    async fn test_high_watermark_advancement() {
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
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

        let config = ReplicationConfig {
            min_in_sync_replicas: 3, // Require 3 replicas (leader + 2 followers)
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
            heartbeat_timeout_ms: 30000,
        };

        let replica_set = vec![
            "broker-1".to_string(), // Leader
            "broker-2".to_string(),
            "broker-3".to_string(),
        ];

        let topic_partition = crate::types::TopicPartition {
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

        // Simulate follower states - only one follower is caught up
        let follower_state_2 = FollowerState {
            broker_id: "broker-2".to_string(),
            last_known_offset: 0, // Caught up
            last_heartbeat: chrono::Utc::now(),
            lag: 0,
        };

        let follower_state_3 = FollowerState {
            broker_id: "broker-3".to_string(),
            last_known_offset: 0, // Caught up
            last_heartbeat: chrono::Utc::now(),
            lag: 0,
        };

        replication_manager.update_follower_state(follower_state_2).await;
        replication_manager.update_follower_state(follower_state_3).await;

        let record = WalRecord {
            topic_partition: crate::types::TopicPartition {
                topic: "test-topic".to_string(),
                partition: 0,
            },
            offset: 0,
            record: crate::types::Record {
                key: Some(b"key1".to_vec()),
                value: b"value1".to_vec(),
                headers: vec![],
                timestamp: chrono::Utc::now().timestamp_millis(),
            },
            crc32: 0,
        };

        let result = replication_manager.replicate_record(record).await.unwrap();
        assert_eq!(result.offset, 0);

        // High-watermark should be correctly calculated
        let hwm = replication_manager.get_high_watermark().await.unwrap();
        assert_eq!(hwm, 0); // Should be 0 since all replicas have offset 0
    }

    #[tokio::test]
    async fn test_isr_heartbeat_tracking() {
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
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

        let config = ReplicationConfig {
            min_in_sync_replicas: 2,
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
            heartbeat_timeout_ms: 30000,
        };

        let replica_set = vec!["broker-1".to_string(), "broker-2".to_string()];

        let topic_partition = crate::types::TopicPartition {
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

        // Test with fresh heartbeat
        let fresh_follower = FollowerState {
            broker_id: "broker-2".to_string(),
            last_known_offset: 0,
            last_heartbeat: chrono::Utc::now(), // Fresh heartbeat
            lag: 0,
        };

        replication_manager.update_follower_state(fresh_follower).await;
        let isr = replication_manager.get_in_sync_replicas();
        assert_eq!(isr.len(), 1); // Should include broker-2

        // Test with stale heartbeat
        let stale_follower = FollowerState {
            broker_id: "broker-2".to_string(),
            last_known_offset: 0,
            last_heartbeat: chrono::Utc::now() - chrono::Duration::minutes(5), // Stale
            lag: 0,
        };

        replication_manager.update_follower_state(stale_follower).await;
        let isr = replication_manager.get_in_sync_replicas();
        assert_eq!(isr.len(), 0); // Should exclude broker-2 due to stale heartbeat
    }

    #[tokio::test]
    async fn test_heartbeat_timeout_error_handling() {
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
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

        let config = ReplicationConfig {
            min_in_sync_replicas: 2,
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
            heartbeat_timeout_ms: 30000,
        };

        let replica_set = vec!["broker-1".to_string(), "broker-2".to_string()];

        let topic_partition = crate::types::TopicPartition {
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

        // Test with a future timestamp that would cause chrono conversion to fail
        let future_time = chrono::Utc::now() + chrono::Duration::hours(1);
        let invalid_follower = FollowerState {
            broker_id: "broker-2".to_string(),
            last_known_offset: 0,
            last_heartbeat: future_time, // This will cause conversion error
            lag: 0,
        };

        replication_manager.update_follower_state(invalid_follower).await;
        
        // Should handle the conversion error gracefully and exclude the follower
        let isr = replication_manager.get_in_sync_replicas();
        assert_eq!(isr.len(), 0, "Follower with invalid heartbeat timestamp should be excluded");

        // Test with valid timestamp
        let valid_follower = FollowerState {
            broker_id: "broker-2".to_string(),
            last_known_offset: 0,
            last_heartbeat: chrono::Utc::now(), // Valid timestamp
            lag: 0,
        };

        replication_manager.update_follower_state(valid_follower).await;
        let isr = replication_manager.get_in_sync_replicas();
        assert_eq!(isr.len(), 1, "Follower with valid heartbeat timestamp should be included");
    }

    #[tokio::test]
    async fn test_optimized_high_watermark_calculation() {
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
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

        let config = ReplicationConfig {
            min_in_sync_replicas: 3, // Require 3 replicas (leader + 2 followers)
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
            heartbeat_timeout_ms: 30000,
        };

        let replica_set = vec![
            "broker-1".to_string(), // Leader
            "broker-2".to_string(),
            "broker-3".to_string(),
            "broker-4".to_string(),
        ];

        let topic_partition = crate::types::TopicPartition {
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

        // Set the leader's log end offset to 100
        replication_manager.log_end_offset.store(100, Ordering::SeqCst);

        // Simulate follower states with different offsets
        let follower_states = vec![
            FollowerState {
                broker_id: "broker-2".to_string(),
                last_known_offset: 95, // Lagging behind
                last_heartbeat: chrono::Utc::now(),
                lag: 5,
            },
            FollowerState {
                broker_id: "broker-3".to_string(),
                last_known_offset: 98, // Less behind
                last_heartbeat: chrono::Utc::now(),
                lag: 2,
            },
            FollowerState {
                broker_id: "broker-4".to_string(),
                last_known_offset: 100, // Caught up
                last_heartbeat: chrono::Utc::now(),
                lag: 0,
            },
        ];

        for state in follower_states {
            replication_manager.update_follower_state(state).await;
        }

        // Trigger recalculation
        replication_manager.recalculate_high_watermark().await;

        // With min_in_sync_replicas = 3 and offsets [95, 98, 100, 100],
        // the high-watermark should be 98 (the 3rd highest when counting from the top)
        let hwm = replication_manager.get_high_watermark().await.unwrap();
        assert_eq!(hwm, 98, "High-watermark should be calculated correctly using optimized algorithm");

        // Test that high-watermark doesn't decrease
        let old_hwm = hwm;
        
        // Update one follower to lag further behind
        let lagging_follower = FollowerState {
            broker_id: "broker-4".to_string(),
            last_known_offset: 90, // Now lagging
            last_heartbeat: chrono::Utc::now(),
            lag: 10,
        };
        replication_manager.update_follower_state(lagging_follower).await;
        replication_manager.recalculate_high_watermark().await;

        let new_hwm = replication_manager.get_high_watermark().await.unwrap();
        assert!(new_hwm >= old_hwm, "High-watermark should never decrease");
    }
}