//! Simplified comprehensive tests for protobuf type conversion layer
//! 
//! This test suite provides extensive coverage for all type conversions between
//! internal RustMQ types and protobuf types, including:
//! - Roundtrip conversion testing
//! - Edge case handling (empty values, nulls, boundaries)
//! - Error propagation and validation

use rustmq::{
    types::*,
    proto::{common, broker},
    proto_convert::{self, ConversionError},
    error::RustMqError,
};
use chrono::{Utc, TimeZone};
use prost_types::Timestamp;
use rand::Rng;

// ============================================================================
// Roundtrip Conversion Tests
// ============================================================================

#[test]
fn test_topic_partition_roundtrip() {
    let original = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 42,
    };
    
    // Internal -> Proto -> Internal
    let proto: common::TopicPartition = original.clone().into();
    let back_to_internal: TopicPartition = proto.into();
    
    assert_eq!(original, back_to_internal);
}

#[test]
fn test_topic_partition_edge_cases() {
    let test_cases = vec![
        TopicPartition {
            topic: "".to_string(), // Empty topic
            partition: 0,
        },
        TopicPartition {
            topic: "a".repeat(1000), // Very long topic name
            partition: u32::MAX,
        },
        TopicPartition {
            topic: "topic-with-special-chars-åäö-🚀".to_string(),
            partition: 12345,
        },
    ];

    for original in test_cases {
        let proto: common::TopicPartition = original.clone().into();
        let back_to_internal: TopicPartition = proto.into();
        assert_eq!(original, back_to_internal);
    }
}

#[test]
fn test_record_roundtrip_success() {
    let original = Record {
        key: Some(b"test-key".to_vec()),
        value: b"test-value".to_vec(),
        headers: vec![
            Header {
                key: "header1".to_string(),
                value: b"value1".to_vec(),
            },
            Header {
                key: "header2".to_string(),
                value: b"value2".to_vec(),
            },
        ],
        timestamp: Utc::now().timestamp_millis(),
    };
    
    // Internal -> Proto -> Internal
    let proto: common::Record = original.clone().try_into().unwrap();
    let back_to_internal: Record = proto.try_into().unwrap();
    
    assert_eq!(original.key, back_to_internal.key);
    assert_eq!(original.value, back_to_internal.value);
    assert_eq!(original.headers.len(), back_to_internal.headers.len());
    // Timestamp may have slight differences due to conversion precision
    assert!((original.timestamp - back_to_internal.timestamp).abs() <= 1);
}

#[test]
fn test_record_edge_cases() {
    let test_cases = vec![
        // Empty key
        Record {
            key: None,
            value: b"value".to_vec(),
            headers: vec![],
            timestamp: 0,
        },
        // Empty value
        Record {
            key: Some(b"key".to_vec()),
            value: vec![],
            headers: vec![],
            timestamp: i64::MAX,
        },
        // Large data
        Record {
            key: Some(vec![0u8; 1000]),
            value: vec![1u8; 10000],
            headers: (0..10).map(|i| Header {
                key: format!("header-{}", i),
                value: vec![i as u8; 10],
            }).collect(),
            timestamp: Utc::now().timestamp_millis(),
        },
    ];

    for original in test_cases {
        let proto_result: Result<common::Record, ConversionError> = original.clone().try_into();
        assert!(proto_result.is_ok(), "Failed to convert to proto: {:?}", original);
        
        let proto = proto_result.unwrap();
        let back_result: Result<Record, ConversionError> = proto.try_into();
        assert!(back_result.is_ok(), "Failed to convert back from proto");
        
        let back_to_internal = back_result.unwrap();
        
        // Verify key handling (None -> empty vec -> None)
        if original.key.is_none() {
            assert!(back_to_internal.key.is_none() || back_to_internal.key == Some(vec![]));
        } else {
            assert_eq!(original.key, back_to_internal.key);
        }
        
        assert_eq!(original.value, back_to_internal.value);
        assert_eq!(original.headers.len(), back_to_internal.headers.len());
    }
}

