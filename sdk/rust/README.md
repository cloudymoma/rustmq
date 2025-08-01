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
│  └── Connection (QUIC Pool, Health Check, Auto-Reconnect)   │
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

### Consumer Features

- **🔄 Auto-Retry**: Exponential backoff retry with dead letter queue support
- **🎯 Offset Management**: Manual and automatic offset commits with seeking
- **📈 Consumer Lag**: Real-time lag monitoring and alerting per partition
- **⏰ Timestamp Seeking**: Seek to specific timestamps for historical processing
- **🔀 Multi-Partition Control**: Full multi-partition assignment, pausing, and resuming
- **🌊 Stream Interface**: Async stream support with `futures::Stream`
- **💾 Persistent State**: Reliable per-partition offset tracking and recovery
- **🚨 Error Handling**: Comprehensive retry logic and failure monitoring
- **📊 Partition Assignment**: Automatic partition assignment with rebalancing support
- **🎯 Selective Processing**: Pause/resume individual partitions for load balancing
- **🔍 Per-Partition Metrics**: Individual partition lag and offset tracking
- **⚡ Concurrent Fetching**: Parallel message fetching from multiple partitions

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

### Advanced Consumer Features

The Consumer implementation provides production-ready features including:

#### Offset Management

```rust
// Manual offset commits
consumer.commit().await?;

// Get current lag
let lag = consumer.get_lag().await?;
println!("Consumer lag: {:?}", lag);

// Get committed offset for partition
let offset = consumer.committed_offset(0).await?;
println!("Committed offset for partition 0: {}", offset);
```

#### Seeking

```rust
// Seek to specific offset
consumer.seek(12345).await?;

// Seek to timestamp (finds offset for timestamp)
let one_hour_ago = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() - 3600;
consumer.seek_to_timestamp(one_hour_ago).await?;
```

#### Failed Message Handling

```rust
let consumer_config = ConsumerConfig {
    // Retry configuration
    max_retry_attempts: 5,
    dead_letter_queue: Some("failed-messages".to_string()),
    
    // Failed messages are automatically retried with exponential backoff:
    // 1st retry: 2 seconds
    // 2nd retry: 4 seconds  
    // 3rd retry: 8 seconds
    // After max retries: sent to dead letter queue
    
    ..Default::default()
};
```

#### Multi-Partition Management

The Consumer implementation provides comprehensive multi-partition support with automatic partition assignment, per-partition offset tracking, and flexible partition control.

```rust
// Get assigned partitions
let partitions = consumer.assigned_partitions().await;
println!("Assigned partitions: {:?}", partitions);

// Get detailed partition assignment information
if let Some(assignment) = consumer.partition_assignment().await {
    println!("Assignment ID: {}", assignment.assignment_id);
    println!("Assigned at: {:?}", assignment.assigned_at);
    println!("Partitions: {:?}", assignment.partitions);
}

// Pause consumption from specific partitions
consumer.pause_partitions(vec![0, 1]).await?;

// Resume consumption
consumer.resume_partitions(vec![0, 1]).await?;

// Get currently paused partitions
let paused = consumer.paused_partitions().await;
println!("Paused partitions: {:?}", paused);

// Multi-partition seeking
// Seek specific partition to offset
consumer.seek(0, 12345).await?;

// Seek all partitions to same offset
consumer.seek_all(10000).await?;

// Seek specific partition to timestamp
consumer.seek_to_timestamp(0, timestamp).await?;

// Seek all partitions to timestamp (returns partition -> offset mapping)
let partition_offsets = consumer.seek_all_to_timestamp(timestamp).await?;
println!("Seeked to timestamp with offsets: {:?}", partition_offsets);

// Get per-partition consumer lag
let lag_map = consumer.get_lag().await?;
for (partition, lag) in lag_map {
    println!("Partition {}: {} messages behind", partition, lag);
}

// Get committed offset for specific partition
let offset = consumer.committed_offset(0).await?;
println!("Committed offset for partition 0: {}", offset);
```

