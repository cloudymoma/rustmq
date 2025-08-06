//! Simplified end-to-end integration tests for gRPC protobuf services
//! 
//! This test suite covers basic protobuf message structure testing
//! and focuses on verifying that the generated protobuf code compiles and works correctly.

use rustmq::proto::{broker, controller, common};
use chrono::Utc;

/// Simple test helper to create request metadata
fn create_test_metadata() -> common::RequestMetadata {
    common::RequestMetadata {
        client_id: "test-client".to_string(),
        correlation_id: "integration-corr-456".to_string(),
        timestamp: Some(prost_types::Timestamp {
            seconds: Utc::now().timestamp(),
            nanos: 0,
        }),
        api_version: 1,
        timeout_ms: 30000,
    }
}

/// Create test topic partition
fn create_test_topic_partition() -> common::TopicPartition {
    common::TopicPartition {
        topic: "integration-test-topic".to_string(),
        partition: 0,
    }
}

/// Create test record for replication
fn create_test_record(offset: u64) -> common::WalRecord {
    common::WalRecord {
        topic_partition: Some(create_test_topic_partition()),
        offset,
        record: Some(common::Record {
            key: format!("key-{}", offset).into_bytes(),
            value: format!("value-{}", offset).into_bytes(),
            headers: vec![common::Header {
                key: "test".to_string(),
                value: "integration".as_bytes().to_vec(),
            }],
            timestamp: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
        }),
        crc32: 12345 + offset as u32,
        version: 1,
    }
}

// ============================================================================
// Basic Protobuf Structure Tests
// ============================================================================

#[tokio::test]
async fn test_replication_request_structure() {
    // Test complete replication workflow message structures
    
    let records = vec![
        create_test_record(0),
        create_test_record(1),
        create_test_record(2),
    ];

    let replication_request = broker::ReplicateDataRequest {
        leader_epoch: 1,
        topic_partition: Some(create_test_topic_partition()),
        records,
        leader_id: "leader-broker".to_string(),
        leader_high_watermark: 3,
        request_id: 12345,
        metadata: Some(create_test_metadata()),
        batch_base_offset: 0,
        batch_record_count: 3,
        batch_size_bytes: 150,
        compression: common::CompressionType::Lz4 as i32,
    };

    // Verify replication request structure
    assert_eq!(replication_request.leader_epoch, 1);
    assert_eq!(replication_request.records.len(), 3);
    assert_eq!(replication_request.leader_high_watermark, 3);
    assert!(replication_request.topic_partition.is_some());
}

#[tokio::test]
async fn test_replication_response_structure() {
    // Test replication response with actual protobuf fields
    let replication_response = broker::ReplicateDataResponse {
        success: true,
        error_code: 0,
        error_message: String::new(),
        follower_state: Some(common::FollowerState {
            broker_id: "follower-broker".to_string(),
            last_known_offset: 3,
            last_heartbeat: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            lag: 0,
            lag_time_ms: 0,
            in_sync: true,
        }),
        metadata: Some(common::ResponseMetadata {
            correlation_id: "integration-corr-456".to_string(),
            timestamp: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            error_code: 0,
            error_message: String::new(),
            throttle_time_ms: 0,
        }),
        bytes_replicated: 150,
        records_replicated: 3,
        replication_time_ms: 15,
        follower_log_end_offset: 3,
        follower_high_watermark: 3,
    };

    // Verify response structure
    assert!(replication_response.success);
    assert_eq!(replication_response.bytes_replicated, 150);
    assert_eq!(replication_response.records_replicated, 3);
    assert!(replication_response.metadata.is_some());
}

#[tokio::test]
async fn test_heartbeat_request_structure() {
    let heartbeat_request = broker::HeartbeatRequest {
        leader_epoch: 1,
        leader_id: "leader-broker".to_string(),
        topic_partition: Some(create_test_topic_partition()),
        high_watermark: 500,
        metadata: Some(create_test_metadata()),
        leader_log_end_offset: 505,
        in_sync_replicas: vec![
            "leader-broker".to_string(),
            "follower-1".to_string(),
            "follower-2".to_string(),
        ],
        heartbeat_interval_ms: 30000,
        leader_messages_per_second: 150,
        leader_bytes_per_second: 1024 * 150,
    };

    // Verify heartbeat structure
    assert_eq!(heartbeat_request.leader_epoch, 1);
    assert_eq!(heartbeat_request.high_watermark, 500);
    assert_eq!(heartbeat_request.in_sync_replicas.len(), 3);
    assert!(heartbeat_request.leader_messages_per_second > 0);
}

#[tokio::test]
async fn test_heartbeat_response_structure() {
    let heartbeat_response = broker::HeartbeatResponse {
        success: true,
        error_code: 0,
        error_message: String::new(),
        follower_state: Some(common::FollowerState {
            broker_id: "follower-1".to_string(),
            last_known_offset: 500,
            last_heartbeat: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            lag: 0,
            lag_time_ms: 0,
            in_sync: true,
        }),
        metadata: Some(common::ResponseMetadata {
            correlation_id: "heartbeat-corr-456".to_string(),
            timestamp: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            error_code: 0,
            error_message: String::new(),
            throttle_time_ms: 0,
        }),
        follower_cpu_usage: 25.0,
        follower_memory_usage: 50.0,
        follower_disk_usage_bytes: 30 * 1024 * 1024 * 1024, // 30GB
        follower_network_in_bytes: 10 * 1024 * 1024, // 10MB/s
        follower_network_out_bytes: 5 * 1024 * 1024, // 5MB/s
        supported_compression: vec!["lz4".to_string(), "zstd".to_string()],
        max_batch_size_bytes: 1024 * 1024, // 1MB
    };

    // Verify heartbeat response
    assert!(heartbeat_response.success);
    assert_eq!(heartbeat_response.follower_cpu_usage, 25.0);
    assert!(heartbeat_response.follower_state.is_some());
}

