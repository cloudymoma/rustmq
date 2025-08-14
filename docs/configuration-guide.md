# RustMQ Configuration Guide

This guide provides comprehensive information about RustMQ configuration files, their usage, and best practices for different environments.

## Configuration File Overview

RustMQ uses a well-structured configuration system with separate files for different environments and use cases. All configuration files are located in the `config/` directory.

### Configuration File Types

#### Production Configurations
- `config/broker.toml` - Production broker configuration
- `config/controller.toml` - Production controller configuration 
- `config/example-production.toml` - Production template with detailed comments

#### Development Configurations
- `config/broker-dev.toml` - Development broker configuration
- `config/controller-dev.toml` - Development controller configuration
- `config/example-development.toml` - Comprehensive development template
- `config/admin-dev.toml` - Development admin configuration

#### Test Configurations
- `config/test-broker.toml` - Optimized test broker configuration
- `config/test-controller.toml` - Optimized test controller configuration

#### Specialized Configurations
- `config/controller-simple.toml` - Minimal controller setup
- `config/admin-rate-limiting-example.toml` - Rate limiting configuration examples

## Configuration Usage by Environment

### Testing Environment

The existing test configurations are already optimal for testing scenarios:

```bash
# Unit tests - use existing test configs
cargo test --lib

# Integration tests with test configs
RUSTMQ_BROKER_CONFIG=config/test-broker.toml cargo test --test integration

# Start test services
cargo run --bin rustmq-broker -- --config config/test-broker.toml
cargo run --bin rustmq-controller -- --config config/test-controller.toml
```

**Test Configuration Features:**
- ✅ Use `/tmp` directories for ephemeral test data
- ✅ Disabled `fsync_on_write = false` for speed
- ✅ Small cache sizes (16MB/32MB) for efficient testing
- ✅ Short timeouts for fast test iteration
- ✅ Single replication factor for simplicity
- ✅ Local storage only (no cloud dependencies)

### Development Environment

```bash
# Start development services
cargo run --bin rustmq-broker -- --config config/broker-dev.toml
cargo run --bin rustmq-controller -- --config config/controller-dev.toml

# Use development example configuration
cargo run --example producer -- --config config/example-development.toml
```

**Development Configuration Features:**
- ✅ Local filesystem paths under `./data/`
- ✅ Security enabled with self-signed certificates
- ✅ Debug logging level
- ✅ Smaller resource allocations for local development
- ✅ Fast iteration settings (shorter timeouts, smaller segments)

### Production Environment

```bash
# Start production services
cargo run --bin rustmq-broker -- --config config/broker.toml
cargo run --bin rustmq-controller -- --config config/controller.toml

# Use production template as starting point
cp config/example-production.toml config/my-production.toml
# Edit my-production.toml for your environment
cargo run --bin rustmq-broker -- --config config/my-production.toml
```

**Production Configuration Features:**
- ✅ Cloud storage backends (S3/GCS/Azure)
- ✅ Production-grade security with proper certificates
- ✅ Large cache sizes and optimized performance settings
- ✅ Comprehensive monitoring and metrics
- ✅ High availability and replication settings

## Key Configuration Sections

### Broker Configuration

#### Network Settings
```toml
[network]
quic_listen = "0.0.0.0:9092"        # QUIC/HTTP3 client endpoint
rpc_listen = "0.0.0.0:9093"         # Internal gRPC endpoint
max_connections = 10000              # Maximum concurrent connections
connection_timeout_ms = 30000        # Connection timeout
```

#### Storage Configuration
```toml
[wal]
path = "/var/lib/rustmq/wal"        # WAL storage path
capacity_bytes = 10737418240        # 10GB WAL capacity
fsync_on_write = true               # Force sync on write (durability)
segment_size_bytes = 1073741824     # 1GB segment size

[cache]
write_cache_size_bytes = 1073741824  # 1GB hot data cache
read_cache_size_bytes = 2147483648   # 2GB cold data cache
eviction_policy = "Moka"             # Cache eviction policy

[object_storage]
storage_type = "S3"                 # Storage backend (S3/Gcs/Azure/Local)
bucket = "rustmq-data"              # Storage bucket name
region = "us-central1"              # Storage region
```

#### Security Configuration
```toml
[security]
enabled = true
tls_cert_path = "./certs/server.pem"
tls_key_path = "./certs/server.key"
ca_cert_path = "./certs/ca.pem"
require_client_cert = true
```

### Controller Configuration

#### Raft Consensus
```toml
[raft]
data_dir = "./data/raft"
heartbeat_interval_ms = 500
election_timeout_ms = 2000
max_append_entries_size = 1000

[cluster]
initial_cluster = ["controller-01=127.0.0.1:9095"]
```

#### Admin API
```toml
[admin]
port = 9642                             # Admin REST API port
health_check_interval_ms = 15000        # Health check interval
health_timeout_ms = 30000               # Health timeout
enable_cors = true                      # Enable CORS headers

# Rate limiting configuration
[admin.rate_limiting]
enabled = true                          # Enable rate limiting
global_burst_size = 1000               # Global burst capacity
global_refill_rate = 60                # Global requests per minute
```

