//! Test utilities and mock implementations for gRPC protobuf service testing
//! 
//! This module provides comprehensive testing infrastructure including:
//! - Mock service implementations for testing
//! - Test data generators and factories
//! - Helper functions for common test patterns
//! - Performance measurement utilities
//! - Error simulation capabilities

use rustmq::{
    types::*,
    proto::{broker, controller, common},
    config::{WalConfig, ScalingConfig},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use chrono::Utc;
use async_trait::async_trait;
use rand::{Rng, thread_rng};
use tempfile::TempDir;
use bytes::Bytes;

// ============================================================================
// Test Data Factories
// ============================================================================

pub struct TestDataFactory;

impl TestDataFactory {
    /// Generate a test topic partition with optional customization
    pub fn topic_partition(topic: Option<&str>, partition: Option<u32>) -> TopicPartition {
        TopicPartition {
            topic: topic.unwrap_or("test-topic").to_string(),
            partition: partition.unwrap_or(0),
        }
    }

    /// Generate a test record with customizable fields
    pub fn record(
        key: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        header_count: usize,
    ) -> Record {
        let headers = (0..header_count).map(|i| Header::new(
            format!("header-{}", i),
            format!("value-{}", i).into_bytes(),
        )).collect();

        Record::new(
            key,
            value.unwrap_or_else(|| b"test-value".to_vec()),
            headers,
            Utc::now().timestamp_millis(),
        )
    }

    /// Generate a WAL record with specified offset
    pub fn wal_record(
        topic_partition: TopicPartition,
        offset: u64,
        record: Option<Record>,
    ) -> WalRecord {
        WalRecord {
            topic_partition,
            offset,
            record: record.unwrap_or_else(|| Self::record(None, None, 1)),
            crc32: Self::random_crc32(),
        }
    }

    /// Generate a batch of WAL records
    pub fn wal_record_batch(
        topic_partition: TopicPartition,
        start_offset: u64,
        count: usize,
    ) -> Vec<WalRecord> {
        (0..count).map(|i| {
            Self::wal_record(
                topic_partition.clone(),
                start_offset + i as u64,
                Some(Self::record(
                    Some(format!("key-{}", i).into_bytes()),
                    Some(format!("value-{}", i).into_bytes()),
                    i % 3, // Vary header count
                ))
            )
        }).collect()
    }

    /// Generate random CRC32 value
    pub fn random_crc32() -> u32 {
        thread_rng().r#gen()
    }

    /// Generate broker info for testing
    pub fn broker_info(id: &str, rack: Option<&str>) -> BrokerInfo {
        BrokerInfo {
            id: id.to_string(),
            host: "127.0.0.1".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: rack.map(|r| r.to_string()).unwrap_or_else(|| "default-rack".to_string()),
        }
    }

    /// Generate follower state for testing
    pub fn follower_state(broker_id: &str, lag: u64) -> FollowerState {
        FollowerState {
            broker_id: broker_id.to_string(),
            last_known_offset: 1000,
            last_heartbeat: Utc::now(),
            lag,
        }
    }

    /// Generate request metadata with optional customization
    pub fn request_metadata(
        request_id: Option<&str>,
        timeout_ms: Option<u32>,
    ) -> common::RequestMetadata {
        common::RequestMetadata {
            client_id: "test-client".to_string(),
            correlation_id: request_id.unwrap_or("test-request").to_string(),
            timestamp: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            api_version: 1,
            timeout_ms: timeout_ms.unwrap_or(30000),
        }
    }

    /// Generate log entry for Raft testing
    pub fn log_entry(
        index: u64,
        term: u64,
        entry_type: controller::LogEntryType,
        data: Vec<u8>,
    ) -> controller::LogEntry {
        controller::LogEntry {
            index,
            term,
            r#type: entry_type as i32,
            data_size: data.len() as u32,
            data: data.into(),
            timestamp: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            node_id: "test-node".to_string(),
            checksum: Self::random_crc32(),
            correlation_id: format!("entry-{}", index),
            priority: 0,
            tags: vec!["test".to_string()],
        }
    }
}

// ============================================================================
// Mock Service Implementations
// ============================================================================

/// Mock implementation of BrokerReplicationService for testing
pub struct MockBrokerReplicationService {
    pub broker_id: String,
    pub fail_requests: bool,
    pub error_code: u32,
    pub error_message: String,
    pub simulated_latency: Duration,
    pub request_log: Arc<Mutex<Vec<String>>>,
}

impl MockBrokerReplicationService {
    pub fn new(broker_id: String) -> Self {
        Self {
            broker_id,
            fail_requests: false,
            error_code: 0,
            error_message: String::new(),
            simulated_latency: Duration::from_millis(0),
            request_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_failure(mut self, error_code: u32, message: &str) -> Self {
        self.fail_requests = true;
        self.error_code = error_code;
        self.error_message = message.to_string();
        self
    }

    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.simulated_latency = latency;
        self
    }

    pub async fn get_request_log(&self) -> Vec<String> {
        self.request_log.lock().await.clone()
    }

    async fn log_request(&self, operation: &str) {
        let mut log = self.request_log.lock().await;
        log.push(format!("{}: {}", Utc::now().format("%H:%M:%S%.3f"), operation));
    }

    async fn apply_latency_and_failure(&self, operation: &str) -> Result<(), Status> {
        self.log_request(operation).await;
        
        if !self.simulated_latency.is_zero() {
            tokio::time::sleep(self.simulated_latency).await;
        }

        if self.fail_requests {
            return Err(Status::internal(self.error_message.clone()));
        }

        Ok(())
    }
}

#[async_trait]
impl broker::broker_replication_service_server::BrokerReplicationService for MockBrokerReplicationService {
    async fn replicate_data(
        &self,
        request: Request<broker::ReplicateDataRequest>,
    ) -> Result<Response<broker::ReplicateDataResponse>, Status> {
        self.apply_latency_and_failure("replicate_data").await?;
        
        let req = request.into_inner();
        let response = broker::ReplicateDataResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            follower_state: Some(common::FollowerState {
                broker_id: self.broker_id.clone(),
                last_known_offset: req.records.len() as u64,
                last_heartbeat: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                lag: 0,
                lag_time_ms: 0,
                in_sync: true,
            }),
            metadata: Some(common::ResponseMetadata {
                correlation_id: "mock-response".to_string(),
                timestamp: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                error_code: 0,
                error_message: String::new(),
                throttle_time_ms: self.simulated_latency.as_millis() as u32,
            }),
            bytes_replicated: req.batch_size_bytes,
            records_replicated: req.batch_record_count as u64,
            replication_time_ms: self.simulated_latency.as_millis() as u64,
            follower_log_end_offset: req.records.len() as u64,
            follower_high_watermark: req.records.len() as u64,
        };

        Ok(Response::new(response))
    }

    async fn send_heartbeat(
        &self,
        request: Request<broker::HeartbeatRequest>,
    ) -> Result<Response<broker::HeartbeatResponse>, Status> {
        self.apply_latency_and_failure("send_heartbeat").await?;
        
        let _req = request.into_inner();
        let response = broker::HeartbeatResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            follower_state: Some(common::FollowerState {
                broker_id: self.broker_id.clone(),
                last_known_offset: 1000,
                last_heartbeat: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                lag: 5,
                lag_time_ms: 100,
                in_sync: true,
            }),
            metadata: None,
            follower_cpu_usage: 25.5,
            follower_memory_usage: 60.0,
            follower_disk_usage_bytes: 1024 * 1024 * 1024,
            follower_network_in_bytes: 1024 * 100,
            follower_network_out_bytes: 1024 * 50,
            supported_compression: vec!["none".to_string(), "lz4".to_string(), "zstd".to_string()],
            max_batch_size_bytes: 1024 * 1024,
        };

        Ok(Response::new(response))
    }

    async fn transfer_leadership(
        &self,
        request: Request<broker::TransferLeadershipRequest>,
    ) -> Result<Response<broker::TransferLeadershipResponse>, Status> {
        self.apply_latency_and_failure("transfer_leadership").await?;
        
        let req = request.into_inner();
        let response = broker::TransferLeadershipResponse {
            success: req.new_leader_id != req.current_leader_id, // Succeed if different leaders
            error_code: 0,
            error_message: String::new(),
            new_leader_epoch: req.current_leader_epoch + 1,
            metadata: None,
            actual_new_leader_id: req.new_leader_id.clone(),
            transfer_time_ms: self.simulated_latency.as_millis() as u64,
            final_log_end_offset: 1000,
            new_isr: vec![req.new_leader_id, self.broker_id.clone()],
        };

        Ok(Response::new(response))
    }

    async fn assign_partition(
        &self,
        request: Request<broker::AssignPartitionRequest>,
    ) -> Result<Response<broker::AssignPartitionResponse>, Status> {
        self.apply_latency_and_failure("assign_partition").await?;
        
        let _req = request.into_inner();
        let response = broker::AssignPartitionResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            assigned_log_end_offset: 0,
            assigned_wal_path: "/tmp/test-wal".to_string(),
            estimated_setup_time_ms: 1000,
            allocated_memory_bytes: 1024 * 1024 * 100, // 100MB
            allocated_disk_bytes: 1024 * 1024 * 1024, // 1GB
            allocated_network_mbs: 10,
        };

        Ok(Response::new(response))
    }

    async fn remove_partition(
        &self,
        request: Request<broker::RemovePartitionRequest>,
    ) -> Result<Response<broker::RemovePartitionResponse>, Status> {
        self.apply_latency_and_failure("remove_partition").await?;
        
        let _req = request.into_inner();
        let response = broker::RemovePartitionResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            final_log_end_offset: 1000,
            removed_data_bytes: 1024 * 1024 * 50, // 50MB
            removal_time_ms: self.simulated_latency.as_millis() as u64,
            freed_memory_bytes: 1024 * 1024 * 100,
            freed_disk_bytes: 1024 * 1024 * 1024,
            cleanup_status: "completed".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn get_replication_status(
        &self,
        request: Request<broker::ReplicationStatusRequest>,
    ) -> Result<Response<broker::ReplicationStatusResponse>, Status> {
        self.apply_latency_and_failure("get_replication_status").await?;
        
        let req = request.into_inner();
        let response = broker::ReplicationStatusResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            current_leader: "leader-1".to_string(),
            leader_epoch: 5,
            in_sync_replicas: vec!["leader-1".to_string(), self.broker_id.clone()],
            out_of_sync_replicas: vec![],
            replication_lag_messages: 10,
            replication_lag_time_ms: 100,
            replication_throughput_mbs: 50.0,
            follower_details: if req.include_follower_details {
                vec![broker::FollowerDetails {
                    broker_id: self.broker_id.clone(),
                    state: Some(common::FollowerState {
                        broker_id: self.broker_id.clone(),
                        last_known_offset: 1000,
                        last_heartbeat: Some(prost_types::Timestamp {
                            seconds: Utc::now().timestamp(),
                            nanos: 0,
                        }),
                        lag: 10,
                        lag_time_ms: 100,
                        in_sync: true,
                    }),
                    last_fetch_offset: 1000,
                    last_fetch_time: Some(prost_types::Timestamp {
                        seconds: Utc::now().timestamp(),
                        nanos: 0,
                    }),
                    fetch_rate_mbs: 25,
                    error_count: 0,
                    last_error: String::new(),
                    health_score: 0.95,
                }]
            } else {
                vec![]
            },
        };

        Ok(Response::new(response))
    }

    async fn sync_isr(
        &self,
        request: Request<broker::SyncIsrRequest>,
    ) -> Result<Response<broker::SyncIsrResponse>, Status> {
        self.apply_latency_and_failure("sync_isr").await?;
        
        let req = request.into_inner();
        let response = broker::SyncIsrResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            approved_isr: req.proposed_isr, // Approve the proposed ISR
            isr_version: 1,
            controller_comment: "ISR update approved".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn truncate_log(
        &self,
        request: Request<broker::TruncateLogRequest>,
    ) -> Result<Response<broker::TruncateLogResponse>, Status> {
        self.apply_latency_and_failure("truncate_log").await?;
        
        let req = request.into_inner();
        let response = broker::TruncateLogResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            actual_truncate_offset: req.truncate_offset,
            truncated_bytes: 1024 * 100, // 100KB truncated
            truncation_time_ms: self.simulated_latency.as_millis() as u64,
            new_log_end_offset: req.truncate_offset,
            new_high_watermark: req.truncate_offset,
        };

        Ok(Response::new(response))
    }

    async fn health_check(
        &self,
        request: Request<broker::HealthCheckRequest>,
    ) -> Result<Response<broker::HealthCheckResponse>, Status> {
        self.apply_latency_and_failure("health_check").await?;
        
        let _req = request.into_inner();
        let response = broker::HealthCheckResponse {
            overall_healthy: true,
            broker_id: self.broker_id.clone(),
            timestamp: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            uptime_seconds: 3600,
            metadata: Some(common::ResponseMetadata {
                correlation_id: "mock-health-response".to_string(),
                timestamp: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                error_code: 0,
                error_message: String::new(),
                throttle_time_ms: self.simulated_latency.as_millis() as u32,
            }),
            wal_health: Some(broker::ComponentHealth {
                status: broker::HealthStatus::Healthy as i32,
                last_check: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                latency_ms: 5,
                error_count: 0,
                last_error: String::new(),
                details: std::collections::HashMap::new(),
                throughput_ops_per_sec: 100.0,
                total_operations: 10000,
                failed_operations: 0,
            }),
            cache_health: Some(broker::ComponentHealth {
                status: broker::HealthStatus::Healthy as i32,
                last_check: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                latency_ms: 2,
                error_count: 0,
                last_error: String::new(),
                details: std::collections::HashMap::new(),
                throughput_ops_per_sec: 500.0,
                total_operations: 50000,
                failed_operations: 0,
            }),
            object_storage_health: Some(broker::ComponentHealth {
                status: broker::HealthStatus::Healthy as i32,
                last_check: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                latency_ms: 50,
                error_count: 0,
                last_error: String::new(),
                details: std::collections::HashMap::new(),
                throughput_ops_per_sec: 10.0,
                total_operations: 1000,
                failed_operations: 0,
            }),
            network_health: Some(broker::ComponentHealth {
                status: broker::HealthStatus::Healthy as i32,
                last_check: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                latency_ms: 10,
                error_count: 0,
                last_error: String::new(),
                details: std::collections::HashMap::new(),
                throughput_ops_per_sec: 200.0,
                total_operations: 20000,
                failed_operations: 0,
            }),
            replication_health: Some(broker::ComponentHealth {
                status: broker::HealthStatus::Healthy as i32,
                last_check: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                latency_ms: 15,
                error_count: 0,
                last_error: String::new(),
                details: std::collections::HashMap::new(),
                throughput_ops_per_sec: 75.0,
                total_operations: 7500,
                failed_operations: 0,
            }),
            resource_usage: Some(broker::ResourceUsage {
                cpu_usage_percent: 25.0,
                memory_usage_bytes: 1024 * 1024 * 512, // 512MB
                memory_total_bytes: 1024 * 1024 * 1024 * 2, // 2GB
                disk_usage_bytes: 1024 * 1024 * 1024, // 1GB
                disk_total_bytes: 1024 * 1024 * 1024 * 10, // 10GB
                network_in_bytes_per_sec: 1024 * 100,
                network_out_bytes_per_sec: 1024 * 50,
                open_file_descriptors: 100,
                active_connections: 10,
                heap_usage_bytes: 1024 * 1024 * 256, // 256MB
                heap_total_bytes: 1024 * 1024 * 512, // 512MB
                gc_count: 5,
                gc_time_ms: 10,
            }),
            partition_count: 5,
            error_summary: String::new(),
        };

        Ok(Response::new(response))
    }
}

