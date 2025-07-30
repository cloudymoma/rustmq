# WebAssembly ETL Examples

This directory contains example WebAssembly ETL modules for RustMQ, demonstrating various message processing patterns and techniques.

## Examples

### message-processor

A comprehensive example showing:
- JSON message transformation (temperature unit conversion, email normalization)
- Text message processing (uppercase transformation)
- Message filtering (spam detection, size limits)
- Message enrichment (geolocation, content analysis)
- Error handling and validation
- Comprehensive test coverage

**Features:**
- Multiple content type handling (JSON, text, binary)
- Geolocation enrichment based on IP headers
- Language detection
- Content type detection
- Processing metadata injection

## Quick Start

1. **Build the example:**
   ```bash
   cd message-processor
   ./build-and-deploy.sh
   ```

2. **Create a test pipeline:**
   ```bash
   curl -X POST http://localhost:9094/api/v1/etl/pipelines \
     -H "Content-Type: application/json" \
     -d '{
       "pipeline_name": "example-transformation",
       "source_topic": "raw-events",
       "destination_topic": "processed-events",
       "module_name": "message-processor",
       "module_version": "latest",
       "enabled": true
     }'
   ```

3. **Send test messages:**
   ```bash
   # JSON message
   curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
     -H "Content-Type: application/json" \
     -d '{
       "key": "sensor-001",
       "value": "{\"temperature_celsius\": 23.5, \"email\": \"USER@EXAMPLE.COM\"}",
       "headers": {
         "content-type": "application/json",
         "source": "iot-sensors",
         "client_ip": "192.168.1.100"
       }
     }'

   # Text message
   curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
     -H "Content-Type: application/json" \
     -d '{
       "key": "text-001",
       "value": "hello world from the sensor",
       "headers": {
         "content-type": "text/plain",
         "source": "user-input"
       }
     }'
   ```

4. **Check processed results:**
   ```bash
   curl -s "http://localhost:9092/api/v1/topics/processed-events/messages?offset=0&max_messages=10" | jq '.'
   ```

## Development Patterns

### Testing Your Module

Run the built-in tests:
```bash
cd message-processor
cargo test
```

### Local Development

For rapid iteration during development:
```bash
# Watch for changes and rebuild
cargo watch -x "build --target wasm32-unknown-unknown"

# Or use the build script with a local broker
./build-and-deploy.sh http://localhost:9094
```

### Performance Optimization

- Keep WASM modules under 1MB for optimal performance
- Use `wasm-opt` for size optimization
- Profile critical paths with `cargo bench`
- Monitor processing metrics via the admin API

### Debugging

Enable debug logging in your broker configuration:
```toml
[logging]
level = "debug"
targets = ["rustmq::etl"]
```

Check processing metrics:
```bash
curl -s http://localhost:9094/api/v1/etl/pipelines/your-pipeline-id/metrics | jq '.'
```

## Additional Resources

- [Main WASM ETL Deployment Guide](../../docs/wasm-etl-deployment-guide.md)
- [RustMQ API Reference](../../docs/api-reference.md)
- [Performance Optimization Guide](../../docs/performance-optimization.md)