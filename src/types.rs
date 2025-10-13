use serde::{Deserialize, Serialize, Deserializer, Serializer};
use std::fmt;
use bytes::Bytes;

// Custom serde implementations for Bytes to maintain serialization compatibility
mod bytes_serde {
    use super::*;
    
    pub fn serialize<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes)
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<u8> = Vec::deserialize(deserializer)?;
        Ok(Bytes::from(vec))
    }
}

mod option_bytes_serde {
    use super::*;
    
    pub fn serialize<S>(opt_bytes: &Option<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt_bytes {
            Some(bytes) => serializer.serialize_some(bytes.as_ref()),
            None => serializer.serialize_none(),
        }
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Bytes>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt_vec: Option<Vec<u8>> = Option::deserialize(deserializer)?;
        Ok(opt_vec.map(Bytes::from))
    }
}

pub type BrokerId = String;
pub type TopicName = String;
pub type PartitionId = u32;
pub type Offset = u64;
pub type StreamId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct TopicPartition {
    pub topic: TopicName,
    pub partition: PartitionId,
}

impl fmt::Display for TopicPartition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.topic, self.partition)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    #[serde(with = "option_bytes_serde")]
    pub key: Option<Bytes>,
    #[serde(with = "bytes_serde")]
    pub value: Bytes,
    pub headers: Vec<Header>,
    pub timestamp: i64,
}

impl Record {
    /// Create a new Record with Vec<u8> data (will be converted to Bytes)
    pub fn new(key: Option<Vec<u8>>, value: Vec<u8>, headers: Vec<Header>, timestamp: i64) -> Self {
        Self {
            key: key.map(Bytes::from),
            value: Bytes::from(value),
            headers,
            timestamp,
        }
    }
    
    /// Create a new Record with Bytes data (zero-copy)
    pub fn from_bytes(key: Option<Bytes>, value: Bytes, headers: Vec<Header>, timestamp: i64) -> Self {
        Self {
            key,
            value,
            headers,
            timestamp,
        }
    }
    
    /// Get the key as a Vec<u8> (copies data)
    pub fn key_as_vec(&self) -> Option<Vec<u8>> {
        self.key.as_ref().map(|k| k.to_vec())
    }
    
    /// Get the value as a Vec<u8> (copies data)
    pub fn value_as_vec(&self) -> Vec<u8> {
        self.value.to_vec()
    }
    
    /// Get a zero-copy slice of the value
    pub fn value_slice(&self, range: std::ops::Range<usize>) -> Bytes {
        self.value.slice(range)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    #[serde(with = "bytes_serde")]
    pub value: Bytes,
}

impl Header {
    /// Create a new Header with Vec<u8> value (will be converted to Bytes)
    pub fn new(key: String, value: Vec<u8>) -> Self {
        Self {
            key,
            value: Bytes::from(value),
        }
    }
    
    /// Create a new Header with Bytes value (zero-copy)
    pub fn from_bytes(key: String, value: Bytes) -> Self {
        Self {
            key,
            value,
        }
    }
    
