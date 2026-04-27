## 📦 Client SDKs

RustMQ provides official client SDKs for multiple programming languages with production-ready features and comprehensive documentation.

### 🦀 Rust SDK
- **Location**: [`sdk/rust/`](sdk/rust/)
- **Documentation**: [sdk/rust/README.md](sdk/rust/README.md) | [Admin SDK Guide](sdk/rust/ADMIN_API_CLIENT.md)
- **Status**: ✅ **Fully Implemented** - Production-ready client library with comprehensive APIs
- **Features**:
  - **Producer/Consumer APIs**: Builder pattern with intelligent batching, streaming, and offset management
  - **Admin SDK**: Complete cluster management (CA, certificates, ACL, audit logging)
  - **Async/Await**: Built on Tokio with zero-copy operations and streaming support
  - **QUIC Transport**: Modern HTTP/3 protocol for low-latency communication
  - **Enterprise Security**: mTLS authentication, JWT tokens, ACL authorization
  - **Comprehensive Error Handling**: Detailed error types with retry logic and timeout management
  - **Performance Monitoring**: Built-in metrics for messages sent/failed, batch sizes, and timing
- **Build**: `cargo build --release`
- **Install**: Add to `Cargo.toml`: `rustmq-client = { path = "sdk/rust" }`
- **Tests**: 40 tests (31 client tests + 9 admin tests) - All passing ✅

#### Producer API

The Rust SDK provides a comprehensive Producer API with intelligent batching, flush mechanisms, and production-ready features. Based on the actual implementation:

##### Basic Producer Usage

```rust
use rustmq_client::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client connection
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("my-app-producer".to_string()),
        connect_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    
    let client = RustMqClient::new(config).await?;
    
    // Create producer with custom configuration
    let producer_config = ProducerConfig {
        batch_size: 100,                           // Batch up to 100 messages
        batch_timeout: Duration::from_millis(10),  // Or send after 10ms
        ack_level: AckLevel::All,                   // Wait for all replicas
        producer_id: Some("my-app-producer".to_string()),
        ..Default::default()
    };
    
    let producer = ProducerBuilder::new()
        .topic("user-events")
        .config(producer_config)
        .client(client)
        .build()
        .await?;
    
    // Send a single message and wait for acknowledgment
    let message = Message::builder()
        .topic("user-events")
        .payload("user logged in")
        .header("user-id", "12345")
        .header("event-type", "login")
        .build()?;
    
    let result = producer.send(message).await?;
    println!("Message sent to partition {} at offset {}", 
             result.partition, result.offset);
    
    Ok(())
}
```

##### Fire-and-Forget Messages

```rust
// High-throughput fire-and-forget sending
for i in 0..1000 {
    let message = Message::builder()
        .topic("metrics")
        .payload(format!("{{\"value\": {}, \"timestamp\": {}}}", i, timestamp))
        .header("sensor-id", &format!("sensor-{}", i % 10))
        .build()?;
    
    // Returns immediately after queuing - no waiting for broker
    producer.send_async(message).await?;
}

// Flush to ensure all messages are sent
producer.flush().await?;
```

##### Batch Operations

```rust
// Prepare a batch of messages
let messages: Vec<_> = (0..50).map(|i| {
    Message::builder()
        .topic("batch-topic")
        .payload(format!("message-{}", i))
        .header("batch-id", "batch-123")
        .build().unwrap()
}).collect();

// Send batch and wait for all acknowledgments
let results = producer.send_batch(messages).await?;

for result in results {
    println!("Message {} sent to offset {}", 
             result.message_id, result.offset);
}
```

##### Producer Configuration Options

```rust
let producer_config = ProducerConfig {
    // Batching configuration
    batch_size: 100,                           // Messages per batch
    batch_timeout: Duration::from_millis(10),  // Maximum wait time
    
    // Reliability configuration  
    ack_level: AckLevel::All,                   // All, Leader, or None
    max_message_size: 1024 * 1024,             // 1MB max message size
    idempotent: true,                           // Enable idempotent producer
    
    // Producer identification
    producer_id: Some("my-producer".to_string()),
    
    // Advanced configuration
    compression: CompressionConfig {
        enabled: true,
        algorithm: CompressionAlgorithm::Lz4,
        level: 6,
        min_size: 1024,
    },
    
    default_properties: HashMap::from([
        ("app".to_string(), "my-application".to_string()),
        ("version".to_string(), "1.0.0".to_string()),
    ]),
};
```

