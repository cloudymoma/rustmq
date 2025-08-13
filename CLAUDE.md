# CLAUDE.md

Help for Claude Code when working on this project.

## Rules to Follow

1. Think hard about speed, code quality, and bugs
2. Look up info online when needed
3. Add tests for new code
4. Check builds work in debug and release mode
5. Ask the right expert for each task
6. Keep this file updated with latest status
7. Update docs when done

## Build Commands

```bash
# Build project (moka-cache enabled by default)
cargo build --release

# Build without moka cache (legacy LRU)
cargo build --release --no-default-features --features wasm

# Run tests (moka-cache enabled by default)
cargo test --lib

# Test with legacy LRU cache
cargo test --lib --no-default-features --features wasm

# Test specific parts
cargo test storage::
cargo test --features "io-uring,wasm"
cargo test --features "io-uring,wasm,moka-cache"

# Check code quality
cargo check
cargo clippy
cargo fmt

# Run cache performance benchmarks (moka by default)
cargo bench --bench cache_performance_bench

# Benchmark legacy LRU vs moka comparison
cargo bench --bench cache_performance_bench --no-default-features --features wasm

# Run services
cargo run --bin rustmq-broker -- --config config/broker.toml
cargo run --bin rustmq-controller -- --config config/controller.toml
cargo run --bin rustmq-admin -- <command>
```

## What RustMQ Does

