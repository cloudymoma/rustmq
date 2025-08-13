//! Basic tests for ControllerRaftService gRPC implementation
//! 
//! This test suite focuses on testing the gRPC interface and protobuf conversion
//! for the 8 RPC methods of the ControllerRaftService.

use rustmq::proto::{controller, common};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use chrono::Utc;
use async_trait::async_trait;

// Import the gRPC service trait
use controller::controller_raft_service_server::ControllerRaftService;

/// Simplified mock implementation for testing gRPC interface
pub struct MockControllerRaftService {
    pub node_id: String,
    pub current_term: std::sync::atomic::AtomicU64,
    pub is_leader: std::sync::atomic::AtomicBool,
}

impl MockControllerRaftService {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            current_term: std::sync::atomic::AtomicU64::new(1),
            is_leader: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn with_term(self, term: u64) -> Self {
        self.current_term.store(term, std::sync::atomic::Ordering::SeqCst);
        self
    }

    pub fn as_leader(self) -> Self {
        self.is_leader.store(true, std::sync::atomic::Ordering::SeqCst);
        self
    }
}

#[async_trait]
impl ControllerRaftService for MockControllerRaftService {
    async fn request_vote(
        &self,
        request: Request<controller::RequestVoteRequest>,
    ) -> std::result::Result<Response<controller::RequestVoteResponse>, Status> {
        let req = request.into_inner();
        
        // Basic validation
        if req.candidate_id.is_empty() {
            return Err(Status::invalid_argument("Empty candidate ID"));
        }
        
        let current_term = self.current_term.load(std::sync::atomic::Ordering::SeqCst);
        let vote_granted = req.term >= current_term && req.candidate_id != self.node_id;
        
        if req.term > current_term {
            self.current_term.store(req.term, std::sync::atomic::Ordering::SeqCst);
        }
        
        let response = controller::RequestVoteResponse {
            term: self.current_term.load(std::sync::atomic::Ordering::SeqCst),
            vote_granted,
            voter_id: self.node_id.clone(),
            metadata: None,
            vote_reason: "test".to_string(),
            voter_last_log_index: 0,
            voter_last_log_term: 0,
            voter_version: "1.0.0".to_string(),
            voter_capabilities: vec!["consensus".to_string()],
            voter_cpu_usage: 25.0,
            voter_memory_usage: 50.0,
        };

        Ok(Response::new(response))
    }

    async fn append_entries(
        &self,
        request: Request<controller::AppendEntriesRequest>,
    ) -> std::result::Result<Response<controller::AppendEntriesResponse>, Status> {
        let req = request.into_inner();
        let current_term = self.current_term.load(std::sync::atomic::Ordering::SeqCst);
        let success = req.term >= current_term;
        
        if req.term > current_term {
            self.current_term.store(req.term, std::sync::atomic::Ordering::SeqCst);
        }
        
        let response = controller::AppendEntriesResponse {
            term: self.current_term.load(std::sync::atomic::Ordering::SeqCst),
            success,
            follower_id: self.node_id.clone(),
            match_index: if success { req.prev_log_index + req.entries.len() as u64 } else { 0 },
            metadata: None,
            conflict_index: if success { 0 } else { req.prev_log_index },
            conflict_term: 0,
            first_index_of_term: 0,
            follower_log_size: 100,
            follower_commit_index: 0,
            follower_applied_index: 0,
            append_time_ms: 5,
            bytes_appended: req.entries.len() as u64,
            preferred_max_entries: 1000,
            preferred_max_bytes: 1024 * 1024,
        };

        Ok(Response::new(response))
    }

    async fn install_snapshot(
        &self,
        request: Request<controller::InstallSnapshotRequest>,
    ) -> std::result::Result<Response<controller::InstallSnapshotResponse>, Status> {
        let req = request.into_inner();
        
        let response = controller::InstallSnapshotResponse {
            term: self.current_term.load(std::sync::atomic::Ordering::SeqCst),
            success: true,
            follower_id: self.node_id.clone(),
            bytes_received: req.data.len() as u64,
            metadata: None,
            chunks_received: req.chunk_index + 1,
            installation_progress: (req.chunk_index as f64 + 1.0) / req.total_chunks as f64,
            installation_time_ms: 100,
            preferred_chunk_size: 1024,
            available_disk_space: 1024 * 1024 * 1024,
            cpu_usage: 25.0,
            memory_usage: 50.0,
            error_details: String::new(),
            retry_chunk: false,
        };

        Ok(Response::new(response))
    }

