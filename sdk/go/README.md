# RustMQ Go SDK

High-performance Go client library for RustMQ message queue system with QUIC transport.

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Installation](#installation)
- [Building](#building)
- [Configuration](#configuration)
- [Quick Start](#quick-start)
- [Usage Guide](#usage-guide)
- [Performance Tuning](#performance-tuning)
- [Best Practices](#best-practices)
- [Testing](#testing)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)
- [API Reference](#api-reference)
- [Contributing](#contributing)

## Overview

The RustMQ Go SDK provides a native, high-performance client library for interacting with RustMQ message queues. Built with modern Go patterns and QUIC transport, it offers efficient connection pooling, automatic retries, and comprehensive streaming capabilities.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    RustMQ Go SDK                           │
├─────────────────────────────────────────────────────────────┤
│  Client                                                     │
│  ├── Producer (Batching, Compression)                       │
│  ├── Consumer (Streaming, Auto-commit)                      │
│  ├── MessageStream (Real-time Processing)                   │
│  └── Connection (QUIC Pool, Health Check)                   │
├─────────────────────────────────────────────────────────────┤
│  QUIC Transport Layer (quic-go)                             │
├─────────────────────────────────────────────────────────────┤
│  RustMQ Broker Cluster                                      │
└─────────────────────────────────────────────────────────────┘
```

## Features

- **🚀 High Performance**: Built with QUIC transport for low latency
- **🔌 Connection Pooling**: Efficient connection management and reuse
- **📦 Message Batching**: Configurable batching for high throughput
- **🗜️ Compression**: Multiple compression algorithms (LZ4, Zstd, Gzip)
- **🔐 Security**: TLS/mTLS support with authentication
- **📊 Observability**: Built-in metrics and monitoring support
- **🎯 Type Safety**: Comprehensive type definitions and error handling
- **🌊 Streaming**: Real-time message processing pipelines
- **⚡ Goroutines**: Efficient concurrent processing
- **🔄 Auto-retry**: Configurable retry logic with backoff

## Installation

### From RustMQ Repository

Since this is part of the RustMQ repository, you can import it directly:

```go
import "github.com/rustmq/rustmq/sdk/go/rustmq"
```

### Go Module

Add to your `go.mod`:

```go
require github.com/rustmq/rustmq/sdk/go v0.1.0
```

### Prerequisites

- Go 1.21 or later
- Network access to RustMQ brokers

## Building

### Build Commands

```bash
# Build the SDK
go build ./...

# Run tests
go test ./...

# Run tests with coverage
go test -cover ./...

# Run benchmarks
go test -bench=. ./benchmarks/...

# Build examples
go build -o examples/producer examples/simple_producer.go
go build -o examples/consumer examples/simple_consumer.go

# Format code
go fmt ./...

# Run linter
golangci-lint run

# Generate documentation
go doc -all ./rustmq
```

### Cross-compilation

```bash
# Build for different platforms
GOOS=linux GOARCH=amd64 go build ./...
GOOS=windows GOARCH=amd64 go build ./...
GOOS=darwin GOARCH=arm64 go build ./...
```

### Build Tags

```bash
# Build with specific tags
go build -tags "debug" ./...
go build -tags "production" ./...
```

## Configuration

## Quick Start

### Producer Example

```go
package main

import (
    "context"
    "fmt"
    "log"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Create client
    config := rustmq.DefaultClientConfig()
    config.Brokers = []string{"localhost:9092"}
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Create producer
    producer, err := client.CreateProducer("my-topic")
    if err != nil {
        log.Fatal(err)
    }
    defer producer.Close()

    // Send message
    message := rustmq.NewMessage().
        Topic("my-topic").
        PayloadString("Hello, RustMQ!").
        Header("sender", "go-client").
        Build()

    ctx := context.Background()
    result, err := producer.Send(ctx, message)
    if err != nil {
        log.Fatal(err)
    }

    fmt.Printf("Message sent: offset=%d, partition=%d\n", result.Offset, result.Partition)
}
```

### Consumer Example

```go
package main

import (
    "context"
    "fmt"
    "log"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Create client
    config := rustmq.DefaultClientConfig()
    config.Brokers = []string{"localhost:9092"}
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Create consumer
    consumer, err := client.CreateConsumer("my-topic", "my-group")
    if err != nil {
        log.Fatal(err)
    }
    defer consumer.Close()

    // Consume messages
    for {
        ctx := context.Background()
        message, err := consumer.Receive(ctx)
        if err != nil {
            log.Printf("Error: %v", err)
            continue
        }

        fmt.Printf("Received: %s\n", message.Message.PayloadAsString())
        
        // Acknowledge message
        if err := message.Ack(); err != nil {
            log.Printf("Ack error: %v", err)
        }
    }
}
```

### Stream Processing Example

```go
package main

import (
    "context"
    "strings"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

// Custom processor that converts messages to uppercase
type UppercaseProcessor struct{}

func (p *UppercaseProcessor) Process(ctx context.Context, message *rustmq.Message) (*rustmq.Message, error) {
    transformed := rustmq.NewMessage().
        Topic("processed-topic").
        PayloadString(strings.ToUpper(message.PayloadAsString())).
        Header("processor", "uppercase").
        Build()
    
    return transformed, nil
}

func (p *UppercaseProcessor) OnStart(ctx context.Context) error { return nil }
func (p *UppercaseProcessor) OnStop(ctx context.Context) error { return nil }
func (p *UppercaseProcessor) OnError(ctx context.Context, message *rustmq.Message, err error) error { return nil }
func (p *UppercaseProcessor) ProcessBatch(ctx context.Context, messages []*rustmq.Message) ([]*rustmq.Message, error) {
    // Implementation for batch processing
    return nil, nil
}

func main() {
    client, _ := rustmq.NewClient(rustmq.DefaultClientConfig())
    defer client.Close()

    // Configure stream
    streamConfig := &rustmq.StreamConfig{
        InputTopics:   []string{"input-topic"},
        OutputTopic:   "output-topic",
        ConsumerGroup: "processors",
        Parallelism:   4,
    }

    // Create and start stream
    stream, _ := client.CreateStream(streamConfig)
    stream = stream.WithProcessor(&UppercaseProcessor{})
    
    ctx := context.Background()
    stream.Start(ctx)
    defer stream.Stop(ctx)

    // Stream processes messages automatically
    select {} // Keep running
}
```

## Configuration

### Client Configuration

```go
config := &rustmq.ClientConfig{
    Brokers:           []string{"localhost:9092", "localhost:9093"},
    ClientID:          "my-app",
    ConnectTimeout:    10 * time.Second,
    RequestTimeout:    30 * time.Second,
    EnableTLS:         true,
    MaxConnections:    10,
    KeepAliveInterval: 30 * time.Second,
}
```

### Producer Configuration

```go
producerConfig := &rustmq.ProducerConfig{
    BatchSize:      100,
    BatchTimeout:   100 * time.Millisecond,
    MaxMessageSize: 1024 * 1024, // 1MB
    AckLevel:       rustmq.AckAll,
    Idempotent:     true,
}

producer, err := client.CreateProducer("topic", producerConfig)
```

### Consumer Configuration

```go
consumerConfig := &rustmq.ConsumerConfig{
    ConsumerGroup:      "my-group",
    AutoCommitInterval: 5 * time.Second,
    EnableAutoCommit:   true,
    FetchSize:          100,
    StartPosition: rustmq.StartPosition{
        Type: rustmq.StartEarliest,
    },
}

consumer, err := client.CreateConsumer("topic", "group", consumerConfig)
```

## Advanced Features

### Message Building

```go
message := rustmq.NewMessage().
    Topic("events").
    KeyString("user-123").
    PayloadJSON(map[string]interface{}{
        "event": "user_login",
        "timestamp": time.Now(),
    }).
    Header("version", "1.0").
    Header("source", "auth-service").
    Build()
```

### Batch Operations

```go
// Send multiple messages
messages := []*rustmq.Message{msg1, msg2, msg3}
results, err := producer.SendBatch(ctx, messages)

// Process message batches
batch := rustmq.NewMessageBatch(messages)
smallerBatches := batch.SplitBySize(1024 * 1024) // 1MB chunks
```

### Error Handling

```go
// Configure retry strategy
streamConfig.ErrorStrategy = rustmq.ErrorStrategy{
    Type:        rustmq.ErrorStrategyRetry,
    MaxAttempts: 3,
    BackoffMs:   1000,
}

// Or use dead letter queue
streamConfig.ErrorStrategy = rustmq.ErrorStrategy{
    Type:            rustmq.ErrorStrategyDeadLetter,
    DeadLetterTopic: "failed-messages",
}
```

### Metrics and Monitoring

```go
// Producer metrics
metrics := producer.Metrics()
fmt.Printf("Sent: %d, Failed: %d, Batches: %d\n", 
    metrics.MessagesSent, metrics.MessagesFailed, metrics.BatchesSent)

// Consumer metrics  
metrics := consumer.Metrics()
fmt.Printf("Received: %d, Processed: %d, Lag: %d\n",
    metrics.MessagesReceived, metrics.MessagesProcessed, metrics.Lag)

// Connection stats
stats := client.Stats()
fmt.Printf("Active: %d/%d connections\n", 
    stats.ActiveConnections, stats.TotalConnections)
```

## Testing

Run the test suite:

```bash
# Unit tests
go test ./tests/...

# Benchmarks
go test -bench=. ./benchmarks/...

# With coverage
go test -cover ./tests/...
```

## Examples

See the `examples/` directory for complete working examples:

- `simple_producer.go` - Basic message production
- `simple_consumer.go` - Basic message consumption  
- `stream_processor.go` - Real-time stream processing

## Performance

The Go client is designed for high performance:

- **QUIC Transport**: Reduced connection overhead vs TCP
- **Connection Pooling**: Efficient connection reuse
- **Message Batching**: Configurable batching for throughput
- **Zero-Copy**: Efficient memory usage where possible
- **Async Operations**: Non-blocking message sending

## Requirements

- Go 1.21 or later
- RustMQ broker running
- Network access to broker endpoints

## License

MIT OR Apache-2.0

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## Support

- GitHub Issues: Report bugs and feature requests
- Documentation: See RustMQ main repository
- Examples: Check the examples directory