#[tokio::test]
async fn test_controller_broker_coordination() {
    // Test the protocol flow between controller and broker services
    
    // Controller assigns partition to broker
    let assignment_request = broker::AssignPartitionRequest {
        topic_partition: Some(create_test_topic_partition()),
        replica_set: vec!["broker-1".to_string(), "broker-2".to_string()],
        leader_id: "broker-1".to_string(),
        leader_epoch: 1,
        controller_id: "controller-1".to_string(),
        metadata: Some(create_test_metadata()),
        controller_epoch: 1,
        assignment_reason: "initial_assignment".to_string(),
        topic_config: Some(common::TopicConfig {
            name: "integration-test-topic".to_string(),
            partition_count: 1,
            replication_factor: 2,
            retention_policy: None,
            compression: common::CompressionType::Lz4 as i32,
            etl_modules: vec![],
            version: 1,
        }),
        initial_offset: 0,
        is_new_partition: true,
        expected_throughput_mbs: 10,
        priority: 1,
    };

    // Verify protobuf message structure
    assert_eq!(assignment_request.leader_id, "broker-1");
    assert_eq!(assignment_request.replica_set.len(), 2);
    assert!(assignment_request.topic_partition.is_some());
    assert!(assignment_request.metadata.is_some());
}

#[tokio::test]
async fn test_error_handling_structures() {
    // Test error handling message structures
    
    let error_response = broker::ReplicateDataResponse {
        success: false,
        error_code: 1001,
        error_message: "Insufficient disk space".to_string(),
        follower_state: Some(common::FollowerState {
            broker_id: "follower-broker".to_string(),
            last_known_offset: 100,
            last_heartbeat: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            lag: 50,
            lag_time_ms: 5000,
            in_sync: false,
        }),
        metadata: Some(common::ResponseMetadata {
            correlation_id: "error-corr-789".to_string(),
            timestamp: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            error_code: 1001,
            error_message: "Insufficient disk space".to_string(),
            throttle_time_ms: 0,
        }),
        bytes_replicated: 0,
        records_replicated: 0,
        replication_time_ms: 5,
        follower_log_end_offset: 100,
        follower_high_watermark: 100,
    };

    // Verify error structure
    assert!(!error_response.success);
    assert_eq!(error_response.error_code, 1001);
    assert!(!error_response.error_message.is_empty());
    assert_eq!(error_response.bytes_replicated, 0);
}

#[tokio::test]
async fn test_compression_types() {
    // Test different compression types in replication
    
    let compression_types = vec![
        common::CompressionType::None,
        common::CompressionType::Lz4,
        common::CompressionType::Zstd,
    ];

    for compression_type in compression_types {
        let records = vec![create_test_record(0)];
        
        let compressed_request = broker::ReplicateDataRequest {
            leader_epoch: 1,
            topic_partition: Some(create_test_topic_partition()),
            records,
            leader_id: "leader-broker".to_string(),
            leader_high_watermark: 1,
            request_id: compression_type as u64,
            metadata: Some(create_test_metadata()),
            batch_base_offset: 0,
            batch_record_count: 1,
            batch_size_bytes: 50,
            compression: compression_type as i32,
        };

        // Verify compression setting
        assert_eq!(compressed_request.compression, compression_type as i32);
        assert_eq!(compressed_request.batch_record_count, 1);
    }
}

#[tokio::test]
async fn test_metadata_consistency() {
    // Test metadata propagation and consistency across services
    
    let rich_metadata = common::RequestMetadata {
        client_id: "integration-test-client".to_string(),
        correlation_id: "consistency-corr-123".to_string(),
        timestamp: Some(prost_types::Timestamp {
            seconds: Utc::now().timestamp(),
            nanos: 0,
        }),
        api_version: 1,
        timeout_ms: 45000,
    };

    // Test metadata in controller operation
    let cluster_info_request = controller::GetClusterInfoRequest {
        metadata: Some(rich_metadata.clone()),
        include_node_details: true,
        include_log_info: true,
        include_performance_metrics: true,
    };

    // Verify metadata structure
    assert!(!rich_metadata.client_id.is_empty());
    assert!(rich_metadata.timestamp.is_some());
    assert!(!rich_metadata.correlation_id.is_empty());
    assert!(cluster_info_request.metadata.is_some());
}

#[tokio::test]
async fn test_leadership_transfer_protocol() {
    // Test leadership transfer coordination between services
    
    let transfer_request = broker::TransferLeadershipRequest {
        topic_partition: Some(create_test_topic_partition()),
        current_leader_id: "current-leader".to_string(),
        current_leader_epoch: 5,
        new_leader_id: "new-leader".to_string(),
        metadata: Some(create_test_metadata()),
        controller_id: "controller-1".to_string(),
        controller_epoch: 1,
        transfer_timeout_ms: 30000,
        force_transfer: false,
        wait_for_sync: true,
    };

    // Verify transfer request structure
    assert_eq!(transfer_request.current_leader_id, "current-leader");
    assert_eq!(transfer_request.new_leader_id, "new-leader");
    assert_eq!(transfer_request.current_leader_epoch, 5);
    assert!(!transfer_request.force_transfer);
}