##### Error Handling

```rust
use rustmq_sdk::error::ClientError;

match producer.send(message).await {
    Ok(result) => {
        println!("Success: {} at offset {}", result.message_id, result.offset);
    }
    Err(ClientError::Timeout { timeout_ms }) => {
        println!("Request timed out after {}ms", timeout_ms);
        // Implement retry logic
    }
    Err(ClientError::Broker(msg)) => {
        println!("Broker error: {}", msg);
        // Handle broker-side errors
    }
    Err(ClientError::MessageTooLarge { size, max_size }) => {
        println!("Message too large: {} bytes (max: {})", size, max_size);
        // Reduce message size
    }
    Err(e) => {
        println!("Other error: {}", e);
    }
}
```

##### Monitoring and Metrics

```rust
// Get producer performance metrics
let metrics = producer.metrics().await;

println!("Messages sent: {}", 
         metrics.messages_sent.load(std::sync::atomic::Ordering::Relaxed));
println!("Messages failed: {}", 
         metrics.messages_failed.load(std::sync::atomic::Ordering::Relaxed));
println!("Batches sent: {}", 
         metrics.batches_sent.load(std::sync::atomic::Ordering::Relaxed));
println!("Average batch size: {:.2}", 
         *metrics.average_batch_size.read().await);

if let Some(last_send) = *metrics.last_send_time.read().await {
    println!("Last send: {:?} ago", last_send.elapsed());
}
```

##### Graceful Shutdown

```rust
// Proper producer shutdown
async fn shutdown_producer(producer: Producer) -> Result<(), ClientError> {
    // Flush all pending messages
    producer.flush().await?;
    
    // Close producer and cleanup resources
    producer.close().await?;
    
    println!("Producer shut down gracefully");
    Ok(())
}
```

### 🐹 Go SDK
- **Location**: [`sdk/go/`](sdk/go/)
- **Documentation**: [sdk/go/README.md](sdk/go/README.md)
- **Status**: ✅ **Fully Implemented** - Production-ready client library with comprehensive APIs
- **Features**:
  - **Producer/Consumer APIs**: QUIC transport with intelligent batching, streaming, and connection management
  - **Admin SDK**: Complete cluster management (CA, certificates, ACL, audit logging)
  - **Advanced Connection Management**: Connection pooling, round-robin load balancing, and automatic failover
  - **Comprehensive TLS/mTLS Support**: Full client certificate authentication with CA validation
  - **Health Check System**: Real-time broker health monitoring with automatic cleanup
  - **Robust Reconnection Logic**: Exponential backoff with jitter, per-broker state tracking
  - **Extensive Statistics**: Connection metrics, health check tracking, error monitoring, traffic analytics
  - **Production-Ready Features**: Concurrent-safe operations, goroutine-based processing, comprehensive error handling
- **Build**: `go build ./...`
- **Install**: `import "github.com/rustmq/rustmq/sdk/go/rustmq"`
- **Tests**: 75+ tests (client + security + admin tests) - All passing ✅

### Go SDK Connection Layer Highlights

The Go SDK features a sophisticated connection management system designed for production environments:

#### TLS/mTLS Configuration
```go
config := &rustmq.ClientConfig{
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ClientCert: "/etc/ssl/certs/client.pem",
        ClientKey:  "/etc/ssl/private/client.key",
        ServerName: "rustmq.example.com",
    },
}
```

#### Health Check & Reconnection
```go
// Automatic health monitoring with configurable intervals
config.KeepAliveInterval = 30 * time.Second

// Exponential backoff with jitter for reconnection
config.RetryConfig = &rustmq.RetryConfig{
    MaxRetries: 10,
    BaseDelay:  100 * time.Millisecond,
    MaxDelay:   30 * time.Second,
    Multiplier: 2.0,
    Jitter:     true,
}
```