    async fn pre_vote(
        &self,
        request: Request<controller::PreVoteRequest>,
    ) -> std::result::Result<Response<controller::PreVoteResponse>, Status> {
        let req = request.into_inner();
        let current_term = self.current_term.load(std::sync::atomic::Ordering::SeqCst);
        let vote_granted = req.term >= current_term;
        
        let response = controller::PreVoteResponse {
            term: current_term,
            vote_granted,
            voter_id: self.node_id.clone(),
            metadata: None,
            vote_reason: "pre_vote_test".to_string(),
            has_leader: false,
            current_leader_id: String::new(),
        };

        Ok(Response::new(response))
    }

    async fn transfer_leadership(
        &self,
        request: Request<controller::TransferLeadershipRequest>,
    ) -> std::result::Result<Response<controller::TransferLeadershipResponse>, Status> {
        let req = request.into_inner();
        let is_leader = self.is_leader.load(std::sync::atomic::Ordering::SeqCst);
        let success = is_leader && req.current_leader_id == self.node_id;
        
        let response = controller::TransferLeadershipResponse {
            success,
            error_code: if success { 0 } else { 1 },
            error_message: if success { String::new() } else { "Not leader".to_string() },
            new_leader_id: if success { req.target_leader_id } else { String::new() },
            new_term: self.current_term.load(std::sync::atomic::Ordering::SeqCst),
            metadata: None,
            transfer_time_ms: if success { 100 } else { 0 },
            final_log_index: 0,
        };

        Ok(Response::new(response))
    }

    async fn get_cluster_info(
        &self,
        _request: Request<controller::GetClusterInfoRequest>,
    ) -> std::result::Result<Response<controller::GetClusterInfoResponse>, Status> {
        let response = controller::GetClusterInfoResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            cluster_id: "test-cluster".to_string(),
            current_leader_id: if self.is_leader.load(std::sync::atomic::Ordering::SeqCst) {
                self.node_id.clone()
            } else {
                "unknown".to_string()
            },
            current_term: self.current_term.load(std::sync::atomic::Ordering::SeqCst),
            commit_index: 0,
            applied_index: 0,
            nodes: vec![controller::NodeInfo {
                node_id: self.node_id.clone(),
                address: "127.0.0.1:9090".to_string(),
                state: controller::NodeState::Follower as i32,
                last_contact: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                term: 1,
                last_log_index: 0,
                last_log_term: 0,
                commit_index: 0,
                applied_index: 0,
                capabilities: vec!["consensus".to_string()],
                version: "1.0.0".to_string(),
                health: Some(controller::NodeHealth {
                    status: controller::HealthStatus::Healthy as i32,
                    cpu_usage: 25.0,
                    memory_usage: 50.0,
                    disk_usage_bytes: 30 * 1024 * 1024 * 1024, // 30GB
                    network_in_bytes_per_sec: 10 * 1024 * 1024, // 10MB/s
                    network_out_bytes_per_sec: 5 * 1024 * 1024, // 5MB/s
                    warnings: vec![],
                }),
                performance: Some(common::PerformanceMetrics {
                    messages_per_second: 100,
                    bytes_per_second: 1024 * 100,
                    avg_latency_ms: 10.0,
                    p99_latency_ms: 25.0,
                    total_messages: 10000,
                    total_bytes: 1024 * 1024 * 100,
                    error_count: 1,
                    start_time: Some(prost_types::Timestamp {
                        seconds: Utc::now().timestamp(),
                        nanos: 0,
                    }),
                    last_updated: Some(prost_types::Timestamp {
                        seconds: Utc::now().timestamp(),
                        nanos: 0,
                    }),
                }),
                priority: 100,
            }],
            log_size: 0,
            snapshot_index: 0,
            health: Some(controller::ClusterHealth {
                status: controller::HealthStatus::Healthy as i32,
                healthy_nodes: 1,
                total_nodes: 1,
                consensus_availability: 1.0,
                issues: vec![],
                average_latency_ms: 10.0,
                throughput_ops_per_second: 100.0,
            }),
            performance: Some(common::PerformanceMetrics {
                messages_per_second: 100,
                bytes_per_second: 1024 * 100,
                avg_latency_ms: 10.0,
                p99_latency_ms: 25.0,
                total_messages: 10000,
                total_bytes: 1024 * 1024 * 100,
                error_count: 1,
                start_time: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                last_updated: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
            }),
        };

