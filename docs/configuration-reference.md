# RustMQ Configuration Reference

This document provides a complete reference for all RustMQ configuration options, including their purpose, default values, valid ranges, and production recommendations.

## Configuration File Format

RustMQ uses TOML format for configuration files. Configuration can be loaded from files or environment variables, and supports runtime updates for most parameters.

## Core Configuration Sections

### [broker]

Broker identity and rack awareness configuration.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `id` | String | `"broker-001"` | Unique broker identifier across the cluster | Non-empty string |
| `rack_id` | String | `"default"` | Rack identifier for rack-aware partition placement | Non-empty string |

**Production Recommendations:**
- Use descriptive broker IDs (e.g., `broker-us-west-1a-001`)
- Set `rack_id` to actual rack/AZ for fault tolerance

### [network]

Network configuration for client and inter-broker communication.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `quic_listen` | String | `"0.0.0.0:9092"` | QUIC server address for client connections | Valid socket address |
| `rpc_listen` | String | `"0.0.0.0:9093"` | gRPC server address for inter-broker communication | Valid socket address |
| `max_connections` | Integer | `10000` | Maximum concurrent client connections | 1 - 1,000,000 |
| `connection_timeout_ms` | Integer | `30000` | Connection timeout in milliseconds | 1000 - 300,000 |

**Production Recommendations:**
- Use `0.0.0.0` for containerized deployments
- Set `max_connections` based on expected client load
- Monitor connection metrics to tune timeouts

### [wal]

Write-Ahead Log configuration with intelligent upload triggers.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `path` | Path | `"/tmp/rustmq/wal"` | WAL storage directory path | Valid directory path |
| `capacity_bytes` | Integer | `10737418240` | Total WAL capacity (10GB) | 1GB - 1TB |
| `fsync_on_write` | Boolean | `true` | Enable fsync on every write for durability | true/false |
| `segment_size_bytes` | Integer | `134217728` | Upload threshold in bytes (128MB) | 1MB - 1GB |
| `buffer_size` | Integer | `65536` | WAL buffer size (64KB) | 4KB - 1MB |
| `upload_interval_ms` | Integer | `600000` | Time-based upload trigger (10 minutes) | 1000 - 3,600,000 |
| `flush_interval_ms` | Integer | `1000` | Background flush interval when fsync disabled | 100 - 10,000 |

**Production Recommendations:**
- Use fast NVMe SSDs for WAL path
- Set `capacity_bytes` to 10-20% of daily message volume
- Use 128MB segments for optimal object storage efficiency
- Enable `fsync_on_write` for financial/critical data
- Disable `fsync_on_write` for high-throughput scenarios with acceptable data loss risk

**Runtime Updateable:** ✅ All parameters support hot updates

### [cache]

Caching configuration for write and read workload isolation.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `write_cache_size_bytes` | Integer | `1073741824` | Write cache size (1GB) | 64MB - 64GB |
| `read_cache_size_bytes` | Integer | `2147483648` | Read cache size (2GB) | 64MB - 128GB |
| `eviction_policy` | Enum | `"Lru"` | Cache eviction policy | Lru, Lfu, Random |

**Production Recommendations:**
- Size caches based on available memory and workload patterns
- Use 1:2 ratio for write:read cache for typical workloads
- Monitor cache hit rates to optimize sizing

### [object_storage]

Object storage backend configuration for persistent data.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `storage_type` | Enum | `Local` | Storage backend type | S3, Gcs, Azure, Local |
| `bucket` | String | `"rustmq-data"` | Object storage bucket name | Valid bucket name |
| `region` | String | `"us-central1"` | Storage region | Valid region string |
| `endpoint` | String | `"https://storage.googleapis.com"` | Storage endpoint URL | Valid URL |
| `access_key` | String (optional) | None | Access key for authentication | Base64 string |
| `secret_key` | String (optional) | None | Secret key for authentication | Base64 string |
| `multipart_threshold` | Integer | `104857600` | Multipart upload threshold (100MB) | 5MB - 5GB |
| `max_concurrent_uploads` | Integer | `10` | Maximum concurrent upload operations | 1 - 100 |

**Production Recommendations:**
- Use IAM roles instead of access keys when possible
- Set `multipart_threshold` to 100MB for optimal performance
- Monitor upload bandwidth and adjust `max_concurrent_uploads`

## Operational Configuration Sections

### [scaling]

Broker scaling operation parameters.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `max_concurrent_additions` | Integer | `3` | Maximum brokers added simultaneously | 1 - 10 |
| `max_concurrent_decommissions` | Integer | `1` | Maximum brokers decommissioned simultaneously | 1 - 10 |
| `rebalance_timeout_ms` | Integer | `300000` | Partition rebalancing timeout (5 minutes) | 60,000 - 1,800,000 |
| `traffic_migration_rate` | Float | `0.1` | Traffic migration rate (10% per minute) | 0.01 - 1.0 |
| `health_check_timeout_ms` | Integer | `30000` | Health check timeout for new brokers | 5,000 - 300,000 |

