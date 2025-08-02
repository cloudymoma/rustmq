# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

```bash
# Build the project
cargo build --release

# Run all tests
cargo test --lib

# Run a specific test
cargo test test_tiered_storage_engine

# Run tests for a specific module
cargo test storage::

# Run SDK connection layer tests
cargo test connection::

# Run tests with specific features
cargo test --features "io-uring,wasm"

# Run benchmarks
cargo bench

# Check for compilation errors and warnings
cargo check

# Run clippy for additional linting
cargo clippy

# Format code
cargo fmt

# Run specific binary targets
cargo run --bin rustmq-broker -- --config config/broker.toml
cargo run --bin rustmq-controller -- --config config/controller.toml
cargo run --bin rustmq-admin -- --config config/admin.toml
```

## Architecture Overview

RustMQ is a cloud-native distributed message queue system with a **storage-compute separation architecture**. The system is designed around several key architectural principles with comprehensive operational capabilities for production deployment.

### Core Architecture Pattern

**Stateless Brokers + Shared Storage**: Unlike traditional message queues, RustMQ brokers are completely stateless. All persistent data lives in object storage (S3/GCS/Azure), while brokers maintain only ephemeral WAL files and caches for performance.

### Key Components and Their Interactions

1. **Storage Layer** (`src/storage/`):
   - **TieredStorageEngine**: Orchestrates the two-tier storage architecture
   - **DirectIOWal**: Enhanced local NVMe WAL with intelligent upload triggers and configurable flush behavior
     - Size-based upload triggers (default: 128MB segments)
     - Time-based upload triggers (default: 10 minutes)
     - Runtime configuration updates
     - Background flush tasks when fsync is disabled
   - **ObjectStorage**: Abstracts cloud storage backends (S3/GCS/Azure/Local)
   - **CacheManager**: Manages separate write (hot) and read (cold) caches for workload isolation
   - **BufferPool**: Aligned buffer management for zero-copy operations

2. **Replication System** (`src/replication/`):
   - **ReplicationManager**: Implements leader-follower replication with configurable acknowledgment levels
   - **FollowerReplicationHandler**: Enhanced follower logic with:
     - Real-time lag calculation based on leader high watermark vs follower WAL offset
     - Actual WAL offset tracking using `get_end_offset()` instead of hardcoded values
     - Leader epoch validation and automatic leadership updates
     - Proper error handling for WAL operations with specific error codes
   - Uses shared storage to eliminate traditional data movement during failover
   - Heartbeat monitoring and automatic failover without data loss

3. **Network Layer** (`src/network/`):
   - **QuicServer**: QUIC/HTTP3 protocol for client communication (eliminates head-of-line blocking)
   - **gRPC**: Internal broker-to-broker communication
   - Zero-copy data movement throughout the network stack

4. **Control Plane** (`src/controller/`):
   - Raft-based consensus for cluster coordination
   - Manages partition leadership assignment and load balancing
   - Metadata consistency across the cluster

5. **Scaling Operations** (`src/scaling/`):
   - **ScalingManager**: Automated broker addition and removal
   - **PartitionRebalancer**: Intelligent partition redistribution
   - Batch broker addition support (configurable concurrency)
   - Safety-first single broker removal
   - Real-time progress tracking and health verification

6. **Operational Management** (`src/operations/`):
   - **RollingUpgradeManager**: Zero-downtime cluster upgrades
   - **KubernetesDeploymentManager**: Production-ready Kubernetes manifests
   - **VolumeRecoveryManager**: Persistent volume recovery for broker failures
   - **RuntimeConfigManager**: Hot configuration updates without restarts

7. **Admin REST API** (`src/admin/`):
   - **AdminApi**: Comprehensive cluster management REST API server
   - **BrokerHealthTracker**: Real-time broker health monitoring with background checks
   - **Health Endpoints**: Service uptime tracking and cluster status assessment
   - **Topic Management**: CRUD operations for topics with partition and replication management
   - **Error Handling**: Production-ready error responses with leader hints
   - **Testing**: Complete test coverage with 11 unit tests for all API functionality

### Data Flow Architecture

```
Client → QUIC → Broker → Local WAL → Cache → Object Storage
                   ↓
                Replication → Other Brokers
```

**Write Path**: 
1. Record arrives via QUIC
2. Written to local WAL (sync or async based on durability requirements)
3. Cached in write cache
4. Replicated to followers
5. Intelligent upload to object storage based on:
   - Segment size threshold (configurable, default 128MB)
   - Time interval threshold (configurable, default 10 minutes)
6. WAL space reclaimed after successful upload

**Read Path**:
1. Check write cache (for tailing consumers)
2. Check read cache (for historical consumers)  
3. Fetch from object storage if cache miss
4. Populate read cache

**Operational Flows**:
- **Scaling Operations**: Automated broker addition/removal with partition rebalancing
- **Rolling Upgrades**: Zero-downtime version upgrades with health verification
- **Volume Recovery**: Automatic persistent volume reattachment in Kubernetes environments

### Critical Design Decisions

- **Shared Storage**: Enables instant broker scaling and eliminates expensive data rebalancing
- **Workload Isolation**: Separate caches for hot (tailing) vs cold (historical) reads prevent interference
- **QUIC Protocol**: Reduces connection overhead and eliminates head-of-line blocking vs TCP
- **WebAssembly ETL**: Secure, sandboxed real-time data processing
- **Object Storage Optimization**: Intelligent compaction and bandwidth limiting for cost efficiency
- **Intelligent Upload Triggers**: Dual conditions (size + time) ensure optimal object storage utilization
- **Runtime Configuration**: Hot updates without service interruption for operational flexibility

