# RustMQ gRPC Protobuf Architecture Design

## Executive Summary

This document outlines a comprehensive gRPC protobuf service architecture for RustMQ, following best practices from etcd, Kafka, and modern distributed systems. The design emphasizes clean separation of concerns, efficient serialization, backward compatibility, and seamless integration with existing Rust types.

## 1. Service Architecture Overview

### Service Domain Separation

The architecture organizes services into four primary domains:

```
proto/
├── common/
│   ├── types.proto          # Shared data types
│   ├── errors.proto         # Error codes and status
│   └── metadata.proto       # Cluster metadata types
├── client/
│   ├── producer.proto       # Producer API
│   ├── consumer.proto       # Consumer API
│   └── metadata.proto       # Client metadata operations
├── broker/
│   ├── replication.proto   # Inter-broker replication
│   ├── partition.proto     # Partition management
│   └── health.proto        # Health and monitoring
├── controller/
│   ├── raft.proto          # Raft consensus protocol
│   ├── cluster.proto       # Cluster management
│   └── admin.proto         # Administrative operations
└── streaming/
    └── stream.proto        # Streaming protocols
```

## 2. Core Service Definitions

### 2.1 Client Services

```protobuf
// client/producer.proto
syntax = "proto3";
package rustmq.client;

import "common/types.proto";
import "google/protobuf/timestamp.proto";

service ProducerService {
  // Single record production
  rpc Produce(ProduceRequest) returns (ProduceResponse);
  
  // Batch production for efficiency
  rpc ProduceBatch(ProduceBatchRequest) returns (ProduceBatchResponse);
  
  // Streaming production for high throughput
  rpc ProduceStream(stream ProduceRequest) returns (stream ProduceResponse);
  
  // Transaction support (future)
  rpc InitTransaction(InitTransactionRequest) returns (InitTransactionResponse);
  rpc CommitTransaction(CommitTransactionRequest) returns (CommitTransactionResponse);
  rpc AbortTransaction(AbortTransactionRequest) returns (AbortTransactionResponse);
}

// client/consumer.proto
service ConsumerService {
  // Pull-based consumption
  rpc Fetch(FetchRequest) returns (FetchResponse);
  
  // Batch fetch for efficiency
  rpc FetchBatch(FetchBatchRequest) returns (FetchBatchResponse);
  
  // Push-based streaming consumption
  rpc Subscribe(SubscribeRequest) returns (stream FetchResponse);
  
  // Consumer group coordination
  rpc JoinGroup(JoinGroupRequest) returns (JoinGroupResponse);
  rpc LeaveGroup(LeaveGroupRequest) returns (LeaveGroupResponse);
  rpc CommitOffsets(CommitOffsetsRequest) returns (CommitOffsetsResponse);
}

// client/metadata.proto
service MetadataService {
  // Topic operations
  rpc GetTopicMetadata(GetTopicMetadataRequest) returns (GetTopicMetadataResponse);
  rpc ListTopics(ListTopicsRequest) returns (ListTopicsResponse);
  
  // Broker discovery
  rpc GetBrokerList(GetBrokerListRequest) returns (GetBrokerListResponse);
  rpc GetPartitionLeader(GetPartitionLeaderRequest) returns (GetPartitionLeaderResponse);
  
  // Cluster info
  rpc GetClusterMetadata(GetClusterMetadataRequest) returns (GetClusterMetadataResponse);
}
```

### 2.2 Broker Replication Services

```protobuf
// broker/replication.proto
syntax = "proto3";
package rustmq.broker;

service BrokerReplicationService {
  // Data replication from leader to followers
  rpc ReplicateData(ReplicateDataRequest) returns (ReplicateDataResponse);
  
  // Streaming replication for efficiency
  rpc ReplicateStream(stream ReplicateDataRequest) returns (stream ReplicateDataResponse);
  
  // Heartbeat and health monitoring
  rpc SendHeartbeat(HeartbeatRequest) returns (HeartbeatResponse);
  
  // Leadership operations
  rpc TransferLeadership(TransferLeadershipRequest) returns (TransferLeadershipResponse);
  rpc GetLeaderEpoch(GetLeaderEpochRequest) returns (GetLeaderEpochResponse);
  
  // ISR management
  rpc UpdateISR(UpdateISRRequest) returns (UpdateISRResponse);
  rpc GetISR(GetISRRequest) returns (GetISRResponse);
}

// broker/partition.proto
service PartitionManagementService {
  // Partition lifecycle
  rpc AssignPartition(AssignPartitionRequest) returns (AssignPartitionResponse);
  rpc RemovePartition(RemovePartitionRequest) returns (RemovePartitionResponse);
  rpc MigratePartition(MigratePartitionRequest) returns (stream MigratePartitionProgress);
  
  // Partition state
  rpc GetPartitionState(GetPartitionStateRequest) returns (GetPartitionStateResponse);
  rpc UpdatePartitionConfig(UpdatePartitionConfigRequest) returns (UpdatePartitionConfigResponse);
}
```