        Ok(Response::new(response))
    }

    async fn add_node(
        &self,
        request: Request<controller::AddNodeRequest>,
    ) -> std::result::Result<Response<controller::AddNodeResponse>, Status> {
        let req = request.into_inner();
        let success = !req.node_id.is_empty() && req.node_address.contains(':');
        
        let response = controller::AddNodeResponse {
            success,
            error_code: if success { 0 } else { 1 },
            error_message: if success { String::new() } else { "Invalid node info".to_string() },
            metadata: None,
            configuration_index: if success { 1 } else { 0 },
            configuration_term: self.current_term.load(std::sync::atomic::Ordering::SeqCst),
        };

        Ok(Response::new(response))
    }

    async fn remove_node(
        &self,
        request: Request<controller::RemoveNodeRequest>,
    ) -> std::result::Result<Response<controller::RemoveNodeResponse>, Status> {
        let req = request.into_inner();
        let success = !req.node_id.is_empty() && req.node_id != self.node_id;
        
        let response = controller::RemoveNodeResponse {
            success,
            error_code: if success { 0 } else { 1 },
            error_message: if success { String::new() } else { "Cannot remove self".to_string() },
            metadata: None,
            configuration_index: if success { 1 } else { 0 },
            configuration_term: self.current_term.load(std::sync::atomic::Ordering::SeqCst),
        };

        Ok(Response::new(response))
    }
}

// Helper functions
fn create_test_service() -> MockControllerRaftService {
    MockControllerRaftService::new("test-controller-1".to_string())
}

fn current_timestamp() -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: Utc::now().timestamp(),
        nanos: 0,
    }
}

