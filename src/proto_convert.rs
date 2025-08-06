//! Type conversion layer between internal RustMQ types and protobuf types
//! 
//! This module provides comprehensive bidirectional conversion implementations
//! between existing Rust types in `types.rs` and the generated protobuf types.
//! All conversions are designed to be zero-copy where possible and maintain
//! semantic correctness.

use crate::{
    types as internal,
    proto::{common, broker},
    error::RustMqError,
};
use prost_types::Timestamp;
use chrono::{DateTime, Utc};

/// Result type for conversion operations
pub type ConversionResult<T> = Result<T, ConversionError>;

/// Conversion error types
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("Missing required field: {field}")]
    MissingField { field: String },
    
    #[error("Invalid timestamp: {error}")]
    InvalidTimestamp { error: String },
    
    #[error("Invalid enum value: {value} for enum {enum_name}")]
    InvalidEnumValue { value: i32, enum_name: String },
    
    #[error("Invalid data format: {message}")]
    InvalidFormat { message: String },
}

// ============================================================================
// Core Type Conversions: TopicPartition
// ============================================================================

impl From<internal::TopicPartition> for common::TopicPartition {
    fn from(tp: internal::TopicPartition) -> Self {
        Self {
            topic: tp.topic,
            partition: tp.partition,
        }
    }
}

impl From<common::TopicPartition> for internal::TopicPartition {
    fn from(tp: common::TopicPartition) -> Self {
        Self {
            topic: tp.topic,
            partition: tp.partition,
        }
    }
}

// ============================================================================
// Core Type Conversions: Record and Header
// ============================================================================

impl TryFrom<internal::Record> for common::Record {
    type Error = ConversionError;
    
    fn try_from(record: internal::Record) -> ConversionResult<Self> {
        let timestamp = timestamp_to_proto(record.timestamp)?;
        
        Ok(Self {
            key: record.key.unwrap_or_default(),
            value: record.value,
            headers: record.headers.into_iter().map(Into::into).collect(),
            timestamp: Some(timestamp),
        })
    }
}

impl TryFrom<common::Record> for internal::Record {
    type Error = ConversionError;
    
    fn try_from(record: common::Record) -> ConversionResult<Self> {
        let timestamp = record.timestamp
            .map(timestamp_from_proto)
            .transpose()?
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            
        Ok(Self {
            key: if record.key.is_empty() { None } else { Some(record.key) },
            value: record.value,
            headers: record.headers.into_iter().map(Into::into).collect(),
            timestamp,
        })
    }
}

impl From<internal::Header> for common::Header {
    fn from(header: internal::Header) -> Self {
        Self {
            key: header.key,
            value: header.value,
        }
    }
}

impl From<common::Header> for internal::Header {
    fn from(header: common::Header) -> Self {
        Self {
            key: header.key,
            value: header.value,
        }
    }
}

// ============================================================================
// Core Type Conversions: WalRecord
// ============================================================================

impl TryFrom<internal::WalRecord> for common::WalRecord {
    type Error = ConversionError;
    
    fn try_from(wal_record: internal::WalRecord) -> ConversionResult<Self> {
        Ok(Self {
            topic_partition: Some(wal_record.topic_partition.into()),
            offset: wal_record.offset,
            record: Some(wal_record.record.try_into()?),
            crc32: wal_record.crc32,
            version: 1, // Default version for compatibility
        })
    }
}

impl TryFrom<common::WalRecord> for internal::WalRecord {
    type Error = ConversionError;
    
    fn try_from(wal_record: common::WalRecord) -> ConversionResult<Self> {
        let topic_partition = wal_record.topic_partition
            .ok_or_else(|| ConversionError::MissingField { 
                field: "topic_partition".to_string() 
            })?;
            
        let record = wal_record.record
            .ok_or_else(|| ConversionError::MissingField { 
                field: "record".to_string() 
            })?;
            
        Ok(Self {
            topic_partition: topic_partition.into(),
            offset: wal_record.offset,
            record: record.try_into()?,
            crc32: wal_record.crc32,
        })
    }
}

