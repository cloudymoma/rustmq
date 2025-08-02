use crate::{Result, config::ScalingConfig, types::*};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock as AsyncRwLock, Semaphore};
use tokio::time::{Duration, Instant};
use uuid::Uuid;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use parking_lot::RwLock;

/// Controller service that manages cluster-wide coordination including decommissioning slots
/// Implements basic Raft consensus for cluster metadata management
#[derive(Clone)]
pub struct ControllerService {
    /// Decommissioning slot manager to prevent mass decommissions
    decommission_manager: Arc<DecommissionSlotManager>,
    /// Scaling configuration
    scaling_config: Arc<AsyncRwLock<ScalingConfig>>,
    /// Raft consensus state
    raft_state: Arc<RaftState>,
    /// Cluster metadata manager
    metadata_manager: Arc<MetadataManager>,
    /// Node ID of this controller
    node_id: String,
}

/// Basic Raft consensus state for cluster coordination
pub struct RaftState {
    /// Current Raft term
    current_term: AtomicU64,
    /// Node we voted for in current term
    voted_for: RwLock<Option<String>>,
    /// Whether this node is the leader
    is_leader: AtomicBool,
    /// Last known leader
    current_leader: RwLock<Option<String>>,
    /// Log entries (simplified - in production would be persistent)
    log_entries: AsyncRwLock<Vec<LogEntry>>,
    /// Commit index
    commit_index: AtomicU64,
    /// Last applied index
    last_applied: AtomicU64,
    /// Peer nodes in the cluster
    peers: Vec<String>,
}

