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

### Client Configuration

The Go SDK supports comprehensive configuration for production deployments:

```go
config := &rustmq.ClientConfig{
    Brokers:           []string{"localhost:9092", "localhost:9093"},
    ClientID:          "my-app",
    ConnectTimeout:    10 * time.Second,
    RequestTimeout:    30 * time.Second,
    EnableTLS:         true,
    MaxConnections:    10,
    KeepAliveInterval: 30 * time.Second,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ClientCert: "/etc/ssl/certs/client.pem", 
        ClientKey:  "/etc/ssl/private/client.key",
        ServerName: "rustmq.example.com",
    },
    RetryConfig: &rustmq.RetryConfig{
        MaxRetries: 5,
        BaseDelay:  100 * time.Millisecond,
        MaxDelay:   30 * time.Second,
        Multiplier: 2.0,
        Jitter:     true,
    },
}
```

### TLS/mTLS Configuration

The SDK provides comprehensive TLS support for secure connections:

#### Basic TLS with Server Verification

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ServerName: "rustmq.example.com",
    },
}
```

#### Mutual TLS (mTLS) with Client Certificates

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ClientCert: "/etc/ssl/certs/client.pem",
        ClientKey:  "/etc/ssl/private/client.key", 
        ServerName: "rustmq.example.com",
    },
}
```

#### Development/Testing with Insecure Skip Verify

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"localhost:9092"},
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        InsecureSkipVerify: true, // Only for development!
    },
}
```

### Connection Health Checks

The SDK includes comprehensive health checking with automatic reconnection:

#### Health Check Configuration

```go
config := &rustmq.ClientConfig{
    Brokers:           []string{"localhost:9092"},
    KeepAliveInterval: 30 * time.Second, // Health check interval
    RequestTimeout:    5 * time.Second,  // Health check timeout
}

client, err := rustmq.NewClient(config)
if err != nil {
    log.Fatal(err)
}

// Manual health check
if err := client.HealthCheck(); err != nil {
    log.Printf("Health check failed: %v", err)
}
```

#### Health Check Response Format

Health checks exchange JSON messages with brokers:

```json
{
  "status": "healthy",
  "details": {
    "broker_id": "broker-001",
    "uptime": "24h30m15s",
    "memory_usage": "45%"
  },
  "timestamp": "2023-10-15T14:30:00Z"
}
```

### Reconnection Configuration

Robust reconnection with exponential backoff and jitter:

#### Basic Reconnection Setup

```go
config := &rustmq.ClientConfig{
    Brokers: []string{"localhost:9092", "localhost:9093"},
    RetryConfig: &rustmq.RetryConfig{
        MaxRetries: 10,                    // Maximum retry attempts per broker
        BaseDelay:  100 * time.Millisecond, // Initial retry delay
        MaxDelay:   30 * time.Second,      // Maximum retry delay
        Multiplier: 2.0,                   // Exponential backoff multiplier
        Jitter:     true,                  // Add random jitter to prevent thundering herd
    },
}
```

#### Advanced Reconnection Patterns

```go
// Create client with custom reconnection strategy
client, err := rustmq.NewClient(config)
if err != nil {
    log.Fatal(err)
}

// Monitor connection status
go func() {
    ticker := time.NewTicker(5 * time.Second)
    defer ticker.Stop()
    
    for range ticker.C {
        if !client.IsConnected() {
            log.Println("Connection lost, automatic reconnection in progress...")
        }
        
        // Get detailed connection statistics
        stats := client.Stats()
        log.Printf("Active connections: %d/%d, Reconnect attempts: %d",
            stats.ActiveConnections, stats.TotalConnections, stats.ReconnectAttempts)
    }
}()
```

### Connection Statistics and Monitoring

Comprehensive statistics for monitoring and observability:

#### Available Statistics

```go
stats := client.Stats()

// Connection health
fmt.Printf("Active connections: %d/%d\n", stats.ActiveConnections, stats.TotalConnections)
fmt.Printf("Brokers: %v\n", stats.Brokers)