/// Mock implementation of ControllerRaftService for testing
pub struct MockControllerRaftService {
    pub node_id: String,
    pub current_term: Arc<Mutex<u64>>,
    pub voted_for: Arc<Mutex<Option<String>>>,
    pub is_leader: Arc<Mutex<bool>>,
    pub fail_requests: bool,
    pub error_code: u32,
    pub simulated_latency: Duration,
    pub request_log: Arc<Mutex<Vec<String>>>,
}

impl MockControllerRaftService {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            current_term: Arc::new(Mutex::new(0)),
            voted_for: Arc::new(Mutex::new(None)),
            is_leader: Arc::new(Mutex::new(false)),
            fail_requests: false,
            error_code: 0,
            simulated_latency: Duration::from_millis(0),
            request_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_failure(mut self, error_code: u32) -> Self {
        self.fail_requests = true;
        self.error_code = error_code;
        self
    }

    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.simulated_latency = latency;
        self
    }

    pub async fn set_leader(&self, is_leader: bool) {
        *self.is_leader.lock().await = is_leader;
    }

    pub async fn set_term(&self, term: u64) {
        *self.current_term.lock().await = term;
    }

    async fn log_request(&self, operation: &str) {
        let mut log = self.request_log.lock().await;
        log.push(format!("{}: {}", Utc::now().format("%H:%M:%S%.3f"), operation));
    }

    async fn apply_latency_and_failure(&self, operation: &str) -> Result<(), Status> {
        self.log_request(operation).await;
        
        if !self.simulated_latency.is_zero() {
            tokio::time::sleep(self.simulated_latency).await;
        }

        if self.fail_requests {
            return Err(Status::internal(format!("Mock failure: {}", self.error_code)));
        }

        Ok(())
    }
}