// ============================================================================
// Broker Information Conversions
// ============================================================================

impl TryFrom<internal::BrokerInfo> for common::BrokerInfo {
    type Error = ConversionError;
    
    fn try_from(broker: internal::BrokerInfo) -> ConversionResult<Self> {
        Ok(Self {
            id: broker.id,
            host: broker.host,
            port_quic: broker.port_quic as u32,
            port_rpc: broker.port_rpc as u32,
            rack_id: broker.rack_id,
            status: common::BrokerStatus::Online as i32, // Default status
            version: 1, // Default version
        })
    }
}

impl TryFrom<common::BrokerInfo> for internal::BrokerInfo {
    type Error = ConversionError;
    
    fn try_from(broker: common::BrokerInfo) -> ConversionResult<Self> {
        Ok(Self {
            id: broker.id,
            host: broker.host,
            port_quic: broker.port_quic as u16,
            port_rpc: broker.port_rpc as u16,
            rack_id: broker.rack_id,
        })
    }
}

// ============================================================================
// Enum Conversions
// ============================================================================

impl From<internal::AcknowledgmentLevel> for common::AcknowledgmentLevel {
    fn from(ack: internal::AcknowledgmentLevel) -> Self {
        match ack {
            internal::AcknowledgmentLevel::None => Self::None,
            internal::AcknowledgmentLevel::Leader => Self::Leader,
            internal::AcknowledgmentLevel::Majority => Self::Majority,
            internal::AcknowledgmentLevel::All => Self::All,
            internal::AcknowledgmentLevel::Custom(_) => Self::Custom,
        }
    }
}

impl From<common::AcknowledgmentLevel> for internal::AcknowledgmentLevel {
    fn from(ack: common::AcknowledgmentLevel) -> Self {
        match ack {
            common::AcknowledgmentLevel::None => Self::None,
            common::AcknowledgmentLevel::Leader => Self::Leader,
            common::AcknowledgmentLevel::Majority => Self::Majority,
            common::AcknowledgmentLevel::All => Self::All,
            common::AcknowledgmentLevel::Custom => Self::Custom(3), // Default
        }
    }
}

impl From<internal::DurabilityLevel> for common::DurabilityLevel {
    fn from(durability: internal::DurabilityLevel) -> Self {
        match durability {
            internal::DurabilityLevel::LocalOnly => Self::LocalOnly,
            internal::DurabilityLevel::Durable => Self::Durable,
        }
    }
}

impl From<common::DurabilityLevel> for internal::DurabilityLevel {
    fn from(durability: common::DurabilityLevel) -> Self {
        match durability {
            common::DurabilityLevel::LocalOnly => Self::LocalOnly,
            common::DurabilityLevel::Durable => Self::Durable,
        }
    }
}

impl From<internal::CompressionType> for common::CompressionType {
    fn from(compression: internal::CompressionType) -> Self {
        match compression {
            internal::CompressionType::None => Self::None,
            internal::CompressionType::Lz4 => Self::Lz4,
            internal::CompressionType::Zstd => Self::Zstd,
        }
    }
}

impl From<common::CompressionType> for internal::CompressionType {
    fn from(compression: common::CompressionType) -> Self {
        match compression {
            common::CompressionType::None => Self::None,
            common::CompressionType::Lz4 => Self::Lz4,
            common::CompressionType::Zstd => Self::Zstd,
        }
    }
}

// ============================================================================
// Complex Type Conversions: FollowerState
// ============================================================================

impl TryFrom<internal::FollowerState> for common::FollowerState {
    type Error = ConversionError;
    