// Traffic statistics  
fmt.Printf("Bytes sent: %d, received: %d\n", stats.BytesSent, stats.BytesReceived)
fmt.Printf("Requests: %d, responses: %d\n", stats.RequestsSent, stats.ResponsesReceived)

// Error tracking
fmt.Printf("Total errors: %d\n", stats.Errors)

// Health monitoring
fmt.Printf("Health checks: %d (errors: %d)\n", stats.HealthChecks, stats.HealthCheckErrors)
fmt.Printf("Last heartbeat: %v\n", stats.LastHeartbeat)

// Reconnection tracking
fmt.Printf("Reconnection attempts: %d\n", stats.ReconnectAttempts)
```

#### JSON Statistics Export

```go
// Export statistics as JSON for monitoring systems
statsJSON, err := json.MarshalIndent(client.Stats(), "", "  ")
if err == nil {
    fmt.Println(string(statsJSON))
}
```

## Quick Start

### Producer Example

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Create client with TLS and retry configuration
    config := &rustmq.ClientConfig{
        Brokers:           []string{"localhost:9092", "localhost:9093"},
        ClientID:          "go-producer-example",
        ConnectTimeout:    10 * time.Second,
        RequestTimeout:    30 * time.Second,
        EnableTLS:         false, // Set to true for production
        KeepAliveInterval: 30 * time.Second,
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 5,
            BaseDelay:  100 * time.Millisecond,
            MaxDelay:   10 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Monitor connection health
    go func() {
        ticker := time.NewTicker(10 * time.Second)
        defer ticker.Stop()
        
        for range ticker.C {
            if err := client.HealthCheck(); err != nil {
                log.Printf("Health check failed: %v", err)
            } else {
                stats := client.Stats()
                log.Printf("Health check OK - Active connections: %d/%d", 
                    stats.ActiveConnections, stats.TotalConnections)
            }
        }
    }()

    // Create producer
    producer, err := client.CreateProducer("my-topic")
    if err != nil {
        log.Fatal(err)
    }
    defer producer.Close()

    // Send message with connection monitoring
    message := rustmq.NewMessage().
        Topic("my-topic").
        PayloadString("Hello, RustMQ with enhanced connection layer!").
        Header("sender", "go-client").
        Header("version", "v2.0").
        Build()

    ctx := context.Background()
    result, err := producer.Send(ctx, message)
    if err != nil {
        log.Fatal(err)
    }

    fmt.Printf("Message sent: offset=%d, partition=%d\n", result.Offset, result.Partition)
    
    // Print connection statistics
    stats := client.Stats()
    fmt.Printf("Connection stats: Sent=%d bytes, Received=%d bytes, Errors=%d\n",
        stats.BytesSent, stats.BytesReceived, stats.Errors)
}
```