#[async_trait]
impl controller::controller_raft_service_server::ControllerRaftService for MockControllerRaftService {
    async fn request_vote(
        &self,
        request: Request<controller::RequestVoteRequest>,
    ) -> Result<Response<controller::RequestVoteResponse>, Status> {
        self.apply_latency_and_failure("request_vote").await?;
        
        let req = request.into_inner();
        let mut current_term = self.current_term.lock().await;
        let mut voted_for = self.voted_for.lock().await;

        // Raft vote logic simulation
        let vote_granted = if req.term > *current_term {
            *current_term = req.term;
            *voted_for = Some(req.candidate_id.clone());
            true
        } else if req.term == *current_term && voted_for.is_none() {
            *voted_for = Some(req.candidate_id.clone());
            true
        } else {
            false
        };

        let response = controller::RequestVoteResponse {
            term: *current_term,
            vote_granted,
            voter_id: self.node_id.clone(),
            metadata: Some(common::ResponseMetadata {
                correlation_id: "mock-vote-response".to_string(),
                timestamp: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                error_code: 0,
                error_message: String::new(),
                throttle_time_ms: self.simulated_latency.as_millis() as u32,
            }),
            vote_reason: if vote_granted { "vote_granted".to_string() } else { "vote_denied".to_string() },
            voter_last_log_index: 100,
            voter_last_log_term: *current_term,
            voter_version: "1.0.0".to_string(),
            voter_capabilities: vec!["consensus".to_string(), "replication".to_string()],
            voter_cpu_usage: 30.0,
            voter_memory_usage: 70.0,
        };

        Ok(Response::new(response))
    }