    /// Get the value as a Vec<u8> (copies data)
    pub fn value_as_vec(&self) -> Vec<u8> {
        self.value.to_vec()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalRecord {
    pub topic_partition: TopicPartition,
    pub offset: Offset,
    pub record: Record,
    pub crc32: u32,
}

impl WalRecord {
    pub fn size(&self) -> usize {
        bincode::serialized_size(self).unwrap_or(0) as usize
    }

    pub fn serialize_to_buffer(&self, buffer: &mut [u8]) -> crate::Result<usize> {
        let serialized = bincode::serialize(self)?;
        if serialized.len() > buffer.len() {
            return Err(crate::error::RustMqError::BufferTooSmall);
        }
        buffer[..serialized.len()].copy_from_slice(&serialized);
        Ok(serialized.len())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrokerInfo {
    pub id: BrokerId,
    pub host: String,
    pub port_quic: u16,
    pub port_rpc: u16,
    pub rack_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    pub name: TopicName,
    pub partition_count: u32,
    pub replication_factor: u32,
    pub retention_policy: RetentionPolicy,
    pub compression: CompressionType,
    pub etl_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetentionPolicy {
    Time { retention_ms: i64 },
    Size { retention_bytes: u64 },
    Infinite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Lz4,
    Zstd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub id: PartitionId,
    pub topic: TopicName,
    pub leader: Option<BrokerId>,
    pub replicas: Vec<BrokerId>,
    pub in_sync_replicas: Vec<BrokerId>,
    pub leader_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AcknowledgmentLevel {
    None,
    Leader,
    Majority,
    All,
    Custom(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurabilityLevel {
    LocalOnly,
    Durable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationResult {
    pub offset: Offset,
    pub durability: DurabilityLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowerState {
    pub broker_id: BrokerId,
    pub last_known_offset: Offset,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub lag: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProduceRequest {
    pub topic: TopicName,
    pub partition_id: PartitionId,
    pub records: Vec<Record>,
    pub acks: AcknowledgmentLevel,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProduceResponse {
    pub offset: Offset,
    pub error_code: u32,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchRequest {
    pub topic: TopicName,
    pub partition_id: PartitionId,
    pub fetch_offset: Offset,
    pub max_bytes: u32,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResponse {
    pub records: Vec<Record>,
    pub high_watermark: Offset,
    pub error_code: u32,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicateDataRequest {
    pub leader_epoch: u64,
    pub topic_partition: TopicPartition,
    pub records: Vec<WalRecord>,
    pub leader_id: BrokerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicateDataResponse {
    pub success: bool,
    pub error_code: u32,
    pub error_message: Option<String>,
    pub follower_state: Option<FollowerState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub leader_epoch: u64,
    pub leader_id: BrokerId,
    pub topic_partition: TopicPartition,
    pub high_watermark: Offset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub error_code: u32,
    pub error_message: Option<String>,
    pub follower_state: Option<FollowerState>,
}

/// Leadership transfer request with epoch enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferLeadershipRequest {
    pub topic_partition: TopicPartition,
    pub current_leader_id: BrokerId,
    pub current_leader_epoch: u64,
    pub new_leader_id: BrokerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferLeadershipResponse {
    pub success: bool,
    pub error_code: u32,
    pub error_message: Option<String>,
    pub new_leader_epoch: Option<u64>,
}

/// Partition assignment request from controller
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignPartitionRequest {
    pub topic_partition: TopicPartition,
    pub replica_set: Vec<BrokerId>,
    pub leader_id: BrokerId,
    pub leader_epoch: u64,
    pub controller_id: BrokerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignPartitionResponse {
    pub success: bool,
    pub error_code: u32,
    pub error_message: Option<String>,
}

/// Partition removal request from controller
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovePartitionRequest {
    pub topic_partition: TopicPartition,
    pub controller_id: BrokerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovePartitionResponse {
    pub success: bool,
    pub error_code: u32,
    pub error_message: Option<String>,
}

/// Client request types for QUIC server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    Produce = 1,
    Fetch = 2,
    Metadata = 3,
}

impl TryFrom<u8> for RequestType {
    type Error = crate::error::RustMqError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(RequestType::Produce),
            2 => Ok(RequestType::Fetch),
            3 => Ok(RequestType::Metadata),
            _ => Err(crate::error::RustMqError::InvalidOperation(
                format!("Invalid request type: {}", value)
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRequest {
    pub topics: Option<Vec<TopicName>>,
    pub include_cluster_info: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataResponse {
    pub brokers: Vec<BrokerInfo>,
    pub topics: Vec<TopicMetadata>,
    pub cluster_id: String,
    pub controller_id: Option<BrokerId>,
    pub error_code: u32,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicMetadata {
    pub name: TopicName,
    pub partitions: Vec<PartitionInfo>,
    pub error_code: u32,
    pub error_message: Option<String>,
}

/// Comprehensive health check request for broker components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckRequest {
    pub check_wal: bool,
    pub check_cache: bool,
    pub check_object_storage: bool,
    pub check_network: bool,
    pub check_replication: bool,
    pub timeout_ms: Option<u32>,
}

/// Detailed health check response with component-specific status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub overall_healthy: bool,
    pub broker_id: BrokerId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub uptime_seconds: u64,
    pub wal_health: ComponentHealth,
    pub cache_health: ComponentHealth,
    pub object_storage_health: ComponentHealth,
    pub network_health: ComponentHealth,
    pub replication_health: ComponentHealth,
    pub resource_usage: ResourceUsage,
    pub partition_count: u32,
    pub error_summary: Option<String>,
}

/// Health status for individual broker components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub latency_ms: Option<u32>,
    pub error_count: u32,
    pub last_error: Option<String>,
    pub details: std::collections::HashMap<String, String>,
}

/// Health status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Resource usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_total_bytes: u64,
    pub disk_usage_bytes: u64,
    pub disk_total_bytes: u64,
    pub network_in_bytes_per_sec: u64,
    pub network_out_bytes_per_sec: u64,
    pub open_file_descriptors: u32,
    pub active_connections: u32,
}

impl Default for HealthCheckRequest {
    fn default() -> Self {
        Self {
            check_wal: true,
            check_cache: true,
            check_object_storage: true,
            check_network: true,
            check_replication: true,
            timeout_ms: Some(5000),
        }
    }
}

impl ComponentHealth {
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            last_check: chrono::Utc::now(),
            latency_ms: None,
            error_count: 0,
            last_error: None,
            details: std::collections::HashMap::new(),
        }
    }

    pub fn unhealthy(error: String) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            last_check: chrono::Utc::now(),
            latency_ms: None,
            error_count: 1,
            last_error: Some(error),
            details: std::collections::HashMap::new(),
        }
    }

    pub fn degraded(latency_ms: u32) -> Self {
        Self {
            status: HealthStatus::Degraded,
            last_check: chrono::Utc::now(),
            latency_ms: Some(latency_ms),
            error_count: 0,
            last_error: None,
            details: std::collections::HashMap::new(),
        }
    }
}