### Consumer Example

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Create client with production-ready configuration
    config := &rustmq.ClientConfig{
        Brokers:           []string{"localhost:9092", "localhost:9093"},
        ClientID:          "go-consumer-example",
        ConnectTimeout:    10 * time.Second,
        RequestTimeout:    30 * time.Second,
        EnableTLS:         false, // Set to true for production
        KeepAliveInterval: 30 * time.Second,
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 5,
            BaseDelay:  100 * time.Millisecond,
            MaxDelay:   10 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Start connection monitoring
    go func() {
        ticker := time.NewTicker(30 * time.Second)
        defer ticker.Stop()
        
        for range ticker.C {
            stats := client.Stats()
            log.Printf("Connection Health: Active=%d/%d, Reconnects=%d, Health Checks=%d", 
                stats.ActiveConnections, stats.TotalConnections,
                stats.ReconnectAttempts, stats.HealthChecks)
                
            if stats.HealthCheckErrors > 0 {
                log.Printf("Health check errors detected: %d", stats.HealthCheckErrors)
            }
        }
    }()

    // Create consumer
    consumer, err := client.CreateConsumer("my-topic", "my-group")
    if err != nil {
        log.Fatal(err)
    }
    defer consumer.Close()

    // Consume messages with connection resilience
    for {
        ctx := context.Background()
        message, err := consumer.Receive(ctx)
        if err != nil {
            log.Printf("Receive error: %v", err)
            
            // Check if it's a connection issue
            if !client.IsConnected() {
                log.Println("Connection lost, waiting for automatic reconnection...")
                time.Sleep(1 * time.Second)
            }
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

### Production TLS Example

Complete example with mTLS for production environments:

```go
package main

import (
    "context"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Production TLS configuration
    config := &rustmq.ClientConfig{
        Brokers:           []string{"rustmq-prod-1.example.com:9092", "rustmq-prod-2.example.com:9092"},
        ClientID:          "production-service",
        ConnectTimeout:    15 * time.Second,
        RequestTimeout:    30 * time.Second,
        EnableTLS:         true,
        KeepAliveInterval: 30 * time.Second,
        TLSConfig: &rustmq.TLSConfig{
            CACert:     "/etc/ssl/certs/rustmq-ca.pem",
            ClientCert: "/etc/ssl/certs/client.pem",
            ClientKey:  "/etc/ssl/private/client.key",
            ServerName: "rustmq.example.com",
        },
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 10,
            BaseDelay:  200 * time.Millisecond,
            MaxDelay:   30 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }

    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatalf("Failed to create client: %v", err)
    }
    defer client.Close()

    // Verify TLS connection
    if err := client.HealthCheck(); err != nil {
        log.Fatalf("Initial health check failed: %v", err)
    }
    
    log.Println("Successfully connected with TLS")
    
    // Your application logic here...
}
```

### Connection Resilience Example

Example showing robust connection handling:

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    config := &rustmq.ClientConfig{
        Brokers: []string{"localhost:9092", "localhost:9093"},
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 5,
            BaseDelay:  100 * time.Millisecond,
            MaxDelay:   10 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }

    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Connection monitoring and alerting
    go monitorConnection(client)

    producer, err := client.CreateProducer("events")
    if err != nil {
        log.Fatal(err)
    }
    defer producer.Close()

    // Resilient message sending
    for i := 0; i < 1000; i++ {
        message := rustmq.NewMessage().
            Topic("events").
            PayloadString(fmt.Sprintf("Event %d", i)).
            Build()

        // Retry sending with exponential backoff
        err := retryWithBackoff(func() error {
            _, err := producer.Send(context.Background(), message)
            return err
        }, 3, 100*time.Millisecond)

        if err != nil {
            log.Printf("Failed to send message %d after retries: %v", i, err)
        } else {
            log.Printf("Sent message %d", i)
        }

        time.Sleep(100 * time.Millisecond)
    }
}

func monitorConnection(client *rustmq.Client) {
    ticker := time.NewTicker(10 * time.Second)
    defer ticker.Stop()

    for range ticker.C {
        stats := client.Stats()
        
        if stats.ActiveConnections == 0 {
            log.Println("ALERT: No active connections!")
        } else if stats.ActiveConnections < stats.TotalConnections {
            log.Printf("WARNING: %d/%d connections active", 
                stats.ActiveConnections, stats.TotalConnections)
        }

        // Check error rates
        if stats.Errors > 0 {
            errorRate := float64(stats.Errors) / float64(stats.RequestsSent)
            if errorRate > 0.05 { // 5% error rate threshold
                log.Printf("ALERT: High error rate: %.2f%%", errorRate*100)
            }
        }

        // Health check performance
        if stats.HealthCheckErrors > 0 {
            log.Printf("Health check issues: %d errors out of %d checks",
                stats.HealthCheckErrors, stats.HealthChecks)
        }
    }
}

func retryWithBackoff(operation func() error, maxRetries int, baseDelay time.Duration) error {
    var err error
    for i := 0; i < maxRetries; i++ {
        err = operation()
        if err == nil {
            return nil
        }

        if i < maxRetries-1 {
            delay := time.Duration(1<<uint(i)) * baseDelay
            log.Printf("Operation failed, retrying in %v: %v", delay, err)
            time.Sleep(delay)
        }
    }
    return err
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