    async fn append_entries(
        &self,
        request: Request<controller::AppendEntriesRequest>,
    ) -> Result<Response<controller::AppendEntriesResponse>, Status> {
        self.apply_latency_and_failure("append_entries").await?;
        
        let req = request.into_inner();
        let current_term = *self.current_term.lock().await;

        let success = req.term >= current_term;
        let match_index = if success { req.prev_log_index + req.entries.len() as u64 } else { 0 };

        let response = controller::AppendEntriesResponse {
            term: current_term,
            success,
            follower_id: self.node_id.clone(),
            match_index,
            metadata: None,
            conflict_index: if success { 0 } else { req.prev_log_index },
            conflict_term: if success { 0 } else { current_term },
            first_index_of_term: 0,
            follower_log_size: 1000,
            follower_commit_index: 500,
            follower_applied_index: 500,
            append_time_ms: self.simulated_latency.as_millis() as u64,
            bytes_appended: if success { req.total_batch_size_bytes } else { 0 },
            preferred_max_entries: 1000,
            preferred_max_bytes: 1024 * 1024,
        };

        Ok(Response::new(response))
    }

    async fn install_snapshot(
        &self,
        request: Request<controller::InstallSnapshotRequest>,
    ) -> Result<Response<controller::InstallSnapshotResponse>, Status> {
        self.apply_latency_and_failure("install_snapshot").await?;
        
        let req = request.into_inner();
        let response = controller::InstallSnapshotResponse {
            term: *self.current_term.lock().await,
            success: true,
            follower_id: self.node_id.clone(),
            bytes_received: req.offset + req.data.len() as u64,
            metadata: None,
            chunks_received: req.chunk_index + 1,
            installation_progress: (req.chunk_index + 1) as f64 / req.total_chunks as f64,
            installation_time_ms: self.simulated_latency.as_millis() as u64,
            preferred_chunk_size: 64 * 1024, // 64KB
            available_disk_space: 1024 * 1024 * 1024 * 10, // 10GB
            cpu_usage: 25.0,
            memory_usage: 60.0,
            error_details: String::new(),
            retry_chunk: false,
        };

        Ok(Response::new(response))
    }

