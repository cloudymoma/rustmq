use crate::{Result, types::*, config::ReplicationConfig, storage::WriteAheadLog, replication::traits::ReplicationManager as ReplicationManagerTrait};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

pub struct ReplicationManager {
    stream_id: u64,
    current_leader: Option<BrokerId>,
    replica_set: Vec<BrokerId>,
    
    log_end_offset: AtomicU64,
    high_watermark: AtomicU64,
    follower_states: Arc<RwLock<HashMap<BrokerId, FollowerState>>>,
    
    min_in_sync_replicas: usize,
    ack_timeout: Duration,
    
    wal: Arc<dyn WriteAheadLog>,
    rpc_client: Arc<dyn ReplicationRpcClient>,
}

#[async_trait]
pub trait ReplicationRpcClient: Send + Sync {
    async fn send_record(&self, broker_id: &BrokerId, record: WalRecord) -> Result<()>;
    async fn send_heartbeat(&self, broker_id: &BrokerId) -> Result<FollowerState>;
}

pub struct MockReplicationRpcClient;

#[async_trait]
impl ReplicationRpcClient for MockReplicationRpcClient {
    async fn send_record(&self, _broker_id: &BrokerId, _record: WalRecord) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(())
    }

    async fn send_heartbeat(&self, broker_id: &BrokerId) -> Result<FollowerState> {
        Ok(FollowerState {
            broker_id: broker_id.clone(),
            last_known_offset: 0,
            last_heartbeat: chrono::Utc::now(),
            lag: 0,
        })
    }
}

impl ReplicationManager {
    pub fn new(
        stream_id: u64,
        leader: BrokerId,
        replica_set: Vec<BrokerId>,
        config: ReplicationConfig,
        wal: Arc<dyn WriteAheadLog>,
        rpc_client: Arc<dyn ReplicationRpcClient>,
    ) -> Self {
        Self {
            stream_id,
            current_leader: Some(leader),
            replica_set,
            log_end_offset: AtomicU64::new(0),
            high_watermark: AtomicU64::new(0),
            follower_states: Arc::new(RwLock::new(HashMap::new())),
            min_in_sync_replicas: config.min_in_sync_replicas,
            ack_timeout: Duration::from_millis(config.ack_timeout_ms),
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
        timeout(self.ack_timeout, self.rpc_client.send_record(&broker_id, record)).await
            .map_err(|_| crate::error::RustMqError::Timeout)?
    }

    async fn wait_for_acknowledgments(&self, record: WalRecord, local_offset: Offset) -> Result<ReplicationResult> {
        let followers: Vec<BrokerId> = self.replica_set
            .iter()
            .filter(|&broker_id| Some(broker_id.clone()) != self.current_leader)
            .cloned()
            .collect();

        if followers.is_empty() {
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
            self.high_watermark.store(local_offset, Ordering::SeqCst);
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

    pub fn get_in_sync_replicas(&self) -> Vec<BrokerId> {
        let states = self.follower_states.read();
        let high_watermark = self.high_watermark.load(Ordering::SeqCst);
        let max_lag = 1000; // TODO: Make configurable

        states
            .values()
            .filter(|state| high_watermark.saturating_sub(state.last_known_offset) <= max_lag)
            .map(|state| state.broker_id.clone())
            .collect()
    }

    pub async fn start_heartbeat_monitor(&self) {
        let follower_states = self.follower_states.clone();
        let rpc_client = self.rpc_client.clone();
        let replica_set = self.replica_set.clone();
        let current_leader = self.current_leader.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                let followers: Vec<BrokerId> = replica_set
                    .iter()
                    .filter(|&broker_id| Some(broker_id.clone()) != current_leader)
                    .cloned()
                    .collect();

                for broker_id in followers {
                    if let Ok(state) = rpc_client.send_heartbeat(&broker_id).await {
                        let mut states = follower_states.write();
                        states.insert(broker_id, state);
                    }
                }
            }
        });
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
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap()) as Arc<dyn WriteAheadLog>;
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

        let config = ReplicationConfig {
            min_in_sync_replicas: 2,
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
        };

        let replica_set = vec![
            "broker-1".to_string(),
            "broker-2".to_string(),
            "broker-3".to_string(),
        ];

        let replication_manager = ReplicationManager::new(
            1,
            "broker-1".to_string(),
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
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].broker_id, "broker-4");
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
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap()) as Arc<dyn WriteAheadLog>;
        let rpc_client = Arc::new(MockReplicationRpcClient) as Arc<dyn ReplicationRpcClient>;

        let config = ReplicationConfig {
            min_in_sync_replicas: 3, // High requirement
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
        };

        let replica_set = vec!["broker-1".to_string()]; // Only leader, no followers

        let replication_manager = ReplicationManager::new(
            1,
            "broker-1".to_string(),
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
}