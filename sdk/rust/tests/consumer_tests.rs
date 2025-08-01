use rustmq_client::{
    config::{ClientConfig, ConsumerConfig, StartPosition},
    consumer::{ConsumerBuilder, ConsumerRequest, ConsumerResponse},
    error::{ClientError, Result},
    message::{MessageBuilder},
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{Duration, Instant};
use std::collections::HashMap;

// Note: Mock client implementation would require access to internal connection traits
// For unit testing, we focus on protocol message serialization/deserialization

#[tokio::test]
async fn test_consumer_builder() {
    let _config = ClientConfig::default();
    
    // Test missing topic
    let result = ConsumerBuilder::new()
        .consumer_group("test-group")
        .build()
        .await;
    assert!(result.is_err());
    
    // Test missing consumer group
    let result = ConsumerBuilder::new()
        .topic("test-topic")
        .build()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_consumer_subscription_protocol() {
    // Test subscription request format
    let request = ConsumerRequest::Subscribe {
        topic: "test-topic".to_string(),
        consumer_group: "test-group".to_string(),
        consumer_id: "test-consumer".to_string(),
        start_position: StartPosition::Latest,
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::Subscribe { topic, consumer_group, consumer_id, start_position } => {
            assert_eq!(topic, "test-topic");
            assert_eq!(consumer_group, "test-group");
            assert_eq!(consumer_id, "test-consumer");
            assert!(matches!(start_position, StartPosition::Latest));
        }
        _ => panic!("Unexpected request type"),
    }
}

#[tokio::test]
async fn test_consumer_fetch_protocol() {
    // Test fetch request format
    let request = ConsumerRequest::Fetch {
        topic: "test-topic".to_string(),
        partition: 0,
        offset: 100,
        max_messages: 10,
        timeout_ms: 1000,
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::Fetch { topic, partition, offset, max_messages, timeout_ms } => {
            assert_eq!(topic, "test-topic");
            assert_eq!(partition, 0);
            assert_eq!(offset, 100);
            assert_eq!(max_messages, 10);
            assert_eq!(timeout_ms, 1000);
        }
        _ => panic!("Unexpected request type"),
    }
}

#[tokio::test]
async fn test_consumer_commit_protocol() {
    // Test commit request format
    let mut offsets = HashMap::new();
    offsets.insert(0u32, 150u64);
    
    let request = ConsumerRequest::CommitOffsets {
        topic: "test-topic".to_string(),
        consumer_group: "test-group".to_string(),
        offsets,
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::CommitOffsets { topic, consumer_group, offsets } => {
            assert_eq!(topic, "test-topic");
            assert_eq!(consumer_group, "test-group");
            assert_eq!(offsets.get(&0), Some(&150));
        }
        _ => panic!("Unexpected request type"),
    }
}

#[tokio::test]
async fn test_consumer_seek_protocol() {
    // Test seek request format
    let request = ConsumerRequest::Seek {
        topic: "test-topic".to_string(),
        partition: 0,
        offset: 200,
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::Seek { topic, partition, offset } => {
            assert_eq!(topic, "test-topic");
            assert_eq!(partition, 0);
            assert_eq!(offset, 200);
        }
        _ => panic!("Unexpected request type"),
    }
}

#[tokio::test]
async fn test_consumer_seek_to_timestamp_protocol() {
    // Test timestamp seek request format
    let request = ConsumerRequest::SeekToTimestamp {
        topic: "test-topic".to_string(),
        partition: 0,
        timestamp: 1234567890,
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::SeekToTimestamp { topic, partition, timestamp } => {
            assert_eq!(topic, "test-topic");
            assert_eq!(partition, 0);
            assert_eq!(timestamp, 1234567890);
        }
        _ => panic!("Unexpected request type"),
    }
}

#[tokio::test]
async fn test_consumer_response_serialization() {
    // Test SubscribeOk response
    let response = ConsumerResponse::SubscribeOk {
        assigned_partitions: vec![0, 1, 2],
    };

    let serialized = serde_json::to_vec(&response).unwrap();
    let deserialized: ConsumerResponse = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerResponse::SubscribeOk { assigned_partitions } => {
            assert_eq!(assigned_partitions, vec![0, 1, 2]);
        }
        _ => panic!("Unexpected response type"),
    }

    // Test FetchOk response
    let messages = vec![
        MessageBuilder::new()
            .topic("test-topic")
            .payload("message 1")
            .build()
            .unwrap(),
        MessageBuilder::new()
            .topic("test-topic")
            .payload("message 2")
            .build()
            .unwrap(),
    ];

    let response = ConsumerResponse::FetchOk {
        messages: messages.clone(),
        next_offset: 105,
        partition: 0,
    };

    let serialized = serde_json::to_vec(&response).unwrap();
    let deserialized: ConsumerResponse = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerResponse::FetchOk { messages: fetched_messages, next_offset, partition } => {
            assert_eq!(fetched_messages.len(), 2);
            assert_eq!(next_offset, 105);
            assert_eq!(partition, 0);
        }
        _ => panic!("Unexpected response type"),
    }

    // Test Error response
    let response = ConsumerResponse::Error {
        message: "Test error".to_string(),
        error_code: 404,
    };

    let serialized = serde_json::to_vec(&response).unwrap();
    let deserialized: ConsumerResponse = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerResponse::Error { message, error_code } => {
            assert_eq!(message, "Test error");
            assert_eq!(error_code, 404);
        }
        _ => panic!("Unexpected response type"),
    }
}