    async fn pre_vote(
        &self,
        request: Request<controller::PreVoteRequest>,
    ) -> Result<Response<controller::PreVoteResponse>, Status> {
        self.apply_latency_and_failure("pre_vote").await?;
        
        let req = request.into_inner();
        let current_term = *self.current_term.lock().await;
        
        let response = controller::PreVoteResponse {
            term: current_term,
            vote_granted: req.term > current_term,
            voter_id: self.node_id.clone(),
            metadata: None,
            vote_reason: "pre_vote_granted".to_string(),
            has_leader: *self.is_leader.lock().await,
            current_leader_id: if *self.is_leader.lock().await { self.node_id.clone() } else { String::new() },
        };

        Ok(Response::new(response))
    }

    async fn transfer_leadership(
        &self,
        request: Request<controller::TransferLeadershipRequest>,
    ) -> Result<Response<controller::TransferLeadershipResponse>, Status> {
        self.apply_latency_and_failure("transfer_leadership").await?;
        
        let req = request.into_inner();
        let response = controller::TransferLeadershipResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            new_leader_id: req.target_leader_id,
            new_term: *self.current_term.lock().await + 1,
            metadata: None,
            transfer_time_ms: self.simulated_latency.as_millis() as u64,
            final_log_index: 1000,
        };