#### Advanced Multi-Partition Features

```rust
// Per-partition state management
let assigned_partitions = consumer.assigned_partitions().await;
for partition in assigned_partitions {
    // Check individual partition lag
    match consumer.committed_offset(partition).await {
        Ok(offset) => println!("Partition {} at offset {}", partition, offset),
        Err(e) => println!("Error getting offset for partition {}: {}", partition, e),
    }
}

// Selective partition processing
let partitions_to_process = vec![0, 2, 4]; // Process only even partitions
let all_partitions = consumer.assigned_partitions().await;
let partitions_to_pause: Vec<u32> = all_partitions
    .into_iter()
    .filter(|p| !partitions_to_process.contains(p))
    .collect();

consumer.pause_partitions(partitions_to_pause).await?;

// Process only active partitions
while let Some(message) = consumer.receive().await? {
    // Message will only come from unpaused partitions
    println!("Received from partition {}: {}", 
             message.message.partition, 
             message.message.payload_as_string()?);
    message.ack().await?;
}
```

## Comprehensive Multi-Partition Consumer Guide

The RustMQ Rust SDK provides industry-leading multi-partition consumer support with production-ready features for high-throughput, distributed message processing.

### Multi-Partition Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Multi-Partition Consumer                 │
├─────────────────────────────────────────────────────────────┤
│  PartitionAssignment        OffsetTracker                  │
│  ├── partitions: [0,1,2,4]  ├── Per-partition offsets     │
│  ├── assignment_id          ├── Pending message tracking  │
│  └── assigned_at            └── Consecutive commit logic  │
├─────────────────────────────────────────────────────────────┤
│  Concurrent Fetching     │  Partition Management          │
│  ├── Parallel requests  │  ├── Pause/Resume individual   │
│  ├── Load balancing     │  ├── Selective processing      │
│  └── Backpressure       │  └── Error isolation          │
├─────────────────────────────────────────────────────────────┤
│  Advanced Seeking        │  Performance Monitoring        │
│  ├── Per-partition seek  │  ├── Per-partition lag        │
│  ├── Bulk operations     │  ├── Processing metrics       │
│  └── Timestamp seeking   │  └── Health monitoring        │
└─────────────────────────────────────────────────────────────┘
```

### Key Multi-Partition Features

✅ **Production-Ready Implementation**
- **Automatic Partition Assignment**: Seamless integration with RustMQ's partition rebalancing
- **Per-Partition Offset Tracking**: Independent offset management with gap handling
- **Concurrent Message Fetching**: Parallel requests across assigned partitions
- **Intelligent Commit Strategy**: Consecutive offset calculation with out-of-order support

✅ **Advanced Partition Control**
- **Selective Partition Processing**: Pause/resume individual partitions for load balancing
- **Partition-Specific Seeking**: Seek individual partitions to different offsets/timestamps
- **Bulk Operations**: Seek all partitions simultaneously with atomic operations
- **Error Isolation**: Handle partition-specific errors without affecting others

✅ **Performance & Reliability**
- **Zero-Copy Operations**: Efficient memory usage across multiple partitions
- **Backpressure Handling**: Automatic flow control per partition
- **Comprehensive Error Handling**: Retry logic with partition-specific strategies
- **Real-Time Monitoring**: Per-partition lag tracking and health metrics

### Complete Multi-Partition Example

```rust
use rustmq_client::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Create client with multi-partition optimization
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        max_connections: 15, // More connections for concurrent fetching
        ..Default::default()
    };
    let client = RustMqClient::new(config).await?;
    
    // Configure consumer for multi-partition workloads
    let consumer_config = ConsumerConfig {
        consumer_group: "multi-partition-processors".to_string(),
        enable_auto_commit: false, // Manual commits for precision
        fetch_size: 1000,          // Larger batches for efficiency
        fetch_timeout: Duration::from_millis(500),
        max_retry_attempts: 5,
        dead_letter_queue: Some("failed-mp-messages".to_string()),
        ..Default::default()
    };
    
    let consumer = ConsumerBuilder::new()
        .topic("high-throughput-topic")
        .consumer_group("multi-partition-processors")
        .config(consumer_config)
        .client(client)
        .build()
        .await?;
    
    // Get partition assignment
    let partitions = consumer.assigned_partitions().await;
    println!("Assigned to {} partitions: {:?}", partitions.len(), partitions);
    
    // Process messages from all partitions
    let mut partition_counts = HashMap::new();
    for _ in 0..100 {
        if let Some(consumer_message) = consumer.receive().await? {
            let message = &consumer_message.message;
            
            // Track per-partition processing
            *partition_counts.entry(message.partition).or_insert(0) += 1;
            
            println!("Processing message from partition {}: {}", 
                     message.partition, message.payload_as_string()?);
            
            // Acknowledge message
            consumer_message.ack().await?;
            
            // Commit every 10 messages
            if partition_counts.values().sum::<u32>() % 10 == 0 {
                consumer.commit().await?;
            }
        }
    }
    
    // Show processing distribution
    println!("Messages processed per partition: {:?}", partition_counts);
    
    // Get consumer lag across all partitions
    let lag_map = consumer.get_lag().await?;
    for (partition, lag) in lag_map {
        println!("Partition {}: {} messages behind", partition, lag);
    }
    
    Ok(())
}
```

### Advanced Multi-Partition Scenarios

#### Partition-Specific Error Handling

```rust
// Handle errors per partition
while let Some(consumer_message) = consumer.receive().await? {
    let message = &consumer_message.message;
    
    match process_message_for_partition(message).await {
        Ok(_) => consumer_message.ack().await?,
        Err(PartitionError::Retryable(partition)) => {
            // Pause problematic partition temporarily
            consumer.pause_partitions(vec![partition]).await?;
            consumer_message.nack().await?;
            
            // Resume after delay
            tokio::time::sleep(Duration::from_secs(5)).await;
            consumer.resume_partitions(vec![partition]).await?;
        }
        Err(PartitionError::Fatal(partition)) => {
            // Skip message but continue processing other partitions
            consumer_message.ack().await?;
            eprintln!("Fatal error on partition {}, continuing with others", partition);
        }
    }
}
```

#### Bulk Partition Operations

```rust
// Seek all partitions to 1 hour ago
let one_hour_ago = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)?
    .as_secs() - 3600;

