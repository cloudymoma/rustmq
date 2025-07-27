use serde::{Deserialize, Serialize};
use std::fmt;

pub type BrokerId = String;
pub type TopicName = String;
pub type PartitionId = u32;
pub type Offset = u64;
pub type StreamId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    pub key: Option<Vec<u8>>,
    pub value: Vec<u8>,
    pub headers: Vec<Header>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    pub value: Vec<u8>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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