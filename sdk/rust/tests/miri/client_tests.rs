//! Miri memory safety tests for RustMQ SDK client components
//! 
//! These tests focus on detecting memory safety issues in:
//! - Client connection management
//! - Message serialization/deserialization
//! - Producer operations
//! - Consumer operations
//! - Security handling

#[cfg(miri)]
mod miri_client_tests {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::runtime::Runtime;
    
    use rustmq_sdk::{
        client::RustMQClient,
        producer::Producer,
        consumer::Consumer,
        message::Message,
        types::{Topic, Partition, Offset},
        error::RustMQError,
    };

    #[test]
    fn test_message_memory_safety() {
        // Test message creation and manipulation
        let topic = Topic::new("test.topic".to_string());
        let payload = b"test message payload for memory safety validation";
        
        let message = Message::new(topic.clone(), payload.to_vec());
        
        // Test basic accessors
        assert_eq!(message.topic(), &topic);
        assert_eq!(message.payload(), payload);
        
        // Test cloning
        let cloned_message = message.clone();
        assert_eq!(message.topic(), cloned_message.topic());
        assert_eq!(message.payload(), cloned_message.payload());
        
        // Test that cloned message has independent memory
        drop(message);
        assert_eq!(cloned_message.payload(), payload);
    }

    #[test]
    fn test_message_serialization_memory_safety() {
        let topic = Topic::new("serialization.test".to_string());
        
        // Test with various payload sizes
        let test_cases = vec![
            vec![],                           // Empty payload
            vec![0x42],                       // Single byte
            vec![0x00, 0xFF, 0x80, 0x7F],    // Edge bytes
            (0..256).map(|i| i as u8).collect(), // Pattern
            vec![0xAAu8; 4096],               // Large payload
        ];
        
        for payload in test_cases {
            let original = Message::new(topic.clone(), payload.clone());
            
            // Test serialization roundtrip
            let serialized = original.serialize().unwrap();
            let deserialized = Message::deserialize(&serialized).unwrap();
            
            assert_eq!(original.topic(), deserialized.topic());
            assert_eq!(original.payload(), deserialized.payload());
            assert_eq!(payload, deserialized.payload());
        }
    }

    #[test]
    fn test_client_connection_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Note: This test doesn't actually connect to avoid external dependencies in Miri
            // It focuses on memory safety of client initialization
            
            let client_result = RustMQClient::new("localhost:9092".to_string()).await;
            