    fn try_from(state: internal::FollowerState) -> ConversionResult<Self> {
        let last_heartbeat = Some(timestamp_to_proto(state.last_heartbeat.timestamp_millis())?);
        
        Ok(Self {
            broker_id: state.broker_id,
            last_known_offset: state.last_known_offset,
            last_heartbeat,
            lag: state.lag,
            lag_time_ms: 0, // Not tracked in internal type
            in_sync: state.lag < 1000, // Heuristic: in sync if lag < 1000 messages
        })
    }
}

impl TryFrom<common::FollowerState> for internal::FollowerState {
    type Error = ConversionError;
    
    fn try_from(state: common::FollowerState) -> ConversionResult<Self> {
        let last_heartbeat = state.last_heartbeat
            .map(timestamp_from_proto)
            .transpose()?
            .map(|ts| DateTime::from_timestamp_millis(ts).unwrap_or_else(|| Utc::now()))
            .unwrap_or_else(|| Utc::now());
            
        Ok(Self {
            broker_id: state.broker_id,
            last_known_offset: state.last_known_offset,
            last_heartbeat,
            lag: state.lag,
        })
    }
}

// ============================================================================
// Request/Response Conversions: Replication
// ============================================================================

impl TryFrom<internal::ReplicateDataRequest> for broker::ReplicateDataRequest {
    type Error = ConversionError;
    
    fn try_from(req: internal::ReplicateDataRequest) -> ConversionResult<Self> {
        let records: Result<Vec<_>, _> = req.records.into_iter()
            .map(|r| r.try_into())
            .collect();
        
        Ok(Self {
            leader_epoch: req.leader_epoch,
            topic_partition: Some(req.topic_partition.into()),
            records: records?,
            leader_id: req.leader_id,
            leader_high_watermark: 0, // Not available in internal type
            request_id: 0, // Generate unique ID in real implementation
            metadata: None, // Not available in internal type
            batch_base_offset: 0,
            batch_record_count: 0,
            batch_size_bytes: 0,
            compression: common::CompressionType::None as i32,
        })
    }
}

impl TryFrom<broker::ReplicateDataRequest> for internal::ReplicateDataRequest {
    type Error = ConversionError;
    
    fn try_from(req: broker::ReplicateDataRequest) -> ConversionResult<Self> {
        let topic_partition = req.topic_partition
            .ok_or_else(|| ConversionError::MissingField { 
                field: "topic_partition".to_string() 
            })?;
        
        let records: Result<Vec<_>, _> = req.records.into_iter()
            .map(|r| r.try_into())
            .collect();
        
        Ok(Self {
            leader_epoch: req.leader_epoch,
            topic_partition: topic_partition.into(),
            records: records?,
            leader_id: req.leader_id,
        })
    }
}

impl TryFrom<internal::ReplicateDataResponse> for broker::ReplicateDataResponse {
    type Error = ConversionError;
    
    fn try_from(resp: internal::ReplicateDataResponse) -> ConversionResult<Self> {
        let follower_state = resp.follower_state
            .map(|s| s.try_into())
            .transpose()?;
        
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: resp.error_message.unwrap_or_default(),
            follower_state,
            metadata: None,
            bytes_replicated: 0,
            records_replicated: 0,
            replication_time_ms: 0,
            follower_log_end_offset: 0,
            follower_high_watermark: 0,
        })
    }
}

impl TryFrom<broker::ReplicateDataResponse> for internal::ReplicateDataResponse {
    type Error = ConversionError;
    
    fn try_from(resp: broker::ReplicateDataResponse) -> ConversionResult<Self> {
        let follower_state = resp.follower_state
            .map(|s| s.try_into())
            .transpose()?;
        
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: if resp.error_message.is_empty() { 
                None 
            } else { 
                Some(resp.error_message) 
            },
            follower_state,
        })
    }
}

// ============================================================================
// Request/Response Conversions: Heartbeat
// ============================================================================

impl TryFrom<internal::HeartbeatRequest> for broker::HeartbeatRequest {
    type Error = ConversionError;
    
