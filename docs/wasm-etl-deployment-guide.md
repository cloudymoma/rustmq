# WebAssembly ETL Deployment Guide

This guide provides step-by-step instructions for deploying WebAssembly ETL modules in RustMQ for single message processing. WebAssembly ETL enables secure, sandboxed real-time data transformation and processing with near-native performance.

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Step 1: Configure RustMQ for WASM ETL](#step-1-configure-rustmq-for-wasm-etl)
4. [Step 2: Create a WASM ETL Module](#step-2-create-a-wasm-etl-module)
5. [Step 3: Build the WASM Module](#step-3-build-the-wasm-module)
6. [Step 4: Deploy the WASM Module](#step-4-deploy-the-wasm-module)
7. [Step 5: Configure Message Processing](#step-5-configure-message-processing)
8. [Step 6: Test the ETL Pipeline](#step-6-test-the-etl-pipeline)
9. [Advanced Configuration](#advanced-configuration)
10. [Troubleshooting](#troubleshooting)
11. [Best Practices](#best-practices)

## Overview

RustMQ's WebAssembly ETL system allows you to:
- Transform messages in real-time as they flow through topics
- Apply filtering, enrichment, and validation logic
- Execute secure, sandboxed code with limited resource access
- Process messages with minimal latency overhead
- Deploy updates without restarting the broker

## Prerequisites

Before starting, ensure you have:
- RustMQ broker running with WASM ETL enabled
- Rust toolchain with `wasm32-unknown-unknown` target
- `wasm-pack` for building WebAssembly modules
- Admin access to RustMQ cluster

### Install Required Tools

```bash
# Install Rust if not already installed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WebAssembly target
rustup target add wasm32-unknown-unknown

# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

## Step 1: Configure RustMQ for WASM ETL

### 1.1 Update Broker Configuration

Edit your broker configuration file (e.g., `config/broker.toml`):

```toml
[etl]
# Enable WebAssembly ETL processing
enabled = true

# Memory limit per WASM execution (64MB)
memory_limit_bytes = 67108864

# Maximum execution time for ETL operations (5 seconds)
execution_timeout_ms = 5000

# Maximum concurrent WASM executions
max_concurrent_executions = 100

# Path to store WASM modules (optional, defaults to temp)
modules_path = "/var/lib/rustmq/wasm"
```

### 1.2 Restart Broker

```bash
# Restart the broker to apply configuration changes
sudo systemctl restart rustmq-broker

# Or if running directly
cargo run --bin rustmq-broker -- --config config/broker.toml
```

### 1.3 Verify ETL is Enabled

```bash
# Check broker status via admin API
curl -s http://localhost:9094/api/v1/status | jq '.etl.enabled'
# Should return: true
```

## Step 2: Create a WASM ETL Module

### 2.1 Initialize a New Rust Project

```bash
# Create a new Rust library project
cargo new --lib message-processor
cd message-processor
```

### 2.2 Configure Cargo.toml

```toml
[package]
name = "message-processor"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# WebAssembly System Interface
wasi = "0.11"

# JSON processing
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Optional: logging (WASM-compatible)
log = "0.4"

[dependencies.web-sys]
version = "0.3"
features = [
  "console",
]

# Development dependencies for local testing
[dev-dependencies]
wasm-bindgen-test = "0.3"
```

### 2.3 Create the ETL Module

Create `src/lib.rs` with your message processing logic:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Define the message structure that RustMQ will pass to your ETL
#[derive(Deserialize, Serialize, Debug)]
pub struct Message {
    pub key: Option<String>,
    pub value: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub timestamp: i64,
}

// Define the result structure your ETL returns
#[derive(Serialize, Debug)]
pub struct ProcessResult {
    pub transformed_messages: Vec<Message>,
    pub should_continue: bool,
    pub error: Option<String>,
}

// Main ETL processing function - this is called by RustMQ for each message
#[no_mangle]
pub extern "C" fn process_message(input_ptr: *const u8, input_len: usize) -> *mut u8 {
    // Parse input message from RustMQ
    let input_slice = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    
    let message: Message = match serde_json::from_slice(input_slice) {
        Ok(msg) => msg,
        Err(e) => {
            let error_result = ProcessResult {
                transformed_messages: vec![],
                should_continue: false,
                error: Some(format!("Failed to parse message: {}", e)),
            };
            return serialize_result(error_result);
        }
    };

    // Your custom message processing logic here
    match transform_message(message) {
        Ok(result) => serialize_result(result),
        Err(e) => {
            let error_result = ProcessResult {
                transformed_messages: vec![],
                should_continue: false,
                error: Some(e),
            };
            serialize_result(error_result)
        }
    }
}

// Your custom transformation logic
fn transform_message(mut message: Message) -> Result<ProcessResult, String> {
    // Example 1: Add timestamp header
    message.headers.insert(
        "processed_at".to_string(),
        chrono::Utc::now().timestamp().to_string(),
    );

    // Example 2: Transform JSON messages
    if let Some(content_type) = message.headers.get("content-type") {
        if content_type == "application/json" {
            message = transform_json_message(message)?;
        }
    }

    // Example 3: Filter messages based on criteria
    if should_filter_message(&message) {
        return Ok(ProcessResult {
            transformed_messages: vec![], // Empty = message filtered out
            should_continue: true,
            error: None,
        });
    }

    // Example 4: Enrich message with additional data
    message = enrich_message(message)?;

    // Return the transformed message
    Ok(ProcessResult {
        transformed_messages: vec![message],
        should_continue: true,
        error: None,
    })
}

fn transform_json_message(mut message: Message) -> Result<Message, String> {
    // Parse JSON from message value
    let mut json_value: serde_json::Value = serde_json::from_slice(&message.value)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    // Example transformation: add processing metadata
    if let serde_json::Value::Object(ref mut obj) = json_value {
        obj.insert("_processed".to_string(), serde_json::Value::Bool(true));
        obj.insert("_processor_version".to_string(), serde_json::Value::String("1.0.0".to_string()));
        
        // Example: convert temperature from Celsius to Fahrenheit
        if let Some(temp_c) = obj.get("temperature_celsius").and_then(|v| v.as_f64()) {
            let temp_f = (temp_c * 9.0 / 5.0) + 32.0;
            obj.insert("temperature_fahrenheit".to_string(), serde_json::Value::Number(
                serde_json::Number::from_f64(temp_f).unwrap()
            ));
        }
    }

    // Serialize back to bytes
    message.value = serde_json::to_vec(&json_value)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

    Ok(message)
}

fn should_filter_message(message: &Message) -> bool {
    // Example filtering logic
    
    // Filter out messages without required headers
    if !message.headers.contains_key("source") {
        return true;
    }

    // Filter based on message size
    if message.value.len() > 1024 * 1024 { // 1MB limit
        return true;
    }

    // Filter based on content
    if let Ok(text) = std::str::from_utf8(&message.value) {
        if text.contains("SPAM") || text.contains("DELETE") {
            return true;
        }
    }

    false
}

fn enrich_message(mut message: Message) -> Result<Message, String> {
    // Add enrichment headers
    message.headers.insert("enriched".to_string(), "true".to_string());
    message.headers.insert("processor".to_string(), "message-processor-v1".to_string());

    // Example: add geolocation data based on IP header
    if let Some(ip) = message.headers.get("client_ip") {
        if let Ok(geo_data) = lookup_geolocation(ip) {
            message.headers.insert("geo_country".to_string(), geo_data.country);
            message.headers.insert("geo_city".to_string(), geo_data.city);
        }
    }

    Ok(message)
}

// Mock geolocation lookup (replace with actual service)
struct GeoData {
    country: String,
    city: String,
}

fn lookup_geolocation(ip: &str) -> Result<GeoData, String> {
    // In a real implementation, you might call an external service
    // For this example, we'll use simple IP prefix matching
    if ip.starts_with("192.168.") || ip.starts_with("10.") {
        Ok(GeoData {
            country: "Local".to_string(),
            city: "Private Network".to_string(),
        })
    } else if ip.starts_with("8.8.") {
        Ok(GeoData {
            country: "US".to_string(),
            city: "Mountain View".to_string(),
        })
    } else {
        Ok(GeoData {
            country: "Unknown".to_string(),
            city: "Unknown".to_string(),
        })
    }
}

// Helper function to serialize and return result
fn serialize_result(result: ProcessResult) -> *mut u8 {
    let serialized = match serde_json::to_vec(&result) {
        Ok(data) => data,
        Err(_) => {
            // Fallback error result
            let error_result = ProcessResult {
                transformed_messages: vec![],
                should_continue: false,
                error: Some("Serialization failed".to_string()),
            };
            serde_json::to_vec(&error_result).unwrap_or_else(|_| b"{}".to_vec())
        }
    };

    // Allocate memory in WASM linear memory and return pointer
    let ptr = serialized.as_ptr() as *mut u8;
    std::mem::forget(serialized); // Prevent deallocation
    ptr
}

// Memory management functions required by RustMQ
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    let layout = std::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_transformation() {
        let mut message = Message {
            key: Some("test-key".to_string()),
            value: r#"{"temperature_celsius": 25.0, "sensor_id": "temp001"}"#.as_bytes().to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h.insert("source".to_string(), "sensor-network".to_string());
                h
            },
            timestamp: 1640995200000,
        };

        let result = transform_message(message).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);
        
        let transformed = &result.transformed_messages[0];
        let json: serde_json::Value = serde_json::from_slice(&transformed.value).unwrap();
        
        assert!(json.get("_processed").unwrap().as_bool().unwrap());
        assert!(json.get("temperature_fahrenheit").is_some());
        assert_eq!(json.get("temperature_fahrenheit").unwrap().as_f64().unwrap(), 77.0);
    }

    #[test]
    fn test_message_filtering() {
        let message = Message {
            key: Some("spam-key".to_string()),
            value: b"This is SPAM content".to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("source".to_string(), "unknown".to_string());
                h
            },
            timestamp: 1640995200000,
        };

        assert!(should_filter_message(&message));
    }

    #[test]
    fn test_message_enrichment() {
        let message = Message {
            key: Some("test-key".to_string()),
            value: b"test message".to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("client_ip".to_string(), "192.168.1.1".to_string());
                h
            },
            timestamp: 1640995200000,
        };

        let enriched = enrich_message(message).unwrap();
        assert_eq!(enriched.headers.get("enriched").unwrap(), "true");
        assert_eq!(enriched.headers.get("geo_country").unwrap(), "Local");
    }
}
```

## Step 3: Build the WASM Module

### 3.1 Build for WebAssembly

```bash
# Build the WASM module
wasm-pack build --target web --out-dir pkg

# Or build directly with cargo
cargo build --target wasm32-unknown-unknown --release
```

### 3.2 Optimize the WASM Module (Optional)

```bash
# Install wasm-opt for size optimization
npm install -g wasm-opt

# Optimize the WASM file
wasm-opt --strip-debug --strip-producers -Oz \
  target/wasm32-unknown-unknown/release/message_processor.wasm \
  -o message_processor_optimized.wasm
```

## Step 4: Deploy the WASM Module

### 4.1 Upload the WASM Module

```bash
# Upload the WASM module to RustMQ
curl -X POST http://localhost:9094/api/v1/etl/modules \
  -H "Content-Type: application/octet-stream" \
  -H "X-Module-Name: message-processor" \
  -H "X-Module-Version: 1.0.0" \
  --data-binary @target/wasm32-unknown-unknown/release/message_processor.wasm

# Response should indicate successful upload
# {"status": "uploaded", "module_id": "message-processor-v1.0.0", "size_bytes": 12345}
```

### 4.2 Verify Module Upload

```bash
# List available WASM modules
curl -s http://localhost:9094/api/v1/etl/modules | jq '.'

# Get specific module info
curl -s http://localhost:9094/api/v1/etl/modules/message-processor-v1.0.0 | jq '.'
```

## Step 5: Configure Message Processing

### 5.1 Create ETL Pipeline Configuration

Create `etl-pipeline.json`:

```json
{
  "pipeline_name": "message-transformation",
  "description": "Transform and enrich incoming messages",
  "source_topic": "raw-events",
  "destination_topic": "processed-events",
  "error_topic": "etl-errors",
  "module_name": "message-processor",
  "module_version": "1.0.0",
  "configuration": {
    "batch_size": 1,
    "processing_mode": "single_message",
    "timeout_ms": 1000,
    "retry_policy": {
      "max_retries": 3,
      "backoff_ms": 100
    }
  },
  "enabled": true
}
```

### 5.2 Deploy the Pipeline

```bash
# Create the ETL pipeline
curl -X POST http://localhost:9094/api/v1/etl/pipelines \
  -H "Content-Type: application/json" \
  -d @etl-pipeline.json

# Response: {"pipeline_id": "pipeline-123", "status": "created"}
```

### 5.3 Start the Pipeline

```bash
# Start the ETL pipeline
curl -X POST http://localhost:9094/api/v1/etl/pipelines/pipeline-123/start

# Check pipeline status
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/status | jq '.'
```

## Step 6: Test the ETL Pipeline

### 6.1 Send Test Messages

```bash
# Send a test message to the source topic
curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
  -H "Content-Type: application/json" \
  -d '{
    "key": "sensor-001",
    "value": "{\"temperature_celsius\": 23.5, \"humidity\": 65.2, \"sensor_id\": \"env001\"}",
    "headers": {
      "content-type": "application/json",
      "source": "iot-sensors",
      "client_ip": "192.168.1.100"
    }
  }'
```

### 6.2 Verify Processed Messages

```bash
# Consume messages from the destination topic
curl -s "http://localhost:9092/api/v1/topics/processed-events/messages?offset=0&max_messages=10" | jq '.'

# Expected output should show transformed message with:
# - temperature_fahrenheit field added
# - _processed: true
# - enrichment headers
# - processing timestamps
```

### 6.3 Monitor Pipeline Metrics

```bash
# Get ETL pipeline metrics
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/metrics | jq '.'

# Expected metrics:
# {
#   "messages_processed": 1,
#   "messages_filtered": 0,
#   "messages_failed": 0,
#   "avg_processing_time_ms": 2.5,
#   "throughput_msgs_per_sec": 100.0
# }
```

## Advanced Configuration

### Multiple Message Output

Your ETL module can output multiple messages from a single input:

```rust
fn transform_message(message: Message) -> Result<ProcessResult, String> {
    let mut results = vec![];
    
    // Split a batch message into individual messages
    if message.headers.get("type") == Some(&"batch".to_string()) {
        let batch_data: Vec<serde_json::Value> = serde_json::from_slice(&message.value)?;
        
        for (i, item) in batch_data.into_iter().enumerate() {
            let mut new_message = Message {
                key: Some(format!("{}-{}", message.key.unwrap_or_default(), i)),
                value: serde_json::to_vec(&item)?,
                headers: message.headers.clone(),
                timestamp: message.timestamp,
            };
            new_message.headers.insert("batch_index".to_string(), i.to_string());
            results.push(new_message);
        }
    } else {
        results.push(message);
    }
    
    Ok(ProcessResult {
        transformed_messages: results,
        should_continue: true,
        error: None,
    })
}
```

### Error Handling and Dead Letter Queue

```rust
fn transform_message(message: Message) -> Result<ProcessResult, String> {
    // Validate message structure
    if message.value.is_empty() {
        return Ok(ProcessResult {
            transformed_messages: vec![],
            should_continue: true,
            error: Some("Empty message body".to_string()),
        });
    }
    
    // Try processing with fallback
    match risky_processing(&message) {
        Ok(processed) => Ok(ProcessResult {
            transformed_messages: vec![processed],
            should_continue: true,
            error: None,
        }),
        Err(e) => {
            // Send to dead letter queue with error context
            let mut error_message = message.clone();
            error_message.headers.insert("error".to_string(), e.clone());
            error_message.headers.insert("failed_at".to_string(), 
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string());
            
            Ok(ProcessResult {
                transformed_messages: vec![error_message],
                should_continue: true,
                error: Some(format!("Processing failed: {}", e)),
            })
        }
    }
}
```

### State Management (Limited)

WASM modules are stateless between invocations, but you can use static variables for simple state:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static MESSAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn transform_message(mut message: Message) -> Result<ProcessResult, String> {
    let count = MESSAGE_COUNTER.fetch_add(1, Ordering::SeqCst);
    
    // Add sequence number to message
    message.headers.insert("sequence".to_string(), count.to_string());
    
    // Reset counter periodically
    if count % 10000 == 0 {
        MESSAGE_COUNTER.store(0, Ordering::SeqCst);
    }
    
    Ok(ProcessResult {
        transformed_messages: vec![message],
        should_continue: true,
        error: None,
    })
}
```

## Troubleshooting

### Common Issues

#### 1. Module Upload Fails

```bash
# Check module size (should be < 50MB)
ls -lh target/wasm32-unknown-unknown/release/message_processor.wasm

# Verify WASM format
file target/wasm32-unknown-unknown/release/message_processor.wasm
# Should output: "WebAssembly (wasm) binary module"

# Check broker logs
tail -f /var/log/rustmq/broker.log | grep -i "etl\|wasm"
```

#### 2. Pipeline Not Processing Messages

```bash
# Check pipeline status
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/status

# Verify source topic has messages
curl -s "http://localhost:9092/api/v1/topics/raw-events/messages?offset=0&max_messages=1"

# Check broker ETL metrics
curl -s http://localhost:9094/api/v1/metrics | jq '.etl'
```

#### 3. Processing Errors

```bash
# Check error logs
curl -s "http://localhost:9092/api/v1/topics/etl-errors/messages?offset=0&max_messages=10" | jq '.'

# Monitor processing metrics
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/metrics | jq '.messages_failed'

# Enable debug logging in broker config
[logging]
level = "debug"
targets = ["rustmq::etl"]
```

#### 4. Performance Issues

```bash
# Check WASM execution times
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/metrics | jq '.avg_processing_time_ms'

# Monitor memory usage
curl -s http://localhost:9094/api/v1/metrics | jq '.etl.memory_usage_bytes'

# Adjust configuration for better performance
curl -X PATCH http://localhost:9094/api/v1/etl/pipelines/pipeline-123 \
  -H "Content-Type: application/json" \
  -d '{"configuration": {"timeout_ms": 500, "batch_size": 10}}'
```

## Best Practices

### 1. Performance Optimization

- **Keep modules small**: Target < 1MB for optimal loading
- **Minimize allocations**: Reuse data structures where possible
- **Avoid expensive operations**: No network calls, file I/O, or crypto
- **Use efficient serialization**: Consider binary formats for large messages

### 2. Error Handling

- **Always handle errors gracefully**: Return error information rather than panicking
- **Validate input data**: Check message structure before processing
- **Use timeouts**: Set reasonable execution time limits
- **Log meaningful errors**: Include context for debugging

### 3. Security

- **Validate all inputs**: Never trust message content
- **Limit resource usage**: Respect memory and CPU limits
- **Avoid unsafe operations**: Stick to safe Rust patterns
- **Test thoroughly**: Include edge cases and malformed data

### 4. Development Workflow

```bash
# Use a development script for rapid iteration
#!/bin/bash
# build-and-deploy.sh

set -e

echo "Building WASM module..."
cargo build --target wasm32-unknown-unknown --release

echo "Uploading module..."
curl -X POST http://localhost:9094/api/v1/etl/modules \
  -H "Content-Type: application/octet-stream" \
  -H "X-Module-Name: message-processor" \
  -H "X-Module-Version: $(date +%s)" \
  --data-binary @target/wasm32-unknown-unknown/release/message_processor.wasm

echo "Testing with sample message..."
curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
  -H "Content-Type: application/json" \
  -d '{
    "key": "test",
    "value": "{\"test\": true}",
    "headers": {"content-type": "application/json", "source": "test"}
  }'

echo "Done!"
```

### 5. Monitoring and Observability

```rust
// Add metrics to your ETL module
static PROCESSED_COUNT: AtomicU64 = AtomicU64::new(0);
static ERROR_COUNT: AtomicU64 = AtomicU64::new(0);

fn transform_message(message: Message) -> Result<ProcessResult, String> {
    let start_time = std::time::Instant::now();
    
    let result = match do_transform(&message) {
        Ok(transformed) => {
            PROCESSED_COUNT.fetch_add(1, Ordering::SeqCst);
            ProcessResult {
                transformed_messages: vec![transformed],
                should_continue: true,
                error: None,
            }
        }
        Err(e) => {
            ERROR_COUNT.fetch_add(1, Ordering::SeqCst);
            ProcessResult {
                transformed_messages: vec![],
                should_continue: true,
                error: Some(e),
            }
        }
    };
    
    let duration = start_time.elapsed();
    // Log performance metrics (if logging is available)
    
    Ok(result)
}
```

## Conclusion

This guide provides a comprehensive foundation for deploying WebAssembly ETL modules in RustMQ. The combination of Rust's safety, WebAssembly's sandboxing, and RustMQ's performance enables powerful real-time message processing capabilities.

For additional examples and advanced patterns, see:
- [examples/wasm-etl/](../examples/wasm-etl/) - Complete example projects
- [API Reference](./api-reference.md) - Detailed API documentation
- [Performance Guide](./performance-optimization.md) - Advanced optimization techniques