#[test]
fn test_wal_record_roundtrip() {
    let original = WalRecord {
        topic_partition: TopicPartition {
            topic: "test-topic".to_string(),
            partition: 5,
        },
        offset: 12345,
        record: Record {
            key: Some(b"wal-key".to_vec()),
            value: b"wal-value".to_vec(),
            headers: vec![],
            timestamp: Utc::now().timestamp_millis(),
        },
        crc32: 98765,
    };
    
    let proto: common::WalRecord = original.clone().try_into().unwrap();
    let back_to_internal: WalRecord = proto.try_into().unwrap();
    
    assert_eq!(original.topic_partition, back_to_internal.topic_partition);
    assert_eq!(original.offset, back_to_internal.offset);
    assert_eq!(original.crc32, back_to_internal.crc32);
}

#[test]
fn test_wal_record_missing_fields() {
    // Test missing topic_partition
    let proto_wal = common::WalRecord {
        topic_partition: None,
        offset: 100,
        record: Some(common::Record {
            key: b"key".to_vec(),
            value: b"value".to_vec(),
            headers: vec![],
            timestamp: Some(Timestamp { seconds: 0, nanos: 0 }),
        }),
        crc32: 12345,
        version: 1,
    };
    
    let result: Result<WalRecord, ConversionError> = proto_wal.try_into();
    assert!(result.is_err());
    match result.unwrap_err() {
        ConversionError::MissingField { field } => assert_eq!(field, "topic_partition"),
        _ => panic!("Expected MissingField error"),
    }

    // Test missing record
    let proto_wal = common::WalRecord {
        topic_partition: Some(common::TopicPartition {
            topic: "test".to_string(),
            partition: 0,
        }),
        offset: 100,
        record: None,
        crc32: 12345,
        version: 1,
    };
    
    let result: Result<WalRecord, ConversionError> = proto_wal.try_into();
    assert!(result.is_err());
    match result.unwrap_err() {
        ConversionError::MissingField { field } => assert_eq!(field, "record"),
        _ => panic!("Expected MissingField error"),
    }
}

#[test]
fn test_broker_info_roundtrip() {
    let original = BrokerInfo {
        id: "broker-123".to_string(),
        host: "192.168.1.100".to_string(),
        port_quic: 9092,
        port_rpc: 9093,
        rack_id: "rack-a".to_string(),
    };
    
    let proto: common::BrokerInfo = original.clone().try_into().unwrap();
    let back_to_internal: BrokerInfo = proto.try_into().unwrap();
    
    // Manual comparison since BrokerInfo doesn't implement PartialEq
    assert_eq!(original.id, back_to_internal.id);
    assert_eq!(original.host, back_to_internal.host);
    assert_eq!(original.port_quic, back_to_internal.port_quic);
    assert_eq!(original.port_rpc, back_to_internal.port_rpc);
    assert_eq!(original.rack_id, back_to_internal.rack_id);
}

#[test]
fn test_broker_info_edge_cases() {
    let test_cases = vec![
        BrokerInfo {
            id: "".to_string(), // Empty ID
            host: "localhost".to_string(),
            port_quic: 1,
            port_rpc: 2,
            rack_id: String::new(),
        },
        BrokerInfo {
            id: "broker-max".to_string(),
            host: "2001:db8::1".to_string(), // IPv6
            port_quic: 65535,
            port_rpc: 65534,
            rack_id: "rack-with-special-chars-åäö".to_string(),
        },
    ];

    for original in test_cases {
        let proto: common::BrokerInfo = original.clone().try_into().unwrap();
        let back_to_internal: BrokerInfo = proto.try_into().unwrap();
        // Manual comparison since BrokerInfo doesn't implement PartialEq
        assert_eq!(original.id, back_to_internal.id);
        assert_eq!(original.host, back_to_internal.host);
        assert_eq!(original.port_quic, back_to_internal.port_quic);
        assert_eq!(original.port_rpc, back_to_internal.port_rpc);
        assert_eq!(original.rack_id, back_to_internal.rack_id);
    }
}

// ============================================================================
// Enum Conversion Tests
// ============================================================================

#[test]
fn test_acknowledgment_level_all_variants() {
    let levels = vec![
        AcknowledgmentLevel::None,
        AcknowledgmentLevel::Leader,
        AcknowledgmentLevel::Majority,
        AcknowledgmentLevel::All,
        AcknowledgmentLevel::Custom(5),
        AcknowledgmentLevel::Custom(0),
        AcknowledgmentLevel::Custom(u32::MAX as usize),
    ];
    
    for level in levels {
        let proto_level: common::AcknowledgmentLevel = level.clone().into();
        let back_to_internal: AcknowledgmentLevel = proto_level.into();
        
        // Custom levels will map to Custom(3) due to default mapping
        match level {
            AcknowledgmentLevel::Custom(_) => {
                assert!(matches!(back_to_internal, AcknowledgmentLevel::Custom(3)));
            }
            _ => {
                assert_eq!(
                    std::mem::discriminant(&level),
                    std::mem::discriminant(&back_to_internal)
                );
            }
        }
    }
}