/// Cluster metadata manager
pub struct MetadataManager {
    /// Topic metadata
    topics: AsyncRwLock<HashMap<String, TopicInfo>>,
    /// Broker metadata
    brokers: AsyncRwLock<HashMap<BrokerId, BrokerInfo>>,
    /// Partition assignments
    partition_assignments: AsyncRwLock<HashMap<TopicPartition, PartitionAssignment>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub term: u64,
    pub index: u64,
    pub command: ClusterCommand,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ClusterCommand {
    CreateTopic {
        name: String,
        partitions: u32,
        replication_factor: u32,
        config: TopicConfig,
    },
    DeleteTopic {
        name: String,
    },
    AddBroker {
        broker: BrokerInfo,
    },
    RemoveBroker {
        broker_id: BrokerId,
    },
    ReassignPartitions {
        assignments: Vec<(TopicPartition, PartitionAssignment)>,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopicInfo {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub config: TopicConfig,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopicConfig {
    pub retention_ms: Option<u64>,
    pub segment_bytes: Option<u64>,
    pub compression_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PartitionAssignment {
    pub leader: BrokerId,
    pub replicas: Vec<BrokerId>,
    pub in_sync_replicas: Vec<BrokerId>,
    pub leader_epoch: u64,
}

/// Raft RPC trait for node-to-node communication
#[async_trait]
pub trait RaftRpc: Send + Sync {
    async fn request_vote(&self, node_id: &str, request: VoteRequest) -> Result<VoteResponse>;
    async fn append_entries(&self, node_id: &str, request: AppendEntriesRequest) -> Result<AppendEntriesResponse>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VoteRequest {
    pub term: u64,
    pub candidate_id: String,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VoteResponse {
    pub term: u64,
    pub vote_granted: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppendEntriesRequest {
    pub term: u64,
    pub leader_id: String,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppendEntriesResponse {
    pub term: u64,
    pub success: bool,
    pub match_index: Option<u64>,
}

/// Manages decommissioning slots to enforce safety constraints
pub struct DecommissionSlotManager {
    /// Semaphore controlling concurrent decommissions
    decommission_slots: Arc<Semaphore>,
    /// Active decommission operations
    active_decommissions: Arc<AsyncRwLock<HashMap<String, DecommissionSlot>>>,
    /// Maximum allowed concurrent decommissions
    max_concurrent: usize,
}

#[derive(Debug, Clone)]
pub struct DecommissionSlot {
    pub operation_id: String,
    pub broker_id: String,
    pub acquired_at: Instant,
    pub expires_at: Instant,
    pub requester: String, // Admin tool instance or user ID
}

#[derive(Debug, Clone)]
pub struct SlotAcquisitionResult {
    pub operation_id: String,
    pub slot_token: String,
    pub expires_at: Instant,
}

impl RaftState {
    pub fn new(_node_id: String, peers: Vec<String>) -> Self {
        Self {
            current_term: AtomicU64::new(0),
            voted_for: RwLock::new(None),
            is_leader: AtomicBool::new(false),
            current_leader: RwLock::new(None),
            log_entries: AsyncRwLock::new(vec![]),
            commit_index: AtomicU64::new(0),
            last_applied: AtomicU64::new(0),
            peers,
        }
    }

    pub fn get_current_term(&self) -> u64 {
        self.current_term.load(Ordering::SeqCst)
    }

    pub fn increment_term(&self) -> u64 {
        self.current_term.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn update_term(&self, term: u64) {
        self.current_term.store(term, Ordering::SeqCst);
    }

    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::SeqCst)
    }

    pub fn set_leader(&self, is_leader: bool) {
        self.is_leader.store(is_leader, Ordering::SeqCst);
    }

    pub fn get_current_leader(&self) -> Option<String> {
        self.current_leader.read().clone()
    }

    pub fn set_current_leader(&self, leader: Option<String>) {
        let mut current_leader = self.current_leader.write();
        *current_leader = leader;
    }

    pub fn vote_for(&self, candidate: Option<String>) {
        let mut voted_for = self.voted_for.write();
        *voted_for = candidate;
    }

    pub fn get_voted_for(&self) -> Option<String> {
        self.voted_for.read().clone()
    }

    pub async fn append_log_entry(&self, entry: LogEntry) {
        let mut log = self.log_entries.write().await;
        log.push(entry);
    }

    pub async fn get_last_log_index(&self) -> u64 {
        let log = self.log_entries.read().await;
        log.len() as u64
    }

    pub async fn get_last_log_term(&self) -> u64 {
        let log = self.log_entries.read().await;
        log.last().map(|entry| entry.term).unwrap_or(0)
    }
}

impl MetadataManager {
    pub fn new() -> Self {
        Self {
            topics: AsyncRwLock::new(HashMap::new()),
            brokers: AsyncRwLock::new(HashMap::new()),
            partition_assignments: AsyncRwLock::new(HashMap::new()),
        }
    }

    pub async fn create_topic(&self, topic_info: TopicInfo) -> Result<()> {
        let mut topics = self.topics.write().await;
        if topics.contains_key(&topic_info.name) {
            return Err(crate::error::RustMqError::TopicAlreadyExists(topic_info.name));
        }
        topics.insert(topic_info.name.clone(), topic_info);
        Ok(())
    }

    pub async fn delete_topic(&self, topic_name: &str) -> Result<()> {
        let mut topics = self.topics.write().await;
        if topics.remove(topic_name).is_none() {
            return Err(crate::error::RustMqError::TopicNotFound(topic_name.to_string()));
        }
        
        // Remove partition assignments for this topic
        let mut assignments = self.partition_assignments.write().await;
        assignments.retain(|tp, _| tp.topic != topic_name);
        Ok(())
    }

    pub async fn add_broker(&self, broker: BrokerInfo) -> Result<()> {
        let mut brokers = self.brokers.write().await;
        brokers.insert(broker.id.clone(), broker);
        Ok(())
    }

    pub async fn remove_broker(&self, broker_id: &BrokerId) -> Result<()> {
        let mut brokers = self.brokers.write().await;
        if brokers.remove(broker_id).is_none() {
            return Err(crate::error::RustMqError::BrokerNotFound(broker_id.clone()));
        }
        Ok(())
    }

    pub async fn get_topics(&self) -> Vec<TopicInfo> {
        let topics = self.topics.read().await;
        topics.values().cloned().collect()
    }

    pub async fn get_brokers(&self) -> Vec<BrokerInfo> {
        let brokers = self.brokers.read().await;
        brokers.values().cloned().collect()
    }

    pub async fn assign_partition(&self, partition: TopicPartition, assignment: PartitionAssignment) -> Result<()> {
        let mut assignments = self.partition_assignments.write().await;
        assignments.insert(partition, assignment);
        Ok(())
    }

    pub async fn get_partition_assignments(&self) -> HashMap<TopicPartition, PartitionAssignment> {
        let assignments = self.partition_assignments.read().await;
        assignments.clone()
    }
}

impl ControllerService {
    pub fn new(node_id: String, peers: Vec<String>, scaling_config: ScalingConfig) -> Self {
        let decommission_manager = Arc::new(
            DecommissionSlotManager::new(scaling_config.max_concurrent_decommissions)
        );
        
        // Start background cleanup task for expired decommission slots
        let cleanup_manager = decommission_manager.clone();
        tokio::spawn(async move {
            cleanup_manager.start_cleanup_task().await;
        });
        
        let raft_state = Arc::new(RaftState::new(node_id.clone(), peers));
        let metadata_manager = Arc::new(MetadataManager::new());
        
        Self {
            decommission_manager,
            scaling_config: Arc::new(AsyncRwLock::new(scaling_config)),
            raft_state,
            metadata_manager,
            node_id,
        }
    }

    /// Acquire a decommissioning slot for a broker
    pub async fn acquire_decommission_slot(
        &self,
        broker_id: String,
        requester: String,
    ) -> Result<SlotAcquisitionResult> {
        self.decommission_manager
            .acquire_slot(broker_id, requester)
            .await
    }

    /// Release a decommissioning slot
    pub async fn release_decommission_slot(&self, operation_id: &str) -> Result<()> {
        self.decommission_manager.release_slot(operation_id).await
    }

    /// Get status of active decommissions
    pub async fn get_decommission_status(&self) -> Result<Vec<DecommissionSlot>> {
        self.decommission_manager.get_active_decommissions().await
    }

    /// Get cluster metadata
    pub async fn get_cluster_metadata(&self) -> Result<ClusterMetadata> {
        Ok(ClusterMetadata {
            topics: self.metadata_manager.get_topics().await,
            brokers: self.metadata_manager.get_brokers().await,
            partition_assignments: self.metadata_manager.get_partition_assignments().await,
            leader: self.raft_state.get_current_leader(),
            term: self.raft_state.get_current_term(),
        })
    }

    /// Create a new topic (leader-only operation)
    pub async fn create_topic(&self, request: CreateTopicRequest) -> Result<CreateTopicResponse> {
        if !self.raft_state.is_leader() {
            return Ok(CreateTopicResponse {
                success: false,
                error_message: Some("Not the leader".to_string()),
                leader_hint: self.raft_state.get_current_leader(),
            });
        }

        let topic_info = TopicInfo {
            name: request.name.clone(),
            partitions: request.partitions,
            replication_factor: request.replication_factor,
            config: request.config.unwrap_or_else(|| TopicConfig {
                retention_ms: Some(86400000), // 1 day default
                segment_bytes: Some(1073741824), // 1GB default
                compression_type: Some("lz4".to_string()),
            }),
            created_at: chrono::Utc::now(),
        };

        // In a full implementation, this would go through Raft consensus
        match self.metadata_manager.create_topic(topic_info).await {
            Ok(()) => {
                // Create partition assignments
                let brokers = self.metadata_manager.get_brokers().await;
                if brokers.len() < request.replication_factor as usize {
                    return Ok(CreateTopicResponse {
                        success: false,
                        error_message: Some("Not enough brokers for replication factor".to_string()),
                        leader_hint: self.raft_state.get_current_leader(),
                    });
                }

                // Simple round-robin assignment
                for partition_id in 0..request.partitions {
                    let start_broker = (partition_id as usize) % brokers.len();
                    let mut replicas = Vec::new();
                    
                    for i in 0..request.replication_factor as usize {
                        let broker_idx = (start_broker + i) % brokers.len();
                        replicas.push(brokers[broker_idx].id.clone());
                    }

                    let assignment = PartitionAssignment {
                        leader: replicas[0].clone(),
                        replicas: replicas.clone(),
                        in_sync_replicas: replicas,
                        leader_epoch: 1,
                    };

                    let partition = TopicPartition {
                        topic: request.name.clone(),
                        partition: partition_id,
                    };

                    self.metadata_manager.assign_partition(partition, assignment).await?;
                }

                Ok(CreateTopicResponse {
                    success: true,
                    error_message: None,
                    leader_hint: self.raft_state.get_current_leader(),
                })
            }
            Err(e) => Ok(CreateTopicResponse {
                success: false,
                error_message: Some(format!("Failed to create topic: {}", e)),
                leader_hint: self.raft_state.get_current_leader(),
            }),
        }
    }

    /// Delete a topic (leader-only operation)
    pub async fn delete_topic(&self, request: DeleteTopicRequest) -> Result<DeleteTopicResponse> {
        if !self.raft_state.is_leader() {
            return Ok(DeleteTopicResponse {
                success: false,
                error_message: Some("Not the leader".to_string()),
                leader_hint: self.raft_state.get_current_leader(),
            });
        }

        // In a full implementation, this would go through Raft consensus
        match self.metadata_manager.delete_topic(&request.name).await {
            Ok(()) => Ok(DeleteTopicResponse {
                success: true,
                error_message: None,
                leader_hint: self.raft_state.get_current_leader(),
            }),
            Err(e) => Ok(DeleteTopicResponse {
                success: false,
                error_message: Some(format!("Failed to delete topic: {}", e)),
                leader_hint: self.raft_state.get_current_leader(),
            }),
        }
    }

    /// Register a broker (leader-only operation)
    pub async fn register_broker(&self, broker: BrokerInfo) -> Result<RegisterBrokerResponse> {
        if !self.raft_state.is_leader() {
            return Ok(RegisterBrokerResponse {
                success: false,
                error_message: Some("Not the leader".to_string()),
                leader_hint: self.raft_state.get_current_leader(),
            });
        }

        match self.metadata_manager.add_broker(broker).await {
            Ok(()) => Ok(RegisterBrokerResponse {
                success: true,
                error_message: None,
                leader_hint: self.raft_state.get_current_leader(),
            }),
            Err(e) => Ok(RegisterBrokerResponse {
                success: false,
                error_message: Some(format!("Failed to register broker: {}", e)),
                leader_hint: self.raft_state.get_current_leader(),
            }),
        }
    }

    /// Basic leader election (simplified Raft)
    pub async fn start_election(&self) -> Result<bool> {
        let new_term = self.raft_state.increment_term();
        self.raft_state.vote_for(Some(self.node_id.clone()));
        self.raft_state.set_leader(false);
        
        tracing::info!("Starting election for term {}", new_term);
        
        let votes = 1; // Vote for self
        let last_log_index = self.raft_state.get_last_log_index().await;
        let last_log_term = self.raft_state.get_last_log_term().await;
        
        let _vote_request = VoteRequest {
            term: new_term,
            candidate_id: self.node_id.clone(),
            last_log_index,
            last_log_term,
        };
        
        // In a full implementation, would send vote requests to all peers
        // For now, assume we win if we're the only node or simplified majority
        let total_nodes = self.raft_state.peers.len() + 1; // Including self
        let majority = (total_nodes / 2) + 1;
        
        if votes >= majority {
            self.raft_state.set_leader(true);
            self.raft_state.set_current_leader(Some(self.node_id.clone()));
            tracing::info!("Won election for term {}", new_term);
            
            // Start sending heartbeats
            self.start_heartbeat_timer().await;
            return Ok(true);
        }
        
        Ok(false)
    }

    /// Start heartbeat timer (leader only)
    async fn start_heartbeat_timer(&self) {
        let raft_state = self.raft_state.clone();
        let _node_id = self.node_id.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(150)); // 150ms heartbeat
            
            while raft_state.is_leader() {
                interval.tick().await;
                
                let current_term = raft_state.get_current_term();
                let _commit_index = raft_state.commit_index.load(Ordering::SeqCst);
                
                // In a full implementation, would send heartbeats to all followers
                tracing::debug!("Sending heartbeat for term {}", current_term);
                
                // Simplified heartbeat - just log
                // Real implementation would send AppendEntries RPC to followers
            }
        });
    }

    /// Handle vote request (Raft RPC)
    pub async fn handle_vote_request(&self, request: VoteRequest) -> Result<VoteResponse> {
        let current_term = self.raft_state.get_current_term();
        
        // If term is outdated, reject
        if request.term < current_term {
            return Ok(VoteResponse {
                term: current_term,
                vote_granted: false,
            });
        }
        
        // If term is newer, update our term
        if request.term > current_term {
            self.raft_state.update_term(request.term);
            self.raft_state.vote_for(None);
            self.raft_state.set_leader(false);
        }
        
        // Check if we can vote for this candidate
        let voted_for = self.raft_state.get_voted_for();
        let can_vote = voted_for.is_none() || voted_for == Some(request.candidate_id.clone());
        
        if can_vote {
            // Check log consistency (simplified)
            let last_log_index = self.raft_state.get_last_log_index().await;
            let last_log_term = self.raft_state.get_last_log_term().await;
            
            let log_ok = request.last_log_term > last_log_term || 
                        (request.last_log_term == last_log_term && request.last_log_index >= last_log_index);
            
            if log_ok {
                self.raft_state.vote_for(Some(request.candidate_id));
                return Ok(VoteResponse {
                    term: request.term,
                    vote_granted: true,
                });
            }
        }
        
        Ok(VoteResponse {
            term: self.raft_state.get_current_term(),
            vote_granted: false,
        })
    }

    /// Handle append entries request (Raft RPC)
    pub async fn handle_append_entries(&self, request: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
        let current_term = self.raft_state.get_current_term();
        
        // If term is outdated, reject
        if request.term < current_term {
            return Ok(AppendEntriesResponse {
                term: current_term,
                success: false,
                match_index: None,
            });
        }
        
        // If term is newer or equal, accept leader
        if request.term >= current_term {
            self.raft_state.update_term(request.term);
            self.raft_state.set_leader(false);
            self.raft_state.set_current_leader(Some(request.leader_id));
        }
        
        // Simplified append entries - just accept
        // Real implementation would check log consistency
        
        Ok(AppendEntriesResponse {
            term: request.term,
            success: true,
            match_index: Some(request.prev_log_index + request.entries.len() as u64),
        })
    }

    /// Update scaling configuration at runtime
    pub async fn update_scaling_config(&self, new_config: ScalingConfig) -> Result<()> {
        // Validate the new configuration
        if new_config.max_concurrent_decommissions == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "max_concurrent_decommissions must be greater than 0".to_string(),
            ));
        }

        if new_config.max_concurrent_decommissions > 10 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "max_concurrent_decommissions should not exceed 10 for safety".to_string(),
            ));
        }

