## 🧠 WebAssembly ETL Processing

RustMQ features a powerful WebAssembly (WASM) ETL system that transforms messages in real-time as they flow through the system. This allows you to process, filter, and enrich data without additional infrastructure.

### What is WASM ETL?

WASM ETL lets you write custom code in Rust (or other languages) that runs safely inside RustMQ to process messages. It's like having a tiny, secure computer program that can:

- **Transform data**: Convert temperature units, normalize emails, add timestamps
- **Filter messages**: Remove spam, block oversized messages, require certain headers  
- **Enrich content**: Add geolocation, detect language, analyze content type
- **Split or combine**: Turn one message into many, or combine multiple messages

### Key Benefits

- **Safe and Secure**: Code runs in a sandbox - can't access files, network, or harm the system
- **High Performance**: Near-native speed with efficient binary processing
- **Hot Deployment**: Update processing logic without restarting RustMQ
- **Priority-Based**: Process messages in stages with different priorities
- **Smart Filtering**: Only process messages that match specific topics or conditions

### Simple Example

Here's what a basic message processor looks like:

```rust
// Transform JSON messages to add processing info
if message.headers.get("content-type") == Some("application/json") {
    let mut json: Value = serde_json::from_slice(&message.value)?;
    json["processed_at"] = chrono::Utc::now().timestamp().into();
    json["processor"] = "my-etl-v1".into();
    message.value = serde_json::to_vec(&json)?;
}
```

### Multi-Stage Processing

Configure complex pipelines that process messages in stages:

1. **Priority 0** (First): Validate message format and required fields
2. **Priority 1** (Second): Transform data and add enrichments (can run in parallel)
3. **Priority 2** (Last): Final formatting and cleanup

### Topic Filtering

Only process messages from specific topics using patterns:

- **Exact**: `"events.user.login"` - matches exactly
- **Wildcard**: `"logs.*.error"` - matches any middle part
- **Regex**: `"^sensor-\\d+\\."` - complex pattern matching
- **Prefix/Suffix**: `"iot.devices."` or `".critical"`

### Getting Started

The complete WASM ETL system includes priority-based pipelines, instance pooling, smart filtering, and comprehensive monitoring. For detailed setup instructions, examples, and advanced configuration:

**📖 [Complete WASM ETL Deployment Guide](docs/wasm-etl-deployment-guide.md)**

This guide covers:
- Step-by-step setup and configuration
- Writing production-ready ETL modules in Rust
- Building and deploying WASM modules
- Configuring multi-stage processing pipelines
- Advanced filtering and conditional processing
- Performance optimization and monitoring
- Troubleshooting and best practices