#[test]
fn test_durability_level_roundtrip() {
    let levels = vec![
        DurabilityLevel::LocalOnly,
        DurabilityLevel::Durable,
    ];
    
    for level in levels {
        let proto_level: common::DurabilityLevel = level.clone().into();
        let back_to_internal: DurabilityLevel = proto_level.into();
        assert_eq!(
            std::mem::discriminant(&level),
            std::mem::discriminant(&back_to_internal)
        );
    }
}

#[test]
fn test_compression_type_roundtrip() {
    let types = vec![
        CompressionType::None,
        CompressionType::Lz4,
        CompressionType::Zstd,
    ];
    
    for compression_type in types {
        let proto_type: common::CompressionType = compression_type.clone().into();
        let back_to_internal: CompressionType = proto_type.into();
        assert_eq!(
            std::mem::discriminant(&compression_type),
            std::mem::discriminant(&back_to_internal)
        );
    }
}

// ============================================================================
// Follower State Conversion Tests
// ============================================================================

#[test]
fn test_follower_state_roundtrip() {
    let original = FollowerState {
        broker_id: "follower-1".to_string(),
        last_known_offset: 12345,
        last_heartbeat: Utc::now(),
        lag: 500,
    };
    
    let proto: common::FollowerState = original.clone().try_into().unwrap();
    let back_to_internal: FollowerState = proto.try_into().unwrap();
    
    assert_eq!(original.broker_id, back_to_internal.broker_id);
    assert_eq!(original.last_known_offset, back_to_internal.last_known_offset);
    assert_eq!(original.lag, back_to_internal.lag);
    // Timestamp conversion may have slight differences
    assert!((original.last_heartbeat.timestamp_millis() - back_to_internal.last_heartbeat.timestamp_millis()).abs() <= 1000);
}

#[test]
fn test_follower_state_edge_cases() {
    let test_cases = vec![
        FollowerState {
            broker_id: "".to_string(), // Empty broker ID
            last_known_offset: 0,
            last_heartbeat: Utc.timestamp_opt(0, 0).unwrap(),
            lag: 0,
        },
        FollowerState {
            broker_id: "follower-max".to_string(),
            last_known_offset: u64::MAX,
            last_heartbeat: Utc::now(),
            lag: u64::MAX,
        },
    ];

    for original in test_cases {
        let proto_result: Result<common::FollowerState, ConversionError> = original.clone().try_into();
        assert!(proto_result.is_ok());
        
        let proto = proto_result.unwrap();
        let back_result: Result<FollowerState, ConversionError> = proto.try_into();
        assert!(back_result.is_ok());
        
        let back_to_internal = back_result.unwrap();
        assert_eq!(original.broker_id, back_to_internal.broker_id);
        assert_eq!(original.last_known_offset, back_to_internal.last_known_offset);
        assert_eq!(original.lag, back_to_internal.lag);
    }
}

// ============================================================================
// Request/Response Conversion Tests
// ============================================================================

#[test]
fn test_replicate_data_request_roundtrip() {
    let original = ReplicateDataRequest {
        leader_epoch: 42,
        topic_partition: TopicPartition {
            topic: "test-topic".to_string(),
            partition: 3,
        },
        records: vec![
            WalRecord {
                topic_partition: TopicPartition {
                    topic: "test-topic".to_string(),
                    partition: 3,
                },
                offset: 100,
                record: Record {
                    key: Some(b"key1".to_vec()),
                    value: b"value1".to_vec(),
                    headers: vec![],
                    timestamp: Utc::now().timestamp_millis(),
                },
                crc32: 12345,
            },
        ],
        leader_id: "leader-1".to_string(),
    };
    
    let proto: broker::ReplicateDataRequest = original.clone().try_into().unwrap();
    let back_to_internal: ReplicateDataRequest = proto.try_into().unwrap();
    
    assert_eq!(original.leader_epoch, back_to_internal.leader_epoch);
    assert_eq!(original.topic_partition, back_to_internal.topic_partition);
    assert_eq!(original.leader_id, back_to_internal.leader_id);
    assert_eq!(original.records.len(), back_to_internal.records.len());
}