        Ok(Response::new(response))
    }

    async fn get_cluster_info(
        &self,
        request: Request<controller::GetClusterInfoRequest>,
    ) -> Result<Response<controller::GetClusterInfoResponse>, Status> {
        self.apply_latency_and_failure("get_cluster_info").await?;
        
        let req = request.into_inner();
        let is_leader = *self.is_leader.lock().await;
        
        let response = controller::GetClusterInfoResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: req.metadata.map(|m| common::ResponseMetadata {
                correlation_id: m.correlation_id,
                timestamp: Some(prost_types::Timestamp {
                    seconds: Utc::now().timestamp(),
                    nanos: 0,
                }),
                error_code: 0,
                error_message: String::new(),
                throttle_time_ms: 0,
            }),
            cluster_id: "test-cluster".to_string(),
            current_leader_id: if is_leader { self.node_id.clone() } else { "unknown".to_string() },
            current_term: *self.current_term.lock().await,
            nodes: if req.include_node_details {
                vec![controller::NodeInfo {
                    node_id: self.node_id.clone(),
                    address: "127.0.0.1:9090".to_string(),
                    state: if is_leader { 
                        controller::NodeState::Leader 
                    } else { 
                        controller::NodeState::Follower 
                    } as i32,
                    term: *self.current_term.lock().await,
                    last_log_index: 1000,
                    last_log_term: *self.current_term.lock().await,
                    commit_index: 500,
                    applied_index: 500,
                    last_contact: Some(prost_types::Timestamp {
                        seconds: Utc::now().timestamp(),
                        nanos: 0,
                    }),
                    health: Some(controller::NodeHealth {
                        status: controller::HealthStatus::Healthy as i32,
                        cpu_usage: 30.0,
                        memory_usage: 60.0,
                        disk_usage_bytes: 1024 * 1024 * 1024,
                        network_in_bytes_per_sec: 1024 * 100,
                        network_out_bytes_per_sec: 1024 * 50,
                        warnings: vec![],
                    }),
                    performance: if req.include_performance_metrics {
                        Some(common::PerformanceMetrics {
                            messages_per_second: 100,
                            bytes_per_second: 1024 * 100,
                            avg_latency_ms: 10.0,
                            p99_latency_ms: 25.0,
                            total_messages: 1000000,
                            total_bytes: 1024 * 1024 * 100,
                            error_count: 10,
                            start_time: Some(prost_types::Timestamp {
                                seconds: Utc::now().timestamp() - 3600, // 1 hour ago
                                nanos: 0,
                            }),
                            last_updated: Some(prost_types::Timestamp {
                                seconds: Utc::now().timestamp(),
                                nanos: 0,
                            }),
                        })
                    } else {
                        None
                    },
                    version: "1.0.0".to_string(),
                    capabilities: vec!["consensus".to_string(), "replication".to_string()],
                    priority: 100,
                }]
            } else {
                vec![]
            },
            commit_index: 500,
            applied_index: 500,
            log_size: 1000,
            snapshot_index: 0,
            health: Some(controller::ClusterHealth {
                status: controller::HealthStatus::Healthy as i32,
                healthy_nodes: 1,
                total_nodes: 1,
                consensus_availability: 100.0,
                issues: vec![],
                average_latency_ms: 10.0,
                throughput_ops_per_second: 100.0,
            }),
            performance: if req.include_performance_metrics {
                Some(common::PerformanceMetrics {
                    messages_per_second: 100,
                    bytes_per_second: 1024 * 100,
                    avg_latency_ms: 10.0,
                    p99_latency_ms: 25.0,
                    total_messages: 1000000,
                    total_bytes: 1024 * 1024 * 100,
                    error_count: 10,
                    start_time: Some(prost_types::Timestamp {
                        seconds: Utc::now().timestamp() - 3600, // 1 hour ago
                        nanos: 0,
                    }),
                    last_updated: Some(prost_types::Timestamp {
                        seconds: Utc::now().timestamp(),
                        nanos: 0,
                    }),
                })
            } else {
                None
            },
        };

        Ok(Response::new(response))
    }

    async fn add_node(
        &self,
        request: Request<controller::AddNodeRequest>,
    ) -> Result<Response<controller::AddNodeResponse>, Status> {
        self.apply_latency_and_failure("add_node").await?;
        
        let _req = request.into_inner();
        let response = controller::AddNodeResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            configuration_index: 1001,
            configuration_term: *self.current_term.lock().await,
        };

        Ok(Response::new(response))
    }

    async fn remove_node(
        &self,
        request: Request<controller::RemoveNodeRequest>,
    ) -> Result<Response<controller::RemoveNodeResponse>, Status> {
        self.apply_latency_and_failure("remove_node").await?;
        
        let _req = request.into_inner();
        let response = controller::RemoveNodeResponse {
            success: true,
            error_code: 0,
            error_message: String::new(),
            metadata: None,
            configuration_index: 1002,
            configuration_term: *self.current_term.lock().await,
        };

        Ok(Response::new(response))
    }
}

