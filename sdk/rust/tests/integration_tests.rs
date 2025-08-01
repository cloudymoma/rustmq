use rustmq_client::{*, message::MessageBatch};
use std::time::Duration;

#[tokio::test]
async fn test_client_creation_and_connection() {
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("test-client".to_string()),
        ..Default::default()
    };

    // This should fail in tests since no broker is running
    let result = RustMqClient::new(config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_producer_creation() {
    let config = ClientConfig::default();
    let client = RustMqClient::new(config).await;
    
    if let Ok(client) = client {
        let producer_result = client.create_producer("test-topic").await;
        // May succeed or fail depending on broker availability
        println!("Producer creation result: {:?}", producer_result.is_ok());
    }
}

#[tokio::test]
async fn test_consumer_creation() {
    let config = ClientConfig::default();
    let client = RustMqClient::new(config).await;
    
    if let Ok(client) = client {
        let consumer_result = client.create_consumer("test-topic", "test-group").await;
        // May succeed or fail depending on broker availability
        println!("Consumer creation result: {:?}", consumer_result.is_ok());
    }
}

#[tokio::test]
async fn test_message_building() {
    let message = Message::builder()
        .topic("test-topic")
        .payload("Hello, World!")
        .header("key", "value")
        .build();

    assert!(message.is_ok());
    let msg = message.unwrap();
    assert_eq!(msg.topic, "test-topic");
    assert_eq!(msg.payload_as_string().unwrap(), "Hello, World!");
    assert_eq!(msg.get_header("key"), Some(&"value".to_string()));
}

#[tokio::test]
async fn test_message_batch_creation() {
    let messages = vec![
        Message::builder()
            .topic("test-topic")
            .payload("Message 1")
            .build().unwrap(),
        Message::builder()
            .topic("test-topic")
            .payload("Message 2")
            .build().unwrap(),
    ];

    let batch = MessageBatch::new(messages);
    assert_eq!(batch.len(), 2);
    assert!(!batch.is_empty());
    assert!(!batch.batch_id.is_empty());
}

#[tokio::test]
async fn test_config_serialization() {
    let config = ClientConfig::default();
    let json = serde_json::to_string(&config);
    assert!(json.is_ok());

    let deserialized: std::result::Result<ClientConfig, _> = serde_json::from_str(&json.unwrap());
    assert!(deserialized.is_ok());
}

#[tokio::test]
async fn test_error_categorization() {
    let connection_error = ClientError::Connection("test".to_string());
    assert_eq!(connection_error.category(), "connection");
    assert!(connection_error.is_retryable());

    let auth_error = ClientError::Authentication("test".to_string());
    assert_eq!(auth_error.category(), "authentication");
    assert!(!auth_error.is_retryable());
}

#[tokio::test]
async fn test_stream_config_defaults() {
    let config = StreamConfig::default();
    assert_eq!(config.input_topics.len(), 1);
    assert_eq!(config.consumer_group, "stream-processor");
    assert_eq!(config.parallelism, 1);
    assert!(config.output_topic.is_some());
}

#[tokio::test]
async fn test_topic_partition_equality() {
    let tp1 = TopicPartition::new("topic1".to_string(), 0);
    let tp2 = TopicPartition::new("topic1".to_string(), 0);
    let tp3 = TopicPartition::new("topic1".to_string(), 1);

    assert_eq!(tp1, tp2);
    assert_ne!(tp1, tp3);
}

#[tokio::test]
async fn test_consumer_configuration() {
    use rustmq_client::config::{ConsumerConfig, StartPosition};
    use std::time::Duration;
    
    let config = ConsumerConfig {
        consumer_id: Some("integration-test-consumer".to_string()),
        consumer_group: "integration-test-group".to_string(),
        auto_commit_interval: Duration::from_secs(10),
        enable_auto_commit: true,
        fetch_size: 50,
        fetch_timeout: Duration::from_millis(500),
        start_position: StartPosition::Earliest,
        max_retry_attempts: 2,
        dead_letter_queue: Some("test-dlq".to_string()),
    };
    
    assert_eq!(config.consumer_id.as_ref().unwrap(), "integration-test-consumer");
    assert_eq!(config.consumer_group, "integration-test-group");
    assert_eq!(config.fetch_size, 50);
    assert!(matches!(config.start_position, StartPosition::Earliest));
}

#[tokio::test]
async fn test_consumer_with_custom_config() {
    use rustmq_client::consumer::ConsumerBuilder;
    use rustmq_client::config::{ConsumerConfig, StartPosition};
    use std::time::Duration;
    
    let config = ClientConfig::default();
    
    // This test will fail without a broker, but validates the consumer creation flow
    if let Ok(client) = RustMqClient::new(config).await {
        let consumer_config = ConsumerConfig {
            consumer_id: Some("test-consumer-123".to_string()),
            fetch_size: 25,
            enable_auto_commit: false,
            start_position: StartPosition::Offset(100),
            max_retry_attempts: 5,
            dead_letter_queue: Some("test-failures".to_string()),
            ..Default::default()
        };
        
        let consumer_result = ConsumerBuilder::new()
            .topic("integration-test-topic")
            .consumer_group("integration-test-group")
            .config(consumer_config)
            .client(client)
            .build()
            .await;
            
        // May succeed or fail depending on broker availability
        match consumer_result {
            Ok(consumer) => {
                assert_eq!(consumer.id(), "test-consumer-123");
                assert_eq!(consumer.topic(), "integration-test-topic");
                assert_eq!(consumer.consumer_group(), "integration-test-group");
                println!("Consumer created successfully in integration test");
            }
            Err(e) => {
                println!("Consumer creation failed (expected without broker): {}", e);
            }
        }
    }
}

#[tokio::test]
async fn test_consumer_stream_interface() {
    use futures::StreamExt;
    
    let config = ClientConfig::default();
    
    if let Ok(client) = RustMqClient::new(config).await {
        if let Ok(consumer) = client.create_consumer("stream-test-topic", "stream-test-group").await {
            let mut stream = consumer.stream();
            
            // Test that stream can be created (won't receive messages without broker)
            // Use timeout to avoid hanging
            let timeout_result = tokio::time::timeout(
                Duration::from_millis(100),
                stream.next()
            ).await;
            
            match timeout_result {
                Ok(Some(_)) => println!("Received message from stream"),
                Ok(None) => println!("Stream ended"),
                Err(_) => println!("Stream timeout (expected without broker)"),
            }
        }
    }
}

#[tokio::test]
async fn test_consumer_metrics_integration() {
    let config = ClientConfig::default();
    
    if let Ok(client) = RustMqClient::new(config).await {
        if let Ok(consumer) = client.create_consumer("metrics-test-topic", "metrics-test-group").await {
            let metrics = consumer.metrics().await;
            
            // Check initial metrics state
            assert_eq!(metrics.messages_received.load(std::sync::atomic::Ordering::Relaxed), 0);
            assert_eq!(metrics.messages_processed.load(std::sync::atomic::Ordering::Relaxed), 0);
            assert_eq!(metrics.messages_failed.load(std::sync::atomic::Ordering::Relaxed), 0);
            assert_eq!(metrics.bytes_received.load(std::sync::atomic::Ordering::Relaxed), 0);
            
            println!("Consumer metrics validated");
        }
    }
}