let partition_offsets = consumer.seek_all_to_timestamp(one_hour_ago).await?;
println!("Seeked all partitions: {:?}", partition_offsets);

// Selective partition processing
let high_priority_partitions = vec![0, 1]; // Process high-priority partitions only
let all_partitions = consumer.assigned_partitions().await;
let low_priority: Vec<u32> = all_partitions.into_iter()
    .filter(|p| !high_priority_partitions.contains(p))
    .collect();

consumer.pause_partitions(low_priority).await?;
// Now only receive messages from high-priority partitions
```

#### Performance Monitoring

```rust
// Monitor multi-partition performance
let metrics = consumer.metrics().await;
let lag_map = consumer.get_lag().await?;

println!("Multi-Partition Performance:");
println!("  Total messages processed: {}", 
         metrics.messages_processed.load(Ordering::Relaxed));
println!("  Active partitions: {}", consumer.assigned_partitions().await.len());
println!("  Paused partitions: {:?}", consumer.paused_partitions().await);

// Calculate total lag across all partitions
let total_lag: u64 = lag_map.values().sum();
println!("  Total consumer lag: {} messages", total_lag);

// Efficiency calculation
let received = metrics.messages_received.load(Ordering::Relaxed);
let processed = metrics.messages_processed.load(Ordering::Relaxed);
if received > 0 {
    let efficiency = (processed as f64 / received as f64) * 100.0;
    println!("  Processing efficiency: {:.1}%", efficiency);
}
```

### Multi-Partition Best Practices

🎯 **Configuration Optimization**
- Use `enable_auto_commit: false` for precise offset control across partitions
- Set `fetch_size: 1000+` for efficient batch processing across multiple partitions
- Configure `max_connections: 15+` for concurrent partition fetching
- Enable `dead_letter_queue` for robust error handling per partition

⚡ **Performance Tuning**
- Monitor per-partition lag to identify bottlenecks: `consumer.get_lag().await?`
- Use partition pause/resume for dynamic load balancing
- Implement parallel processing for independent partitions
- Batch commits across multiple partitions for efficiency

🛡️ **Error Handling Strategy**
- Isolate partition-specific errors to prevent cascading failures
- Use partition pausing for temporary issues (network, resource constraints)
- Implement partition-aware retry logic with exponential backoff
- Monitor partition health and rebalance as needed

📊 **Monitoring & Observability**
- Track per-partition processing rates and lag
- Monitor partition assignment changes and rebalancing events
- Set up alerts for high lag or failed partitions
- Use metrics for capacity planning and scaling decisions

## Connection Layer

The RustMQ Rust SDK implements a sophisticated QUIC-based connection layer that provides:

### Key Features
- **Connection Pooling**: Automatic management of multiple broker connections with round-robin load balancing
- **Auto-Reconnection**: Exponential backoff retry logic with jitter to prevent thundering herd effects
- **Health Monitoring**: Background health checks with ping/pong protocol and automatic cleanup of failed connections
- **Request/Response Protocol**: Structured message framing with UUIDs and length prefixes for reliable communication
- **Flow Control**: Built-in backpressure handling and semaphore-based concurrency control

### Connection Management

```rust
use rustmq_client::*;