// ============================================================================
// Performance Measurement Utilities
// ============================================================================

pub struct PerformanceMetrics {
    pub operation_name: String,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub request_count: u64,
    pub error_count: u64,
    pub total_bytes: u64,
}

impl PerformanceMetrics {
    pub fn new(operation_name: &str) -> Self {
        Self {
            operation_name: operation_name.to_string(),
            start_time: Instant::now(),
            end_time: None,
            request_count: 0,
            error_count: 0,
            total_bytes: 0,
        }
    }

    pub fn record_request(&mut self, bytes: u64, is_error: bool) {
        self.request_count += 1;
        self.total_bytes += bytes;
        if is_error {
            self.error_count += 1;
        }
    }

    pub fn finish(mut self) -> Self {
        self.end_time = Some(Instant::now());
        self
    }

    pub fn duration(&self) -> Duration {
        match self.end_time {
            Some(end) => end.duration_since(self.start_time),
            None => Instant::now().duration_since(self.start_time),
        }
    }

    pub fn requests_per_second(&self) -> f64 {
        let duration_secs = self.duration().as_secs_f64();
        if duration_secs > 0.0 {
            self.request_count as f64 / duration_secs
        } else {
            0.0
        }
    }

    pub fn throughput_mbps(&self) -> f64 {
        let duration_secs = self.duration().as_secs_f64();
        if duration_secs > 0.0 {
            (self.total_bytes as f64 / 1_000_000.0) / duration_secs
        } else {
            0.0
        }
    }

    pub fn error_rate(&self) -> f64 {
        if self.request_count > 0 {
            self.error_count as f64 / self.request_count as f64
        } else {
            0.0
        }
    }