    fn try_from(req: internal::HeartbeatRequest) -> ConversionResult<Self> {
        Ok(Self {
            leader_epoch: req.leader_epoch,
            leader_id: req.leader_id,
            topic_partition: Some(req.topic_partition.into()),
            high_watermark: req.high_watermark,
            metadata: None,
            leader_log_end_offset: 0,
            in_sync_replicas: vec![],
            heartbeat_interval_ms: 30000, // Default 30s
            leader_messages_per_second: 0,
            leader_bytes_per_second: 0,
        })
    }
}

impl TryFrom<broker::HeartbeatRequest> for internal::HeartbeatRequest {
    type Error = ConversionError;
    
    fn try_from(req: broker::HeartbeatRequest) -> ConversionResult<Self> {
        let topic_partition = req.topic_partition
            .ok_or_else(|| ConversionError::MissingField { 
                field: "topic_partition".to_string() 
            })?;
        
        Ok(Self {
            leader_epoch: req.leader_epoch,
            leader_id: req.leader_id,
            topic_partition: topic_partition.into(),
            high_watermark: req.high_watermark,
        })
    }
}

impl TryFrom<internal::HeartbeatResponse> for broker::HeartbeatResponse {
    type Error = ConversionError;
    
    fn try_from(resp: internal::HeartbeatResponse) -> ConversionResult<Self> {
        let follower_state = resp.follower_state
            .map(|s| s.try_into())
            .transpose()?;
        
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: resp.error_message.unwrap_or_default(),
            follower_state,
            metadata: None,
            follower_cpu_usage: 0.0,
            follower_memory_usage: 0.0,
            follower_disk_usage_bytes: 0,
            follower_network_in_bytes: 0,
            follower_network_out_bytes: 0,
            supported_compression: vec!["none".to_string(), "lz4".to_string()],
            max_batch_size_bytes: 1024 * 1024, // 1MB default
        })
    }
}

impl TryFrom<broker::HeartbeatResponse> for internal::HeartbeatResponse {
    type Error = ConversionError;
    
    fn try_from(resp: broker::HeartbeatResponse) -> ConversionResult<Self> {
        let follower_state = resp.follower_state
            .map(|s| s.try_into())
            .transpose()?;
        
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: if resp.error_message.is_empty() { 
                None 
            } else { 
                Some(resp.error_message) 
            },
            follower_state,
        })
    }
}

// ============================================================================
// Additional Request/Response Conversions: Assignment and Removal
// ============================================================================

impl TryFrom<internal::AssignPartitionRequest> for broker::AssignPartitionRequest {
    type Error = ConversionError;
    
    fn try_from(req: internal::AssignPartitionRequest) -> ConversionResult<Self> {
        Ok(Self {
            topic_partition: Some(req.topic_partition.into()),
            replica_set: req.replica_set,
            leader_id: req.leader_id,
            leader_epoch: req.leader_epoch,
            controller_id: req.controller_id,
            metadata: None,
            controller_epoch: 1, // Default
            assignment_reason: "controller_assignment".to_string(),
            topic_config: None,
            initial_offset: 0,
            is_new_partition: false,
            expected_throughput_mbs: 0,
            priority: 0,
        })
    }
}

impl TryFrom<broker::AssignPartitionRequest> for internal::AssignPartitionRequest {
    type Error = ConversionError;
    
    fn try_from(req: broker::AssignPartitionRequest) -> ConversionResult<Self> {
        let topic_partition = req.topic_partition
            .ok_or_else(|| ConversionError::MissingField { 
                field: "topic_partition".to_string() 
            })?;
        
        Ok(Self {
            topic_partition: topic_partition.into(),
            replica_set: req.replica_set,
            leader_id: req.leader_id,
            leader_epoch: req.leader_epoch,
            controller_id: req.controller_id,
        })
    }
}

impl TryFrom<internal::AssignPartitionResponse> for broker::AssignPartitionResponse {
    type Error = ConversionError;
    