// The connection layer automatically handles:
// 1. Initial connection establishment to all brokers
// 2. Connection pooling and load balancing
// 3. Automatic reconnection on failure
// 4. Health monitoring and cleanup

let config = ClientConfig {
    brokers: vec![
        "broker1.example.com:9092".to_string(),
        "broker2.example.com:9092".to_string(),
        "broker3.example.com:9092".to_string(),
    ],
    
    // Connection pool settings
    max_connections: 10,
    connect_timeout: Duration::from_secs(10),
    request_timeout: Duration::from_secs(30),
    
    // Health check configuration
    keep_alive_interval: Duration::from_secs(30),
    health_check_timeout: Duration::from_secs(5),
    
    // Retry configuration with exponential backoff
    retry_config: RetryConfig {
        max_retries: 3,
        base_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(10),
        multiplier: 2.0,
        jitter: true,
    },
    
    ..Default::default()
};

let client = RustMqClient::new(config).await?;

// Connection statistics
let stats = client.connection_stats().await?;
println!("Active connections: {}", stats.active_connections);
println!("Total requests: {}", stats.total_requests);
println!("Failed requests: {}", stats.failed_requests);
```

### QUIC Transport Configuration

```rust
let config = ClientConfig {
    // QUIC-specific transport parameters
    quic_config: QuicConfig {
        // Keep-alive settings
        keep_alive_interval: Duration::from_secs(30),
        max_idle_timeout: Duration::from_secs(120),
        
        // Stream limits for concurrency
        initial_max_streams_bidi: 100,
        initial_max_streams_uni: 100,
        
        // Data limits for flow control
        initial_max_data: 10_000_000, // 10MB total
        initial_max_stream_data_bidi_local: 1_000_000, // 1MB per stream
        initial_max_stream_data_bidi_remote: 1_000_000,
        initial_max_stream_data_uni: 1_000_000,
        
        // Buffer sizes for performance
        send_buffer_size: 1_000_000, // 1MB
        recv_buffer_size: 1_000_000, // 1MB
        
        // TLS configuration
        server_name: Some("rustmq.example.com".to_string()),
        alpn_protocols: vec![b"rustmq".to_vec()],
        
        // Development mode (disable certificate verification)
        insecure: false, // Set to true only for testing
    },
    
    ..Default::default()
};
```

### Error Handling and Recovery

```rust
use rustmq_client::{ClientError, Result};

