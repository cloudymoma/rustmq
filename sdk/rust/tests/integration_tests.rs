use rustmq_client::*;
use tokio_test;
use tempfile::TempDir;

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

    let deserialized: Result<ClientConfig, _> = serde_json::from_str(&json.unwrap());
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
async fn test_consumer_lag_calculation() {
    let lag = ConsumerLag::calculate_lag(100, 150);
    assert_eq!(lag, 50);

    let lag_zero = ConsumerLag::calculate_lag(100, 100);
    assert_eq!(lag_zero, 0);

    let lag_underflow = ConsumerLag::calculate_lag(150, 100);
    assert_eq!(lag_underflow, 0); // saturating_sub prevents underflow
}