    fn try_from(resp: internal::AssignPartitionResponse) -> ConversionResult<Self> {
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: resp.error_message.unwrap_or_default(),
            metadata: None,
            assigned_log_end_offset: 0,
            assigned_wal_path: String::new(),
            estimated_setup_time_ms: 0,
            allocated_memory_bytes: 0,
            allocated_disk_bytes: 0,
            allocated_network_mbs: 0,
        })
    }
}

impl TryFrom<broker::AssignPartitionResponse> for internal::AssignPartitionResponse {
    type Error = ConversionError;
    
    fn try_from(resp: broker::AssignPartitionResponse) -> ConversionResult<Self> {
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: if resp.error_message.is_empty() { 
                None 
            } else { 
                Some(resp.error_message) 
            },
        })
    }
}

impl TryFrom<internal::RemovePartitionRequest> for broker::RemovePartitionRequest {
    type Error = ConversionError;
    
    fn try_from(req: internal::RemovePartitionRequest) -> ConversionResult<Self> {
        Ok(Self {
            topic_partition: Some(req.topic_partition.into()),
            controller_id: req.controller_id,
            metadata: None,
            controller_epoch: 1,
            removal_reason: "controller_removal".to_string(),
            graceful_removal: true,
            removal_timeout_ms: 30000,
            preserve_data: false,
            backup_location: String::new(),
        })
    }
}

impl TryFrom<broker::RemovePartitionRequest> for internal::RemovePartitionRequest {
    type Error = ConversionError;
    
    fn try_from(req: broker::RemovePartitionRequest) -> ConversionResult<Self> {
        let topic_partition = req.topic_partition
            .ok_or_else(|| ConversionError::MissingField { 
                field: "topic_partition".to_string() 
            })?;
        
        Ok(Self {
            topic_partition: topic_partition.into(),
            controller_id: req.controller_id,
        })
    }
}

impl TryFrom<internal::RemovePartitionResponse> for broker::RemovePartitionResponse {
    type Error = ConversionError;
    
    fn try_from(resp: internal::RemovePartitionResponse) -> ConversionResult<Self> {
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: resp.error_message.unwrap_or_default(),
            metadata: None,
            final_log_end_offset: 0,
            removed_data_bytes: 0,
            removal_time_ms: 0,
            freed_memory_bytes: 0,
            freed_disk_bytes: 0,
            cleanup_status: "completed".to_string(),
        })
    }
}

impl TryFrom<broker::RemovePartitionResponse> for internal::RemovePartitionResponse {
    type Error = ConversionError;
    
    fn try_from(resp: broker::RemovePartitionResponse) -> ConversionResult<Self> {
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: if resp.error_message.is_empty() { 
                None 
            } else { 
                Some(resp.error_message) 
            },
        })
    }
}

// ============================================================================
// Additional Request/Response Conversions: Leadership Transfer
// ============================================================================

impl TryFrom<internal::TransferLeadershipRequest> for broker::TransferLeadershipRequest {
    type Error = ConversionError;
    
    fn try_from(req: internal::TransferLeadershipRequest) -> ConversionResult<Self> {
        Ok(Self {
            topic_partition: Some(req.topic_partition.into()),
            current_leader_id: req.current_leader_id,
            current_leader_epoch: req.current_leader_epoch,
            new_leader_id: req.new_leader_id,
            metadata: None,
            controller_id: "controller-1".to_string(), // Default
            controller_epoch: 1,
            transfer_timeout_ms: 30000,
            force_transfer: false,
            wait_for_sync: true,
        })
    }
}

impl TryFrom<broker::TransferLeadershipRequest> for internal::TransferLeadershipRequest {
    type Error = ConversionError;
    