### 2.3 Controller Raft Services

```protobuf
// controller/raft.proto
syntax = "proto3";
package rustmq.controller;

// Inter-controller Raft consensus
service ControllerRaftService {
  // Raft core protocol (following etcd patterns)
  rpc RequestVote(VoteRequest) returns (VoteResponse);
  rpc AppendEntries(AppendEntriesRequest) returns (AppendEntriesResponse);
  rpc InstallSnapshot(stream InstallSnapshotRequest) returns (InstallSnapshotResponse);
  
  // Extended Raft operations
  rpc PreVote(PreVoteRequest) returns (PreVoteResponse);  // Pre-vote optimization
  rpc TimeoutNow(TimeoutNowRequest) returns (TimeoutNowResponse);  // Force election
  
  // Membership changes
  rpc AddPeer(AddPeerRequest) returns (AddPeerResponse);
  rpc RemovePeer(RemovePeerRequest) returns (RemovePeerResponse);
  rpc GetConfiguration(GetConfigurationRequest) returns (GetConfigurationResponse);
}

// controller/cluster.proto
// Controller-to-Broker management
service ControllerBrokerService {
  // Broker lifecycle
  rpc RegisterBroker(RegisterBrokerRequest) returns (RegisterBrokerResponse);
  rpc UnregisterBroker(UnregisterBrokerRequest) returns (UnregisterBrokerResponse);
  rpc HeartbeatBroker(BrokerHeartbeatRequest) returns (BrokerHeartbeatResponse);
  
  // Partition management commands
  rpc CreatePartitions(CreatePartitionsRequest) returns (CreatePartitionsResponse);
  rpc DeletePartitions(DeletePartitionsRequest) returns (DeletePartitionsResponse);
  rpc ReassignPartitions(ReassignPartitionsRequest) returns (stream ReassignPartitionsProgress);
  
  // Configuration propagation
  rpc UpdateClusterConfig(UpdateClusterConfigRequest) returns (UpdateClusterConfigResponse);
  rpc GetClusterConfig(GetClusterConfigRequest) returns (GetClusterConfigResponse);
}
```

## 3. Message Type Definitions

### 3.1 Common Types

```protobuf
// common/types.proto
syntax = "proto3";
package rustmq.common;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// Version field for backward compatibility
message ApiVersion {
  uint32 major = 1;
  uint32 minor = 2;
  uint32 patch = 3;
}

// Core identifiers
message TopicPartition {
  string topic = 1;
  uint32 partition = 2;
}

message BrokerId {
  string id = 1;
  string rack_id = 2;  // For rack-aware placement
}

// Record definition
message Record {
  bytes key = 1;
  bytes value = 2;
  repeated Header headers = 3;
  google.protobuf.Timestamp timestamp = 4;
  uint32 compression_type = 5;  // Enum value
  bytes transactional_id = 6;   // For transaction support
}

message Header {
  string key = 1;
  bytes value = 2;
}

// Batch for efficiency
message RecordBatch {
  uint32 magic = 1;  // Format version
  uint32 compression_type = 2;
  uint64 base_offset = 3;
  uint64 last_offset_delta = 4;
  google.protobuf.Timestamp first_timestamp = 5;
  google.protobuf.Timestamp max_timestamp = 6;
  repeated Record records = 7;
  uint32 crc32 = 8;  // Checksum
}

// Enumerations
enum AcknowledgmentLevel {
  ACK_NONE = 0;
  ACK_LEADER = 1;
  ACK_MAJORITY = 2;
  ACK_ALL = 3;
  ACK_CUSTOM = 4;  // Uses custom_ack_count
}

enum CompressionType {
  COMPRESSION_NONE = 0;
  COMPRESSION_LZ4 = 1;
  COMPRESSION_ZSTD = 2;
  COMPRESSION_SNAPPY = 3;
}

enum ErrorCode {
  OK = 0;
  UNKNOWN_ERROR = 1;
  INVALID_REQUEST = 2;
  NOT_LEADER = 3;
  NOT_AVAILABLE = 4;
  TIMEOUT = 5;
  DUPLICATE_REQUEST = 6;
  STALE_EPOCH = 7;
  UNAUTHORIZED = 8;
  TOPIC_NOT_FOUND = 9;
  PARTITION_NOT_FOUND = 10;
  OFFSET_OUT_OF_RANGE = 11;
  // ... more error codes
}
```