#[tokio::test]
async fn test_consumer_message_acknowledgment_concept() {
    // Test that we can create messages (actual ack testing requires internal access)
    let message = MessageBuilder::new()
        .topic("test-topic")
        .payload("test message")
        .build()
        .unwrap();

    // Verify message properties
    assert_eq!(message.topic, "test-topic");
    assert_eq!(message.payload_as_string().unwrap(), "test message");
    
    // Note: Actual ack/nack testing requires access to internal Consumer methods
    // which are properly tested through integration tests with a real broker
}

#[tokio::test]
async fn test_consumer_config_defaults() {
    let config = ConsumerConfig::default();
    
    assert_eq!(config.consumer_group, "default-group");
    assert_eq!(config.auto_commit_interval, Duration::from_secs(5));
    assert!(config.enable_auto_commit);
    assert_eq!(config.fetch_size, 100);
    assert_eq!(config.fetch_timeout, Duration::from_secs(1));
    assert!(matches!(config.start_position, StartPosition::Latest));
    assert_eq!(config.max_retry_attempts, 3);
    assert!(config.dead_letter_queue.is_none());
}

#[tokio::test]
async fn test_consumer_config_customization() {
    let custom_config = ConsumerConfig {
        consumer_id: Some("custom-consumer".to_string()),
        consumer_group: "custom-group".to_string(),
        auto_commit_interval: Duration::from_secs(10),
        enable_auto_commit: false,
        fetch_size: 50,
        fetch_timeout: Duration::from_secs(2),
        start_position: StartPosition::Earliest,
        max_retry_attempts: 5,
        dead_letter_queue: Some("dlq-topic".to_string()),
    };

    assert_eq!(custom_config.consumer_id, Some("custom-consumer".to_string()));
    assert_eq!(custom_config.consumer_group, "custom-group");
    assert_eq!(custom_config.auto_commit_interval, Duration::from_secs(10));
    assert!(!custom_config.enable_auto_commit);
    assert_eq!(custom_config.fetch_size, 50);
    assert_eq!(custom_config.fetch_timeout, Duration::from_secs(2));
    assert!(matches!(custom_config.start_position, StartPosition::Earliest));
    assert_eq!(custom_config.max_retry_attempts, 5);
    assert_eq!(custom_config.dead_letter_queue, Some("dlq-topic".to_string()));
}