## Key Traits and Abstractions

The codebase heavily uses async traits to abstract major components:

### Core Storage Traits
- `WriteAheadLog`: Local persistence interface with upload callback support
- `ObjectStorage`: Cloud storage backend interface  
- `Cache`: Caching layer interface
- `ReplicationManager`: Replication coordination interface
- `Stream`: High-level streaming interface

### Operational Traits
- `ScalingManager`: Automated broker scaling operations
- `PartitionRebalancer`: Intelligent partition redistribution algorithms
- `RollingUpgradeManager`: Zero-downtime upgrade orchestration
- `BrokerUpgradeOperations`: Individual broker upgrade operations
- `RuntimeConfigManager`: Dynamic configuration management

## Configuration System

All configuration is centralized in `src/config.rs` with TOML file support and runtime update capabilities. Key configuration sections:

### Core Configuration
- `BrokerConfig`: Broker identity and rack awareness
- `WalConfig`: Enhanced WAL configuration with upload triggers and flush behavior
  - `segment_size_bytes`: Upload threshold (default: 128MB)
  - `upload_interval_ms`: Time-based upload trigger (default: 10 minutes)
  - `flush_interval_ms`: Background flush interval when fsync is disabled
- `CacheConfig`: Cache sizes and eviction policies
- `ObjectStorageConfig`: Backend type and connection settings
- `ReplicationConfig`: Acknowledgment levels and timeouts

### Operational Configuration
- `ScalingConfig`: Broker scaling operation parameters
  - `max_concurrent_additions`: Maximum brokers added simultaneously
  - `rebalance_timeout_ms`: Partition rebalancing timeout
  - `traffic_migration_rate`: Gradual traffic migration rate
  - `health_check_timeout_ms`: Health verification timeout
- `OperationsConfig`: Rolling upgrade and maintenance settings
  - `allow_runtime_config_updates`: Enable hot configuration updates
  - `upgrade_velocity`: Brokers upgraded per minute
  - `graceful_shutdown_timeout_ms`: Graceful shutdown timeout
- `KubernetesConfig`: Container orchestration settings
  - `use_stateful_sets`: Enable StatefulSet deployment
  - `pvc_storage_class`: Persistent volume storage class
  - `wal_volume_size`: WAL volume size specification
  - `enable_pod_affinity`: Pod affinity for volume attachment

## Testing Strategy

The codebase has comprehensive unit tests (102 tests currently passing). Tests use:
- `tempfile` for temporary directories in storage tests
- Mock implementations for external dependencies (Kubernetes API, broker operations)
- Property-based testing patterns for complex interactions
- Integration tests for scaling and operational workflows
- Async test patterns for background task verification
- Timeout-based tests for upload triggers and configuration updates
- Replication lag calculation tests verifying accurate follower offset tracking

## Error Handling

Centralized error handling through `src/error.rs` with:
- `RustMqError` enum covering all error categories
- Automatic conversion from underlying library errors
- `Result<T>` type alias used throughout

## Feature Flags

- `io-uring`: Enables high-performance async I/O on Linux
- `wasm`: Enables WebAssembly ETL processing support
- Default features: `["wasm"]`

## Performance Considerations

- **Zero-copy operations**: Extensive use of `Bytes` and vectored I/O
- **Lock-free when possible**: Atomic operations for counters, RwLock for infrequent updates
- **Async throughout**: All I/O operations are async to prevent blocking
- **Buffer pooling**: Reuses aligned buffers to reduce allocation overhead
- **Direct I/O**: Optional direct I/O bypass of OS page cache for WAL

## Module Dependencies

- `types.rs`: Core type definitions used across all modules
- `error.rs`: Error types used everywhere
- `config.rs`: Configuration structures with runtime update support
- `storage/`: Core storage abstractions and implementations
- `replication/`: Replication logic depends on storage traits
- `network/`: Protocol handlers depend on storage and replication
- `controller/`: Cluster coordination depends on all other modules
- `scaling/`: Broker scaling operations depend on storage and replication
- `operations/`: Operational management depends on scaling and storage modules
- `admin/`: Admin REST API depends on controller service for cluster management and health tracking

## Production Deployment

### Kubernetes Deployment
RustMQ provides production-ready Kubernetes manifests including:
- **StatefulSets**: For persistent broker deployment with volume affinity
- **Services**: Headless services for broker discovery and external access
- **ConfigMaps**: Centralized configuration management
- **PodDisruptionBudgets**: High availability during maintenance
- **HorizontalPodAutoscaler**: Automatic scaling based on resource metrics

### Operational Features
- **Zero-downtime Rolling Upgrades**: Coordinated version updates with health verification
- **Automated Broker Scaling**: Add multiple brokers or remove single brokers safely
- **Volume Recovery**: Automatic persistent volume reattachment after pod failures
- **Runtime Configuration**: Hot configuration updates without service interruption
- **Intelligent Upload Management**: Optimized object storage utilization with dual triggers

### Monitoring and Observability
- Upload callback hooks for monitoring WAL segment uploads
- Progress tracking for scaling and upgrade operations
- Health check endpoints for Kubernetes probes
- Comprehensive error propagation and logging