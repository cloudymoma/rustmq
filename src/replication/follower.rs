use crate::{Result, types::*, error::RustMqError, storage::WriteAheadLog};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;

/// Follower-side replication handler that validates leader epochs
pub struct FollowerReplicationHandler {
    /// Current known leader epoch for this partition
    leader_epoch: AtomicU64,
    /// Current known leader ID
    current_leader: RwLock<Option<BrokerId>>,
    /// Topic partition this handler is responsible for
    topic_partition: TopicPartition,
    /// WAL for persisting replicated records
    wal: Arc<dyn WriteAheadLog>,
    /// Current broker ID (this follower)
    broker_id: BrokerId,
}

impl FollowerReplicationHandler {
    pub fn new(
        topic_partition: TopicPartition,
        initial_leader_epoch: u64,
        initial_leader: Option<BrokerId>,
        wal: Arc<dyn WriteAheadLog>,
        broker_id: BrokerId,
    ) -> Self {
        Self {
            leader_epoch: AtomicU64::new(initial_leader_epoch),
            current_leader: RwLock::new(initial_leader),
            topic_partition,
            wal,
            broker_id,
        }
    }

    /// Handle incoming replication request with leader epoch validation
    pub async fn handle_replicate_data(&self, request: ReplicateDataRequest) -> Result<ReplicateDataResponse> {
        // First, validate the leader epoch
        let current_epoch = self.leader_epoch.load(Ordering::SeqCst);
        
        if request.leader_epoch < current_epoch {
            // Stale leader - reject the request
            return Ok(ReplicateDataResponse {
                success: false,
                error_code: 1001, // STALE_LEADER_EPOCH
                error_message: Some(format!(
                    "Stale leader epoch: request epoch {} < current epoch {}",
                    request.leader_epoch, current_epoch
                )),
                follower_state: None,
            });
        }

        // If epoch is higher, update our known epoch and leader
        if request.leader_epoch > current_epoch {
            self.leader_epoch.store(request.leader_epoch, Ordering::SeqCst);
            let mut leader = self.current_leader.write();
            *leader = Some(request.leader_id.clone());
        }

        // Validate topic partition matches
        if request.topic_partition != self.topic_partition {
            return Ok(ReplicateDataResponse {
                success: false,
                error_code: 1002, // WRONG_PARTITION
                error_message: Some(format!(
                    "Wrong partition: expected {:?}, got {:?}",
                    self.topic_partition, request.topic_partition
                )),
                follower_state: None,
            });
        }

        // Process the records
        for record in request.records {
            match self.wal.append(&record).await {
                Ok(_offset) => {
                    // Record appended successfully
                }
                Err(e) => {
                    return Ok(ReplicateDataResponse {
                        success: false,
                        error_code: 1003, // WAL_APPEND_FAILED
                        error_message: Some(format!("WAL append failed: {}", e)),
                        follower_state: None,
                    });
                }
            }
        }

        // Get current WAL offset for lag calculation
        let current_offset = match self.wal.get_end_offset().await {
            Ok(offset) => offset,
            Err(e) => {
                return Ok(ReplicateDataResponse {
                    success: false,
                    error_code: 1004, // WAL_OFFSET_READ_FAILED
                    error_message: Some(format!("Failed to read WAL offset: {}", e)),
                    follower_state: None,
                });
            }
        };

        // Calculate lag - since we just processed records from the leader,
        // and last_offset is the highest offset we wrote, we should be caught up
        // In a real system, this would be calculated based on leader's high watermark
        let lag = 0;

        // Return success with follower state
        Ok(ReplicateDataResponse {
            success: true,
            error_code: 0,
            error_message: None,
            follower_state: Some(FollowerState {
                broker_id: self.broker_id.clone(),
                last_known_offset: current_offset,
                last_heartbeat: chrono::Utc::now(),
                lag,
            }),
        })
    }

    /// Handle incoming heartbeat request with leader epoch validation
    pub async fn handle_heartbeat(&self, request: HeartbeatRequest) -> Result<FollowerState> {
        // Validate the leader epoch
        let current_epoch = self.leader_epoch.load(Ordering::SeqCst);
        
        if request.leader_epoch < current_epoch {
            // Stale leader - return error
            return Err(RustMqError::StaleLeaderEpoch {
                request_epoch: request.leader_epoch,
                current_epoch,
            });
        }

        // If epoch is higher, update our known epoch and leader
        if request.leader_epoch > current_epoch {
            self.leader_epoch.store(request.leader_epoch, Ordering::SeqCst);
            let mut leader = self.current_leader.write();
            *leader = Some(request.leader_id.clone());
        }

        // Validate topic partition matches
        if request.topic_partition != self.topic_partition {
            return Err(RustMqError::InvalidOperation(format!(
                "Wrong partition: expected {:?}, got {:?}",
                self.topic_partition, request.topic_partition
            )));
        }

        // Get current WAL offset
        let current_offset = self.wal.get_end_offset().await?;
        let lag = request.high_watermark.saturating_sub(current_offset);

        Ok(FollowerState {
            broker_id: self.broker_id.clone(),
            last_known_offset: current_offset,
            last_heartbeat: chrono::Utc::now(),
            lag,
        })
    }

