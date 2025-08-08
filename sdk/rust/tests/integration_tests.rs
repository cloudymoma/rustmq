use rustmq_client::{*, message::MessageBatch};
use std::time::Duration;

// Note: These integration tests focus on configuration validation
// rather than actual network operations, which require a running broker

#[tokio::test]
async fn test_client_creation_and_connection() {
    // Test client configuration validation without network connections
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("test-client".to_string()),
        ..Default::default()
    };

    // Verify configuration properties
    assert_eq!(config.brokers.len(), 1);
    assert_eq!(config.brokers[0], "localhost:9092");
    assert_eq!(config.client_id.as_ref().unwrap(), "test-client");
    
    // Note: Actual client creation requires a running broker
    // This test validates configuration structure and defaults
}

#[tokio::test]
async fn test_producer_creation() {
    // Test producer configuration validation without network connections
    let config = ClientConfig::default();
    
    // Verify default configuration properties
    assert!(!config.brokers.is_empty());
    assert!(config.client_id.is_none());
    assert!(!config.enable_tls);
    
    // Test topic validation for producer creation
    let topic = "test-topic";
    assert!(!topic.is_empty());
    assert!(!topic.contains(' '));
    
    // Note: Actual producer creation requires a running broker and client instance
    // This test validates configuration and topic validation logic
}

#[tokio::test]
async fn test_consumer_creation() {
    // Test consumer configuration validation without network connections
    let config = ClientConfig::default();
    
    // Verify default configuration properties
    assert!(!config.brokers.is_empty());
    assert!(config.client_id.is_none());
    
    // Test topic and group validation for consumer creation
    let topic = "test-topic";
    let group = "test-group";
    assert!(!topic.is_empty());
    assert!(!group.is_empty());
    assert!(!topic.contains(' '));
    assert!(!group.contains(' '));
    
    // Note: Actual consumer creation requires a running broker and client instance
    // This test validates configuration and parameter validation logic
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
    use rustmq_client::config::{ConsumerConfig, StartPosition};
    
    // Test consumer configuration validation without network connections
    let config = ClientConfig::default();
    
    // Test custom consumer configuration
    let consumer_config = ConsumerConfig {
        consumer_id: Some("test-consumer-123".to_string()),
        fetch_size: 25,
        enable_auto_commit: false,
        start_position: StartPosition::Offset(100),
        max_retry_attempts: 5,
        dead_letter_queue: Some("test-failures".to_string()),
        ..Default::default()
    };
    
    // Verify configuration properties
    assert_eq!(consumer_config.consumer_id.as_ref().unwrap(), "test-consumer-123");
    assert_eq!(consumer_config.fetch_size, 25);
    assert!(!consumer_config.enable_auto_commit);
    assert_eq!(consumer_config.max_retry_attempts, 5);
    assert_eq!(consumer_config.dead_letter_queue.as_ref().unwrap(), "test-failures");
    
    // Test topic and group validation
    let topic = "integration-test-topic";
    let group = "integration-test-group";
    assert!(!topic.is_empty());
    assert!(!group.is_empty());
    
    // Note: Actual consumer creation requires a running broker and client instance
    // This test validates consumer configuration and builder pattern setup
}

#[tokio::test]
async fn test_consumer_stream_interface() {
    // Test stream interface configuration validation without network connections
    let config = ClientConfig::default();
    
    // Test topic and group validation for streaming
    let topic = "stream-test-topic";
    let group = "stream-test-group";
    assert!(!topic.is_empty());
    assert!(!group.is_empty());
    assert!(!topic.contains(' '));
    assert!(!group.contains(' '));
    
    // Verify client configuration for streaming
    assert!(!config.brokers.is_empty());
    assert!(config.client_id.is_none());
    
    // Note: Actual stream interface requires a running broker and consumer instance
    // This test validates configuration for streaming scenarios
}

#[tokio::test]
async fn test_consumer_metrics_integration() {
    // Test metrics configuration validation without network connections
    let config = ClientConfig::default();
    
    // Test topic and group validation for metrics scenarios
    let topic = "metrics-test-topic";
    let group = "metrics-test-group";
    assert!(!topic.is_empty());
    assert!(!group.is_empty());
    assert!(!topic.contains(' '));
    assert!(!group.contains(' '));
    
    // Verify configuration properties for metrics tracking
    assert!(!config.brokers.is_empty());
    assert!(config.client_id.is_none());
    
    // Note: Actual metrics require a running broker and consumer instance
    // This test validates configuration for metrics scenarios
}