            // In a real scenario this might fail due to no server, but we're testing memory safety
            // The important part is that creation/destruction doesn't cause memory issues
            match client_result {
                Ok(client) => {
                    // Test basic client operations that don't require network
                    let topic = Topic::new("test.topic".to_string());
                    
                    // Test client cloning/sharing
                    let client_arc = Arc::new(client);
                    let client_clone = Arc::clone(&client_arc);
                    
                    // Verify both references point to the same client
                    assert_eq!(
                        Arc::as_ptr(&client_arc) as *const _,
                        Arc::as_ptr(&client_clone) as *const _
                    );
                }
                Err(_) => {
                    // Expected when no server is running, this is fine for memory safety testing
                }
            }
        });
    }

    #[test]
    fn test_producer_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Test producer creation without actual network operations
            let config = rustmq_sdk::config::ProducerConfig::default();
            
            // Create producer (may fail without server, but tests memory safety)
            let producer_result = Producer::new("localhost:9092".to_string(), config).await;
            
            match producer_result {
                Ok(producer) => {
                    let topic = Topic::new("producer.test".to_string());
                    let payload = b"producer test message";
                    
                    // Test message preparation (without sending)
                    let message = Message::new(topic, payload.to_vec());
                    
                    // Test producer sharing
                    let producer_arc = Arc::new(producer);
                    let producer_clone = Arc::clone(&producer_arc);
                    
                    // Memory safety check - both should be valid
                    drop(producer_clone);
                    // producer_arc should still be valid
                }
                Err(_) => {
                    // Expected without server, memory safety is still tested
                }
            }
        });
    }

    #[test]
    fn test_consumer_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Test consumer creation without actual network operations
            let config = rustmq_sdk::config::ConsumerConfig::default();
            
            let consumer_result = Consumer::new(
                "localhost:9092".to_string(),
                "test_group".to_string(),
                config
            ).await;
            
            match consumer_result {
                Ok(consumer) => {
                    // Test consumer sharing
                    let consumer_arc = Arc::new(consumer);
                    let consumer_clone = Arc::clone(&consumer_arc);
                    
                    // Test subscription preparation (without actual network call)
                    let topics = vec![Topic::new("test.topic".to_string())];
                    
                    // Memory safety verification - objects should remain valid
                    drop(consumer_clone);
                    // consumer_arc should still be accessible
                }
                Err(_) => {
                    // Expected without server
                }
            }
        });
    }

    #[test]
    fn test_concurrent_message_operations() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let topic = Topic::new("concurrent.test".to_string());
            
            // Test concurrent message creation and manipulation
            let mut handles = Vec::new();
            
            for i in 0..10 {
                let topic_clone = topic.clone();
                let handle = tokio::spawn(async move {
                    let payload = format!("concurrent message {}", i).into_bytes();
                    let message = Message::new(topic_clone, payload.clone());
                    
                    // Test serialization in concurrent context
                    let serialized = message.serialize().unwrap();
                    let deserialized = Message::deserialize(&serialized).unwrap();
                    
                    assert_eq!(message.payload(), deserialized.payload());
                    assert_eq!(payload, deserialized.payload());
                    
                    i
                });
                handles.push(handle);
            }
            
            let results: Vec<_> = futures::future::join_all(handles)
                .await
                .into_iter()
                .map(|r| r.unwrap())
                .collect();
            
            assert_eq!(results.len(), 10);
            for (expected, actual) in (0..10).zip(results.iter()) {
                assert_eq!(expected, *actual);
            }
        });
    }

    #[test]
    fn test_topic_partition_memory_safety() {
        // Test Topic and Partition types
        let topic_name = "memory.safety.test";
        let topic = Topic::new(topic_name.to_string());
        
        assert_eq!(topic.as_str(), topic_name);
        
        // Test cloning
        let cloned_topic = topic.clone();
        assert_eq!(topic.as_str(), cloned_topic.as_str());
        
        // Verify independent memory
        drop(topic);
        assert_eq!(cloned_topic.as_str(), topic_name);
        
        // Test Partition
        let partition = Partition::new(42);
        assert_eq!(partition.id(), 42);
        
        let cloned_partition = partition.clone();
        assert_eq!(partition.id(), cloned_partition.id());
        
        // Test Offset
        let offset = Offset::new(12345);
        assert_eq!(offset.value(), 12345);
        
        let cloned_offset = offset.clone();
        assert_eq!(offset.value(), cloned_offset.value());
    }

    #[test]
    fn test_error_handling_memory_safety() {
        // Test error creation and handling
        let connection_error = RustMQError::ConnectionError("test connection error".to_string());
        let serialization_error = RustMQError::SerializationError("test serialization error".to_string());
        
        // Test error cloning
        let cloned_connection_error = connection_error.clone();
        let cloned_serialization_error = serialization_error.clone();
        
        // Test that cloned errors are independent
        drop(connection_error);
        drop(serialization_error);
        
        // Cloned errors should still be accessible
        match cloned_connection_error {
            RustMQError::ConnectionError(msg) => {
                assert_eq!(msg, "test connection error");
            }
            _ => panic!("Wrong error type"),
        }
        
        match cloned_serialization_error {
            RustMQError::SerializationError(msg) => {
                assert_eq!(msg, "test serialization error");
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_config_memory_safety() {
        // Test configuration objects
        let mut producer_config = rustmq_sdk::config::ProducerConfig::default();
        producer_config.set_batch_size(1000);
        producer_config.set_timeout(Duration::from_secs(30));
        
        // Test cloning
        let cloned_config = producer_config.clone();
        assert_eq!(producer_config.batch_size(), cloned_config.batch_size());
        assert_eq!(producer_config.timeout(), cloned_config.timeout());
        
        // Test consumer config
        let mut consumer_config = rustmq_sdk::config::ConsumerConfig::default();
        consumer_config.set_max_poll_records(500);
        consumer_config.set_session_timeout(Duration::from_secs(60));
        
        let cloned_consumer_config = consumer_config.clone();
        assert_eq!(consumer_config.max_poll_records(), cloned_consumer_config.max_poll_records());
        assert_eq!(consumer_config.session_timeout(), cloned_consumer_config.session_timeout());
        
        // Test that modifications to original don't affect clone
        producer_config.set_batch_size(2000);
        assert_ne!(producer_config.batch_size(), cloned_config.batch_size());
        assert_eq!(cloned_config.batch_size(), 1000);
    }

    #[test]
    fn test_message_batch_operations() {
        // Test batch message operations for memory safety
        let topic = Topic::new("batch.test".to_string());
        let mut messages = Vec::new();
        
        // Create a batch of messages
        for i in 0..100 {
            let payload = format!("batch message {}", i).into_bytes();
            let message = Message::new(topic.clone(), payload);
            messages.push(message);
        }
        
        // Test batch serialization
        let mut serialized_batch = Vec::new();
        for message in &messages {
            let serialized = message.serialize().unwrap();
            serialized_batch.push(serialized);
        }
        
        // Test batch deserialization
        let mut deserialized_messages = Vec::new();
        for serialized in &serialized_batch {
            let message = Message::deserialize(serialized).unwrap();
            deserialized_messages.push(message);
        }
        
        // Verify all messages
        assert_eq!(messages.len(), deserialized_messages.len());
        for (original, deserialized) in messages.iter().zip(deserialized_messages.iter()) {
            assert_eq!(original.topic(), deserialized.topic());
            assert_eq!(original.payload(), deserialized.payload());
        }
        
        // Test memory cleanup
        drop(messages);
        drop(serialized_batch);
        
        // deserialized_messages should still be valid
        assert_eq!(deserialized_messages.len(), 100);
        assert_eq!(deserialized_messages[0].payload(), b"batch message 0");
        assert_eq!(deserialized_messages[99].payload(), b"batch message 99");
    }
}