#[tokio::test]
async fn test_request_vote_success() {
    let service = create_test_service();
    
    let proto_request = controller::RequestVoteRequest {
        term: 2,
        candidate_id: "candidate-1".to_string(),
        last_log_index: 5,
        last_log_term: 1,
        pre_vote: false,
        metadata: None,
        candidate_priority: 100,
        candidate_version: "1.0.0".to_string(),
        candidate_capabilities: vec!["replication".to_string()],
        election_reason: "leader_timeout".to_string(),
        election_timeout_ms: 5000,
    };

    let result = service.request_vote(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert!(response.term >= 2);
    assert_eq!(response.voter_id, "test-controller-1");
    assert!(response.vote_granted);
}

#[tokio::test]
async fn test_request_vote_stale_term() {
    let service = create_test_service().with_term(5);
    
    let stale_request = controller::RequestVoteRequest {
        term: 3, // Lower than current term (5)
        candidate_id: "stale-candidate".to_string(),
        last_log_index: 5,
        last_log_term: 1,
        pre_vote: false,
        metadata: None,
        candidate_priority: 100,
        candidate_version: "1.0.0".to_string(),
        candidate_capabilities: vec![],
        election_reason: "leader_timeout".to_string(),
        election_timeout_ms: 5000,
    };

    let result = service.request_vote(Request::new(stale_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert!(!response.vote_granted); // Should reject stale term
    assert_eq!(response.term, 5); // Should return current term
}

#[tokio::test]
async fn test_request_vote_validation() {
    let service = create_test_service();
    
    // Test empty candidate ID
    let invalid_request = controller::RequestVoteRequest {
        term: 1,
        candidate_id: "".to_string(), // Empty candidate ID
        last_log_index: 5,
        last_log_term: 1,
        pre_vote: false,
        metadata: None,
        candidate_priority: 100,
        candidate_version: "1.0.0".to_string(),
        candidate_capabilities: vec![],
        election_reason: "test".to_string(),
        election_timeout_ms: 5000,
    };

    let result = service.request_vote(Request::new(invalid_request)).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_pre_vote_optimization() {
    let service = create_test_service();
    
    let proto_request = controller::PreVoteRequest {
        term: 2,
        candidate_id: "candidate-1".to_string(),
        last_log_index: 5,
        last_log_term: 1,
        metadata: None,
        election_reason: "leader_timeout".to_string(),
        leader_lease_timeout_ms: 10000,
        last_leader_contact: Some(current_timestamp()),
        candidate_capabilities: vec!["replication".to_string()],
        candidate_priority: 100,
    };

    let result = service.pre_vote(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert_eq!(response.voter_id, "test-controller-1");
    assert!(response.vote_granted);
}

#[tokio::test]
async fn test_append_entries_heartbeat() {
    let service = create_test_service();
    
    let proto_request = controller::AppendEntriesRequest {
        term: 1,
        leader_id: "leader-1".to_string(),
        prev_log_index: 0,
        prev_log_term: 0,
        entries: vec![], // Empty for heartbeat
        leader_commit: 0,
        metadata: None,
        is_heartbeat: true,
        batch_size: 0,
        total_batch_size_bytes: 0,
        leader_log_size: 100,
        leader_snapshot_index: 0,
        max_entries_per_request: 1000,
        max_bytes_per_request: 1024 * 1024,
    };

    let result = service.append_entries(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert_eq!(response.follower_id, "test-controller-1");
    assert!(response.success);
}

#[tokio::test]
async fn test_append_entries_with_logs() {
    let service = create_test_service();
    
    let log_entry = controller::LogEntry {
        index: 1,
        term: 1,
        r#type: controller::LogEntryType::BrokerMetadata as i32,
        data: b"test-broker-metadata".to_vec().into(),
        timestamp: Some(current_timestamp()),
        node_id: "leader-1".to_string(),
        checksum: 12345,
        data_size: 20,
        correlation_id: "test-corr-1".to_string(),
        priority: 0,
        tags: vec!["test".to_string()],
    };

    let proto_request = controller::AppendEntriesRequest {
        term: 1,
        leader_id: "leader-1".to_string(),
        prev_log_index: 0,
        prev_log_term: 0,
        entries: vec![log_entry],
        leader_commit: 0,
        metadata: None,
        is_heartbeat: false,
        batch_size: 1,
        total_batch_size_bytes: 20,
        leader_log_size: 100,
        leader_snapshot_index: 0,
        max_entries_per_request: 1000,
        max_bytes_per_request: 1024 * 1024,
    };

    let result = service.append_entries(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert_eq!(response.follower_id, "test-controller-1");
    assert!(response.success);
    assert_eq!(response.match_index, 1);
}

#[tokio::test]
async fn test_install_snapshot_success() {
    let service = create_test_service();
    
    let snapshot_data = b"snapshot-data-chunk-1";
    
    let proto_request = controller::InstallSnapshotRequest {
        term: 1,
        leader_id: "leader-1".to_string(),
        last_included_index: 100,
        last_included_term: 1,
        offset: 0,
        data: snapshot_data.to_vec().into(),
        done: false, // More chunks to come
        metadata: None,
        snapshot_size_bytes: 1024,
        chunk_size_bytes: snapshot_data.len() as u32,
        chunk_index: 0,
        total_chunks: 5,
        snapshot_id: "snap-123".to_string(),
        snapshot_checksum: 54321,
        snapshot_timestamp: Some(current_timestamp()),
        compression: common::CompressionType::None as i32,
        uncompressed_size: snapshot_data.len() as u64,
    };

    let result = service.install_snapshot(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert_eq!(response.follower_id, "test-controller-1");
    assert_eq!(response.bytes_received, snapshot_data.len() as u64);
    assert_eq!(response.chunks_received, 1);
}

#[tokio::test]
async fn test_transfer_leadership_success() {
    let service = create_test_service().as_leader();
    
    let proto_request = controller::TransferLeadershipRequest {
        current_leader_id: "test-controller-1".to_string(),
        target_leader_id: "new-leader".to_string(),
        timeout_ms: 30000,
        metadata: None,
        transfer_reason: "planned_maintenance".to_string(),
        force_transfer: false,
    };

    let result = service.transfer_leadership(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert!(response.success);
    assert_eq!(response.new_leader_id, "new-leader");
}

#[tokio::test]
async fn test_get_cluster_info_basic() {
    let service = create_test_service();
    
    let proto_request = controller::GetClusterInfoRequest {
        metadata: None,
        include_node_details: false,
        include_log_info: false,
        include_performance_metrics: false,
    };

    let result = service.get_cluster_info(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert_eq!(response.cluster_id, "test-cluster");
    assert_eq!(response.nodes.len(), 1);
}

#[tokio::test]
async fn test_add_node_success() {
    let service = create_test_service();
    
    let proto_request = controller::AddNodeRequest {
        node_id: "new-node-1".to_string(),
        node_address: "192.168.1.100:9090".to_string(),
        voting_member: true,
        metadata: None,
        capabilities: vec!["replication".to_string()],
        priority: 100,
        version: "1.0.0".to_string(),
    };

    let result = service.add_node(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert!(response.success);
    assert_eq!(response.configuration_index, 1);
}

#[tokio::test]
async fn test_remove_node_success() {
    let service = create_test_service();
    
    let proto_request = controller::RemoveNodeRequest {
        node_id: "node-to-remove".to_string(),
        force_removal: false,
        metadata: None,
        removal_reason: "planned_decommission".to_string(),
    };

    let result = service.remove_node(Request::new(proto_request)).await;
    assert!(result.is_ok());
    
    let response = result.unwrap().into_inner();
    assert!(response.success);
    assert_eq!(response.configuration_index, 1);
}

#[tokio::test]
async fn test_concurrent_vote_requests() {
    let service = Arc::new(create_test_service());
    let mut handles = Vec::new();

    // Send multiple concurrent vote requests
    for i in 0..5 {
        let service_clone = service.clone();
        let handle = tokio::spawn(async move {
            let proto_request = controller::RequestVoteRequest {
                term: i + 1,
                candidate_id: format!("candidate-{}", i),
                last_log_index: 5,
                last_log_term: 1,
                pre_vote: false,
                metadata: None,
                candidate_priority: 100,
                candidate_version: "1.0.0".to_string(),
                candidate_capabilities: vec![],
                election_reason: "concurrent_test".to_string(),
                election_timeout_ms: 5000,
            };

            service_clone.request_vote(Request::new(proto_request)).await
        });
        handles.push(handle);
    }

    // All should complete without panic
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}