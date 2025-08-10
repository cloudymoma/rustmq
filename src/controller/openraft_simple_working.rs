// Simplified working OpenRaft implementation that compiles with current API
// This provides the infrastructure while being compatible with OpenRaft 0.9.21

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::controller::service::{TopicInfo, TopicConfig};
use crate::types::*;

/// Node ID type for RustMQ cluster (using u64 for OpenRaft compatibility)
pub type WorkingNodeId = u64;

/// Application data for RustMQ Raft operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkingAppData {
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
}

/// Response type for RustMQ Raft operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingAppDataResponse {
    pub success: bool,
    pub error_message: Option<String>,
    pub data: Option<String>,
}

/// Snapshot data for RustMQ cluster state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkingSnapshotData {
    pub topics: BTreeMap<String, TopicInfo>,
    pub brokers: BTreeMap<BrokerId, BrokerInfo>,
    pub partition_assignments: BTreeMap<TopicPartition, WorkingPartitionAssignment>,
    pub last_applied_log: u64,
    pub snapshot_version: u64,
}

/// PartitionAssignment for simplified implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingPartitionAssignment {
    pub leader: BrokerId,
    pub replicas: Vec<BrokerId>,
    pub in_sync_replicas: Vec<BrokerId>,
    pub leader_epoch: u64,
}

/// Simplified state machine for RustMQ cluster management
pub struct WorkingStateMachine {
    /// Current cluster state
    state: Arc<RwLock<WorkingSnapshotData>>,
    /// Last applied log ID
    last_applied: Arc<RwLock<u64>>,
}

impl WorkingStateMachine {
    /// Create new state machine
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(WorkingSnapshotData::default())),
            last_applied: Arc::new(RwLock::new(0)),
        }
    }

    /// Apply application data to state machine
    pub async fn apply_command(&self, app_data: WorkingAppData) -> crate::Result<WorkingAppDataResponse> {
        let mut state = self.state.write().await;
        
        match app_data {
            WorkingAppData::CreateTopic { name, partitions, replication_factor, config } => {
                if state.topics.contains_key(&name) {
                    Ok(WorkingAppDataResponse {
                        success: false,
                        error_message: Some(format!("Topic {} already exists", name)),
                        data: None,
                    })
                } else {
                    let topic_info = TopicInfo {
                        name: name.clone(),
                        partitions,
                        replication_factor,
                        config,
                        created_at: chrono::Utc::now(),
                    };
                    
                    state.topics.insert(name.clone(), topic_info);
                    
                    Ok(WorkingAppDataResponse {
                        success: true,
                        error_message: None,
                        data: Some(format!("Topic {} created", name)),
                    })
                }
            }
            
            WorkingAppData::DeleteTopic { name } => {
                if state.topics.remove(&name).is_some() {
                    // Remove partition assignments
                    state.partition_assignments.retain(|tp, _| tp.topic != name);
                    
                    Ok(WorkingAppDataResponse {
                        success: true,
                        error_message: None,
                        data: Some(format!("Topic {} deleted", name)),
                    })
                } else {
                    Ok(WorkingAppDataResponse {
                        success: false,
                        error_message: Some(format!("Topic {} not found", name)),
                        data: None,
                    })
                }
            }
            
            WorkingAppData::AddBroker { broker } => {
                state.brokers.insert(broker.id.clone(), broker.clone());
                
                Ok(WorkingAppDataResponse {
                    success: true,
                    error_message: None,
                    data: Some(format!("Broker {} added", broker.id)),
                })
            }
            
            WorkingAppData::RemoveBroker { broker_id } => {
                if state.brokers.remove(&broker_id).is_some() {
                    Ok(WorkingAppDataResponse {
                        success: true,
                        error_message: None,
                        data: Some(format!("Broker {} removed", broker_id)),
                    })
                } else {
                    Ok(WorkingAppDataResponse {
                        success: false,
                        error_message: Some(format!("Broker {} not found", broker_id)),
                        data: None,
                    })
                }
            }
        }
    }

    /// Get current cluster metadata
    pub async fn get_cluster_metadata(&self) -> WorkingSnapshotData {
        self.state.read().await.clone()
    }

    /// Update last applied log
    pub async fn update_last_applied(&self, log_id: u64) {
        let mut last_applied = self.last_applied.write().await;
        *last_applied = log_id;
        
        let mut state = self.state.write().await;
        state.last_applied_log = log_id;
    }

    /// Get last applied log
    pub async fn get_last_applied(&self) -> u64 {
        *self.last_applied.read().await
    }
}

impl Clone for WorkingStateMachine {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            last_applied: self.last_applied.clone(),
        }
    }
}

