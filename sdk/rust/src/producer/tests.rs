use super::*;
use crate::{
    config::{ClientConfig, ProducerConfig, RetryConfig, TlsConfig, AckLevel},
    message::MessageBuilder,
    client::RustMqClient,
};
use tokio::time::{sleep, Duration};
use std::sync::atomic::Ordering;

/// Mock client for testing
#[derive(Clone)]
struct MockClient {
    should_fail: Arc<std::sync::atomic::AtomicBool>,
    request_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl MockClient {
    fn new() -> Self {
        Self {
            should_fail: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            request_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
    
    fn set_should_fail(&self, should_fail: bool) {
        self.should_fail.store(should_fail, Ordering::Relaxed);
    }
    
    fn request_count(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }
}

/// Helper function to create a test producer
async fn create_test_producer() -> Result<Producer> {
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: None,
        enable_tls: false,
        tls_config: None,
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        keep_alive_interval: Duration::from_secs(30),
        max_connections: 10,
        retry_config: RetryConfig::default(),
        compression: Default::default(),
        auth: None,
    };
    
    let client = RustMqClient::new(config).await?;
    
    ProducerBuilder::new()
        .topic("test-topic")
        .config(ProducerConfig {
            batch_size: 3,
            batch_timeout: Duration::from_millis(100),
            ack_level: AckLevel::All,
            producer_id: Some("test-producer".to_string()),
            ..Default::default()
        })
        .client(client)
        .build()
        .await
}

/// Helper function to create a test message
fn create_test_message(id: &str, payload: &str) -> Message {
    MessageBuilder::new()
        .id(id.to_string())
        .topic("test-topic")
        .payload(payload.as_bytes().to_vec())
        .header("test-header", "test-value")
        .build()
        .unwrap()
}

#[tokio::test]
async fn test_producer_creation() {
    let producer = create_test_producer().await.unwrap();
    
    assert_eq!(producer.id(), "test-producer");
    assert_eq!(producer.topic(), "test-topic");
    assert_eq!(producer.config().batch_size, 3);
    assert_eq!(producer.config().ack_level, AckLevel::All);
}

#[tokio::test]
async fn test_producer_builder() {
    // Test builder with all options
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: None,
        enable_tls: false,
        tls_config: None,
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        keep_alive_interval: Duration::from_secs(30),
        max_connections: 10,
        retry_config: RetryConfig::default(),
        compression: Default::default(),
        auth: None,
    };
    
    let client = RustMqClient::new(config).await.unwrap();
    
    let producer_config = ProducerConfig {
        batch_size: 5,
        batch_timeout: Duration::from_millis(200),
        ack_level: AckLevel::Leader,
        producer_id: Some("custom-producer".to_string()),
        ..Default::default()
    };
    
    let producer = ProducerBuilder::new()
        .topic("custom-topic")
        .config(producer_config)
        .client(client)
        .build()
        .await
        .unwrap();
    
    assert_eq!(producer.topic(), "custom-topic");
    assert_eq!(producer.id(), "custom-producer");
    assert_eq!(producer.config().batch_size, 5);
    assert_eq!(producer.config().ack_level, AckLevel::Leader);
}

#[tokio::test]
async fn test_producer_builder_missing_topic() {
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: None,
        enable_tls: false,
        tls_config: None,
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        keep_alive_interval: Duration::from_secs(30),
        max_connections: 10,
        retry_config: RetryConfig::default(),
        compression: Default::default(),
        auth: None,
    };
    
    let client = RustMqClient::new(config).await.unwrap();
    
    let result = ProducerBuilder::new()
        .client(client)
        .build()
        .await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("topic is required"));
}

#[tokio::test]
async fn test_producer_builder_missing_client() {
    let result = ProducerBuilder::new()
        .topic("test-topic")
        .build()
        .await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Client is required"));
}