#### Producer with Intelligent Batching
```go
// Create producer with batching configuration
producerConfig := &rustmq.ProducerConfig{
    BatchSize:    100,
    BatchTimeout: 100 * time.Millisecond,
    AckLevel:     rustmq.AckAll,
    Idempotent:   true,
}

producer, err := client.CreateProducer("topic", producerConfig)
if err != nil {
    log.Fatal(err)
}

// Send message with automatic batching
result, err := producer.Send(ctx, message)
```

#### Connection Statistics
```go
stats := client.Stats()
fmt.Printf("Active: %d/%d, Reconnects: %d, Health Checks: %d", 
    stats.ActiveConnections, stats.TotalConnections,
    stats.ReconnectAttempts, stats.HealthChecks)

// Additional statistics available
fmt.Printf("Bytes: Sent=%d, Received=%d, Errors=%d", 
    stats.BytesSent, stats.BytesReceived, stats.Errors)
```

### Common SDK Features
- **QUIC/HTTP3 Transport**: Low-latency, multiplexed connections
- **Producer APIs**: Sync/async sending, batching, compression
- **Consumer APIs**: Auto-commit, manual offset management, consumer groups
- **Stream Processing**: Real-time message transformation pipelines
- **Configuration**: Comprehensive client, producer, consumer settings
- **Monitoring**: Built-in metrics, health checks, observability
- **Error Handling**: Retry logic, circuit breakers, dead letter queues
- **Security**: TLS/mTLS, authentication, authorization

### Quick Start

#### Rust SDK
```bash
cd sdk/rust

# Basic producer example
cargo run -p rustmq-client --example simple_producer

# Advanced consumer with multi-partition support
cargo run -p rustmq-client --example advanced_consumer

# Stream processing example
cargo run -p rustmq-client --example stream_processor
```

#### Go SDK
```bash
cd sdk/go

# Basic producer example
go run examples/simple_producer.go

# Basic consumer example
go run examples/simple_consumer.go

# Advanced stream processing
go run examples/advanced_stream_processor.go
```

See individual SDK READMEs for detailed usage, configuration, performance tuning, and API documentation.

## 📚 Usage Examples

### Client Examples

**Note**: The following are examples of the intended client API. Current implementation is in early development stage and these clients are not yet available.

#### Rust Client Example

```rust
// Cargo.toml
[dependencies]
rustmq-client = { path = "sdk/rust" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }

// main.rs
use rustmq_client::*;
use serde::{Serialize, Deserialize};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct OrderEvent {
    order_id: String,
    customer_id: String,
    amount: f64,
    timestamp: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client connection
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("order-processor".to_string()),
        ..Default::default()
    };
    
    let client = RustMqClient::new(config).await?;
    
    // Create producer
    let producer = ProducerBuilder::new()
        .topic("orders")
        .client(client.clone())
        .build()
        .await?;

    // Produce messages
    let order = OrderEvent {
        order_id: "order-123".to_string(),
        customer_id: "customer-456".to_string(),
        amount: 99.99,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };

    let message = Message::builder()
        .topic("orders")
        .key(&order.order_id)
        .payload(serde_json::to_vec(&order)?)
        .header("content-type", "application/json")
        .build()?;

    let result = producer.send(message).await?;
    println!("Message produced at offset: {}", result.offset);

    // Create consumer
    let consumer = ConsumerBuilder::new()
        .topic("orders")
        .consumer_group("order-processors")
        .client(client)
        .build()
        .await?;
    
    // Consume with automatic offset management
    while let Some(consumer_message) = consumer.receive().await? {
        let message = &consumer_message.message;
        let order: OrderEvent = serde_json::from_slice(&message.payload)?;
        
        // Process the order
        process_order(order).await?;
        
        // Acknowledge message
        consumer_message.ack().await?;
    }
    
    Ok(())
}

async fn process_order(order: OrderEvent) -> Result<(), Box<dyn std::error::Error>> {
    println!("Processing order {} for customer {} amount ${}", 
             order.order_id, order.customer_id, order.amount);
    
    // Your business logic here
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    Ok(())
}
```

