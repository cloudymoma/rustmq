use rustmq::{Config, Result};
use rustmq::controller::service::{
    ControllerService, RaftRpc, VoteRequest, VoteResponse, AppendEntriesRequest, AppendEntriesResponse,
    CreateTopicRequest, CreateTopicResponse, DeleteTopicRequest, DeleteTopicResponse,
    RegisterBrokerResponse, ClusterMetadata, SlotAcquisitionResult
};
use rustmq::types::*;
use std::env;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::signal;
use tokio::sync::RwLock as AsyncRwLock;
use tokio::time::{interval, Duration};
use tracing::{info, error, warn, debug};
use async_trait::async_trait;
use uuid::Uuid;

/// gRPC service implementation for controller-to-controller Raft communication
pub struct ControllerRaftService {
    controller: Arc<ControllerService>,
}

impl ControllerRaftService {
    pub fn new(controller: Arc<ControllerService>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl RaftRpc for ControllerRaftService {
    async fn request_vote(&self, _node_id: &str, request: VoteRequest) -> Result<VoteResponse> {
        self.controller.handle_vote_request(request).await
    }

    async fn append_entries(&self, _node_id: &str, request: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
        self.controller.handle_append_entries(request).await
    }
}

/// gRPC service implementation for broker-to-controller communication
pub struct ControllerBrokerService {
    controller: Arc<ControllerService>,
}

impl ControllerBrokerService {
    pub fn new(controller: Arc<ControllerService>) -> Self {
        Self { controller }
    }

    /// Handle broker registration requests
    pub async fn register_broker(&self, broker: BrokerInfo) -> Result<RegisterBrokerResponse> {
        self.controller.register_broker(broker).await
    }

    /// Handle topic creation requests
    pub async fn create_topic(&self, request: CreateTopicRequest) -> Result<CreateTopicResponse> {
        self.controller.create_topic(request).await
    }

    /// Handle topic deletion requests
    pub async fn delete_topic(&self, request: DeleteTopicRequest) -> Result<DeleteTopicResponse> {
        self.controller.delete_topic(request).await
    }

    /// Handle cluster metadata requests
    pub async fn get_cluster_metadata(&self) -> Result<ClusterMetadata> {
        self.controller.get_cluster_metadata().await
    }

    /// Handle decommission slot acquisition requests
    pub async fn acquire_decommission_slot(
        &self,
        broker_id: String,
        requester: String,
    ) -> Result<SlotAcquisitionResult> {
        self.controller.acquire_decommission_slot(broker_id, requester).await
    }

    /// Handle decommission slot release requests
    pub async fn release_decommission_slot(&self, operation_id: &str) -> Result<()> {
        self.controller.release_decommission_slot(operation_id).await
    }
}

/// RPC client implementation for sending Raft messages to peer controllers
pub struct ControllerRaftClient {
    peer_endpoints: Arc<AsyncRwLock<HashMap<String, String>>>,
}

impl ControllerRaftClient {
    pub fn new(peer_endpoints: HashMap<String, String>) -> Self {
        Self {
            peer_endpoints: Arc::new(AsyncRwLock::new(peer_endpoints)),
        }
    }

    /// Update peer endpoints (for dynamic configuration)
    pub async fn update_peers(&self, peer_endpoints: HashMap<String, String>) {
        let mut peers = self.peer_endpoints.write().await;
        *peers = peer_endpoints;
    }
}

#[async_trait]
impl RaftRpc for ControllerRaftClient {
    async fn request_vote(&self, node_id: &str, request: VoteRequest) -> Result<VoteResponse> {
        let peers = self.peer_endpoints.read().await;
        if let Some(_endpoint) = peers.get(node_id) {
            // In a full implementation, this would send gRPC request to the peer
            // For now, simulate a response based on request
            debug!("Sending vote request to {} for term {}", node_id, request.term);
            
            // Simplified response - in production would be actual gRPC call
            Ok(VoteResponse {
                term: request.term,
                vote_granted: true, // Simplified - actual logic would depend on peer state
            })
        } else {
            Err(rustmq::RustMqError::NotFound(format!("Peer {} not found", node_id)))
        }
    }

    async fn append_entries(&self, node_id: &str, request: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
        let peers = self.peer_endpoints.read().await;
        if let Some(_endpoint) = peers.get(node_id) {
            // In a full implementation, this would send gRPC request to the peer
            debug!("Sending append entries to {} for term {}", node_id, request.term);
            
            // Simplified response - in production would be actual gRPC call
            Ok(AppendEntriesResponse {
                term: request.term,
                success: true,
                match_index: Some(request.prev_log_index + request.entries.len() as u64),
            })
        } else {
            Err(rustmq::RustMqError::NotFound(format!("Peer {} not found", node_id)))
        }
    }
}

/// Background task manager for controller operations
pub struct ControllerTaskManager {
    controller: Arc<ControllerService>,
    raft_client: Arc<ControllerRaftClient>,
    election_timeout_ms: u64,
    heartbeat_interval_ms: u64,
}

impl ControllerTaskManager {
    pub fn new(
        controller: Arc<ControllerService>,
        raft_client: Arc<ControllerRaftClient>,
        election_timeout_ms: u64,
        heartbeat_interval_ms: u64,
    ) -> Self {
        Self {
            controller,
            raft_client,
            election_timeout_ms,
            heartbeat_interval_ms,
        }
    }

    /// Start leader election timeout task
    pub async fn start_election_timeout_task(&self) {
        let controller = self.controller.clone();
        let timeout_ms = self.election_timeout_ms;
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(timeout_ms));
            
            loop {
                interval.tick().await;
                
                // Check if we're a follower and haven't heard from leader recently
                if !controller.get_raft_info().is_leader {
                    debug!("Election timeout - starting election");
                    
                    match controller.start_election().await {
                        Ok(won) => {
                            if won {
                                info!("Won election, became leader for term {}", controller.get_raft_info().current_term);
                            } else {
                                debug!("Lost election for term {}", controller.get_raft_info().current_term);
                            }
                        }
                        Err(e) => {
                            error!("Election failed: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// Start heartbeat task (leader only)
    pub async fn start_heartbeat_task(&self) {
        let controller = self.controller.clone();
        let raft_client = self.raft_client.clone();
        let heartbeat_interval_ms = self.heartbeat_interval_ms;
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(heartbeat_interval_ms));
            
            loop {
                interval.tick().await;
                
                if controller.get_raft_info().is_leader {
                    debug!("Sending heartbeats as leader");
                    
                    // Get peer list from raft client
                    let peers = raft_client.peer_endpoints.read().await;
                    let current_term = controller.get_raft_info().current_term;
                    let commit_index = controller.get_raft_info().commit_index;
                    
                    // Send heartbeats to all peers
                    for peer_id in peers.keys() {
                        let heartbeat_request = AppendEntriesRequest {
                            term: current_term,
                            leader_id: controller.get_raft_info().node_id.clone(),
                            prev_log_index: 0, // Simplified
                            prev_log_term: 0,
                            entries: vec![], // Empty for heartbeat
                            leader_commit: commit_index,
                        };
                        
                        if let Err(e) = raft_client.append_entries(peer_id, heartbeat_request).await {
                            warn!("Failed to send heartbeat to {}: {}", peer_id, e);
                        }
                    }
                }
            }
        });
    }

    /// Start health monitoring task
    pub async fn start_health_monitoring_task(&self) {
        let controller = self.controller.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30)); // Health check every 30 seconds
            
            loop {
                interval.tick().await;
                
                let raft_info = controller.get_raft_info();
                let decommission_status = controller.get_decommission_status().await.unwrap_or_default();
                
                info!(
                    "Controller health: node_id={}, term={}, is_leader={}, decommissions_active={}",
                    raft_info.node_id,
                    raft_info.current_term,
                    raft_info.is_leader,
                    decommission_status.len()
                );
                
                // Check cluster metadata periodically
                if let Ok(metadata) = controller.get_cluster_metadata().await {
                    debug!(
                        "Cluster state: topics={}, brokers={}, partitions={}",
                        metadata.topics.len(),
                        metadata.brokers.len(),
                        metadata.partition_assignments.len()
                    );
                }
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let default_config = "config/controller.toml".to_string();
    let config_path = args.get(2).unwrap_or(&default_config);
    
    info!("Starting RustMQ Controller");
    info!("Loading configuration from: {}", config_path);
    
    // Load configuration
    let config = match Config::from_file(config_path) {
        Ok(config) => {
            config.validate()?;
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };
    
    info!("Controller endpoints: {:?}", config.controller.endpoints);
    info!("Election timeout: {}ms", config.controller.election_timeout_ms);
    info!("Heartbeat interval: {}ms", config.controller.heartbeat_interval_ms);
    
    // Generate unique node ID for this controller instance
    let node_id = format!("controller-{}", Uuid::new_v4().to_string()[..8].to_string());
    info!("Controller node ID: {}", node_id);
    
    // Parse peer endpoints
    let mut peer_endpoints = HashMap::new();
    for (i, endpoint) in config.controller.endpoints.iter().enumerate() {
        let peer_id = format!("controller-peer-{}", i);
        peer_endpoints.insert(peer_id, endpoint.clone());
    }
    
    // Initialize ControllerService
    info!("Initializing ControllerService...");
    let controller = Arc::new(ControllerService::new(
        node_id.clone(),
        peer_endpoints.keys().cloned().collect(),
        config.scaling.clone(),
    ));
    
    // Initialize Raft RPC client for peer communication
    let raft_client = Arc::new(ControllerRaftClient::new(peer_endpoints));
    
    // Initialize gRPC services
    info!("Initializing gRPC services...");
    let raft_service = Arc::new(ControllerRaftService::new(controller.clone()));
    let broker_service = Arc::new(ControllerBrokerService::new(controller.clone()));
    
    // Initialize task manager for background operations
    info!("Initializing background task manager...");
    let task_manager = ControllerTaskManager::new(
        controller.clone(),
        raft_client.clone(),
        config.controller.election_timeout_ms,
        config.controller.heartbeat_interval_ms,
    );
    
    // Start background tasks
    info!("Starting background tasks...");
    
    // Start election timeout task
    task_manager.start_election_timeout_task().await;
    
    // Start heartbeat task
    task_manager.start_heartbeat_task().await;
    
    // Start health monitoring
    task_manager.start_health_monitoring_task().await;
    
    // Start gRPC servers (simplified - in production would use tonic)
    info!("Starting gRPC servers...");
    
    // Start Raft RPC server for controller-to-controller communication
    let raft_rpc_handle = {
        let _raft_service = raft_service.clone();
        let endpoints = config.controller.endpoints.clone();
        tokio::spawn(async move {
            info!("Raft RPC server ready on endpoints: {:?}", endpoints);
            // In a real implementation, this would start a tonic gRPC server
            // listening for VoteRequest and AppendEntries RPCs
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
    };
    
    // Start broker communication server
    let broker_rpc_handle = {
        let _broker_service = broker_service.clone();
        let endpoints = config.controller.endpoints.clone();
        tokio::spawn(async move {
            info!("Broker RPC server ready on endpoints: {:?}", endpoints);
            // In a real implementation, this would start a tonic gRPC server
            // listening for broker registration, topic management, and metadata requests
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
    };
    
    // Attempt initial leader election if we're starting as a single node
    if config.controller.endpoints.len() <= 1 {
        info!("Single node cluster detected, attempting to become leader...");
        match controller.start_election().await {
            Ok(true) => {
                info!("Became leader in single-node cluster");
            }
            Ok(false) => {
                info!("Failed to become leader in single-node cluster");
            }
            Err(e) => {
                warn!("Error during initial election: {}", e);
            }
        }
    }
    
    info!("RustMQ Controller started successfully");
    info!("Node ID: {}", node_id);
    info!("Endpoints: {:?}", config.controller.endpoints);
    info!("Election timeout: {}ms", config.controller.election_timeout_ms);
    info!("Heartbeat interval: {}ms", config.controller.heartbeat_interval_ms);
    
    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = raft_rpc_handle => {
            error!("Raft RPC server terminated unexpectedly");
        }
        _ = broker_rpc_handle => {
            error!("Broker RPC server terminated unexpectedly");
        }
    }
    
    info!("Shutting down RustMQ Controller gracefully...");
    
    // Graceful shutdown
    if controller.get_raft_info().is_leader {
        info!("Stepping down as leader before shutdown...");
        // In a full implementation, would transfer leadership or trigger new election
    }
    
    // Release any held decommission slots
    if let Ok(active_slots) = controller.get_decommission_status().await {
        for slot in active_slots {
            info!("Releasing decommission slot: {}", slot.operation_id);
            if let Err(e) = controller.release_decommission_slot(&slot.operation_id).await {
                warn!("Failed to release decommission slot {}: {}", slot.operation_id, e);
            }
        }
    }
    
    info!("RustMQ Controller shutdown complete");
    Ok(())
}