#[tokio::test]
async fn test_message_sequence_assignment() {
    let producer = create_test_producer().await.unwrap();
    
    // Send multiple messages and verify sequence numbers are assigned
    let messages = vec![
        create_test_message("msg1", "payload1"),
        create_test_message("msg2", "payload2"),
        create_test_message("msg3", "payload3"),
    ];
    
    // Send messages asynchronously (fire-and-forget)
    for message in messages {
        producer.send_async(message).await.unwrap();
    }
    
    // Give time for processing
    sleep(Duration::from_millis(200)).await;
    
    // Check that sequence counter was incremented
    let sequence_counter = producer.sequence_counter.read().await;
    assert_eq!(*sequence_counter, 3);
}

#[tokio::test]
async fn test_producer_metrics() {
    let producer = create_test_producer().await.unwrap();
    
    // Send some messages
    for i in 0..5 {
        let message = create_test_message(&format!("msg{}", i), &format!("payload{}", i));
        producer.send_async(message).await.unwrap();
    }
    
    // Wait for batching to complete
    sleep(Duration::from_millis(200)).await;
    
    let metrics = producer.metrics().await;
    
    // Check metrics were updated
    assert!(metrics.messages_sent.load(Ordering::Relaxed) > 0);
    assert!(metrics.batches_sent.load(Ordering::Relaxed) > 0);
    
    let avg_batch_size = *metrics.average_batch_size.read().await;
    assert!(avg_batch_size > 0.0);
    
    let last_send_time = *metrics.last_send_time.read().await;
    assert!(last_send_time.is_some());
}