    pub fn print_summary(&self) {
        println!("=== Performance Summary: {} ===", self.operation_name);
        println!("Duration: {:.2?}", self.duration());
        println!("Total Requests: {}", self.request_count);
        println!("Error Count: {}", self.error_count);
        println!("Error Rate: {:.2}%", self.error_rate() * 100.0);
        println!("Total Bytes: {}", self.total_bytes);
        println!("Requests/sec: {:.2}", self.requests_per_second());
        println!("Throughput: {:.2} MB/s", self.throughput_mbps());
        println!("Average Latency: {:.2} ms", 
            if self.request_count > 0 {
                self.duration().as_millis() as f64 / self.request_count as f64
            } else {
                0.0
            }
        );
    }
}

// ============================================================================
// Test Environment Utilities
// ============================================================================

pub struct TestEnvironment {
    pub temp_dirs: Vec<TempDir>,
    pub broker_configs: Vec<WalConfig>,
    pub controller_configs: Vec<ScalingConfig>,
}

impl TestEnvironment {
    pub fn new() -> Self {
        Self {
            temp_dirs: Vec::new(),
            broker_configs: Vec::new(),
            controller_configs: Vec::new(),
        }
    }

    pub fn add_broker_config(&mut self) -> WalConfig {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };
        
        self.temp_dirs.push(temp_dir);
        self.broker_configs.push(config.clone());
        config
    }

    pub fn add_controller_config(&mut self) -> ScalingConfig {
        let config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };
        
        self.controller_configs.push(config.clone());
        config
    }
}

// ============================================================================
// Error Simulation Utilities
// ============================================================================

pub struct ErrorSimulator {
    pub error_probability: f64,
    pub latency_range: (Duration, Duration),
    pub error_types: Vec<Status>,
}

impl ErrorSimulator {
    pub fn new() -> Self {
        Self {
            error_probability: 0.0,
            latency_range: (Duration::from_millis(0), Duration::from_millis(0)),
            error_types: vec![
                Status::internal("Simulated internal error"),
                Status::unavailable("Service temporarily unavailable"),
                Status::deadline_exceeded("Request timeout"),
                Status::resource_exhausted("Resource exhausted"),
            ],
        }
    }

    pub fn with_error_rate(mut self, probability: f64) -> Self {
        self.error_probability = probability.clamp(0.0, 1.0);
        self
    }

    pub fn with_latency_range(mut self, min: Duration, max: Duration) -> Self {
        self.latency_range = (min, max);
        self
    }

    pub async fn maybe_fail(&self) -> Result<(), Status> {
        // Simulate random latency
        if self.latency_range.0 < self.latency_range.1 {
            let latency = Duration::from_millis(
                thread_rng().gen_range(
                    self.latency_range.0.as_millis()..=self.latency_range.1.as_millis()
                ) as u64
            );
            tokio::time::sleep(latency).await;
        }

        // Simulate random failures
        if thread_rng().r#gen::<f64>() < self.error_probability {
            let error_idx = thread_rng().gen_range(0..self.error_types.len());
            return Err(self.error_types[error_idx].clone());
        }

        Ok(())
    }
}

// ============================================================================
// Test Helper Functions
// ============================================================================

/// Assert that a conversion roundtrip maintains data integrity
pub fn assert_conversion_roundtrip<T, P>(original: T) 
where
    T: Clone + PartialEq + std::fmt::Debug + TryInto<P>,
    P: TryInto<T>,
    T::Error: std::fmt::Debug,
    P::Error: std::fmt::Debug,
{
    let proto: P = original.clone().try_into()
        .expect("Failed to convert to protobuf");
    let back_to_internal: T = proto.try_into()
        .expect("Failed to convert back from protobuf");
    assert_eq!(original, back_to_internal, "Roundtrip conversion failed");
}

/// Generate test data with specified size constraints
pub fn generate_test_data(size_kb: usize) -> Vec<u8> {
    let size_bytes = size_kb * 1024;
    let mut data = Vec::with_capacity(size_bytes);
    let mut rng = thread_rng();
    
    for _ in 0..size_bytes {
        data.push(rng.r#gen());
    }
    
    data
}

/// Create a timeout for async operations
pub async fn with_timeout<F, T>(duration: Duration, future: F) -> Result<T, Status>
where
    F: std::future::Future<Output = Result<T, Status>>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(result) => result,
        Err(_) => Err(Status::deadline_exceeded("Operation timed out")),
    }
}