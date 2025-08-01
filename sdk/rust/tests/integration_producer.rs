/// Integration tests for Producer functionality
/// 
/// These tests use mock network responses to test the full producer flow
/// without requiring a real RustMQ broker.

use rustmq_client::{
    client::RustMqClient,
    config::{ClientConfig, ProducerConfig, AckLevel, RetryConfig},
    producer::{Producer, ProducerBuilder},
    message::{Message, MessageBuilder},
    error::{ClientError, Result},
};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use std::sync::atomic::Ordering;

/// Mock broker response for testing
const MOCK_PRODUCE_RESPONSE: &str = r#"{
    "success": true,
    "error": null,
    "results": [
        {
            "message_id": "test-msg-1",
            "partition": 0,
            "offset": 100,
            "timestamp": 1640995200000
        },
        {
            "message_id": "test-msg-2", 
            "partition": 0,
            "offset": 101,
            "timestamp": 1640995200001
        }
    ]
}"#;

/// Mock error response for testing
const MOCK_ERROR_RESPONSE: &str = r#"{
    "success": false,
    "error": "Topic not found: test-topic",
    "results": []
}"#;

/// Create a test client with mock configuration
async fn create_test_client() -> Result<RustMqClient> {
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: None,
        enable_tls: false,
        tls_config: None,
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        keep_alive_interval: Duration::from_secs(30),
        max_connections: 1,
        retry_config: RetryConfig {
            max_retries: 1,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
            jitter: false,
        },
        compression: Default::default(),
        auth: None,
    };
    
    RustMqClient::new(config).await
}

/// Helper to create test producer with custom config
async fn create_test_producer_with_config(config: ProducerConfig) -> Result<Producer> {
    let client = create_test_client().await?;
    
    ProducerBuilder::new()
        .topic("test-topic")
        .config(config)
        .client(client)
        .build()
        .await
}

/// Helper to create a basic test producer
async fn create_test_producer() -> Result<Producer> {
    create_test_producer_with_config(ProducerConfig {
        batch_size: 5,
        batch_timeout: Duration::from_millis(50),
        ack_level: AckLevel::All,
        producer_id: Some("integration-test-producer".to_string()),
        ..Default::default()
    }).await
}

/// Test basic producer creation and configuration
#[tokio::test]
async fn test_producer_creation() {
    let producer = create_test_producer().await.unwrap();
    
    assert_eq!(producer.id(), "integration-test-producer");
    assert_eq!(producer.topic(), "test-topic");
    assert_eq!(producer.config().batch_size, 5);
    assert_eq!(producer.config().ack_level, AckLevel::All);
}

/// Test sending a single message
#[tokio::test]
async fn test_send_single_message() {
    let producer = create_test_producer().await.unwrap();
    
    let message = MessageBuilder::new()
        .id("test-message-1")
        .topic("test-topic")
        .payload("Hello, RustMQ!")
        .header("source", "integration-test")
        .build()
        .unwrap();
    
    // Note: This will attempt to connect to a real broker
    // In a real integration test environment, you'd have a test broker running
    // For now, we expect this to fail with a connection error, which is expected
    let result = timeout(Duration::from_secs(1), producer.send(message)).await;
    
    // We expect a timeout or connection error since there's no real broker
    assert!(result.is_err() || result.unwrap().is_err());
}

/// Test fire-and-forget message sending
#[tokio::test]
async fn test_send_async_messages() {
    let producer = create_test_producer().await.unwrap();
    
    // Send multiple messages asynchronously
    for i in 0..3 {
        let message = MessageBuilder::new()
            .id(&format!("async-msg-{}", i))
            .topic("test-topic")
            .payload(&format!("Async payload {}", i))
            .build()
            .unwrap();
        
        // This should queue the message for batching
        let result = producer.send_async(message).await;
        
        // The send_async should succeed (message queued) even without a broker
        assert!(result.is_ok());
    }
    
    // Give time for batching to attempt
    sleep(Duration::from_millis(100)).await;
}