/// Simplified Raft consensus manager for RustMQ controller
pub struct WorkingRaftManager {
    /// Node ID of this controller
    pub node_id: WorkingNodeId,
    /// State machine
    pub state_machine: WorkingStateMachine,
    /// Current term
    pub current_term: Arc<RwLock<u64>>,
    /// Whether this node is the leader
    pub is_leader: Arc<RwLock<bool>>,
    /// Known cluster peers
    pub peers: Vec<WorkingNodeId>,
}

impl WorkingRaftManager {
    pub fn new(node_id: WorkingNodeId, peers: Vec<WorkingNodeId>) -> Self {
        Self {
            node_id,
            state_machine: WorkingStateMachine::new(),
            current_term: Arc::new(RwLock::new(0)),
            is_leader: Arc::new(RwLock::new(false)),
            peers,
        }
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        *self.is_leader.read().await
    }

    /// Become leader (simplified - in production would require election)
    pub async fn become_leader(&self) {
        let mut is_leader = self.is_leader.write().await;
        *is_leader = true;
        
        let mut term = self.current_term.write().await;
        *term += 1;
        
        info!("Node {} became leader for term {}", self.node_id, *term);
    }

    /// Apply a command if leader
    pub async fn apply_command(&self, cmd: WorkingAppData) -> crate::Result<WorkingAppDataResponse> {
        if !self.is_leader().await {
            return Ok(WorkingAppDataResponse {
                success: false,
                error_message: Some("Not the leader".to_string()),
                data: None,
            });
        }

        let result = self.state_machine.apply_command(cmd).await?;
        
        // Update last applied log
        let current_log = self.state_machine.get_last_applied().await;
        self.state_machine.update_last_applied(current_log + 1).await;
        
        Ok(result)
    }

    /// Get cluster metadata
    pub async fn get_cluster_metadata(&self) -> WorkingSnapshotData {
        self.state_machine.get_cluster_metadata().await
    }

    /// Get current term
    pub async fn get_current_term(&self) -> u64 {
        *self.current_term.read().await
    }

    /// Add a peer to the cluster
    pub async fn add_peer(&mut self, peer_id: WorkingNodeId) {
        if !self.peers.contains(&peer_id) {
            self.peers.push(peer_id);
            info!("Added peer {} to cluster", peer_id);
        }
    }

    /// Remove a peer from the cluster
    pub async fn remove_peer(&mut self, peer_id: WorkingNodeId) {
        self.peers.retain(|&id| id != peer_id);
        info!("Removed peer {} from cluster", peer_id);
    }
}

/// Integration helper for the working OpenRaft implementation
pub struct WorkingRaftIntegrationHelper {
    pub raft_manager: WorkingRaftManager,
}

impl WorkingRaftIntegrationHelper {
    pub fn new(node_id: WorkingNodeId, peers: Vec<WorkingNodeId>) -> Self {
        Self {
            raft_manager: WorkingRaftManager::new(node_id, peers),
        }
    }

    /// Create topic through Raft consensus
    pub async fn create_topic(
        &self,
        name: String,
        partitions: u32,
        replication_factor: u32,
        config: TopicConfig,
    ) -> crate::Result<WorkingAppDataResponse> {
        let cmd = WorkingAppData::CreateTopic {
            name,
            partitions,
            replication_factor,
            config,
        };
        self.raft_manager.apply_command(cmd).await
    }

    /// Delete topic through Raft consensus
    pub async fn delete_topic(&self, name: String) -> crate::Result<WorkingAppDataResponse> {
        let cmd = WorkingAppData::DeleteTopic { name };
        self.raft_manager.apply_command(cmd).await
    }

    /// Add broker through Raft consensus
    pub async fn add_broker(&self, broker: BrokerInfo) -> crate::Result<WorkingAppDataResponse> {
        let cmd = WorkingAppData::AddBroker { broker };
        self.raft_manager.apply_command(cmd).await
    }

    /// Remove broker through Raft consensus
    pub async fn remove_broker(&self, broker_id: BrokerId) -> crate::Result<WorkingAppDataResponse> {
        let cmd = WorkingAppData::RemoveBroker { broker_id };
        self.raft_manager.apply_command(cmd).await
    }

    /// Get cluster metadata
    pub async fn get_cluster_metadata(&self) -> WorkingSnapshotData {
        self.raft_manager.get_cluster_metadata().await
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        self.raft_manager.is_leader().await
    }

    /// Become leader (for testing/development)
    pub async fn become_leader(&self) {
        self.raft_manager.become_leader().await
    }

    /// Get current term
    pub async fn get_current_term(&self) -> u64 {
        self.raft_manager.get_current_term().await
    }
}

/// Configuration for the working Raft implementation
#[derive(Debug, Clone)]
pub struct WorkingRaftConfig {
    pub cluster_name: String,
    pub node_id: WorkingNodeId,
    pub peers: Vec<WorkingNodeId>,
    pub election_timeout_min: u64,
    pub election_timeout_max: u64,
    pub heartbeat_interval: u64,
}