**Production Recommendations:**
- Limit `max_concurrent_additions` to 3 for stability  
- **Keep `max_concurrent_decommissions` at 1 for safety** - prevents accidental mass decommissioning
- Use conservative `traffic_migration_rate` (0.1) for safety
- Increase timeouts for large clusters

**Runtime Updateable:** ✅ All parameters support hot updates

### [operations]

Rolling upgrade and maintenance configuration.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `allow_runtime_config_updates` | Boolean | `true` | Enable hot configuration updates | true/false |
| `upgrade_velocity` | Integer | `1` | Brokers upgraded per minute | 1 - 10 |
| `graceful_shutdown_timeout_ms` | Integer | `60000` | Graceful shutdown timeout (1 minute) | 10,000 - 600,000 |

**Production Recommendations:**
- Enable runtime updates for operational flexibility
- Use conservative upgrade velocity (1 broker/minute)
- Set shutdown timeout based on typical request completion time

**Runtime Updateable:** ❌ Requires restart to change update capability

### [operations.kubernetes]

Kubernetes-specific deployment configuration.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `use_stateful_sets` | Boolean | `true` | Use StatefulSets for deployment | true/false |
| `pvc_storage_class` | String | `"fast-ssd"` | Persistent volume storage class | Valid storage class |
| `wal_volume_size` | String | `"50Gi"` | WAL volume size specification | Kubernetes quantity |
| `enable_pod_affinity` | Boolean | `true` | Enable pod affinity for volume attachment | true/false |

**Production Recommendations:**
- Always use StatefulSets for production
- Use fast SSD storage classes
- Size WAL volumes for 1-2 days of peak load
- Enable pod affinity for volume locality

## Specialized Configuration Sections

### [replication]

Replication and durability configuration.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `min_in_sync_replicas` | Integer | `2` | Minimum replicas for durability | 1 - 10 |
| `ack_timeout_ms` | Integer | `5000` | Acknowledgment timeout | 1,000 - 60,000 |
| `max_replication_lag` | Integer | `1000` | Maximum acceptable lag in ms | 100 - 10,000 |

### [controller]

Cluster coordination configuration.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `endpoints` | Array | `["controller-1:9094"]` | Controller endpoint addresses | Valid endpoints |
| `election_timeout_ms` | Integer | `5000` | Raft election timeout | 1,000 - 30,000 |
| `heartbeat_interval_ms` | Integer | `1000` | Raft heartbeat interval | 100 - 5,000 |

### [etl]

WebAssembly ETL processing configuration.

| Parameter | Type | Default | Description | Valid Range |
|-----------|------|---------|-------------|-------------|
| `enabled` | Boolean | `false` | Enable WASM ETL processing | true/false |
| `memory_limit_bytes` | Integer | `67108864` | WASM memory limit (64MB) | 1MB - 1GB |
| `execution_timeout_ms` | Integer | `5000` | ETL execution timeout | 100 - 60,000 |
| `max_concurrent_executions` | Integer | `100` | Maximum concurrent WASM executions | 1 - 1,000 |

## Configuration Validation

RustMQ performs comprehensive validation on startup and runtime updates:

### Validation Rules

1. **Range Validation**: All numeric parameters must be within valid ranges
2. **Path Validation**: Directory paths must be accessible and writable
3. **Network Validation**: Network addresses must be valid and bindable
4. **Dependency Validation**: Inter-parameter dependencies are checked
5. **Resource Validation**: Memory and disk requirements are verified

### Example Validation Errors

```toml
# Invalid: segment_size_bytes too large
[wal]
segment_size_bytes = 2147483648  # Error: exceeds 1GB limit

# Invalid: negative timeout
[network]
connection_timeout_ms = -1000    # Error: must be positive

# Invalid: invalid storage type
[object_storage]
storage_type = "InvalidType"     # Error: must be S3, Gcs, Azure, or Local
```

## Environment Variable Override

All configuration parameters can be overridden using environment variables:

```bash
# Override WAL configuration
export RUSTMQ_WAL_SEGMENT_SIZE_BYTES=268435456
export RUSTMQ_WAL_UPLOAD_INTERVAL_MS=300000

# Override scaling configuration  
export RUSTMQ_SCALING_MAX_CONCURRENT_ADDITIONS=5
export RUSTMQ_SCALING_TRAFFIC_MIGRATION_RATE=0.2
```

## Runtime Configuration Updates

Most configuration parameters support hot updates via the admin API:

```bash
# Update WAL upload interval
curl -X POST http://broker:9094/config/update \
  -H "Content-Type: application/json" \
  -d '{"wal": {"upload_interval_ms": 300000}}'

# Update scaling parameters
curl -X POST http://broker:9094/config/update \
  -H "Content-Type: application/json" \
  -d '{"scaling": {"max_concurrent_additions": 5}}'
```

## Configuration Templates

See the `config/` directory for complete configuration templates:

- `example-production.toml`: Production-ready configuration
- `example-development.toml`: Development and testing configuration
- `example-high-throughput.toml`: High-throughput optimization
- `example-high-durability.toml`: Maximum durability configuration