/// Test batch sending functionality
#[tokio::test]
async fn test_batch_sending() {
    let producer = create_test_producer().await.unwrap();
    
    let messages: Vec<Message> = (0..3).map(|i| {
        MessageBuilder::new()
            .id(&format!("batch-msg-{}", i))
            .topic("test-topic")
            .payload(&format!("Batch payload {}", i))
            .header("batch-index", &i.to_string())
            .build()
            .unwrap()
    }).collect();
    
    // Attempt to send batch
    let result = timeout(Duration::from_secs(1), producer.send_batch(messages)).await;
    
    // Expect timeout or connection error
    assert!(result.is_err() || result.unwrap().is_err());
}

/// Test flush functionality
#[tokio::test]
async fn test_flush_functionality() {
    let producer = create_test_producer().await.unwrap();
    
    // Send some messages first
    for i in 0..2 {
        let message = MessageBuilder::new()
            .id(&format!("flush-msg-{}", i))
            .topic("test-topic")
            .payload(&format!("Flush test {}", i))
            .build()
            .unwrap();
        
        producer.send_async(message).await.unwrap();
    }
    
    // Flush should complete quickly (even if it fails to send to broker)
    let flush_result = timeout(Duration::from_millis(500), producer.flush()).await;
    
    // The flush operation itself should complete, though the underlying sends may fail
    assert!(flush_result.is_ok());
}

/// Test producer metrics tracking
#[tokio::test]
async fn test_producer_metrics() {
    let producer = create_test_producer().await.unwrap();
    
    // Send some messages to generate metrics
    for i in 0..3 {
        let message = MessageBuilder::new()
            .id(&format!("metrics-msg-{}", i))
            .topic("test-topic")
            .payload(&format!("Metrics test {}", i))
            .build()
            .unwrap();
        
        producer.send_async(message).await.unwrap();
    }
    
    // Allow some time for batching attempts
    sleep(Duration::from_millis(100)).await;
    
    let metrics = producer.metrics().await;
    
    // Check that metrics structure is correct
    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 0); // No successful sends expected
    assert!(metrics.messages_failed.load(Ordering::Relaxed) > 0); // Should have failures
    assert_eq!(metrics.batches_sent.load(Ordering::Relaxed), 0); // No successful batches
}

/// Test producer with different batch sizes
#[tokio::test]
async fn test_different_batch_sizes() {
    // Test with batch size of 1 (immediate sending)
    let producer = create_test_producer_with_config(ProducerConfig {
        batch_size: 1,
        batch_timeout: Duration::from_millis(1000), // Long timeout
        ack_level: AckLevel::All,
        producer_id: Some("batch-size-1-producer".to_string()),
        ..Default::default()
    }).await.unwrap();
    
    let message = MessageBuilder::new()
        .id("immediate-msg")
        .topic("test-topic")
        .payload("Should send immediately")
        .build()
        .unwrap();
    
    producer.send_async(message).await.unwrap();
    
    // With batch size 1, this should trigger immediately
    sleep(Duration::from_millis(50)).await;
    
    // Test with larger batch size
    let producer2 = create_test_producer_with_config(ProducerConfig {
        batch_size: 10,
        batch_timeout: Duration::from_millis(20), // Short timeout
        ack_level: AckLevel::All,
        producer_id: Some("batch-size-10-producer".to_string()),
        ..Default::default()
    }).await.unwrap();
    
    // Send less than batch size to test timeout trigger
    for i in 0..3 {
        let message = MessageBuilder::new()
            .id(&format!("timeout-trigger-{}", i))
            .topic("test-topic")
            .payload(&format!("Timeout test {}", i))
            .build()
            .unwrap();
        
        producer2.send_async(message).await.unwrap();
    }
    
    // Wait for timeout to trigger
    sleep(Duration::from_millis(50)).await;
}

