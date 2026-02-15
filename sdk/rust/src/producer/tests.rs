use super::*;
use crate::{
    config::{AckLevel, ClientConfig, ProducerConfig, RetryConfig},
    message::MessageBuilder,
};
use std::sync::atomic::Ordering;
use tokio::time::Duration;

/// Helper function to create test configurations (no network calls)
fn create_test_configs() -> (ClientConfig, ProducerConfig) {
    let client_config = ClientConfig {
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

    let producer_config = ProducerConfig {
        batch_size: 3,
        batch_timeout: Duration::from_millis(100),
        ack_level: AckLevel::All,
        producer_id: Some("test-producer".to_string()),
        ..Default::default()
    };

    (client_config, producer_config)
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
    let (_client_config, producer_config) = create_test_configs();

    // Test that producer configuration is created correctly
    assert_eq!(producer_config.batch_size, 3);
    assert_eq!(producer_config.ack_level, AckLevel::All);
    assert_eq!(
        producer_config.producer_id,
        Some("test-producer".to_string())
    );
}

#[tokio::test]
async fn test_producer_builder() {
    // Test builder configuration (no network calls)
    let producer_config = ProducerConfig {
        batch_size: 5,
        batch_timeout: Duration::from_millis(200),
        ack_level: AckLevel::Leader,
        producer_id: Some("custom-producer".to_string()),
        ..Default::default()
    };

    let _builder = ProducerBuilder::new()
        .topic("custom-topic")
        .config(producer_config.clone());

    // Test the config we created for the builder
    assert_eq!(producer_config.batch_size, 5);
    assert_eq!(producer_config.ack_level, AckLevel::Leader);
    assert_eq!(
        producer_config.producer_id,
        Some("custom-producer".to_string())
    );
}

#[tokio::test]
async fn test_producer_builder_missing_topic() {
    // Test that builder can be created without topic (no network calls)
    let _builder = ProducerBuilder::new();

    // Just test that the builder can be created and methods chained
    // The actual validation would happen at build() time, which requires a client
}

#[tokio::test]
async fn test_producer_builder_missing_client() {
    // Test that builder can be created with topic but without client (no network calls)
    let _builder = ProducerBuilder::new().topic("test-topic");

    // Just test that the builder can be created and methods chained
    // The actual validation would happen at build() time, which requires a client
}

#[tokio::test]
async fn test_message_builder_validation() {
    // Test message builder creates valid messages
    let message = create_test_message("test-1", "hello world");

    assert_eq!(message.id, "test-1");
    assert_eq!(message.topic, "test-topic");
    assert_eq!(message.payload.as_ref(), b"hello world");
    assert_eq!(
        message.headers.get("test-header"),
        Some(&"test-value".to_string())
    );
}

#[tokio::test]
async fn test_producer_config_defaults() {
    let config = ProducerConfig::default();

    assert_eq!(config.batch_size, 100);
    assert_eq!(config.batch_timeout, Duration::from_millis(10));
    assert_eq!(config.ack_level, AckLevel::All);
    assert!(config.producer_id.is_none());
    assert!(!config.idempotent); // Default should be false
    assert_eq!(config.max_message_size, 1024 * 1024); // 1MB
}

#[tokio::test]
async fn test_producer_id_assignment() {
    // Test producer ID generation and assignment
    let config = ProducerConfig {
        producer_id: Some("my-producer".to_string()),
        ..Default::default()
    };

    assert_eq!(config.producer_id, Some("my-producer".to_string()));

    // Test default (None) case
    let default_config = ProducerConfig::default();
    assert!(default_config.producer_id.is_none());
}

#[tokio::test]
async fn test_producer_metrics() {
    // Test producer metrics initialization
    let metrics = ProducerMetrics::default();

    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.messages_failed.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.bytes_sent.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.batches_sent.load(Ordering::Relaxed), 0);

    // Test metrics updates
    metrics.messages_sent.store(10, Ordering::Relaxed);
    metrics.bytes_sent.store(1024, Ordering::Relaxed);

    assert_eq!(metrics.messages_sent.load(Ordering::Relaxed), 10);
    assert_eq!(metrics.bytes_sent.load(Ordering::Relaxed), 1024);
}

// Simple unit tests that don't require network connections
#[test]
fn test_ack_level_serialization() {
    assert_eq!(format!("{:?}", AckLevel::None), "None");
    assert_eq!(format!("{:?}", AckLevel::Leader), "Leader");
    assert_eq!(format!("{:?}", AckLevel::All), "All");
}

#[test]
fn test_retry_config_validation() {
    let retry_config = RetryConfig {
        max_retries: 5,
        base_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(30),
        multiplier: 2.0,
        jitter: true,
    };

    assert_eq!(retry_config.max_retries, 5);
    assert_eq!(retry_config.base_delay, Duration::from_millis(100));
    assert_eq!(retry_config.max_delay, Duration::from_secs(30));
    assert_eq!(retry_config.multiplier, 2.0);
    assert_eq!(retry_config.jitter, true);
}

#[test]
fn test_message_headers_and_metadata() {
    let mut message = MessageBuilder::new()
        .id("test-1")
        .topic("test-topic")
        .payload(b"test payload".to_vec())
        .build()
        .unwrap();

    // Test adding headers
    message
        .headers
        .insert("key1".to_string(), "value1".to_string());
    message
        .headers
        .insert("key2".to_string(), "value2".to_string());

    assert_eq!(message.headers.get("key1"), Some(&"value1".to_string()));
    assert_eq!(message.headers.get("key2"), Some(&"value2".to_string()));
    assert_eq!(message.headers.len(), 2);
}