    /// Get current leader epoch
    pub fn get_leader_epoch(&self) -> u64 {
        self.leader_epoch.load(Ordering::SeqCst)
    }

    /// Get current leader ID
    pub fn get_current_leader(&self) -> Option<BrokerId> {
        self.current_leader.read().clone()
    }

    /// Update leader epoch and leader ID (called when leadership changes)
    pub fn update_leadership(&self, new_epoch: u64, new_leader: BrokerId) {
        self.leader_epoch.store(new_epoch, Ordering::SeqCst);
        let mut leader = self.current_leader.write();
        *leader = Some(new_leader);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{DirectIOWal, AlignedBufferPool};
    use crate::config::WalConfig;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_follower_rejects_stale_leader_epoch() {
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

        // Create a request with stale epoch
        let stale_request = ReplicateDataRequest {
            leader_epoch: 3, // Lower than current epoch (5)
            topic_partition: topic_partition.clone(),
            records: vec![],
            leader_id: "stale-leader".to_string(),
        };

        let response = handler.handle_replicate_data(stale_request).await.unwrap();
        
        // Should reject the request
        assert!(!response.success);
        assert_eq!(response.error_code, 1001);
        assert!(response.error_message.unwrap().contains("Stale leader epoch"));
    }

    #[tokio::test]
    async fn test_follower_accepts_higher_epoch() {
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

        // Create a request with higher epoch
        let new_request = ReplicateDataRequest {
            leader_epoch: 7, // Higher than current epoch (5)
            topic_partition: topic_partition.clone(),
            records: vec![],
            leader_id: "new-leader".to_string(),
        };

        let response = handler.handle_replicate_data(new_request).await.unwrap();
        
        // Should accept the request and update epoch
        assert!(response.success);
        assert_eq!(handler.get_leader_epoch(), 7);
        assert_eq!(handler.get_current_leader(), Some("new-leader".to_string()));
    }

    #[tokio::test]
    async fn test_heartbeat_stale_epoch_rejection() {
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

        // Create a heartbeat request with stale epoch
        let stale_heartbeat = HeartbeatRequest {
            leader_epoch: 3, // Lower than current epoch (5)
            leader_id: "stale-leader".to_string(),
            topic_partition,
            high_watermark: 100,
        };

        let result = handler.handle_heartbeat(stale_heartbeat).await;
        
        // Should return error for stale epoch
        assert!(result.is_err());
        match result.unwrap_err() {
            RustMqError::StaleLeaderEpoch { request_epoch, current_epoch } => {
                assert_eq!(request_epoch, 3);
                assert_eq!(current_epoch, 5);
            }
            _ => panic!("Expected StaleLeaderEpoch error"),
        }
    }

    #[tokio::test]
    async fn test_lag_calculation_in_heartbeat() {
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

        let topic_partition = TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };

        let handler = FollowerReplicationHandler::new(
            topic_partition.clone(),
            5,
            Some("leader-1".to_string()),
            wal.clone(),
            "follower-1".to_string(),
        );

        // Create a heartbeat request with high watermark
        let heartbeat = HeartbeatRequest {
            leader_epoch: 5,
            leader_id: "leader-1".to_string(),
            topic_partition,
            high_watermark: 100,
        };

        let follower_state = handler.handle_heartbeat(heartbeat).await.unwrap();
        
        // Should calculate lag correctly (high_watermark - current_wal_offset)
        let current_offset = wal.get_end_offset().await.unwrap();
        let expected_lag = 100u64.saturating_sub(current_offset);
        
        assert_eq!(follower_state.lag, expected_lag);
        assert_eq!(follower_state.last_known_offset, current_offset);
        assert_eq!(follower_state.broker_id, "follower-1".to_string());
    }

    #[tokio::test]
    async fn test_wal_offset_retrieval_in_replicate_data() {
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

        let topic_partition = TopicPartition {
            topic: "test-topic".to_string(),
            partition: 0,
        };

        let handler = FollowerReplicationHandler::new(
            topic_partition.clone(),
            5,
            Some("leader-1".to_string()),
            wal.clone(),
            "follower-1".to_string(),
        );

        // Create some test data
        let test_record = WalRecord {
            topic_partition: topic_partition.clone(),
            offset: 0,
            record: Record::new(
                Some(b"test-key".to_vec()),
                b"test-value".to_vec(),
                vec![],
                chrono::Utc::now().timestamp_millis(),
            ),
            crc32: 0,
        };

        let request = ReplicateDataRequest {
            leader_epoch: 5,
            topic_partition: topic_partition.clone(),
            records: vec![test_record],
            leader_id: "leader-1".to_string(),
        };

        let response = handler.handle_replicate_data(request).await.unwrap();
        
        assert!(response.success);
        
        let follower_state = response.follower_state.unwrap();
        let current_offset = wal.get_end_offset().await.unwrap();
        
        // Should return actual WAL offset
        assert_eq!(follower_state.last_known_offset, current_offset);
        assert_eq!(follower_state.broker_id, "follower-1".to_string());
        // Lag should be 0 since we just processed records from leader
        assert_eq!(follower_state.lag, 0);
    }
}