#[test]
fn test_replicate_data_response_roundtrip() {
    let original = ReplicateDataResponse {
        success: true,
        error_code: 0,
        error_message: None,
        follower_state: Some(FollowerState {
            broker_id: "follower-1".to_string(),
            last_known_offset: 200,
            last_heartbeat: Utc::now(),
            lag: 10,
        }),
    };
    
    let proto: broker::ReplicateDataResponse = original.clone().try_into().unwrap();
    let back_to_internal: ReplicateDataResponse = proto.try_into().unwrap();
    
    assert_eq!(original.success, back_to_internal.success);
    assert_eq!(original.error_code, back_to_internal.error_code);
    assert_eq!(original.error_message, back_to_internal.error_message);
    assert!(original.follower_state.is_some());
    assert!(back_to_internal.follower_state.is_some());
}

#[test]
fn test_heartbeat_request_roundtrip() {
    let original = HeartbeatRequest {
        leader_epoch: 5,
        leader_id: "leader-2".to_string(),
        topic_partition: TopicPartition {
            topic: "heartbeat-topic".to_string(),
            partition: 1,
        },
        high_watermark: 1000,
    };
    
    let proto: broker::HeartbeatRequest = original.clone().try_into().unwrap();
    let back_to_internal: HeartbeatRequest = proto.try_into().unwrap();
    
    // Manual comparison since HeartbeatRequest doesn't implement PartialEq
    assert_eq!(original.leader_epoch, back_to_internal.leader_epoch);
    assert_eq!(original.leader_id, back_to_internal.leader_id);
    assert_eq!(original.topic_partition, back_to_internal.topic_partition);
    assert_eq!(original.high_watermark, back_to_internal.high_watermark);
}