## Environment Variables

RustMQ supports configuration through environment variables for containerized deployments:

```bash
# Core settings
RUSTMQ_BROKER_ID=broker-001
RUSTMQ_RACK_ID=us-central1-a
RUSTMQ_LOG_LEVEL=info

# Storage settings
RUSTMQ_WAL_PATH=/var/lib/rustmq/wal
RUSTMQ_STORAGE_BUCKET=rustmq-data
RUSTMQ_STORAGE_REGION=us-central1

# GCP settings
GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json
GCP_PROJECT_ID=your-project-id

# Performance tuning
RUSTMQ_CACHE_SIZE=2147483648
RUSTMQ_MAX_CONNECTIONS=10000
RUSTMQ_BATCH_SIZE=1000

# Admin API rate limiting settings
RUSTMQ_ADMIN_RATE_LIMITING_ENABLED=true
RUSTMQ_ADMIN_GLOBAL_BURST_SIZE=1000
RUSTMQ_ADMIN_GLOBAL_REFILL_RATE=60
```

## Configuration Best Practices

### Development
1. **Use existing development configs**: `config/broker-dev.toml` and `config/controller-dev.toml` are pre-configured for local development
2. **Enable debug logging**: Set `level = "debug"` in logging section
3. **Use local storage**: Configure `storage_type = "local"` for faster iteration
4. **Smaller resource allocations**: Use smaller cache sizes and buffer configurations

### Testing
1. **Use existing test configs**: `config/test-broker.toml` and `config/test-controller.toml` are optimized for testing
2. **Ephemeral storage**: Test configs use `/tmp` directories that are cleaned up automatically
3. **Disable fsync**: `fsync_on_write = false` for faster test execution
4. **Short timeouts**: Quick failure detection for fast test iteration

### Production
1. **Start with production template**: Use `config/example-production.toml` as a starting point
2. **Configure cloud storage**: Set up proper S3/GCS/Azure credentials and regions
3. **Enable security**: Use production certificates and enable all security features
4. **Monitor resource usage**: Configure appropriate cache sizes and connection limits
5. **Set up proper logging**: Use structured logging with appropriate log levels

## Configuration Validation

RustMQ automatically validates configuration on startup:

```bash
# Validate broker configuration
cargo run --bin rustmq-broker -- --config config/broker.toml --validate

# Validate controller configuration  
cargo run --bin rustmq-controller -- --config config/controller.toml --validate
```

## Configuration Troubleshooting

### Common Issues

1. **Invalid TOML syntax**
   ```bash
   # Check for syntax errors
   cargo run --bin rustmq-broker -- --config config/broker.toml
   # Look for "failed to parse configuration" errors
   ```

2. **Missing required fields**
   ```bash
   # Ensure all required fields are present
   # Compare with example configurations
   diff config/broker.toml config/example-production.toml
   ```

3. **Permission issues**
   ```bash
   # Check file permissions
   ls -la config/
   # Ensure configuration files are readable
   chmod 644 config/*.toml
   ```

4. **Path issues**
   ```bash
   # Ensure directories exist
   mkdir -p /var/lib/rustmq/wal
   mkdir -p ./data/wal
   ```

### Validation Commands

```bash
# Test configuration loading
cargo test config::

# Validate specific configurations
cargo run --bin rustmq-broker -- --config config/test-broker.toml --dry-run
cargo run --bin rustmq-controller -- --config config/test-controller.toml --dry-run
```

## Configuration Migration

When upgrading RustMQ versions, use the provided migration tools:

```bash
# Migrate configuration to new version
cargo run --bin rustmq-admin -- config migrate \
  --from config/old-broker.toml \
  --to config/new-broker.toml \
  --version 1.1.0
```

## Advanced Configuration

### Custom Object Storage
```toml
[object_storage]
storage_type = { Custom = { 
    endpoint = "https://custom-s3.example.com",
    access_key_id = "custom-access-key",
    secret_access_key = "custom-secret-key",
    region = "custom-region"
}}
```

### Performance Tuning
```toml
# High-throughput configuration
[wal]
buffer_size = 1048576               # 1MB buffer for high throughput
segment_size_bytes = 2147483648     # 2GB segments

# Low-latency configuration  
[wal]
buffer_size = 4096                  # Small buffers for low latency
flush_interval_ms = 1               # Immediate flush
```

### Security Hardening
```toml
[security]
require_client_cert = true
min_tls_version = "1.3"
cipher_suites = ["TLS_AES_256_GCM_SHA384"]
enable_cert_revocation_check = true
```

## See Also

- [Security Configuration Guide](security/SECURITY_CONFIGURATION.md)
- [Performance Tuning Guide](security/SECURITY_TUNING_GUIDE.md)
- [Production Deployment Guide](kubernetes-deployment.md)
- [Admin API Configuration](admin_cli_security.md)