    fn try_from(req: broker::TransferLeadershipRequest) -> ConversionResult<Self> {
        let topic_partition = req.topic_partition
            .ok_or_else(|| ConversionError::MissingField { 
                field: "topic_partition".to_string() 
            })?;
        
        Ok(Self {
            topic_partition: topic_partition.into(),
            current_leader_id: req.current_leader_id,
            current_leader_epoch: req.current_leader_epoch,
            new_leader_id: req.new_leader_id,
        })
    }
}

impl TryFrom<internal::TransferLeadershipResponse> for broker::TransferLeadershipResponse {
    type Error = ConversionError;
    
    fn try_from(resp: internal::TransferLeadershipResponse) -> ConversionResult<Self> {
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: resp.error_message.unwrap_or_default(),
            new_leader_epoch: resp.new_leader_epoch.unwrap_or(0),
            metadata: None,
            actual_new_leader_id: "".to_string(), // Not tracked in internal type
            transfer_time_ms: 0,
            final_log_end_offset: 0,
            new_isr: vec![],
        })
    }
}

impl TryFrom<broker::TransferLeadershipResponse> for internal::TransferLeadershipResponse {
    type Error = ConversionError;
    
    fn try_from(resp: broker::TransferLeadershipResponse) -> ConversionResult<Self> {
        Ok(Self {
            success: resp.success,
            error_code: resp.error_code,
            error_message: if resp.error_message.is_empty() { 
                None 
            } else { 
                Some(resp.error_message) 
            },
            new_leader_epoch: if resp.new_leader_epoch == 0 { 
                None 
            } else { 
                Some(resp.new_leader_epoch) 
            },
        })
    }
}

// ============================================================================
// Utility Functions for Timestamp Conversion
// ============================================================================

/// Convert Unix timestamp milliseconds to protobuf Timestamp
fn timestamp_to_proto(millis: i64) -> ConversionResult<Timestamp> {
    let seconds = millis / 1000;
    let nanos = ((millis % 1000) * 1_000_000) as i32;
    
    Ok(Timestamp {
        seconds,
        nanos,
    })
}

/// Convert protobuf Timestamp to Unix timestamp milliseconds
fn timestamp_from_proto(timestamp: Timestamp) -> ConversionResult<i64> {
    let millis = timestamp.seconds * 1000 + (timestamp.nanos / 1_000_000) as i64;
    Ok(millis)
}

// ============================================================================
// Error Code Mapping Utilities
// ============================================================================

