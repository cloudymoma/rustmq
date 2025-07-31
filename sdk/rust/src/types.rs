use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Topic metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicMetadata {
    pub name: String,
    pub partitions: Vec<PartitionMetadata>,
    pub replication_factor: u16,
    pub config: HashMap<String, String>,
    pub created_at: u64,
}

/// Partition metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionMetadata {
    pub id: u32,
    pub leader: Option<BrokerInfo>,
    pub replicas: Vec<BrokerInfo>,
    pub in_sync_replicas: Vec<BrokerInfo>,
    pub log_start_offset: u64,
    pub log_end_offset: u64,
}

/// Broker information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerInfo {
    pub id: u32,
    pub host: String,
    pub port: u16,
    pub rack: Option<String>,
}

/// Consumer group metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroupMetadata {
    pub group_id: String,
    pub state: ConsumerGroupState,
    pub members: Vec<ConsumerMember>,
    pub coordinator: BrokerInfo,
    pub protocol_type: String,
    pub protocol: String,
}

/// Consumer group states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsumerGroupState {
    Unknown,
    PreparingRebalance,
    CompletingRebalance,
    Stable,
    Dead,
    Empty,
}

/// Consumer group member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerMember {
    pub member_id: String,
    pub client_id: String,
    pub client_host: String,
    pub assignment: Vec<TopicPartition>,
}

/// Topic and partition identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TopicPartition {
    pub topic: String,
    pub partition: u32,
}

/// Offset and metadata for a topic partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetAndMetadata {
    pub offset: u64,
    pub metadata: Option<String>,
    pub commit_timestamp: u64,
}

/// Cluster metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMetadata {
    pub cluster_id: String,
    pub brokers: Vec<BrokerInfo>,
    pub controller: Option<BrokerInfo>,
    pub topics: Vec<TopicMetadata>,
}

/// Message delivery report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryReport {
    pub message_id: String,
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub timestamp: u64,
    pub error: Option<String>,
}

/// Producer transaction information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub transaction_id: String,
    pub producer_id: u64,
    pub producer_epoch: u16,
    pub timeout_ms: u32,
    pub state: TransactionState,
}

/// Transaction states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionState {
    Empty,
    Ongoing,
    PrepareCommit,
    PrepareAbort,
    CompleteCommit,
    CompleteAbort,
    Dead,
    PrepareEpochFence,
}

/// Partition assignment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionAssignment {
    pub topic: String,
    pub partitions: Vec<u32>,
}

/// Consumer group assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAssignment {
    pub member_id: String,
    pub assignments: Vec<PartitionAssignment>,
}

/// Broker connection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerStats {
    pub broker_id: u32,
    pub connection_count: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub requests_sent: u64,
    pub responses_received: u64,
    pub errors: u64,
    pub last_heartbeat: u64,
}

/// Topic configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    pub name: String,
    pub partition_count: u32,
    pub replication_factor: u16,
    pub config: HashMap<String, String>,
}

/// Admin operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminResult<T> {
    pub success: bool,
    pub result: Option<T>,
    pub error: Option<String>,
}

/// Watermark information for a partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watermarks {
    pub low: u64,
    pub high: u64,
}

/// Consumer lag information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerLag {
    pub topic: String,
    pub partition: u32,
    pub current_offset: u64,
    pub log_end_offset: u64,
    pub lag: u64,
}

impl TopicPartition {
    /// Create a new TopicPartition
    pub fn new(topic: String, partition: u32) -> Self {
        Self { topic, partition }
    }
}

impl OffsetAndMetadata {
    /// Create a new OffsetAndMetadata
    pub fn new(offset: u64, metadata: Option<String>) -> Self {
        let commit_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            offset,
            metadata,
            commit_timestamp,
        }
    }
}

impl ConsumerLag {
    /// Calculate lag
    pub fn calculate_lag(current_offset: u64, log_end_offset: u64) -> u64 {
        log_end_offset.saturating_sub(current_offset)
    }
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: HealthStatus,
    pub details: HashMap<String, String>,
    pub timestamp: u64,
}

/// Health status enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Feature flags for client capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub idempotent_producer: bool,
    pub transactional_producer: bool,
    pub exactly_once_semantics: bool,
    pub stream_processing: bool,
    pub compression: bool,
    pub encryption: bool,
    pub monitoring: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            idempotent_producer: true,
            transactional_producer: false,
            exactly_once_semantics: false,
            stream_processing: true,
            compression: true,
            encryption: false,
            monitoring: true,
        }
    }
}