#[tokio::test]
async fn test_start_position_serialization() {
    // Test all StartPosition variants
    let positions = vec![
        StartPosition::Earliest,
        StartPosition::Latest,
        StartPosition::Offset(12345),
        StartPosition::Timestamp(1234567890),
    ];

    for position in positions {
        let serialized = serde_json::to_string(&position).unwrap();
        let deserialized: StartPosition = serde_json::from_str(&serialized).unwrap();
        
        match (&position, &deserialized) {
            (StartPosition::Earliest, StartPosition::Earliest) => {},
            (StartPosition::Latest, StartPosition::Latest) => {},
            (StartPosition::Offset(a), StartPosition::Offset(b)) => assert_eq!(a, b),
            (StartPosition::Timestamp(a), StartPosition::Timestamp(b)) => assert_eq!(a, b),
            _ => panic!("StartPosition serialization mismatch"),
        }
    }
}

#[tokio::test]
async fn test_consumer_metrics_functionality() {
    use rustmq_client::consumer::ConsumerMetrics;
    use std::sync::atomic::Ordering;

    let metrics = ConsumerMetrics::default();

    // Test atomic counters
    metrics.messages_received.store(10, Ordering::Relaxed);
    metrics.messages_processed.store(8, Ordering::Relaxed);
    metrics.messages_failed.store(2, Ordering::Relaxed);
    metrics.bytes_received.store(1024, Ordering::Relaxed);

    assert_eq!(metrics.messages_received.load(Ordering::Relaxed), 10);
    assert_eq!(metrics.messages_processed.load(Ordering::Relaxed), 8);
    assert_eq!(metrics.messages_failed.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.bytes_received.load(Ordering::Relaxed), 1024);

    // Test increment operations
    metrics.messages_received.fetch_add(5, Ordering::Relaxed);
    assert_eq!(metrics.messages_received.load(Ordering::Relaxed), 15);

    // Test lag and timing
    {
        let mut lag = metrics.lag.write().await;
        *lag = 50;
    }
    assert_eq!(*metrics.lag.read().await, 50);

    {
        let mut last_receive = metrics.last_receive_time.write().await;
        *last_receive = Some(Instant::now());
    }
    assert!(metrics.last_receive_time.read().await.is_some());
}

#[tokio::test]
async fn test_exponential_backoff_calculation() {
    // Test exponential backoff logic (since FailedMessage is private)
    let base_delay = Duration::from_secs(1);
    
    // Test retry delay calculation
    let retry_1 = base_delay * 2_u32.pow(1); // 2 seconds
    let retry_2 = base_delay * 2_u32.pow(2); // 4 seconds
    let retry_3 = base_delay * 2_u32.pow(3); // 8 seconds
    
    assert_eq!(retry_1, Duration::from_secs(2));
    assert_eq!(retry_2, Duration::from_secs(4));
    assert_eq!(retry_3, Duration::from_secs(8));
    
    // Test maximum retry count logic
    let max_retries = 3;
    for retry_count in 0..=max_retries {
        if retry_count < max_retries {
            let delay = base_delay * 2_u32.pow(retry_count + 1);
            assert!(delay >= base_delay);
        } else {
            // Should send to DLQ after max retries
            assert_eq!(retry_count, max_retries);
        }
    }
}

#[tokio::test]
async fn test_consumer_error_handling() {
    // Test various error scenarios
    let connection_error = ClientError::Connection("Connection failed".to_string());
    assert_eq!(connection_error.category(), "connection");
    assert!(connection_error.is_retryable());

    let consumer_error = ClientError::Consumer("Consumer error".to_string());
    assert_eq!(consumer_error.category(), "consumer");
    assert!(!consumer_error.is_retryable());

    let timeout_error = ClientError::Timeout { timeout_ms: 5000 };
    assert_eq!(timeout_error.category(), "timeout");
    assert!(timeout_error.is_retryable());

    let offset_error = ClientError::OffsetOutOfRange { offset: 12345 };
    assert_eq!(offset_error.category(), "offset");
    assert!(!offset_error.is_retryable());
}

#[tokio::test] 
async fn test_message_age_calculation() {
    let mut message = MessageBuilder::new()
        .topic("test-topic")
        .payload("test message")
        .build()
        .unwrap();

    // Set timestamp to 1 second ago
    let one_second_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64 - 1000;
    
    message.timestamp = one_second_ago;

    let age = message.age_ms();
    assert!(age >= 900 && age <= 1100); // Allow for some timing variance
}

