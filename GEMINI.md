# GEMINI.md

This file provides guidance to Gemini when working with code in this repository.

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

RustMQ is a cloud-native distributed message queue system with a **storage-compute separation architecture**. The system is designed around several key architectural principles:

### Core Architecture Pattern

**Stateless Brokers + Shared Storage**: Unlike traditional message queues, RustMQ brokers are completely stateless. All persistent data lives in object storage (S3/GCS/Azure), while brokers maintain only ephemeral WAL files and caches for performance.

### Key Components and Their Interactions

1.  **Storage Layer** (`src/storage/`):
    *   **TieredStorageEngine**: Orchestrates the two-tier storage architecture
    *   **DirectIOWal**: Local NVMe WAL for low-latency writes, uses circular buffer design
    *   **ObjectStorage**: Abstracts cloud storage backends (S3/GCS/Azure/Local)
    *   **CacheManager**: Manages separate write (hot) and read (cold) caches for workload isolation
    *   **BufferPool**: Aligned buffer management for zero-copy operations

2.  **Replication System** (`src/replication/`):
    *   **ReplicationManager**: Implements leader-follower replication with configurable acknowledgment levels
    *   Uses shared storage to eliminate traditional data movement during failover
    *   Heartbeat monitoring and automatic failover without data loss

3.  **Network Layer** (`src/network/`):
    *   **QuicServer**: QUIC/HTTP3 protocol for client communication (eliminates head-of-line blocking)
    *   **gRPC**: Internal broker-to-broker communication
    *   Zero-copy data movement throughout the network stack

4.  **Control Plane** (`src/controller/`):
    *   Raft-based consensus for cluster coordination
    *   Manages partition leadership assignment and load balancing
    *   Metadata consistency across the cluster

### Data Flow Architecture

```
Client → QUIC → Broker → Local WAL → Cache → Object Storage
                   ↓
                Replication → Other Brokers
```

**Write Path**:
1.  Record arrives via QUIC
2.  Written to local WAL (sync or async based on durability requirements)
3.  Cached in write cache
4.  Replicated to followers
5.  Asynchronously uploaded to object storage
6.  WAL space reclaimed after successful upload

**Read Path**:
1.  Check write cache (for tailing consumers)
2.  Check read cache (for historical consumers)
3.  Fetch from object storage if cache miss
4.  Populate read cache

### Critical Design Decisions

*   **Shared Storage**: Enables instant broker scaling and eliminates expensive data rebalancing
*   **Workload Isolation**: Separate caches for hot (tailing) vs cold (historical) reads prevent interference
*   **QUIC Protocol**: Reduces connection overhead and eliminates head-of-line blocking vs TCP
*   **WebAssembly ETL**: Secure, sandboxed real-time data processing
*   **Object Storage Optimization**: Intelligent compaction and bandwidth limiting for cost efficiency

## Key Traits and Abstractions

The codebase heavily uses async traits to abstract major components:

*   `WriteAheadLog`: Local persistence interface
*   `ObjectStorage`: Cloud storage backend interface
*   `Cache`: Caching layer interface
*   `ReplicationManager`: Replication coordination interface
*   `Stream`: High-level streaming interface

## Configuration System

All configuration is centralized in `src/config.rs` with TOML file support. Key configuration sections:
*   `BrokerConfig`: Broker identity and rack awareness
*   `WalConfig`: WAL path, capacity, sync behavior
*   `CacheConfig`: Cache sizes and eviction policies
*   `ObjectStorageConfig`: Backend type and connection settings
*   `ReplicationConfig`: Acknowledgment levels and timeouts

## Testing Strategy

The codebase has comprehensive unit tests. Tests use:
*   `tempfile` for temporary directories in storage tests
*   Mock implementations for external dependencies
*   Property-based testing patterns for complex interactions

## Error Handling

Centralized error handling through `src/error.rs` with:
*   `RustMqError` enum covering all error categories
*   Automatic conversion from underlying library errors
*   `Result<T>` type alias used throughout

## Feature Flags

*   `io-uring`: Enables high-performance async I/O on Linux
*   `wasm`: Enables WebAssembly ETL processing support
*   Default features: `["wasm"]`

## Performance Considerations

*   **Zero-copy operations**: Extensive use of `Bytes` and vectored I/O
*   **Lock-free when possible**: Atomic operations for counters, RwLock for infrequent updates
*   **Async throughout**: All I/O operations are async to prevent blocking
*   **Buffer pooling**: Reuses aligned buffers to reduce allocation overhead
*   **Direct I/O**: Optional direct I/O bypass of OS page cache for WAL

## Module Dependencies

*   `types.rs`: Core type definitions used across all modules
*   `error.rs`: Error types used everywhere
*   `config.rs`: Configuration structures
*   `storage/`: Core storage abstractions and implementations
*   `replication/`: Replication logic depends on storage traits
*   `network/`: Protocol handlers depend on storage and replication
*   `controller/`: Cluster coordination depends on all other modules