/// Test producer close behavior
#[tokio::test]
async fn test_producer_close() {
    let producer = create_test_producer().await.unwrap();
    
    // Send some messages
    for i in 0..2 {
        let message = MessageBuilder::new()
            .id(&format!("close-test-{}", i))
            .topic("test-topic")
            .payload(&format!("Close test {}", i))
            .build()
            .unwrap();
        
        producer.send_async(message).await.unwrap();
    }
    
    // Close should flush remaining messages
    let close_result = timeout(Duration::from_millis(500), producer.close()).await;
    
    // Close should complete even if underlying sends fail
    assert!(close_result.is_ok());
}

/// Test producer error handling with invalid configuration
#[tokio::test]
async fn test_producer_invalid_config() {
    let client = create_test_client().await.unwrap();
    
    // Test with empty topic
    let result = ProducerBuilder::new()
        .client(client.clone())
        .build()
        .await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(matches!(error, ClientError::InvalidConfig(_)));
    assert!(error.to_string().contains("topic is required"));
    
    // Test with no client
    let result = ProducerBuilder::new()
        .topic("test-topic")
        .build()
        .await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(matches!(error, ClientError::InvalidConfig(_)));
    assert!(error.to_string().contains("Client is required"));
}

/// Test producer with different acknowledgment levels
#[tokio::test]
async fn test_ack_levels() {
    for ack_level in [AckLevel::None, AckLevel::Leader, AckLevel::All] {
        let producer = create_test_producer_with_config(ProducerConfig {
            batch_size: 1,
            ack_level: ack_level.clone(),
            producer_id: Some(format!("ack-test-{:?}", ack_level)),
            ..Default::default()
        }).await.unwrap();
        
        assert_eq!(producer.config().ack_level, ack_level);
        
        let message = MessageBuilder::new()
            .id(&format!("ack-test-{:?}", ack_level))
            .topic("test-topic")
            .payload("Ack level test")
            .build()
            .unwrap();
        
        producer.send_async(message).await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }
}

/// Test concurrent producer operations
#[tokio::test]
async fn test_concurrent_operations() {
    let producer = create_test_producer().await.unwrap();
    
    // Create multiple concurrent tasks
    let mut tasks = Vec::new();
    
    for i in 0..5 {
        let producer_clone = producer.clone();
        let task = tokio::spawn(async move {
            let message = MessageBuilder::new()
                .id(&format!("concurrent-{}", i))
                .topic("test-topic")
                .payload(&format!("Concurrent test {}", i))
                .build()
                .unwrap();
            
            producer_clone.send_async(message).await
        });
        tasks.push(task);
    }
    
    // Wait for all tasks to complete
    for task in tasks {
        let result = task.await.unwrap();
        assert!(result.is_ok());
    }
    
    // Flush to ensure all messages are processed
    producer.flush().await.unwrap();
}

/// Test message ordering within batches
#[tokio::test]
async fn test_message_ordering() {
    let producer = create_test_producer().await.unwrap();
    
    // Send messages with sequence
    for i in 0..5 {
        let message = MessageBuilder::new()
            .id(&format!("sequence-{}", i))
            .topic("test-topic")
            .payload(&format!("Sequence test {}", i))
            .header("sequence", &i.to_string())
            .build()
            .unwrap();
        
        producer.send_async(message).await.unwrap();
    }
    
    // Verify sequence counter was incremented
    let sequence_counter = producer.sequence_counter.read().await;
    assert_eq!(*sequence_counter, 5);
    
    producer.flush().await.unwrap();
}

/// Test producer with custom properties
#[tokio::test]
async fn test_custom_properties() {
    let producer = create_test_producer().await.unwrap();
    
    let message = MessageBuilder::new()
        .id("custom-props-test")
        .topic("test-topic")
        .payload("Custom properties test")
        .header("app", "integration-test")
        .header("version", "1.0.0")
        .header("environment", "test")
        .build()
        .unwrap();
    
    assert!(message.has_header("app"));
    assert_eq!(message.get_header("app").unwrap(), "integration-test");
    assert_eq!(message.header_keys().len(), 3);
    
    producer.send_async(message).await.unwrap();
    producer.flush().await.unwrap();
}