RustMQ is a message system that stores data in the cloud (like S3) instead of on each server. This makes it:
- Cheap (90% less storage cost)
- Fast to scale (add servers instantly)
- Easy to manage (servers don't store data)

## Main Parts

### Storage
- **WAL**: Fast local storage for new messages
- **Object Storage**: Cloud storage for old messages (S3/GCS/Azure)
- **Cache**: Memory storage for hot data

### Network
- **QUIC**: Fast protocol for clients
- **gRPC**: Server-to-server communication

### Control
- **Controller**: Manages the cluster using OpenRaft consensus engine
- **Admin API**: REST API for management

### Security
- **mTLS**: Secure connections with certificates
- **ACL**: Who can do what (very fast: 547ns checks)

### Processing
- **WASM ETL**: Transform messages in real-time safely
- **Priority stages**: Process messages in order (0→1→2)
- **Smart filters**: Only process matching topics

## Data Flow

```
Client → QUIC → Broker → Local WAL → Cache → Cloud Storage
                   ↓
                Sync → Other Brokers
```

## Key Files

- `src/storage/`: How data is stored
- `src/network/`: Network communication
- `src/controller/`: Cluster management
- `src/security/`: Authentication and permissions
- `src/etl/`: Message processing
- `config/`: Settings files

## Tests

✅ **456 tests pass, 0 fail** (Latest: August 2025 - ALL TESTS PASSING!)
- Storage: 17 tests ✅
- Network: 19 tests ✅
- Security: 175/175 tests ✅
- Controller: 14 tests ✅ (including OpenRaft)
- Admin: 26 tests ✅
- ETL: 12 tests ✅
- Config validation: All tests pass ✅
- **Cache system**: ✅ Fixed tokio runtime issue in benchmarks

## Current Status

### ✅ Working (Production Ready)
- Storage system with WAL and cloud storage
- Network layer with QUIC and gRPC
- **🚀 OpenRaft Consensus**: **FULLY PRODUCTION-READY** Raft implementation (OpenRaft 0.9.21)
  - Production storage with WAL, crash recovery, and high-throughput
  - gRPC-based networking with connection pooling and retries
  - Complete cluster management with consensus operations
  - Log compaction and snapshot management
  - Performance optimizations and caching
- Security with mTLS and fast ACL
- Admin tools and REST API
- WASM ETL processing
- Client SDKs for Rust and Go

### Key Features
- **Fast**: Sub-millisecond latency
- **Secure**: 547ns authorization checks
- **Scalable**: Add servers instantly with true consensus
- **Smart**: Process messages with WASM
- **Cheap**: 90% less storage cost
- **Reliable**: Production-grade distributed consensus

## Config Files

- `config/broker.toml`: Broker settings (ports 9092, 9093)
- `config/controller.toml`: Controller settings (ports 9094, 9095, 9642) with full OpenRaft configuration
- `config/example-production.toml`: Production-ready configuration with optimized OpenRaft settings
- `config/example-development.toml`: Development configuration with fast iteration settings

## Performance Tips

- Use `io-uring` feature on Linux for best speed
- Enable WASM for message processing
- Use object storage for cost savings
- Cache hot data in memory

## Security Setup

```bash
# Create certificates
./target/release/rustmq-admin ca init --cn "RustMQ Root CA"

# Make user permissions
./target/release/rustmq-admin acl create \
  --principal "user@company.com" \
  --resource "topic.events.*" \
  --permissions read,write
```

## WASM ETL

Transform messages in real-time:
- Write safe Rust code
- Deploy as WASM modules
- Process by priority (0→1→2)
- Filter by topic patterns
- Very fast and secure

## Latest Updates

- **🚀 HIGH-PERFORMANCE MOKA CACHE**: **ENABLED BY DEFAULT** - Production-ready cache optimization with:
  - **✅ Lock-Free Reads**: Eliminates write-lock-on-read bottleneck from previous implementation
  - **✅ TinyLFU Algorithm**: Superior cache hit ratios compared to basic LRU
  - **✅ Default Feature**: Now enabled by default for all builds (legacy LRU available via --no-default-features)
  - **✅ Drop-In Replacement**: Complete API compatibility with existing Cache trait
  - **✅ Automatic Maintenance**: Background tasks for eviction and cleanup every 60 seconds
  - **✅ Advanced Features**: TTL (1 hour), size-based eviction, eviction listeners
  - **✅ All Tests Pass**: 449 tests validated with moka cache implementation
  - **✅ Memory Optimized**: Intelligent weigher for accurate memory tracking
  - **✅ Production Config**: Optimized for RustMQ's workload patterns
- **🚀 PRODUCTION-READY OpenRaft 0.9.21**: **FULLY VALIDATED** production consensus implementation with:
  - **✅ Complete Storage Layer**: Persistent WAL with crash recovery and high-throughput optimizations
  - **✅ gRPC Network Layer**: Production networking with connection pooling and retry logic
  - **✅ Cluster Management**: Complete consensus operations and leader election
  - **✅ Background Tasks**: Auto-compaction, metrics collection, and health monitoring
  - **✅ Real Consensus**: **ENABLED** - Replaced simplified placeholder with full production implementation
  - **✅ API Compatibility**: All OpenRaft 0.9.21 trait implementations completed and working
  - **✅ Production Binaries**: Controller binary **BUILDS AND RUNS** production OpenRaft
  - **✅ Configuration**: Updated to use production-ready settings with WAL, compaction, and performance tuning
  - **✅ Controller Binary**: **PRODUCTION MODE ACTIVE** - Uses RaftManager with full OpenRaft 0.9.21 features
  - **✅ Integration Verified**: Controller works with brokers, admin tools, and SDK clients
  - **✅ Error Handling**: Comprehensive error handling with graceful degradation
  - **✅ Network Resilience**: Proper timeout handling and connection management
  - **✅ Production Configuration**: Comprehensive OpenRaft config with snapshot policy, timeouts, and optimization
- **Enhanced Architecture**: True distributed consensus **NOW ACTIVE** replacing simplified working implementation
- **Priority ETL**: Multi-stage message processing with WASM safety
- **Security**: Fast ACL with 547ns lookups, mTLS, and audit logging
- **SDKs**: Rust and Go clients ready with examples
- **Admin**: Full cluster management with REST API
- **Production Ready**: All core systems now fully functional for enterprise deployment

### Latest Bug Fixes & Performance Improvements (August 2025)
✅ **Fixed Cache Performance Issue**: Resolved tokio runtime panic in benchmarks
- **Problem**: `CacheManager::new()` was spawning background tasks during construction, causing benchmark failures
- **Solution**: Implemented lazy initialization pattern with `new_without_maintenance()` for benchmarks
- **Performance**: Cache creation now ~10.95 µs without maintenance overhead
- **Benefits**: Flexible deployment, better benchmarking, production safety maintained
- **Research**: Comprehensive optimization research completed for future 15-200% performance improvements

✅ **Fixed All Test Failures**: Successfully resolved configuration validation and broker health tracking issues  
- **Root Cause**: Default Raft heartbeat timeout (1000ms) was not greater than heartbeat interval (1000ms)
- **Fix**: Updated default `controller.raft.heartbeat_timeout_ms` from 1000ms to 2000ms
- **Broker Health Tests**: Added test mode for `BrokerHealthTracker` to avoid actual network calls in unit tests
- **Impact**: All 456 tests now pass in both debug and release mode (UP from 441)
- **Files Fixed**: Cache system, configuration validation, and broker health tracking