#### Go Client Example

```go
// go.mod
module rustmq-example

go 1.21

require (
    github.com/rustmq/rustmq/sdk/go v0.1.0
    github.com/google/uuid v1.3.0
)

// main.go
package main

import (
    "context"
    "encoding/json"
    "fmt"
    "log"
    "time"

    "github.com/google/uuid"
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

type OrderEvent struct {
    OrderID    string  `json:"order_id"`
    CustomerID string  `json:"customer_id"`
    Amount     float64 `json:"amount"`
    Timestamp  int64   `json:"timestamp"`
}

func main() {
    // Create client configuration
    config := &rustmq.ClientConfig{
        Brokers:  []string{"localhost:9092"},
        ClientID: "order-processor",
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal("Failed to create client:", err)
    }
    defer client.Close()

    // Producer example
    producer, err := client.CreateProducer("orders")
    if err != nil {
        log.Fatal("Failed to create producer:", err)
    }
    defer producer.Close()

    // Send some orders
    for i := 0; i < 10; i++ {
        order := OrderEvent{
            OrderID:    uuid.New().String(),
            CustomerID: fmt.Sprintf("customer-%d", i%5),
            Amount:     float64((i + 1) * 25),
            Timestamp:  time.Now().UnixMilli(),
        }

        orderBytes, _ := json.Marshal(order)
        
        message := rustmq.NewMessage().
            Topic("orders").
            KeyString(order.OrderID).
            Payload(orderBytes).
            Header("content-type", "application/json").
            Build()

        ctx := context.Background()
        result, err := producer.Send(ctx, message)
        if err != nil {
            log.Printf("Failed to send message: %v", err)
            continue
        }
        
        fmt.Printf("Message sent at offset: %d, partition: %d\n", 
            result.Offset, result.Partition)
    }

    // Consumer example
    consumer, err := client.CreateConsumer("orders", "order-processors")
    if err != nil {
        log.Fatal("Failed to create consumer:", err)
    }
    defer consumer.Close()

    // Consume messages
    for i := 0; i < 10; i++ {
        ctx := context.Background()
        message, err := consumer.Receive(ctx)
        if err != nil {
            log.Printf("Receive error: %v", err)
            continue
        }

        var order OrderEvent
        if err := json.Unmarshal(message.Message.Payload, &order); err != nil {
            log.Printf("Failed to unmarshal order: %v", err)
            message.Ack()
            continue
        }

        // Process the order
        if err := processOrder(order); err != nil {
            log.Printf("Failed to process order %s: %v", order.OrderID, err)
            message.Nack() // Retry
            continue
        }

        fmt.Printf("Processed order %s for customer %s amount $%.2f\n",
            order.OrderID, order.CustomerID, order.Amount)
            
        // Acknowledge successful processing
        message.Ack()
    }
}

func processOrder(order OrderEvent) error {
    // Your business logic here
    time.Sleep(100 * time.Millisecond)
    return nil
}
```

#### Admin Operations

**✅ Fully Implemented**: The Admin REST API is production-ready with comprehensive cluster management capabilities.

```bash
# Create topic with custom configuration
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{
    "name": "user-events",
    "partitions": 24,
    "replication_factor": 3,
    "retention_ms": 604800000,
    "segment_bytes": 1073741824,
    "compression_type": "lz4"
  }'

# List topics
curl http://localhost:8080/api/v1/topics

# Get topic details
curl http://localhost:8080/api/v1/topics/user-events

# Delete topic
curl -X DELETE http://localhost:8080/api/v1/topics/user-events

# Get cluster health and status
curl http://localhost:8080/api/v1/cluster

# List brokers with health status
curl http://localhost:8080/api/v1/brokers

# Check service health and uptime
curl http://localhost:8080/health

# Advanced features (future implementation)
# Partition rebalancing, ETL module management, and metrics endpoints
# will be available in future releases
```