async fn robust_connection_example() -> Result<()> {
    let client = RustMqClient::new(config).await?;
    
    // The connection layer automatically handles:
    match client.health_check().await {
        Ok(stats) => {
            println!("All connections healthy: {} active", stats.healthy_connections);
        }
        Err(ClientError::Connection(msg)) => {
            println!("Connection issues detected: {}", msg);
            // Connection layer will automatically attempt reconnection
        }
        Err(ClientError::Timeout { timeout_ms }) => {
            println!("Health check timed out after {}ms", timeout_ms);
            // Automatic retry with exponential backoff
        }
        Err(e) => {
            println!("Unexpected error: {}", e);
        }
    }
    
    Ok(())
}
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

#### Error Handling with Dead Letter Queue

```rust
while let Some(consumer_message) = consumer.receive().await? {
    let message = &consumer_message.message;
    
    match process_critical_message(message).await {
        Ok(_) => {
            // Successful processing
            consumer_message.ack().await?;
        }
        Err(ProcessingError::Retryable(e)) => {
            // Will be retried with exponential backoff
            warn!("Processing failed, will retry: {}", e);
            consumer_message.nack().await?;
        }
        Err(ProcessingError::Fatal(e)) => {
            // Mark as processed to avoid infinite retries
            error!("Fatal processing error: {}", e);
            consumer_message.ack().await?;
        }
    }
}
```

#### Consumer with Seeking

```rust
// Start processing from a specific timestamp
let yesterday = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() - 86400; // 24 hours ago

consumer.seek_to_timestamp(yesterday).await?;

// Or seek to specific offset
consumer.seek(5000).await?;

// Now consume from that position
while let Some(message) = consumer.receive().await? {
    process_historical_message(&message.message).await?;
    message.ack().await?;
}
```

#### Consumer Metrics and Monitoring

```rust
use std::time::Duration;

// Monitor consumer health
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        
        let metrics = consumer.metrics().await;
        let lag = consumer.get_lag().await.unwrap_or_default();
        
        info!("Consumer metrics:");
        info!("  Messages received: {}", metrics.messages_received.load(Ordering::Relaxed));
        info!("  Messages processed: {}", metrics.messages_processed.load(Ordering::Relaxed));
        info!("  Messages failed: {}", metrics.messages_failed.load(Ordering::Relaxed));
        info!("  Consumer lag: {:?}", lag);
        
        // Alert if lag is too high
        for (partition, lag_count) in lag {
            if lag_count > 10000 {
                warn!("High lag detected on partition {}: {} messages", partition, lag_count);
            }
        }
    }
});
```

### Stream Processing

The RustMQ Rust SDK provides a comprehensive stream processing framework for real-time message transformation and enrichment.

#### Stream Processing Modes

**Individual Processing**: Process messages one by one for real-time processing

```rust
let stream_config = StreamConfig {
    mode: StreamMode::Individual,
    ..Default::default()
};
```

**Batch Processing**: Process messages in batches for higher throughput

```rust
let stream_config = StreamConfig {
    mode: StreamMode::Batch { 
        batch_size: 100, 
        batch_timeout: Duration::from_millis(1000) 
    },
    ..Default::default()
};
```

**Windowed Processing**: Process messages in time-based windows for aggregation

```rust
let stream_config = StreamConfig {
    mode: StreamMode::Windowed { 
        window_size: Duration::from_secs(60),
        slide_interval: Duration::from_secs(30)
    },
    ..Default::default()
};
```

#### Error Handling Strategies

```rust
use rustmq_client::stream::{ErrorStrategy, StreamConfig};

// Skip failed messages
let config = StreamConfig {
    error_strategy: ErrorStrategy::Skip,
    ..Default::default()
};

// Retry failed messages with exponential backoff
let config = StreamConfig {
    error_strategy: ErrorStrategy::Retry { 
        max_attempts: 3, 
        backoff_ms: 1000 
    },
    ..Default::default()
};

// Send failed messages to dead letter queue
let config = StreamConfig {
    error_strategy: ErrorStrategy::DeadLetter { 
        topic: "failed-messages".to_string() 
    },
    ..Default::default()
};

// Stop processing on error
let config = StreamConfig {
    error_strategy: ErrorStrategy::Stop,
    ..Default::default()
};
```