### 3.2 Request/Response Messages

```protobuf
// Producer messages
message ProduceRequest {
  ApiVersion version = 1;
  string client_id = 2;
  TopicPartition topic_partition = 3;
  repeated Record records = 4;
  AcknowledgmentLevel acks = 5;
  uint32 custom_ack_count = 6;  // For ACK_CUSTOM
  google.protobuf.Duration timeout = 7;
  string idempotency_key = 8;  // For exactly-once semantics
}

message ProduceBatchRequest {
  ApiVersion version = 1;
  string client_id = 2;
  repeated ProducePartitionData partitions = 3;
  AcknowledgmentLevel acks = 4;
  google.protobuf.Duration timeout = 5;
}

message ProducePartitionData {
  TopicPartition topic_partition = 1;
  RecordBatch batch = 2;
}

message ProduceResponse {
  ErrorCode error_code = 1;
  string error_message = 2;
  uint64 offset = 3;
  google.protobuf.Timestamp timestamp = 4;
  uint64 log_append_time = 5;
  string leader_id = 6;  // For client caching
}

// Consumer messages
message FetchRequest {
  ApiVersion version = 1;
  string client_id = 2;
  uint32 replica_id = 3;  // -1 for consumers, broker_id for followers
  google.protobuf.Duration max_wait = 4;
  uint32 min_bytes = 5;
  uint32 max_bytes = 6;
  repeated FetchPartition partitions = 7;
  IsolationLevel isolation_level = 8;
}

message FetchPartition {
  TopicPartition topic_partition = 1;
  uint64 fetch_offset = 2;
  uint64 log_start_offset = 3;
  uint32 partition_max_bytes = 4;
}

message FetchResponse {
  ErrorCode error_code = 1;
  string error_message = 2;
  repeated FetchPartitionResponse partitions = 3;
}

message FetchPartitionResponse {
  TopicPartition topic_partition = 1;
  ErrorCode error_code = 2;
  uint64 high_watermark = 3;
  uint64 last_stable_offset = 4;
  uint64 log_start_offset = 5;
  repeated RecordBatch batches = 6;
}
```

### 3.3 Raft Protocol Messages

```protobuf
// Raft consensus messages (following etcd patterns)
message VoteRequest {
  uint64 term = 1;
  string candidate_id = 2;
  uint64 last_log_index = 3;
  uint64 last_log_term = 4;
  bool pre_vote = 5;  // Pre-vote optimization
}

message VoteResponse {
  uint64 term = 1;
  bool vote_granted = 2;
  string reason = 3;  // For debugging
}

message AppendEntriesRequest {
  uint64 term = 1;
  string leader_id = 2;
  uint64 prev_log_index = 3;
  uint64 prev_log_term = 4;
  repeated LogEntry entries = 5;
  uint64 leader_commit = 6;
}

message AppendEntriesResponse {
  uint64 term = 1;
  bool success = 2;
  uint64 match_index = 3;
  uint64 next_index = 4;  // Optimization hint
  string reason = 5;  // For debugging
}

message LogEntry {
  uint64 term = 1;
  uint64 index = 2;
  LogEntryType type = 3;
  bytes data = 4;  // Serialized command
  google.protobuf.Timestamp timestamp = 5;
}

enum LogEntryType {
  LOG_COMMAND = 0;
  LOG_CONFIGURATION = 1;
  LOG_NOOP = 2;
}
```

## 4. Advanced Features

### 4.1 Streaming Support

```protobuf
// streaming/stream.proto
service StreamingService {
  // Bidirectional streaming for interactive queries
  rpc InteractiveQuery(stream QueryRequest) returns (stream QueryResponse);
  
  // Server-side streaming for subscriptions
  rpc Subscribe(SubscriptionRequest) returns (stream Event);
  
  // Client-side streaming for bulk operations
  rpc BulkLoad(stream BulkLoadRequest) returns (BulkLoadResponse);
}
```

### 4.2 Transaction Support

```protobuf
message TransactionMetadata {
  string transaction_id = 1;
  uint64 producer_id = 2;
  uint32 producer_epoch = 3;
  TransactionState state = 4;
  google.protobuf.Timestamp start_time = 5;
  google.protobuf.Duration timeout = 6;
}

enum TransactionState {
  ONGOING = 0;
  PREPARING = 1;
  COMMITTED = 2;
  ABORTED = 3;
}
```

## 5. Implementation Strategy

### 5.1 Code Generation Setup

```toml
# build.rs configuration
[build-dependencies]
tonic-build = "0.11"
prost-build = "0.12"

# Cargo.toml
[dependencies]
tonic = { version = "0.11", features = ["transport", "tls"] }
prost = "0.12"
prost-types = "0.12"
```