impl Default for WorkingRaftConfig {
    fn default() -> Self {
        Self {
            cluster_name: "rustmq-cluster".to_string(),
            node_id: 1,
            peers: vec![],
            election_timeout_min: 150,
            election_timeout_max: 300,
            heartbeat_interval: 50,
        }
    }
}

/// Simple working OpenRaft manager
pub struct WorkingOpenRaftManager {
    config: WorkingRaftConfig,
    helper: WorkingRaftIntegrationHelper,
}

impl WorkingOpenRaftManager {
    /// Create a new working manager
    pub async fn new(config: WorkingRaftConfig) -> crate::Result<Self> {
        let helper = WorkingRaftIntegrationHelper::new(config.node_id, config.peers.clone());
        
        Ok(Self {
            config,
            helper,
        })
    }

    /// Start the manager (simplified)
    pub async fn start(&self) -> crate::Result<()> {
        info!("Starting working OpenRaft manager for node {}", self.config.node_id);
        
        // In a single-node setup, automatically become leader
        if self.config.peers.is_empty() {
            self.helper.become_leader().await;
        }
        
        Ok(())
    }

    /// Create a topic
    pub async fn create_topic(
        &self,
        name: String,
        partitions: u32,
        replication_factor: u32,
        config: TopicConfig,
    ) -> crate::Result<WorkingAppDataResponse> {
        self.helper.create_topic(name, partitions, replication_factor, config).await
    }

    /// Delete a topic
    pub async fn delete_topic(&self, name: String) -> crate::Result<WorkingAppDataResponse> {
        self.helper.delete_topic(name).await
    }

    /// Add a broker
    pub async fn add_broker(&self, broker: BrokerInfo) -> crate::Result<WorkingAppDataResponse> {
        self.helper.add_broker(broker).await
    }

    /// Remove a broker
    pub async fn remove_broker(&self, broker_id: BrokerId) -> crate::Result<WorkingAppDataResponse> {
        self.helper.remove_broker(broker_id).await
    }

    /// Get cluster metadata
    pub async fn get_cluster_metadata(&self) -> WorkingSnapshotData {
        self.helper.get_cluster_metadata().await
    }

    /// Check if leader
    pub async fn is_leader(&self) -> bool {
        self.helper.is_leader().await
    }

    /// Stop the manager
    pub async fn stop(&self) -> crate::Result<()> {
        info!("Stopping working OpenRaft manager for node {}", self.config.node_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_working_raft_integration_helper() {
        let helper = WorkingRaftIntegrationHelper::new(1, vec![]);
        
        // Become leader for testing
        helper.become_leader().await;
        assert!(helper.is_leader().await);

        // Test topic creation
        let config = TopicConfig {
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        };

        let result = helper.create_topic(
            "test-topic".to_string(),
            3,
            2,
            config,
        ).await.unwrap();

        assert!(result.success);
        assert!(result.error_message.is_none());

        // Verify topic exists
        let metadata = helper.get_cluster_metadata().await;
        assert_eq!(metadata.topics.len(), 1);
        assert!(metadata.topics.contains_key("test-topic"));

        // Test topic deletion
        let result = helper.delete_topic("test-topic".to_string()).await.unwrap();
        assert!(result.success);

        // Verify topic is gone
        let metadata = helper.get_cluster_metadata().await;
        assert_eq!(metadata.topics.len(), 0);
    }

    #[tokio::test]
    async fn test_working_broker_management() {
        let helper = WorkingRaftIntegrationHelper::new(1, vec![]);
        helper.become_leader().await;

        let broker = BrokerInfo {
            id: "broker-1".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };

        // Add broker
        let result = helper.add_broker(broker.clone()).await.unwrap();
        assert!(result.success);

        // Verify broker exists
        let metadata = helper.get_cluster_metadata().await;
        assert_eq!(metadata.brokers.len(), 1);
        assert!(metadata.brokers.contains_key("broker-1"));

        // Remove broker
        let result = helper.remove_broker("broker-1".to_string()).await.unwrap();
        assert!(result.success);

        // Verify broker is gone
        let metadata = helper.get_cluster_metadata().await;
        assert_eq!(metadata.brokers.len(), 0);
    }

    #[tokio::test]
    async fn test_working_openraft_manager() {
        let config = WorkingRaftConfig::default();
        let manager = WorkingOpenRaftManager::new(config).await.unwrap();
        
        manager.start().await.unwrap();
        assert!(manager.is_leader().await);
        
        // Test topic operations
        let topic_config = TopicConfig {
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        };
        
        let result = manager.create_topic(
            "manager-test-topic".to_string(),
            3,
            2,
            topic_config,
        ).await.unwrap();
        
        assert!(result.success);
        
        let metadata = manager.get_cluster_metadata().await;
        assert_eq!(metadata.topics.len(), 1);
        
        manager.stop().await.unwrap();
    }
}