#[tokio::test]
async fn test_consumer_group_metadata_protocol() {
    let request = ConsumerRequest::GetConsumerGroupMetadata {
        consumer_group: "test-group".to_string(),
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::GetConsumerGroupMetadata { consumer_group } => {
            assert_eq!(consumer_group, "test-group");
        }
        _ => panic!("Unexpected request type"),
    }

    // Test response
    let mut partitions = HashMap::new();
    partitions.insert(0, 100u64);
    partitions.insert(1, 200u64);

    let response = ConsumerResponse::ConsumerGroupMetadata { partitions: partitions.clone() };

    let serialized = serde_json::to_vec(&response).unwrap();
    let deserialized: ConsumerResponse = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerResponse::ConsumerGroupMetadata { partitions: returned_partitions } => {
            assert_eq!(returned_partitions.len(), 2);
            assert_eq!(returned_partitions.get(&0), Some(&100));
            assert_eq!(returned_partitions.get(&1), Some(&200));
        }
        _ => panic!("Unexpected response type"),
    }
}

#[tokio::test]
async fn test_multi_partition_commit_protocol() {
    // Test commit request with multiple partitions
    let mut offsets = HashMap::new();
    offsets.insert(0u32, 100u64);
    offsets.insert(1u32, 200u64);
    offsets.insert(2u32, 150u64);
    
    let request = ConsumerRequest::CommitOffsets {
        topic: "test-topic".to_string(),
        consumer_group: "test-group".to_string(),
        offsets: offsets.clone(),
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::CommitOffsets { topic, consumer_group, offsets: deserialized_offsets } => {
            assert_eq!(topic, "test-topic");
            assert_eq!(consumer_group, "test-group");
            assert_eq!(deserialized_offsets.len(), 3);
            assert_eq!(deserialized_offsets.get(&0), Some(&100));
            assert_eq!(deserialized_offsets.get(&1), Some(&200));
            assert_eq!(deserialized_offsets.get(&2), Some(&150));
        }
        _ => panic!("Unexpected request type"),
    }
}

#[tokio::test]
async fn test_multi_partition_seek_all_protocol() {
    // Test seek all partitions to timestamp
    let request = ConsumerRequest::SeekAllToTimestamp {
        topic: "test-topic".to_string(),
        timestamp: 1234567890,
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::SeekAllToTimestamp { topic, timestamp } => {
            assert_eq!(topic, "test-topic");
            assert_eq!(timestamp, 1234567890);
        }
        _ => panic!("Unexpected request type"),
    }

    // Test response
    let mut partition_offsets = HashMap::new();
    partition_offsets.insert(0, 500u64);
    partition_offsets.insert(1, 750u64);
    partition_offsets.insert(2, 600u64);

    let response = ConsumerResponse::SeekAllOk { partition_offsets: partition_offsets.clone() };

    let serialized = serde_json::to_vec(&response).unwrap();
    let deserialized: ConsumerResponse = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerResponse::SeekAllOk { partition_offsets: returned_offsets } => {
            assert_eq!(returned_offsets.len(), 3);
            assert_eq!(returned_offsets.get(&0), Some(&500));
            assert_eq!(returned_offsets.get(&1), Some(&750));
            assert_eq!(returned_offsets.get(&2), Some(&600));
        }
        _ => panic!("Unexpected response type"),
    }
}

#[tokio::test]
async fn test_partition_info_serialization() {
    use rustmq_client::consumer::PartitionInfo;
    
    let partition_info = PartitionInfo {
        partition_id: 0,
        leader_broker: "broker-1".to_string(),
        replica_brokers: vec!["broker-1".to_string(), "broker-2".to_string(), "broker-3".to_string()],
        high_watermark: 1000,
        low_watermark: 0,
    };

    let serialized = serde_json::to_string(&partition_info).unwrap();
    let deserialized: PartitionInfo = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.partition_id, 0);
    assert_eq!(deserialized.leader_broker, "broker-1");
    assert_eq!(deserialized.replica_brokers.len(), 3);
    assert_eq!(deserialized.high_watermark, 1000);
    assert_eq!(deserialized.low_watermark, 0);
}