#### Custom Message Processor

```rust
use async_trait::async_trait;
use rustmq_client::stream::MessageProcessor;

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
    
    async fn process_batch(&self, messages: &[Message]) -> Result<Vec<Option<Message>>> {
        let mut results = Vec::with_capacity(messages.len());
        
        // Process messages in parallel for batch mode
        let futures: Vec<_> = messages.iter()
            .map(|msg| self.process(msg))
            .collect();
            
        for result in futures::future::join_all(futures).await {
            results.push(result?);
        }
        
        Ok(results)
    }
    
    async fn on_start(&self) -> Result<()> {
        tracing::info!("Payment processor started");
        // Initialize resources
        Ok(())
    }
    
    async fn on_stop(&self) -> Result<()> {
        tracing::info!("Payment processor stopped");
        // Clean up resources
        Ok(())
    }
    
    async fn on_error(&self, message: &Message, error: &ClientError) -> Result<()> {
        // Log error with context
        tracing::error!(
            message_id = %message.id,
            topic = %message.topic,
            error = %error,
            "Payment processing failed"
        );
        
        Ok(())
    }
}
```

#### Stream Setup and Management

```rust
// Create stream configuration
let stream_config = StreamConfig {
    input_topics: vec!["raw-payments".to_string()],
    output_topic: Some("processed-payments".to_string()),
    consumer_group: "payment-processors".to_string(),
    parallelism: 8,
    processing_timeout: Duration::from_secs(30),
    exactly_once: true,
    max_in_flight: 1000,
    error_strategy: ErrorStrategy::DeadLetter {
        topic: "failed-payments".to_string(),
    },
    mode: StreamMode::Individual,
};

// Create and start stream
let stream = MessageStream::new(client, stream_config).await?
    .with_processor(PaymentProcessor::new());

stream.start().await?;

// Monitor stream metrics
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        
        let metrics = stream.metrics().await;
        tracing::info!("Stream metrics:");
        tracing::info!("  Messages processed: {}", 
                      metrics.messages_processed.load(Ordering::Relaxed));
        tracing::info!("  Messages failed: {}", 
                      metrics.messages_failed.load(Ordering::Relaxed));
        tracing::info!("  Messages skipped: {}", 
                      metrics.messages_skipped.load(Ordering::Relaxed));
        
        if let Ok(avg_time) = metrics.average_processing_time.try_read() {
            tracing::info!("  Avg processing time: {:.2}ms", *avg_time);
        }
    }
});

// Gracefully stop stream
stream.stop().await?;
```

#### Advanced Stream Processing Examples

**Aggregation with Windowed Processing**:

```rust
struct AnalyticsProcessor {
    metrics_store: MetricsStore,
}

#[async_trait]
impl MessageProcessor for AnalyticsProcessor {
    async fn process_batch(&self, messages: &[Message]) -> Result<Vec<Option<Message>>> {
        // Aggregate metrics from the window
        let mut event_counts = HashMap::new();
        let mut total_revenue = 0.0;
        
        for message in messages {
            let event: AnalyticsEvent = serde_json::from_slice(&message.payload)?;
            *event_counts.entry(event.event_type.clone()).or_insert(0) += 1;
            total_revenue += event.revenue;
        }
        
        let aggregated = AggregatedMetrics {
            window_start: messages.first().map(|m| m.timestamp).unwrap_or(0),
            window_end: messages.last().map(|m| m.timestamp).unwrap_or(0),
            event_counts,
            total_revenue,
            message_count: messages.len(),
        };
        
        // Create single aggregated message
        let output_message = Message::builder()
            .topic("analytics-aggregated")
            .payload(serde_json::to_vec(&aggregated)?)
            .header("window-size", &messages.len().to_string())
            .build()?;
        
        // Return single result for the entire batch
        let mut results = vec![None; messages.len()];
        results[0] = Some(output_message); // Only first message produces output
        
        Ok(results)
    }
}

let stream_config = StreamConfig {
    mode: StreamMode::Windowed { 
        window_size: Duration::from_secs(300),  // 5-minute windows
        slide_interval: Duration::from_secs(60) // Slide every minute
    },
    error_strategy: ErrorStrategy::Retry { 
        max_attempts: 3, 
        backoff_ms: 1000 
    },
    ..Default::default()
};
```

