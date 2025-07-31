# RustMQ Rust SDK

High-performance async Rust client library for RustMQ message queue system with QUIC transport.

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Building](#building)
- [Configuration](#configuration)
- [Usage Guide](#usage-guide)
- [Performance Tuning](#performance-tuning)
- [Best Practices](#best-practices)
- [Testing](#testing)
- [Benchmarking](#benchmarking)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)
- [API Reference](#api-reference)
- [Contributing](#contributing)

## Overview

The RustMQ Rust SDK provides a native, high-performance client library for interacting with RustMQ message queues. Built with async/await and Tokio, it offers zero-copy operations, QUIC transport, and comprehensive streaming capabilities.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    RustMQ Rust SDK                          │
├─────────────────────────────────────────────────────────────┤
│  RustMqClient                                               │
│  ├── Producer (Batching, Compression)                       │
│  ├── Consumer (Streaming, Auto-commit)                      │
│  ├── MessageStream (Real-time Processing)                   │
│  └── Connection (QUIC Pool, Health Check)                   │
├─────────────────────────────────────────────────────────────┤
│  QUIC Transport Layer (quinn)                               │
├─────────────────────────────────────────────────────────────┤
│  RustMQ Broker Cluster                                      │
└─────────────────────────────────────────────────────────────┘
```

## Features

- **🚀 High Performance**: Built on Tokio with zero-copy operations
- **🔌 QUIC Transport**: Modern HTTP/3 protocol for reduced latency
- **🔄 Async/Await**: First-class async support with futures
- **📦 Message Batching**: Configurable batching for high throughput
- **🗜️ Compression**: Multiple compression algorithms (LZ4, Zstd, Gzip)
- **🔐 Security**: TLS/mTLS support with authentication
- **📊 Observability**: Built-in metrics and tracing integration
- **🎯 Type Safety**: Strong typing with comprehensive error handling
- **🌊 Streaming**: Real-time message processing pipelines
- **⚡ Zero-Copy**: Efficient memory usage with `Bytes`

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rustmq-client = { path = "../path/to/rustmq/sdk/rust" }
tokio = { version = "1.0", features = ["full"] }
```

Or if using from the RustMQ workspace:

```toml
[dependencies]
rustmq-client = { path = "sdk/rust" }
tokio = { version = "1.0", features = ["full"] }
```

### Feature Flags

```toml
[dependencies]
rustmq-client = { path = "sdk/rust", features = ["io-uring", "compression"] }
```

Available features:
- `io-uring` - High-performance async I/O on Linux
- `wasm` - WebAssembly ETL processing support (default)
- `compression` - Enable message compression
- `encryption` - Enable message encryption
- `metrics` - Enable detailed metrics collection

## Quick Start

### Basic Producer

```rust
use rustmq_client::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Create client
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("my-producer".to_string()),
        ..Default::default()
    };
    
    let client = RustMqClient::new(config).await?;
    
    // Create producer
    let producer = client.create_producer("my-topic").await?;
    
    // Send message
    let message = Message::builder()
        .topic("my-topic")
        .payload("Hello, RustMQ!")
        .header("sender", "rust-client")
        .build()?;
    
    let result = producer.send(message).await?;
    println!("Message sent: offset={}, partition={}", result.offset, result.partition);
    
    Ok(())
}
```

### Basic Consumer

```rust
use rustmq_client::*;

#[tokio::main]
async fn main() -> Result<()> {
    let config = ClientConfig::default();
    let client = RustMqClient::new(config).await?;
    
    let consumer = client.create_consumer("my-topic", "my-group").await?;
    
    while let Some(consumer_message) = consumer.receive().await? {
        let message = &consumer_message.message;
        println!("Received: {}", message.payload_as_string()?);
        
        // Acknowledge message
        consumer_message.ack().await?;
    }
    
    Ok(())
}
```

## Building

### Prerequisites

- Rust 1.70 or later
- Tokio runtime
- Network access to RustMQ brokers

### Build Commands

```bash
# Build the SDK
cargo build --release

# Run tests
cargo test --lib

# Run benchmarks
cargo bench

# Build with features
cargo build --features "io-uring,compression"

# Check for errors
cargo check

# Run clippy linting
cargo clippy

# Format code
cargo fmt
```

### Cross-compilation

```bash
# Build for different targets
cargo build --target x86_64-unknown-linux-musl
cargo build --target aarch64-unknown-linux-gnu
cargo build --target x86_64-pc-windows-gnu
```

## Configuration

### Client Configuration

```rust
use rustmq_client::*;
use std::time::Duration;

let config = ClientConfig {
    // Broker endpoints
    brokers: vec![
        "broker1.example.com:9092".to_string(),
        "broker2.example.com:9092".to_string(),
    ],
    
    // Client identification
    client_id: Some("my-service-v1.0".to_string()),
    
    // Connection settings
    connect_timeout: Duration::from_secs(10),
    request_timeout: Duration::from_secs(30),
    max_connections: 10,
    keep_alive_interval: Duration::from_secs(30),
    
    // TLS configuration
    enable_tls: true,
    tls_config: Some(TlsConfig {
        ca_cert: Some("/path/to/ca.pem".to_string()),
        client_cert: Some("/path/to/client.pem".to_string()),
        client_key: Some("/path/to/client-key.pem".to_string()),
        server_name: Some("rustmq.example.com".to_string()),
        insecure_skip_verify: false,
    }),
    
    // Retry configuration
    retry_config: RetryConfig {
        max_retries: 3,
        base_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(10),
        multiplier: 2.0,
        jitter: true,
    },
    
    // Compression settings
    compression: CompressionConfig {
        enabled: true,
        algorithm: CompressionAlgorithm::Lz4,
        level: 6,
        min_size: 1024,
    },
    
    // Authentication
    auth: Some(AuthConfig {
        method: AuthMethod::SaslPlain,
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        ..Default::default()
    }),
};
```

### Producer Configuration

```rust
let producer_config = ProducerConfig {
    producer_id: Some("producer-1".to_string()),
    
    // Batching settings
    batch_size: 1000,
    batch_timeout: Duration::from_millis(50),
    
    // Message limits
    max_message_size: 4 * 1024 * 1024, // 4MB
    
    // Acknowledgment level
    ack_level: AckLevel::All, // Wait for all replicas
    
    // Idempotent producer (prevents duplicates)
    idempotent: true,
    
    // Compression for this producer
    compression: CompressionConfig {
        enabled: true,
        algorithm: CompressionAlgorithm::Lz4,
        level: 4, // Faster compression
        min_size: 1024,
    },
    
    // Default message properties
    default_properties: {
        let mut props = std::collections::HashMap::new();
        props.insert("service".to_string(), "payment-processor".to_string());
        props.insert("version".to_string(), "1.2.3".to_string());
        props
    },
};

let producer = ProducerBuilder::new()
    .topic("payments")
    .config(producer_config)
    .client(client)
    .build()
    .await?;
```

### Consumer Configuration

```rust
let consumer_config = ConsumerConfig {
    consumer_id: Some("consumer-1".to_string()),
    consumer_group: "payment-processors".to_string(),
    
    // Commit settings
    auto_commit_interval: Duration::from_secs(5),
    enable_auto_commit: true,
    
    // Fetch settings
    fetch_size: 500,
    fetch_timeout: Duration::from_secs(1),
    
    // Starting position
    start_position: StartPosition::Earliest,
    
    // Error handling
    max_retry_attempts: 3,
    dead_letter_queue: Some("failed-payments".to_string()),
};

let consumer = ConsumerBuilder::new()
    .topic("payments")
    .consumer_group("payment-processors")
    .config(consumer_config)
    .client(client)
    .build()
    .await?;
```

## Usage Guide

### Message Creation

```rust
// Simple message
let message = Message::builder()
    .topic("events")
    .payload("Simple text message")
    .build()?;

// Complex message with metadata
let message = Message::builder()
    .topic("user-events")
    .key("user-123")
    .payload(serde_json::to_vec(&user_event)?)
    .header("event-type", "user-registration")
    .header("version", "v1")
    .header("timestamp", &chrono::Utc::now().to_rfc3339())
    .header("correlation-id", &correlation_id)
    .build()?;

// Binary message
let message = Message::builder()
    .topic("images")
    .key("image-456")
    .payload(image_bytes)
    .header("content-type", "image/jpeg")
    .header("size", &image_bytes.len().to_string())
    .build()?;
```

### Producer Patterns

#### Fire-and-Forget

```rust
producer.send_async(message, None).await?;
```

#### Synchronous with Result

```rust
let result = producer.send(message).await?;
println!("Message stored at offset: {}", result.offset);
```

#### Batch Sending

```rust
let messages = vec![message1, message2, message3];
let results = producer.send_batch(messages).await?;
```

#### Callback Pattern

```rust
producer.send_async(message, Some(|result, error| {
    if let Some(result) = result {
        println!("Success: offset {}", result.offset);
    } else if let Some(error) = error {
        eprintln!("Error: {}", error);
    }
})).await?;
```

### Consumer Patterns

#### Basic Consumption

```rust
while let Some(consumer_message) = consumer.receive().await? {
    let message = &consumer_message.message;
    
    // Process message
    process_payment(&message).await?;
    
    // Acknowledge processing
    consumer_message.ack().await?;
}
```

#### Streaming with Futures

```rust
use futures::StreamExt;

let mut stream = consumer.stream();
while let Some(result) = stream.next().await {
    match result {
        Ok(consumer_message) => {
            // Process message
            process_message(&consumer_message.message).await?;
            consumer_message.ack().await?;
        }
        Err(e) => eprintln!("Stream error: {}", e),
    }
}
```

#### Manual Offset Management

```rust
let config = ConsumerConfig {
    enable_auto_commit: false,
    ..Default::default()
};

let consumer = client.create_consumer("topic", "group", config).await?;

// Process messages
while let Some(message) = consumer.receive().await? {
    process_message(&message).await?;
    message.ack().await?;
    
    // Manual commit every 100 messages
    if processed_count % 100 == 0 {
        consumer.commit().await?;
    }
}
```

### Stream Processing

```rust
use async_trait::async_trait;

struct PaymentProcessor {
    fraud_detector: FraudDetector,
}

#[async_trait]
impl MessageProcessor for PaymentProcessor {
    async fn process(&self, message: &Message) -> Result<Option<Message>> {
        let payment: Payment = serde_json::from_slice(&message.payload)?;
        
        // Validate payment
        let result = self.fraud_detector.check(&payment).await?;
        
        let output = ProcessedPayment {
            payment,
            fraud_score: result.score,
            approved: result.score < 0.5,
            processed_at: chrono::Utc::now(),
        };
        
        let output_message = Message::builder()
            .topic("processed-payments")
            .key(&payment.user_id)
            .payload(serde_json::to_vec(&output)?)
            .header("processor", "fraud-detection")
            .header("score", &result.score.to_string())
            .build()?;
        
        Ok(Some(output_message))
    }
    
    async fn on_error(&self, message: &Message, error: &ClientError) -> Result<()> {
        // Log error with context
        tracing::error!(
            message_id = %message.id,
            topic = %message.topic,
            error = %error,
            "Payment processing failed"
        );
        
        // Send to dead letter queue
        self.send_to_dlq(message).await?;
        
        Ok(())
    }
}

// Set up stream processing
let stream_config = StreamConfig {
    input_topics: vec!["raw-payments".to_string()],
    output_topic: Some("processed-payments".to_string()),
    consumer_group: "payment-processors".to_string(),
    parallelism: 8,
    processing_timeout: Duration::from_secs(30),
    error_strategy: ErrorStrategy::DeadLetter {
        topic: "failed-payments".to_string(),
    },
    mode: StreamMode::Individual,
};

let stream = client.create_stream(stream_config).await?
    .with_processor(PaymentProcessor::new());

stream.start().await?;
```

## Performance Tuning

### Producer Optimization

```rust
let config = ProducerConfig {
    // Larger batches for higher throughput
    batch_size: 10000,
    batch_timeout: Duration::from_millis(10),
    
    // Async acknowledgment for lower latency
    ack_level: AckLevel::Leader,
    
    // Enable compression for large messages
    compression: CompressionConfig {
        enabled: true,
        algorithm: CompressionAlgorithm::Lz4, // Fastest
        level: 1, // Speed over compression ratio
        min_size: 1024,
    },
    
    // Disable idempotence for maximum speed (if duplicates are acceptable)
    idempotent: false,
    
    ..Default::default()
};
```

### Consumer Optimization

```rust
let config = ConsumerConfig {
    // Larger fetch sizes
    fetch_size: 10000,
    fetch_timeout: Duration::from_millis(100),
    
    // Less frequent commits
    auto_commit_interval: Duration::from_secs(10),
    
    // Start from latest for real-time processing
    start_position: StartPosition::Latest,
    
    ..Default::default()
};
```

### Connection Optimization

```rust
let config = ClientConfig {
    // More connections for higher throughput
    max_connections: 20,
    
    // Shorter timeouts for faster failure detection
    request_timeout: Duration::from_secs(5),
    keep_alive_interval: Duration::from_secs(15),
    
    // Optimized retry strategy
    retry_config: RetryConfig {
        max_retries: 2,
        base_delay: Duration::from_millis(50),
        max_delay: Duration::from_secs(2),
        multiplier: 1.5,
        jitter: true,
    },
    
    ..Default::default()
};
```

### Memory Optimization

```rust
// Use streaming for large datasets
let mut stream = consumer.stream();
while let Some(result) = stream.next().await {
    let message = result?;
    
    // Process immediately without storing
    process_immediately(&message.message).await?;
    message.ack().await?;
    
    // Optional: yield to scheduler
    if processed % 1000 == 0 {
        tokio::task::yield_now().await;
    }
}
```

### IO-Uring (Linux)

```toml
[dependencies]
rustmq-client = { path = "sdk/rust", features = ["io-uring"] }
```

```rust
// Enable io-uring for maximum performance on Linux
let config = ClientConfig {
    // io-uring will be used automatically when available
    ..Default::default()
};
```

## Best Practices

### Error Handling

```rust
use rustmq_client::{ClientError, Result};

async fn robust_producer_example() -> Result<()> {
    let client = RustMqClient::new(config).await?;
    let producer = client.create_producer("topic").await?;
    
    for i in 0..1000 {
        let message = create_message(i)?;
        
        match producer.send(message).await {
            Ok(result) => {
                tracing::info!("Message {} sent: offset={}", i, result.offset);
            }
            Err(ClientError::MessageTooLarge { size, max_size }) => {
                tracing::warn!("Message {} too large: {}B > {}B", i, size, max_size);
                // Split or compress message
                handle_oversized_message(i, size).await?;
            }
            Err(ClientError::Timeout { timeout_ms }) => {
                tracing::warn!("Message {} timed out after {}ms, retrying", i, timeout_ms);
                // Implement custom retry logic
                retry_with_backoff(|| producer.send(message.clone())).await?;
            }
            Err(e) if e.is_retryable() => {
                tracing::warn!("Retryable error for message {}: {}", i, e);
                // Use built-in retry mechanism
                retry_send(&producer, message).await?;
            }
            Err(e) => {
                tracing::error!("Fatal error for message {}: {}", i, e);
                return Err(e);
            }
        }
    }
    
    Ok(())
}
```

### Resource Management

```rust
// Use connection pooling
struct MessageService {
    client: Arc<RustMqClient>,
    producers: DashMap<String, Producer>,
    consumers: DashMap<String, Consumer>,
}

impl MessageService {
    async fn get_producer(&self, topic: &str) -> Result<Producer> {
        if let Some(producer) = self.producers.get(topic) {
            Ok(producer.clone())
        } else {
            let producer = self.client.create_producer(topic).await?;
            self.producers.insert(topic.to_string(), producer.clone());
            Ok(producer)
        }
    }
    
    async fn shutdown(&self) -> Result<()> {
        // Close all producers
        for producer in self.producers.iter() {
            producer.close().await?;
        }
        
        // Close all consumers
        for consumer in self.consumers.iter() {
            consumer.close().await?;
        }
        
        // Close client
        self.client.close().await?;
        
        Ok(())
    }
}
```

### Monitoring and Observability

```rust
use tracing::{info, error, instrument};

#[instrument(skip(message))]
async fn process_order(message: &Message) -> Result<()> {
    let start = std::time::Instant::now();
    
    info!("Processing order: {}", message.id);
    
    // Your processing logic
    let order: Order = serde_json::from_slice(&message.payload)?;
    validate_order(&order).await?;
    
    let duration = start.elapsed();
    info!("Order processed in {:?}", duration);
    
    // Update metrics
    ORDER_PROCESSING_TIME.observe(duration.as_secs_f64());
    ORDERS_PROCESSED.inc();
    
    Ok(())
}

// Initialize tracing
tracing_subscriber::fmt()
    .with_env_filter("rustmq_client=debug,my_app=info")
    .init();
```

## Testing

### Unit Tests

```bash
# Run all tests
cargo test --lib

# Run specific test
cargo test test_message_building

# Run tests with output
cargo test -- --nocapture

# Run tests with specific features
cargo test --features "io-uring,compression"
```

### Integration Tests

```bash
# Run integration tests (requires running broker)
cargo test --test integration_tests

# Run specific integration test
cargo test --test integration_tests test_producer_consumer_flow
```

### Mock Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    
    #[tokio::test]
    async fn test_message_creation() {
        let message = Message::builder()
            .topic("test-topic")
            .payload("test payload")
            .header("test", "value")
            .build()
            .unwrap();
        
        assert_eq!(message.topic, "test-topic");
        assert_eq!(message.payload_as_string().unwrap(), "test payload");
        assert_eq!(message.get_header("test"), Some(&"value".to_string()));
    }
    
    #[tokio::test]
    async fn test_producer_metrics() {
        let config = ClientConfig::default();
        let client = RustMqClient::new(config).await.unwrap();
        let producer = client.create_producer("test").await.unwrap();
        
        let metrics = producer.metrics().await;
        assert_eq!(metrics.messages_sent.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}
```

## Benchmarking

### Run Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench bench_message_creation

# Generate HTML reports
cargo bench -- --output-format html
```

### Custom Benchmarks

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_message_serialization(c: &mut Criterion) {
    let message = create_test_message();
    
    c.bench_function("message_serialization", |b| {
        b.iter(|| {
            let json = message.to_json();
            black_box(json)
        })
    });
}

criterion_group!(benches, bench_message_serialization);
criterion_main!(benches);
```

## Examples

See the `examples/` directory for complete examples:

- [`simple_producer.rs`](examples/simple_producer.rs) - Basic message production
- [`simple_consumer.rs`](examples/simple_consumer.rs) - Basic message consumption
- [`stream_processor.rs`](examples/stream_processor.rs) - Stream processing pipeline

### Running Examples

```bash
# Run producer example
cargo run --example simple_producer

# Run consumer example  
cargo run --example simple_consumer

# Run stream processor
cargo run --example stream_processor
```

## Troubleshooting

### Common Issues

#### Connection Errors

```rust
// Issue: Connection refused
Error: Connection("connection refused")

// Solution: Check broker configuration
let config = ClientConfig {
    brokers: vec!["correct-broker:9092".to_string()],
    connect_timeout: Duration::from_secs(30), // Increase timeout
    ..Default::default()
};
```

#### Memory Issues

```rust
// Issue: High memory usage
// Solution: Use streaming and limit batch sizes
let config = ProducerConfig {
    batch_size: 1000, // Reduce batch size
    batch_timeout: Duration::from_millis(100),
    ..Default::default()
};
```

#### Performance Issues

```rust
// Issue: Low throughput
// Solution: Optimize configuration
let config = ClientConfig {
    max_connections: 20, // Increase connections
    compression: CompressionConfig {
        enabled: true,
        algorithm: CompressionAlgorithm::Lz4, // Use faster compression
        ..Default::default()
    },
    ..Default::default()
};
```

### Debug Logging

```rust
// Enable debug logging
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracing_subscriber::EnvFilter::new("rustmq_client=debug"))
    .with(tracing_subscriber::fmt::layer())
    .init();
```

### Health Checks

```rust
// Regular health monitoring
async fn health_monitor(client: &RustMqClient) {
    loop {
        match client.health_check().await {
            Ok(_) => tracing::info!("Client healthy"),
            Err(e) => tracing::error!("Health check failed: {}", e),
        }
        
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}
```

## API Reference

### Core Types

- [`RustMqClient`](src/client.rs) - Main client interface
- [`Producer`](src/producer.rs) - Message producer
- [`Consumer`](src/consumer.rs) - Message consumer  
- [`Message`](src/message.rs) - Message representation
- [`MessageStream`](src/stream.rs) - Stream processing

### Configuration Types

- [`ClientConfig`](src/config.rs) - Client configuration
- [`ProducerConfig`](src/config.rs) - Producer configuration
- [`ConsumerConfig`](src/config.rs) - Consumer configuration
- [`StreamConfig`](src/stream.rs) - Stream configuration

### Error Types

- [`ClientError`](src/error.rs) - All client errors
- [`Result<T>`](src/error.rs) - Result type alias

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make changes and add tests
4. Run tests: `cargo test`
5. Run clippy: `cargo clippy`
6. Format code: `cargo fmt`
7. Submit a pull request

### Development Setup

```bash
# Clone repository
git clone https://github.com/rustmq/rustmq.git
cd rustmq/sdk/rust

# Install dependencies
cargo build

# Run tests
cargo test

# Run lints
cargo clippy

# Format code
cargo fmt
```

### Code Standards

- Follow Rust API guidelines
- Add documentation for public APIs
- Include tests for new features
- Use `tracing` for logging
- Handle errors properly with `Result<T>`

---

**License**: MIT OR Apache-2.0

**Repository**: https://github.com/rustmq/rustmq

**Documentation**: Run `cargo doc --open` for detailed API docs