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
- [Security Guide](#security-guide)
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
- **🔐 Security**: Enterprise-grade mTLS, JWT tokens, ACL authorization, certificate management
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
rustmq-client = { path = "sdk/rust", features = ["compression"] }
```

Available features:
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

- Rust 1.88 or later
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
cargo build --features "compression"

# Check for errors
cargo check

# Run clippy linting
cargo clippy

# Format code
cargo fmt
```

### Cross-compilation

```bash

---

## 📖 Full Documentation

For detailed information, see the [Rust SDK Documentation](../../docs/client-sdks.md) in the main Documentation Hub.