/// Map internal RustMqError to protobuf error code
pub fn error_to_code(error: &RustMqError) -> u32 {
    use common::ErrorCode;
    
    match error {
        RustMqError::Io(_) => ErrorCode::InternalError as u32,
        RustMqError::Serialization(_) => ErrorCode::InvalidEncoding as u32,
        RustMqError::Network(_) => ErrorCode::InternalError as u32,
        RustMqError::Storage(_) => ErrorCode::InternalError as u32,
        RustMqError::Replication(_) => ErrorCode::InternalError as u32,
        RustMqError::Config(_) => ErrorCode::InvalidParameter as u32,
        RustMqError::Wal(_) => ErrorCode::InternalError as u32,
        RustMqError::Cache(_) => ErrorCode::InternalError as u32,
        RustMqError::Upload(_) => ErrorCode::InternalError as u32,
        RustMqError::Etl(_) => ErrorCode::InternalError as u32,
        RustMqError::Bandwidth(_) => ErrorCode::ResourceExhausted as u32,
        RustMqError::Balance(_) => ErrorCode::InternalError as u32,
        RustMqError::Sandbox(_) => ErrorCode::InternalError as u32,
        RustMqError::AdminApi(_) => ErrorCode::InternalError as u32,
        RustMqError::BufferTooSmall => ErrorCode::ResourceExhausted as u32,
        RustMqError::InvalidOffset(_) => ErrorCode::OffsetOutOfRange as u32,
        RustMqError::TopicNotFound(_) => ErrorCode::TopicNotFound as u32,
        RustMqError::PartitionNotFound(_) => ErrorCode::PartitionNotFound as u32,
        RustMqError::BrokerNotFound(_) => ErrorCode::ResourceNotFound as u32,
        RustMqError::Timeout => ErrorCode::InternalError as u32,
        RustMqError::InvalidConfig(_) => ErrorCode::InvalidParameter as u32,
        RustMqError::PermissionDenied(_) => ErrorCode::PermissionDenied as u32,
        RustMqError::Tls(_) => ErrorCode::InternalError as u32,
        RustMqError::Certificate(_) => ErrorCode::InternalError as u32,
        RustMqError::Quic(_) => ErrorCode::InternalError as u32,
        RustMqError::QuicConnection(_) => ErrorCode::InternalError as u32,
        RustMqError::InvalidOperation(_) => ErrorCode::InvalidRequest as u32,
        RustMqError::ResourceExhausted(_) => ErrorCode::ResourceExhausted as u32,
        RustMqError::NotFound(_) => ErrorCode::ResourceNotFound as u32,
        RustMqError::NoLeader => ErrorCode::NotLeader as u32,
        RustMqError::ReplicationFailed { .. } => ErrorCode::InternalError as u32,
        RustMqError::StaleLeaderEpoch { .. } => ErrorCode::StaleLeaderEpoch as u32,
        RustMqError::EtlProcessingFailed(_) => ErrorCode::EtlExecutionFailed as u32,
        RustMqError::EtlModuleNotFound(_) => ErrorCode::EtlModuleNotFound as u32,
        RustMqError::TopicAlreadyExists(_) => ErrorCode::TopicAlreadyExists as u32,
        RustMqError::NotLeader(_) => ErrorCode::NotLeader as u32,
        RustMqError::OffsetOutOfRange(_) => ErrorCode::OffsetOutOfRange as u32,
        RustMqError::ObjectNotFound(_) => ErrorCode::ObjectNotFound as u32,
    }
}

/// Create a human-readable error message from RustMqError
pub fn error_to_message(error: &RustMqError) -> String {
    error.to_string()
}