#[tokio::test]
async fn test_batch_size_trigger() {
    let producer = create_test_producer().await.unwrap();
    
    // The producer is configured with batch_size = 3
    // Send exactly 3 messages to trigger batching
    let messages = vec![
        create_test_message("msg1", "payload1"),
        create_test_message("msg2", "payload2"),
        create_test_message("msg3", "payload3"),
    ];
    
    for message in messages {
        producer.send_async(message).await.unwrap();
    }
    
    // Give a short time for batch processing
    sleep(Duration::from_millis(50)).await;
    
    let metrics = producer.metrics().await;
    assert_eq!(metrics.batches_sent.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn test_batch_timeout_trigger() {
    let producer = create_test_producer().await.unwrap();
    
    // Send fewer messages than batch size
    let message = create_test_message("msg1", "payload1");
    producer.send_async(message).await.unwrap();
    
    // Wait for batch timeout to trigger (100ms + some buffer)
    sleep(Duration::from_millis(150)).await;
    
    let metrics = producer.metrics().await;
    assert_eq!(metrics.batches_sent.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_flush_functionality() {
    let producer = create_test_producer().await.unwrap();
    
    // Send a message but don't wait for timeout
    let message = create_test_message("msg1", "payload1");
    producer.send_async(message).await.unwrap();
    
    // Flush should force immediate sending
    producer.flush().await.unwrap();
    
    let metrics = producer.metrics().await;
    assert_eq!(metrics.batches_sent.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_flush_empty_queue() {
    let producer = create_test_producer().await.unwrap();
    
    // Flush with no pending messages should succeed
    producer.flush().await.unwrap();
    
    let metrics = producer.metrics().await;
    assert_eq!(metrics.batches_sent.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_send_batch() {
    let producer = create_test_producer().await.unwrap();
    
    let messages = vec![
        create_test_message("msg1", "payload1"),
        create_test_message("msg2", "payload2"),
        create_test_message("msg3", "payload3"),
    ];
    
    let results = producer.send_batch(messages).await.unwrap();
    
    assert_eq!(results.len(), 3);
    for result in results {
        assert_eq!(result.topic, "test-topic");
        assert!(result.message_id.starts_with("msg"));
    }
}

#[tokio::test]
async fn test_producer_close() {
    let producer = create_test_producer().await.unwrap();
    
    // Send some messages
    let message = create_test_message("msg1", "payload1");
    producer.send_async(message).await.unwrap();
    
    // Close should flush remaining messages
    producer.close().await.unwrap();
    
    let metrics = producer.metrics().await;
    assert_eq!(metrics.batches_sent.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_producer_id_assignment() {
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: None,
        enable_tls: false,
        tls_config: None,
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        keep_alive_interval: Duration::from_secs(30),
        max_connections: 10,
        retry_config: RetryConfig::default(),
        compression: Default::default(),
        auth: None,
    };
    
    let client = RustMqClient::new(config).await.unwrap();
    
    // Test with custom producer ID
    let producer1 = ProducerBuilder::new()
        .topic("test-topic")
        .config(ProducerConfig {
            producer_id: Some("custom-id".to_string()),
            ..Default::default()
        })
        .client(client.clone())
        .build()
        .await
        .unwrap();
    
    assert_eq!(producer1.id(), "custom-id");
    
    // Test with auto-generated producer ID
    let producer2 = ProducerBuilder::new()
        .topic("test-topic")
        .client(client)
        .build()
        .await
        .unwrap();
    
    assert!(producer2.id().starts_with("producer-"));
    assert_ne!(producer2.id(), producer1.id());
}

#[tokio::test]
async fn test_message_headers_and_metadata() {
    let producer = create_test_producer().await.unwrap();
    
    let message = MessageBuilder::new()
        .id("test-msg")
        .topic("test-topic")
        .payload("test payload")
        .header("key1", "value1")
        .header("key2", "value2")
        .build()
        .unwrap();
    
    let result = producer.send(message).await.unwrap();
    
    assert_eq!(result.message_id, "test-msg");
    assert_eq!(result.topic, "test-topic");
    assert!(result.timestamp > 0);
}

#[tokio::test]
async fn test_concurrent_sends() {
    let producer = create_test_producer().await.unwrap();
    
    // Send messages concurrently
    let mut tasks = Vec::new();
    
    for i in 0..10 {
        let producer_clone = producer.clone();
        let task = tokio::spawn(async move {
            let message = create_test_message(&format!("msg{}", i), &format!("payload{}", i));
            producer_clone.send_async(message).await
        });
        tasks.push(task);
    }
    
    // Wait for all tasks to complete
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    
    // Wait for batching to complete
    sleep(Duration::from_millis(200)).await;
    
    let metrics = producer.metrics().await;
    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 10);
    
    // Should have used multiple batches due to batch size of 3
    assert!(metrics.batches_sent.load(Ordering::Relaxed) >= 3);
}

#[tokio::test]
async fn test_producer_config_defaults() {
    let config = ProducerConfig::default();
    
    assert_eq!(config.batch_size, 100);
    assert_eq!(config.batch_timeout, Duration::from_millis(10));
    assert_eq!(config.ack_level, AckLevel::All);
    assert!(config.producer_id.is_none());
}

#[tokio::test]
async fn test_message_builder_validation() {
    // Test missing payload
    let result = MessageBuilder::new()
        .topic("test-topic")
        .build();
    
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("payload is required"));
    
    // Test missing topic
    let result = MessageBuilder::new()
        .payload("test payload")
        .build();
    
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("topic is required"));
    
    // Test valid message
    let result = MessageBuilder::new()
        .topic("test-topic")
        .payload("test payload")
        .build();
    
    assert!(result.is_ok());
    let message = result.unwrap();
    assert_eq!(message.topic, "test-topic");
    assert_eq!(message.payload, "test payload".as_bytes());
    assert!(message.timestamp > 0);
    assert!(!message.id.is_empty());
}

#[tokio::test]
async fn test_error_handling_metrics() {
    let producer = create_test_producer().await.unwrap();
    
    // This test would need a way to simulate broker errors
    // For now, we just verify the error metrics exist
    let metrics = producer.metrics().await;
    assert_eq!(metrics.messages_failed.load(Ordering::Relaxed), 0);
}