#[test]
fn test_heartbeat_response_error_case() {
    let original = HeartbeatResponse {
        success: false,
        error_code: 404,
        error_message: Some("Partition not found".to_string()),
        follower_state: None,
    };
    
    let proto: broker::HeartbeatResponse = original.clone().try_into().unwrap();
    let back_to_internal: HeartbeatResponse = proto.try_into().unwrap();
    
    assert_eq!(original.success, back_to_internal.success);
    assert_eq!(original.error_code, back_to_internal.error_code);
    assert_eq!(original.error_message, back_to_internal.error_message);
    // Manual comparison for follower_state
    assert_eq!(original.follower_state.is_some(), back_to_internal.follower_state.is_some());
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_error_code_mapping_comprehensive() {
    let errors = vec![
        RustMqError::TopicNotFound("test-topic".to_string()),
        RustMqError::PartitionNotFound("test-partition".to_string()),
        RustMqError::BrokerNotFound("test-broker".to_string()),
        RustMqError::StaleLeaderEpoch { request_epoch: 1, current_epoch: 2 },
        RustMqError::NotLeader("test-broker".to_string()),
        RustMqError::OffsetOutOfRange("offset 100".to_string()),
        RustMqError::TopicAlreadyExists("existing-topic".to_string()),
        RustMqError::ResourceExhausted("memory".to_string()),
        RustMqError::PermissionDenied("access denied".to_string()),
        RustMqError::Timeout,
        RustMqError::InvalidOperation("test operation".to_string()),
        RustMqError::ObjectNotFound("test-object".to_string()),
    ];

    for error in errors {
        let error_code = proto_convert::error_to_code(&error);
        let error_message = proto_convert::error_to_message(&error);
        let is_retryable = proto_convert::error_is_retryable(&error);
        
        // Verify error code is valid
        assert!(error_code > 0, "Error code should be positive for {:?}", error);
        
        // Verify error message is not empty
        assert!(!error_message.is_empty(), "Error message should not be empty for {:?}", error);
        
        // Verify retryable classification makes sense
        match error {
            RustMqError::Timeout | RustMqError::ResourceExhausted(_) => {
                assert!(is_retryable, "Should be retryable: {:?}", error);
            }
            RustMqError::TopicNotFound(_) | RustMqError::PermissionDenied(_) => {
                assert!(!is_retryable, "Should not be retryable: {:?}", error);
            }
            _ => {
                // Other errors may or may not be retryable
                println!("Error {:?} retryable: {}", error, is_retryable);
            }
        }
    }
}

// ============================================================================
// Timestamp Conversion Tests (through Record conversion)
// ============================================================================

#[test]
fn test_timestamp_conversion_edge_cases() {
    let test_timestamps = vec![
        0, // Unix epoch
        Utc::now().timestamp_millis(),
        1234567890123, // Specific timestamp
    ];

    for millis in test_timestamps {
        // Test timestamp conversion through Record conversion
        let record = Record {
            key: None,
            value: vec![],
            headers: vec![],
            timestamp: millis,
        };
        let proto_record: common::Record = record.try_into().unwrap();
        let back_record: Record = proto_record.try_into().unwrap();
        
        // Allow small rounding error due to nanosecond precision
        assert!((millis - back_record.timestamp).abs() <= 1, 
            "Timestamp conversion failed: {} != {}", millis, back_record.timestamp);
    }
}

#[test]
fn test_timestamp_precision() {
    // Test millisecond precision preservation
    let base_time = Utc::now().timestamp_millis();
    for i in 0..100 {
        let test_time = base_time + i;
        let record = Record {
            key: None,
            value: vec![],
            headers: vec![],
            timestamp: test_time,
        };
        let proto_record: common::Record = record.try_into().unwrap();
        let back_record: Record = proto_record.try_into().unwrap();
        
        // Allow for rounding error
        assert!((test_time - back_record.timestamp).abs() <= 1, 
            "Precision lost for timestamp {}", test_time);
    }
}

// ============================================================================
// Performance and Stress Tests
// ============================================================================

#[test]
fn test_large_data_conversion() {
    // Test conversion with large data structures
    let large_record = Record {
        key: Some(vec![0u8; 10_000]), // 10KB key
        value: vec![1u8; 100_000],    // 100KB value
        headers: (0..100).map(|i| Header {
            key: format!("header-{}", i),
            value: vec![i as u8; 10],
        }).collect(),
        timestamp: Utc::now().timestamp_millis(),
    };

    let start = std::time::Instant::now();
    let proto_result: Result<common::Record, ConversionError> = large_record.clone().try_into();
    let conversion_time = start.elapsed();
    
    assert!(proto_result.is_ok(), "Large data conversion should succeed");
    assert!(conversion_time.as_millis() < 1000, "Conversion should be fast even for large data");
    
    if let Ok(proto) = proto_result {
        let start = std::time::Instant::now();
        let back_result: Result<Record, ConversionError> = proto.try_into();
        let back_conversion_time = start.elapsed();
        
        assert!(back_result.is_ok(), "Back conversion should succeed");
        assert!(back_conversion_time.as_millis() < 1000, "Back conversion should be fast");
    }
}

#[test]
fn test_random_data_resilience() {
    let mut rng = rand::thread_rng();
    
    for _ in 0..100 {
        // Generate random record data
        let key_len = rng.gen_range(0..100);
        let value_len = rng.gen_range(0..1000);
        let header_count = rng.gen_range(0..10);
        
        let record = Record {
            key: if key_len > 0 { 
                Some((0..key_len).map(|_| rng.r#gen()).collect()) 
            } else { 
                None 
            },
            value: (0..value_len).map(|_| rng.r#gen()).collect(),
            headers: (0..header_count).map(|i| Header {
                key: format!("header-{}", i),
                value: (0..rng.gen_range(0..10)).map(|_| rng.r#gen()).collect(),
            }).collect(),
            timestamp: rng.gen_range(0..i64::MAX / 1000),
        };

        // Should not panic on any random data
        let proto_result: Result<common::Record, ConversionError> = record.try_into();
        if let Ok(proto) = proto_result {
            let _back_result: Result<Record, ConversionError> = proto.try_into();
            // Even if conversion fails, it should not panic
        }
    }
}

#[test]
fn test_utf8_edge_cases() {
    let test_strings = vec![
        String::new(), // Empty string
        "Hello, World!".to_string(), // ASCII
        "Héllo, Wörld! 🌍".to_string(), // Unicode with emoji
        "नमस्ते दुनिया".to_string(), // Hindi
        "こんにちは世界".to_string(), // Japanese
        "🚀🔥⭐🎯💯".to_string(), // Only emojis
    ];

    for test_string in test_strings {
        let topic_partition = TopicPartition {
            topic: test_string.clone(),
            partition: 0,
        };
        
        let proto: common::TopicPartition = topic_partition.clone().into();
        let back: TopicPartition = proto.into();
        
        assert_eq!(topic_partition, back, "UTF-8 conversion failed for: {}", test_string);
    }
}