/// Determine if an error is retryable
pub fn error_is_retryable(error: &RustMqError) -> bool {
    match error {
        RustMqError::Timeout 
        | RustMqError::Network(_)
        | RustMqError::QuicConnection(_)
        | RustMqError::ResourceExhausted(_)
        | RustMqError::Replication(_)
        | RustMqError::Upload(_) => true,
        
        RustMqError::StaleLeaderEpoch { .. }
        | RustMqError::InvalidOperation(_)
        | RustMqError::InvalidConfig(_)
        | RustMqError::TopicNotFound(_)
        | RustMqError::PartitionNotFound(_)
        | RustMqError::BrokerNotFound(_)
        | RustMqError::PermissionDenied(_)
        | RustMqError::TopicAlreadyExists(_)
        | RustMqError::NotLeader(_)
        | RustMqError::OffsetOutOfRange(_) => false,
        
        _ => false, // Conservative default: non-retryable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_topic_partition_conversion() {
        let internal_tp = internal::TopicPartition {
            topic: "test-topic".to_string(),
            partition: 42,
        };
        
        let proto_tp: common::TopicPartition = internal_tp.clone().into();
        assert_eq!(proto_tp.topic, "test-topic");
        assert_eq!(proto_tp.partition, 42);
        
        let back_to_internal: internal::TopicPartition = proto_tp.into();
        assert_eq!(back_to_internal, internal_tp);
    }

    #[test]
    fn test_record_conversion() {
        let internal_record = internal::Record {
            key: Some(b"test-key".to_vec()),
            value: b"test-value".to_vec(),
            headers: vec![internal::Header {
                key: "header1".to_string(),
                value: b"header-value".to_vec(),
            }],
            timestamp: Utc::now().timestamp_millis(),
        };
        
        let proto_record: common::Record = internal_record.clone().try_into().unwrap();
        assert_eq!(proto_record.key, b"test-key".to_vec());
        assert_eq!(proto_record.value, b"test-value".to_vec());
        assert_eq!(proto_record.headers.len(), 1);
        assert!(proto_record.timestamp.is_some());
        
        let back_to_internal: internal::Record = proto_record.try_into().unwrap();
        assert_eq!(back_to_internal.key, Some(b"test-key".to_vec()));
        assert_eq!(back_to_internal.value, b"test-value".to_vec());
        assert_eq!(back_to_internal.headers.len(), 1);
    }

    #[test]
    fn test_wal_record_conversion() {
        let internal_wal = internal::WalRecord {
            topic_partition: internal::TopicPartition {
                topic: "test".to_string(),
                partition: 0,
            },
            offset: 1000,
            record: internal::Record {
                key: None,
                value: b"data".to_vec(),
                headers: vec![],
                timestamp: Utc::now().timestamp_millis(),
            },
            crc32: 123456,
        };
        
        let proto_wal: common::WalRecord = internal_wal.clone().try_into().unwrap();
        assert!(proto_wal.topic_partition.is_some());
        assert_eq!(proto_wal.offset, 1000);
        assert!(proto_wal.record.is_some());
        assert_eq!(proto_wal.crc32, 123456);
        
        let back_to_internal: internal::WalRecord = proto_wal.try_into().unwrap();
        assert_eq!(back_to_internal.topic_partition.topic, "test");
        assert_eq!(back_to_internal.offset, 1000);
        assert_eq!(back_to_internal.crc32, 123456);
    }

    #[test]
    fn test_error_code_mapping() {
        assert_eq!(
            error_to_code(&RustMqError::TopicNotFound("test".to_string())),
            common::ErrorCode::TopicNotFound as u32
        );
        
        assert_eq!(
            error_to_code(&RustMqError::StaleLeaderEpoch { 
                request_epoch: 1, 
                current_epoch: 2 
            }),
            common::ErrorCode::StaleLeaderEpoch as u32
        );
        
        assert!(error_is_retryable(&RustMqError::Timeout));
        assert!(!error_is_retryable(&RustMqError::TopicNotFound("test".to_string())));
    }

    #[test]
    fn test_timestamp_conversion() {
        let now_millis = Utc::now().timestamp_millis();
        let proto_ts = timestamp_to_proto(now_millis).unwrap();
        let back_to_millis = timestamp_from_proto(proto_ts).unwrap();
        
        // Allow for some rounding error in nanosecond conversion
        assert!((back_to_millis - now_millis).abs() <= 1);
    }

    #[test]
    fn test_acknowledgment_level_conversion() {
        let levels = [
            internal::AcknowledgmentLevel::None,
            internal::AcknowledgmentLevel::Leader,
            internal::AcknowledgmentLevel::Majority,
            internal::AcknowledgmentLevel::All,
        ];
        
        for level in levels {
            let proto_level: common::AcknowledgmentLevel = level.clone().into();
            let back_to_internal: internal::AcknowledgmentLevel = proto_level.into();
            // Note: Custom(n) will become Custom(3) due to default mapping
            match level {
                internal::AcknowledgmentLevel::Custom(_) => {
                    matches!(back_to_internal, internal::AcknowledgmentLevel::Custom(3));
                }
                _ => assert_eq!(std::mem::discriminant(&back_to_internal), std::mem::discriminant(&level)),
            }
        }
    }

    #[test]
    fn test_compression_type_conversion() {
        let types = [
            internal::CompressionType::None,
            internal::CompressionType::Lz4,
            internal::CompressionType::Zstd,
        ];
        
        for compression_type in types {
            let proto_type: common::CompressionType = compression_type.clone().into();
            let back_to_internal: internal::CompressionType = proto_type.into();
            assert_eq!(std::mem::discriminant(&back_to_internal), std::mem::discriminant(&compression_type));
        }
    }
}