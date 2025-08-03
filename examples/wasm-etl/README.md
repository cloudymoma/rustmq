# WebAssembly ETL Examples

WebAssembly ETL modules for RustMQ demonstrating real-time message processing patterns.

## message-processor

Production-ready ETL module featuring:
- **JSON transformation**: Temperature conversion, email normalization, coordinate formatting
- **Text processing**: Uppercase transformation with metadata injection
- **Message filtering**: Spam detection, size limits, age-based filtering
- **Content enrichment**: Geolocation lookup, language detection, content analysis
- **Multi-format support**: JSON, text, binary with automatic type detection

## Quick Start

1. **Build and deploy:**
   ```bash
   cd message-processor && ./build-and-deploy.sh
   ```

2. **Create pipeline:**
   ```bash
   curl -X POST http://localhost:9094/api/v1/etl/pipelines \
     -H "Content-Type: application/json" \
     -d '{"pipeline_name": "example-transformation", "source_topic": "raw-events", "destination_topic": "processed-events", "module_name": "message-processor", "module_version": "latest", "enabled": true}'
   ```

3. **Send test data:**
   ```bash
   # JSON with temperature and email
   curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
     -H "Content-Type: application/json" \
     -d '{"key": "sensor-001", "value": "{\"temperature_celsius\": 23.5, \"email\": \"USER@EXAMPLE.COM\"}", "headers": {"content-type": "application/json", "source": "iot-sensors", "client_ip": "192.168.1.100"}}'

   # Plain text
   curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
     -H "Content-Type: application/json" \
     -d '{"key": "text-001", "value": "hello world from sensor", "headers": {"content-type": "text/plain", "source": "user-input"}}'
   ```

4. **View results:**
   ```bash
   curl -s "http://localhost:9092/api/v1/topics/processed-events/messages?offset=0&max_messages=10" | jq
   ```

## Development

### Testing
```bash
cd message-processor && cargo test
```

### Live Development
```bash
# Auto-rebuild on changes
cargo watch -x "build --target wasm32-unknown-unknown"

# Deploy to local broker
./build-and-deploy.sh http://localhost:9094
```

### Performance Tips
- Keep modules under 1MB for optimal loading
- Use `wasm-opt` for size optimization (automatically applied in build script)
- Monitor metrics: `curl -s http://localhost:9094/api/v1/etl/pipelines/ID/metrics | jq`

### Debugging
Enable ETL logging in broker config:
```toml
[logging]
level = "debug"
targets = ["rustmq::etl"]
```

## Resources
- [WASM ETL Deployment Guide](../../docs/wasm-etl-deployment-guide.md)
- [RustMQ API Reference](../../docs/api-reference.md)