#[tokio::test]
async fn test_partition_metadata_request() {
    let request = ConsumerRequest::GetPartitionMetadata {
        topic: "test-topic".to_string(),
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::GetPartitionMetadata { topic } => {
            assert_eq!(topic, "test-topic");
        }
        _ => panic!("Unexpected request type"),
    }
}

#[tokio::test]
async fn test_rebalance_request() {
    let request = ConsumerRequest::RequestRebalance {
        consumer_group: "test-group".to_string(),
        consumer_id: "test-consumer".to_string(),
    };

    let serialized = serde_json::to_vec(&request).unwrap();
    let deserialized: ConsumerRequest = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerRequest::RequestRebalance { consumer_group, consumer_id } => {
            assert_eq!(consumer_group, "test-group");
            assert_eq!(consumer_id, "test-consumer");
        }
        _ => panic!("Unexpected request type"),
    }

    // Test response
    let response = ConsumerResponse::RebalanceTriggered {
        new_assignment: vec![1, 2, 3],
    };

    let serialized = serde_json::to_vec(&response).unwrap();
    let deserialized: ConsumerResponse = serde_json::from_slice(&serialized).unwrap();

    match deserialized {
        ConsumerResponse::RebalanceTriggered { new_assignment } => {
            assert_eq!(new_assignment, vec![1, 2, 3]);
        }
        _ => panic!("Unexpected response type"),
    }
}

#[tokio::test]
async fn test_multi_partition_offset_tracker() {
    use rustmq_client::consumer::OffsetTracker;
    use std::collections::BTreeSet;
    
    let mut tracker = OffsetTracker::default();
    
    // Test adding consecutive pending offsets for different partitions (more realistic scenario)
    // Partition 0: Start with committed offset 0, add messages 1, 2, 3
    tracker.add_pending_offset(0, 1);
    tracker.add_pending_offset(0, 2);
    tracker.add_pending_offset(0, 3);
    
    // Partition 1: Add messages starting from 1
    tracker.add_pending_offset(1, 1);
    tracker.add_pending_offset(1, 2);
    
    // Partition 2: Add a single message
    tracker.add_pending_offset(2, 1);
    
    // Test getting committed offsets (should be 0 initially)
    assert_eq!(tracker.get_committed_offset(0), 0);
    assert_eq!(tracker.get_committed_offset(1), 0);
    assert_eq!(tracker.get_committed_offset(2), 0);
    
    // Test removing and updating consecutive committed offsets
    tracker.remove_pending_offset(0, 1); // Remove offset 1 for partition 0
    tracker.update_consecutive_committed(0); // This should update partition 0 to offset 1
    
    assert_eq!(tracker.get_committed_offset(0), 1);
    assert_eq!(tracker.get_committed_offset(1), 0); // Other partitions unchanged
    
    // Test removing offset 2 for partition 0
    tracker.remove_pending_offset(0, 2);
    tracker.update_consecutive_committed(0);
    assert_eq!(tracker.get_committed_offset(0), 2);
    
    // Test removing offset 3 for partition 0
    tracker.remove_pending_offset(0, 3);
    tracker.update_consecutive_committed(0);
    assert_eq!(tracker.get_committed_offset(0), 3);
    
    // Test partition 1 - acknowledge message 1
    tracker.remove_pending_offset(1, 1);
    tracker.update_consecutive_committed(1);
    assert_eq!(tracker.get_committed_offset(1), 1);
    
    // Test setting committed offsets directly
    tracker.set_committed_offset(2, 299);
    assert_eq!(tracker.get_committed_offset(2), 299);
    
    // Test clearing pending offsets
    tracker.clear_pending_offsets(1);
    tracker.add_pending_offset(1, 250);
    // After clearing, only offset 250 should be pending for partition 1
    
    // Test getting all committed offsets
    let all_offsets = tracker.get_all_committed_offsets();
    assert_eq!(all_offsets.get(&0), Some(&3));
    assert_eq!(all_offsets.get(&1), Some(&1));
    assert_eq!(all_offsets.get(&2), Some(&299));
}