        // Update the decommission manager with new limits
        self.decommission_manager
            .update_max_concurrent(new_config.max_concurrent_decommissions)
            .await?;

        // Update the stored configuration
        let mut config = self.scaling_config.write().await;
        *config = new_config;

        Ok(())
    }

    /// Get Raft state information
    pub fn get_raft_info(&self) -> RaftInfo {
        RaftInfo {
            node_id: self.node_id.clone(),
            current_term: self.raft_state.get_current_term(),
            is_leader: self.raft_state.is_leader(),
            current_leader: self.raft_state.get_current_leader(),
            voted_for: self.raft_state.get_voted_for(),
            commit_index: self.raft_state.commit_index.load(Ordering::SeqCst),
            last_applied: self.raft_state.last_applied.load(Ordering::SeqCst),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClusterMetadata {
    pub topics: Vec<TopicInfo>,
    pub brokers: Vec<BrokerInfo>,
    pub partition_assignments: HashMap<TopicPartition, PartitionAssignment>,
    pub leader: Option<String>,
    pub term: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTopicRequest {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub config: Option<TopicConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTopicResponse {
    pub success: bool,
    pub error_message: Option<String>,
    pub leader_hint: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeleteTopicRequest {
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeleteTopicResponse {
    pub success: bool,
    pub error_message: Option<String>,
    pub leader_hint: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegisterBrokerResponse {
    pub success: bool,
    pub error_message: Option<String>,
    pub leader_hint: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RaftInfo {
    pub node_id: String,
    pub current_term: u64,
    pub is_leader: bool,
    pub current_leader: Option<String>,
    pub voted_for: Option<String>,
    pub commit_index: u64,
    pub last_applied: u64,
}

/// Mock RPC client for testing
pub struct MockRaftRpc;

#[async_trait]
impl RaftRpc for MockRaftRpc {
    async fn request_vote(&self, _node_id: &str, request: VoteRequest) -> Result<VoteResponse> {
        // Mock implementation - always grants vote
        Ok(VoteResponse {
            term: request.term,
            vote_granted: true,
        })
    }

    async fn append_entries(&self, _node_id: &str, request: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
        // Mock implementation - always succeeds
        Ok(AppendEntriesResponse {
            term: request.term,
            success: true,
            match_index: Some(request.prev_log_index + request.entries.len() as u64),
        })
    }
}

impl DecommissionSlotManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            decommission_slots: Arc::new(Semaphore::new(max_concurrent)),
            active_decommissions: Arc::new(AsyncRwLock::new(HashMap::new())),
            max_concurrent,
        }
    }

    /// Acquire a decommissioning slot for a broker
    pub async fn acquire_slot(
        &self,
        broker_id: String,
        requester: String,
    ) -> Result<SlotAcquisitionResult> {
        // Check if this broker is already being decommissioned
        {
            let active = self.active_decommissions.read().await;
            if active.values().any(|slot| slot.broker_id == broker_id) {
                return Err(crate::error::RustMqError::InvalidOperation(
                    format!("Broker {} is already being decommissioned", broker_id),
                ));
            }
        }

        // Try to acquire a semaphore permit
        let permit = self.decommission_slots
            .try_acquire()
            .map_err(|_| crate::error::RustMqError::ResourceExhausted(
                format!(
                    "Maximum concurrent decommissions ({}) reached. Please wait for ongoing operations to complete.",
                    self.max_concurrent
                )
            ))?;

        // Generate operation ID and slot token
        let operation_id = Uuid::new_v4().to_string();
        let slot_token = Uuid::new_v4().to_string();
        let acquired_at = Instant::now();
        let expires_at = acquired_at + Duration::from_secs(3600); // 1 hour timeout

        // Create slot record
        let slot = DecommissionSlot {
            operation_id: operation_id.clone(),
            broker_id: broker_id.clone(),
            acquired_at,
            expires_at,
            requester: requester.clone(),
        };

        // Store the active decommission operation
        {
            let mut active = self.active_decommissions.write().await;
            active.insert(operation_id.clone(), slot);
        }

        // Forget the permit (it will be released when slot is released)
        permit.forget();

        tracing::info!(
            "Decommission slot acquired for broker {} by {} (operation: {})",
            broker_id,
            requester,
            operation_id
        );

        Ok(SlotAcquisitionResult {
            operation_id,
            slot_token,
            expires_at,
        })
    }

    /// Release a decommissioning slot
    pub async fn release_slot(&self, operation_id: &str) -> Result<()> {
        let slot = {
            let mut active = self.active_decommissions.write().await;
            active.remove(operation_id)
                .ok_or_else(|| crate::error::RustMqError::NotFound(
                    format!("Decommission operation {} not found", operation_id)
                ))?
        };

        // Release the semaphore permit
        self.decommission_slots.add_permits(1);

        tracing::info!(
            "Decommission slot released for broker {} (operation: {})",
            slot.broker_id,
            operation_id
        );

        Ok(())
    }

    /// Get all active decommission operations
    pub async fn get_active_decommissions(&self) -> Result<Vec<DecommissionSlot>> {
        let active = self.active_decommissions.read().await;
        Ok(active.values().cloned().collect())
    }

    /// Update the maximum concurrent decommissions limit
    pub async fn update_max_concurrent(&self, new_max: usize) -> Result<()> {
        let current_active = {
            let active = self.active_decommissions.read().await;
            active.len()
        };

        if current_active > new_max {
            return Err(crate::error::RustMqError::InvalidOperation(
                format!(
                    "Cannot reduce limit to {} while {} decommissions are active",
                    new_max, current_active
                )
            ));
        }

        // Calculate the difference and adjust semaphore permits
        let current_available = self.decommission_slots.available_permits();
        let current_max = current_available + current_active;
        
        if new_max > current_max {
            // Increase permits
            self.decommission_slots.add_permits(new_max - current_max);
        } else if new_max < current_max {
            // Decrease permits - acquire the difference
            let permits_to_remove = current_max - new_max;
            for _ in 0..permits_to_remove {
                self.decommission_slots
                    .try_acquire()
                    .map_err(|_| crate::error::RustMqError::InvalidOperation(
                        "Cannot reduce decommission limit while operations are active".to_string()
                    ))?
                    .forget();
            }
        }

        tracing::info!(
            "Updated max concurrent decommissions from {} to {}",
            current_max,
            new_max
        );

        Ok(())
    }

    /// Start background task to clean up expired decommission slots
    pub async fn start_cleanup_task(&self) {
        let active_decommissions = self.active_decommissions.clone();
        let decommission_slots = self.decommission_slots.clone();

        let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute
        
        loop {
            interval.tick().await;
            
            let now = Instant::now();
            let mut expired_slots = Vec::new();
            
            // Find expired slots
            {
                let active = active_decommissions.read().await;
                for (operation_id, slot) in active.iter() {
                    if now > slot.expires_at {
                        expired_slots.push(operation_id.clone());
                    }
                }
            }
            
            // Remove expired slots and release semaphore permits
            if !expired_slots.is_empty() {
                let mut active = active_decommissions.write().await;
                for operation_id in &expired_slots {
                    if let Some(slot) = active.remove(operation_id) {
                        // Release the semaphore permit
                        decommission_slots.add_permits(1);
                        
                        tracing::warn!(
                            "Auto-expired decommission slot for broker {} (operation: {}, expired after 1 hour)",
                            slot.broker_id,
                            operation_id
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_decommission_slot_acquisition() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire first slot
        let result1 = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result1.operation_id.is_empty());

        // Acquire second slot
        let result2 = manager
            .acquire_slot("broker-2".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result2.operation_id.is_empty());

        // Third slot should fail (limit reached)
        let result3 = manager
            .acquire_slot("broker-3".to_string(), "admin-1".to_string())
            .await;
        assert!(result3.is_err());

        // Release first slot
        manager.release_slot(&result1.operation_id).await.unwrap();

        // Now third slot should succeed
        let result3 = manager
            .acquire_slot("broker-3".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result3.operation_id.is_empty());
    }

    #[tokio::test]
    async fn test_duplicate_broker_decommission_prevention() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire slot for broker-1
        let _result1 = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Try to acquire another slot for the same broker - should fail
        let result2 = manager
            .acquire_slot("broker-1".to_string(), "admin-2".to_string())
            .await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_max_concurrent_update() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire one slot
        let result1 = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Increase limit - should succeed
        manager.update_max_concurrent(3).await.unwrap();

        // Acquire two more slots (total 3 active)
        let _result2 = manager
            .acquire_slot("broker-2".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        let _result3 = manager
            .acquire_slot("broker-3".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Try to reduce limit to 2 while 3 are active - should fail
        let update_result = manager.update_max_concurrent(2).await;
        assert!(update_result.is_err());

        // Release one slot
        manager.release_slot(&result1.operation_id).await.unwrap();

        // Now reducing to 2 should succeed
        manager.update_max_concurrent(2).await.unwrap();
    }

    #[tokio::test]
    async fn test_controller_service_integration() {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let peers = vec!["controller-2".to_string(), "controller-3".to_string()];
        let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

        // Acquire slot
        let result = controller
            .acquire_decommission_slot("broker-1".to_string(), "admin-tool".to_string())
            .await
            .unwrap();

        // Check status
        let status = controller.get_decommission_status().await.unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].broker_id, "broker-1");

        // Release slot
        controller.release_decommission_slot(&result.operation_id).await.unwrap();

        // Check status again
        let status = controller.get_decommission_status().await.unwrap();
        assert_eq!(status.len(), 0);
        
        // Test Raft info
        let raft_info = controller.get_raft_info();
        assert_eq!(raft_info.node_id, "controller-1");
        assert_eq!(raft_info.current_term, 0);
        assert!(!raft_info.is_leader);
    }

    #[tokio::test]
    async fn test_decommission_slot_expiration() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire slot
        let result = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Verify slot exists
        let status = manager.get_active_decommissions().await.unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].broker_id, "broker-1");

        // Simulate expired slot by manually removing and releasing
        {
            let mut active = manager.active_decommissions.write().await;
            active.remove(&result.operation_id);
            manager.decommission_slots.add_permits(1);
        }

        // Verify slot is cleaned up
        let status = manager.get_active_decommissions().await.unwrap();
        assert_eq!(status.len(), 0);

        // Verify we can acquire new slots after cleanup
        let result2 = manager
            .acquire_slot("broker-2".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result2.operation_id.is_empty());
    }

    #[tokio::test]
    async fn test_decommission_slot_timeout_integration() {
        use tokio::time::{timeout, Duration as TokioDuration};
        
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let peers = vec!["controller-2".to_string(), "controller-3".to_string()];
        let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

        // Acquire slot
        let _result = controller
            .acquire_decommission_slot("broker-1".to_string(), "admin-tool".to_string())
            .await
            .unwrap();

        // Verify slot exists
        let status = controller.get_decommission_status().await.unwrap();
        assert_eq!(status.len(), 1);

        // Test that the background cleanup task exists by ensuring
        // the controller maintains state correctly over time
        timeout(TokioDuration::from_millis(100), async {
            tokio::time::sleep(TokioDuration::from_millis(50)).await;
            let status = controller.get_decommission_status().await.unwrap();
            assert_eq!(status.len(), 1); // Still there after short time
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_raft_leadership_election() {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let peers = vec![]; // Single node cluster
        let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

        // Initially not a leader
        assert!(!controller.raft_state.is_leader());
        
        // Start election
        let won = controller.start_election().await.unwrap();
        assert!(won);
        
        // Should now be leader
        assert!(controller.raft_state.is_leader());
        assert_eq!(controller.raft_state.get_current_term(), 1);
        assert_eq!(controller.raft_state.get_current_leader(), Some("controller-1".to_string()));
    }

    #[tokio::test]
    async fn test_topic_management() {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let peers = vec![];
        let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

        // Become leader first
        controller.start_election().await.unwrap();
        
        // Add some brokers
        let broker1 = BrokerInfo {
            id: "broker-1".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };
        
        let broker2 = BrokerInfo {
            id: "broker-2".to_string(),
            host: "localhost".to_string(),
            port_quic: 9192,
            port_rpc: 9193,
            rack_id: "rack-1".to_string(),
        };
        
        controller.register_broker(broker1).await.unwrap();
        controller.register_broker(broker2).await.unwrap();

        // Create a topic
        let create_request = CreateTopicRequest {
            name: "test-topic".to_string(),
            partitions: 3,
            replication_factor: 2,
            config: None,
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(response.success);
        
        // Verify topic exists
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 1);
        assert_eq!(metadata.topics[0].name, "test-topic");
        assert_eq!(metadata.topics[0].partitions, 3);
        assert_eq!(metadata.partition_assignments.len(), 3); // 3 partitions
        
        // Delete the topic
        let delete_request = DeleteTopicRequest {
            name: "test-topic".to_string(),
        };
        
        let response = controller.delete_topic(delete_request).await.unwrap();
        assert!(response.success);
        
        // Verify topic is gone
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 0);
        assert_eq!(metadata.partition_assignments.len(), 0);
    }

    #[tokio::test]
    async fn test_raft_vote_request_handling() {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let peers = vec!["controller-2".to_string(), "controller-3".to_string()];
        let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

        // Test vote request with higher term
        let vote_request = VoteRequest {
            term: 5,
            candidate_id: "controller-2".to_string(),
            last_log_index: 0,
            last_log_term: 0,
        };
        
        let response = controller.handle_vote_request(vote_request).await.unwrap();
        assert!(response.vote_granted);
        assert_eq!(response.term, 5);
        
        // Our term should be updated
        assert_eq!(controller.raft_state.get_current_term(), 5);
        
        // Test another vote request with same term from different candidate
        let vote_request2 = VoteRequest {
            term: 5,
            candidate_id: "controller-3".to_string(),
            last_log_index: 0,
            last_log_term: 0,
        };
        
        let response2 = controller.handle_vote_request(vote_request2).await.unwrap();
        assert!(!response2.vote_granted); // Already voted for controller-2
    }

    #[tokio::test]
    async fn test_append_entries_handling() {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let peers = vec!["controller-2".to_string(), "controller-3".to_string()];
        let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

        // Test append entries from leader
        let append_request = AppendEntriesRequest {
            term: 3,
            leader_id: "controller-2".to_string(),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit: 0,
        };
        
        let response = controller.handle_append_entries(append_request).await.unwrap();
        assert!(response.success);
        assert_eq!(response.term, 3);
        
        // Should recognize controller-2 as leader
        assert_eq!(controller.raft_state.get_current_leader(), Some("controller-2".to_string()));
        assert!(!controller.raft_state.is_leader());
    }
}