**Complex Event Processing with Multiple Input Topics**:

```rust
let stream_config = StreamConfig {
    input_topics: vec![
        "user-events".to_string(),
        "transaction-events".to_string(),
        "system-events".to_string(),
    ],
    output_topic: Some("enriched-events".to_string()),
    consumer_group: "event-enricher".to_string(),
    mode: StreamMode::Batch { 
        batch_size: 50, 
        batch_timeout: Duration::from_millis(500) 
    },
    error_strategy: ErrorStrategy::DeadLetter { 
        topic: "failed-enrichment".to_string() 
    },
    ..Default::default()
};
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
    
    // Optimized retry strategy with exponential backoff
    retry_config: RetryConfig {
        max_retries: 3,
        base_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(10),
        multiplier: 2.0,
        jitter: true, // Prevents thundering herd
    },
    
    // QUIC transport optimization
    quic_config: QuicConfig {
        keep_alive_interval: Duration::from_secs(30),
        max_idle_timeout: Duration::from_secs(120),
        initial_max_streams_bidi: 100,
        initial_max_data: 10_000_000, // 10MB
        initial_max_stream_data_bidi_local: 1_000_000, // 1MB
        send_buffer_size: 1_000_000, // 1MB
        recv_buffer_size: 1_000_000, // 1MB
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
- [`advanced_consumer.rs`](examples/advanced_consumer.rs) - Advanced consumer with seeking, error handling, and partition management
- [`multi_partition_consumer.rs`](examples/multi_partition_consumer.rs) - **Comprehensive multi-partition consumer demonstration**
- [`stream_processor.rs`](examples/stream_processor.rs) - Stream processing pipeline

### Running Examples

```bash
# Run producer example
cargo run --example simple_producer

# Run consumer example  
cargo run --example simple_consumer

# Run advanced consumer example
cargo run --example advanced_consumer

# Run comprehensive multi-partition consumer demo
cargo run --example multi_partition_consumer

# Run stream processor
cargo run --example stream_processor
```

## Troubleshooting

### Common Issues

#### Connection Errors

```rust
// Issue: Connection refused
Error: Connection("connection refused")

// Solution: Check broker configuration and network connectivity
let config = ClientConfig {
    brokers: vec!["correct-broker:9092".to_string()],
    connect_timeout: Duration::from_secs(30), // Increase timeout
    retry_config: RetryConfig {
        max_retries: 5, // More retries for unstable networks
        base_delay: Duration::from_millis(200),
        max_delay: Duration::from_secs(30),
        ..Default::default()
    },
    ..Default::default()
};

// Issue: QUIC connection failures
Error: QuicError("certificate verification failed")

// Solution: Configure TLS properly or use insecure mode for development
let config = ClientConfig {
    quic_config: QuicConfig {
        // For development only - never use in production
        insecure: true,
        // Or configure proper TLS
        server_name: Some("rustmq.example.com".to_string()),
        ..Default::default()
    },
    ..Default::default()
};

// Issue: Connection timeout during health checks
Error: Timeout { timeout_ms: 5000 }

// Solution: Increase health check timeout or check network latency
let config = ClientConfig {
    health_check_timeout: Duration::from_secs(10),
    keep_alive_interval: Duration::from_secs(60), // Less frequent checks
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