#[tokio::test]
async fn test_comprehensive_multi_partition_flow() {
    use rustmq_client::consumer::OffsetTracker;
    
    let mut tracker = OffsetTracker::default();
    
    // Simulate receiving messages from multiple partitions out of order
    // Partition 0: messages 1, 3, 2 (out of order)
    tracker.add_pending_offset(0, 1);
    tracker.add_pending_offset(0, 3);
    tracker.add_pending_offset(0, 2);
    
    // Partition 1: messages 1, 2
    tracker.add_pending_offset(1, 1);
    tracker.add_pending_offset(1, 2);
    
    // Partition 2: message 1
    tracker.add_pending_offset(2, 1);
    
    // All partitions should start with committed offset 0
    assert_eq!(tracker.get_committed_offset(0), 0);
    assert_eq!(tracker.get_committed_offset(1), 0);
    assert_eq!(tracker.get_committed_offset(2), 0);
    
    // Acknowledge message 1 from partition 0 - should advance to 1
    tracker.remove_pending_offset(0, 1);
    tracker.update_consecutive_committed(0);
    assert_eq!(tracker.get_committed_offset(0), 1);
    
    // Acknowledge message 3 from partition 0 - should NOT advance beyond 1 (gap at 2)
    tracker.remove_pending_offset(0, 3);
    tracker.update_consecutive_committed(0);
    assert_eq!(tracker.get_committed_offset(0), 1); // Still at 1 due to gap
    
    // Acknowledge message 2 from partition 0 - should now advance to 3 (fills gap)
    tracker.remove_pending_offset(0, 2);
    tracker.update_consecutive_committed(0);
    assert_eq!(tracker.get_committed_offset(0), 3); // Now advanced to 3
    
    // Test commit request generation
    let commit_offsets = tracker.get_all_committed_offsets();
    assert_eq!(commit_offsets.get(&0), Some(&3));
    // Partitions 1 and 2 haven't been explicitly set, so they don't appear in the HashMap
    assert_eq!(commit_offsets.get(&1), None); // Not yet explicitly set
    assert_eq!(commit_offsets.get(&2), None); // Not yet explicitly set
    
    // Acknowledge messages from other partitions
    tracker.remove_pending_offset(1, 1);
    tracker.update_consecutive_committed(1);
    assert_eq!(tracker.get_committed_offset(1), 1);
    
    tracker.remove_pending_offset(1, 2);
    tracker.update_consecutive_committed(1);
    assert_eq!(tracker.get_committed_offset(1), 2);
    
    tracker.remove_pending_offset(2, 1);
    tracker.update_consecutive_committed(2);
    assert_eq!(tracker.get_committed_offset(2), 1);
    
    // Final commit offsets check
    let final_offsets = tracker.get_all_committed_offsets();
    assert_eq!(final_offsets.get(&0), Some(&3));
    assert_eq!(final_offsets.get(&1), Some(&2));
    assert_eq!(final_offsets.get(&2), Some(&1));
    
    // Test clearing pending offsets (used during seek)
    tracker.add_pending_offset(0, 10);
    tracker.add_pending_offset(0, 11);
    tracker.clear_pending_offsets(0);
    
    // Should be able to add new pending offsets after clear
    // Add consecutive offset 4 (next after current committed offset 3)
    tracker.add_pending_offset(0, 4);
    tracker.remove_pending_offset(0, 4);
    tracker.update_consecutive_committed(0);
    assert_eq!(tracker.get_committed_offset(0), 4); // Advanced from 3 to 4
    
    // Test that seeking behavior works correctly - clear and set new committed offset
    tracker.clear_pending_offsets(0);
    tracker.set_committed_offset(0, 10); // Simulate seeking to offset 10
    assert_eq!(tracker.get_committed_offset(0), 10);
}