### 5.2 Build Script

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_files = &[
        "proto/common/types.proto",
        "proto/client/producer.proto",
        "proto/client/consumer.proto",
        "proto/broker/replication.proto",
        "proto/controller/raft.proto",
    ];
    
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/generated")
        .compile(proto_files, &["proto/"])?;
        
    Ok(())
}
```

### 5.3 Type Mapping

```rust
// src/proto_convert.rs
use crate::types;
use crate::generated::rustmq::common;

impl From<types::TopicPartition> for common::TopicPartition {
    fn from(tp: types::TopicPartition) -> Self {
        Self {
            topic: tp.topic,
            partition: tp.partition,
        }
    }
}

impl From<common::TopicPartition> for types::TopicPartition {
    fn from(tp: common::TopicPartition) -> Self {
        Self {
            topic: tp.topic,
            partition: tp.partition,
        }
    }
}
```

## 6. Performance Optimizations

### 6.1 Message Batching
- Group multiple records into RecordBatch for network efficiency
- Use repeated fields for batch operations
- Implement client-side buffering with configurable flush intervals

### 6.2 Compression
- Support multiple compression algorithms (LZ4, ZSTD, Snappy)
- Compress at batch level, not individual records
- Negotiate compression type during connection establishment

### 6.3 Zero-Copy Optimizations
- Use bytes type for binary data to avoid conversions
- Implement sendfile for large transfers
- Use memory-mapped files for reading

### 6.4 Connection Pooling
- Maintain persistent gRPC connections
- Implement connection multiplexing
- Use HTTP/2 features for stream multiplexing

## 7. Backward Compatibility

### 7.1 Versioning Strategy
- Include ApiVersion in all requests
- Use field numbers consistently (never reuse)
- Add new fields with higher numbers
- Use oneof for mutually exclusive fields
- Deprecate fields instead of removing

### 7.2 Migration Path
```protobuf
// Version 1
message ProduceRequestV1 {
  string topic = 1;
  bytes value = 2;
}

// Version 2 (backward compatible)
message ProduceRequestV2 {
  string topic = 1;
  bytes value = 2;
  bytes key = 3;  // New field
  repeated Header headers = 4;  // New field
}
```

## 8. Error Handling

### 8.1 Comprehensive Error Codes
- Define granular error codes for different failure scenarios
- Include error_message for human-readable descriptions
- Provide hints for recovery (e.g., NOT_LEADER includes leader_id)

### 8.2 Retry Guidance
```protobuf
message ErrorResponse {
  ErrorCode code = 1;
  string message = 2;
  bool retriable = 3;
  google.protobuf.Duration retry_after = 4;
  string leader_hint = 5;  // For NOT_LEADER errors
}
```

## 9. Security Considerations

### 9.1 Authentication
```protobuf
message AuthenticationRequest {
  string mechanism = 1;  // PLAIN, SCRAM-SHA-256, etc.
  bytes auth_data = 2;
}

message AuthenticationResponse {
  bool authenticated = 1;
  bytes session_token = 2;
  google.protobuf.Duration expires_in = 3;
}
```

### 9.2 Authorization
- Include auth tokens in metadata
- Implement ACLs at topic/partition level
- Support TLS/mTLS for transport security

## 10. Monitoring and Observability

### 10.1 Metrics Collection
```protobuf
message MetricsRequest {
  repeated string metric_names = 1;
  google.protobuf.Timestamp start_time = 2;
  google.protobuf.Timestamp end_time = 3;
}

message MetricsResponse {
  repeated Metric metrics = 1;
}

message Metric {
  string name = 1;
  double value = 2;
  map<string, string> labels = 3;
  google.protobuf.Timestamp timestamp = 4;
}
```

### 10.2 Tracing Support
- Include trace_id in all requests
- Propagate OpenTelemetry context
- Support distributed tracing

## 11. Testing Strategy

### 11.1 Protocol Testing
- Generate test cases from proto definitions
- Implement property-based testing for wire compatibility
- Fuzz testing for malformed messages

### 11.2 Compatibility Testing
- Test backward compatibility with previous versions
- Verify cross-language compatibility
- Implement integration tests with real services

## Summary

This protobuf architecture provides:

1. **Clean Separation**: Services organized by functional domain
2. **Industry Best Practices**: Following etcd and Kafka patterns
3. **Performance**: Batching, streaming, and compression support
4. **Compatibility**: Versioning and migration strategies
5. **Production Ready**: Comprehensive error handling and monitoring
6. **Security**: Built-in authentication and authorization support

The design balances elegance with practicality, ensuring high performance while maintaining clean abstractions that map directly to your existing Rust types. The modular structure allows for independent evolution of different service domains while maintaining overall system coherence.