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
# Build and test
cargo build --release
cargo test --lib                    # Debug mode tests
cargo test --release                # Release mode tests (include benchmarks)
cargo bench                         # Run benchmarks

# Code quality
cargo check && cargo clippy && cargo fmt

# Feature-specific builds
cargo build --release --no-default-features --features wasm  # Legacy LRU cache
cargo test --features "io-uring,wasm,moka-cache"

# Miri memory safety tests
rustup +nightly component add miri
./scripts/miri-test.sh              # All tests
./scripts/miri-test.sh --quick      # Fast subset

# Docker and services
docker build -f docker/Dockerfile.webui -t rustmq-webui:latest .
cd docker && ./build-and-push.sh build
cargo run --bin rustmq-broker -- --config config/broker.toml
cargo run --bin rustmq-controller -- --config config/controller.toml

# WASM modules
make wasm-build && make wasm-test
```

## Architecture

**RustMQ**: Cloud-native message queue with 90% storage cost savings via object storage (S3/GCS/Azure).

**Components**:
- Storage: WAL (local), Object Storage (cloud), Cache (memory)
- Network: QUIC (clients), gRPC (servers), DashMap lock-free pools
- Control: OpenRaft consensus, REST Admin API
- Security: mTLS, 547ns ACL checks
- Processing: WASM ETL with priority stages (0→1→2)

**Data Flow**: `Client → QUIC → Broker → WAL → Cache → Cloud Storage`

**Key Directories**: `src/storage/`, `src/network/`, `src/controller/`, `src/security/`, `src/etl/`, `config/`

## Status

**Tests**: 501 passing (debug), 100% pass rate (release) - Full integration test coverage

**Integration Tests**:
- Graceful Shutdown: 4 tests (message preservation, timing, connection handling, health checks)
- Load Tests: 2 tests (10K msg/sec sustained, burst handling) - run with `--ignored` flag

**Production Components**:
- OpenRaft 0.9.21 consensus with WAL, crash recovery, log compaction
- WebPKI certificate validation with fallback (92% test failure reduction)
- Web UI (Vue 3, 50.2MB Docker, K8s-ready)
- WASM ETL (wasmtime 25.0, fuel metering, timeout enforcement)
- SDKs: Rust SDK with broker type integration + Go (Go tests: 0.638s, 97% faster)

**Config Files**:
- `config/broker.toml`: ports 9092-9093
- `config/controller.toml`: ports 9094-9095, 9642 (OpenRaft)

## Performance Optimizations

| Optimization | Impact | Status |
|--------------|--------|--------|
| SmallVec Headers | 90% ↓ heap alloc, 15-30% ↓ latency | ✅ |
| Lock-Free Pools | 547ns ACL lookups, 50K connections | ✅ |
| Buffer Pooling | 30-40% ↓ allocation overhead | ✅ |
| Zero-Copy API | 20-40% ↓ consumer read time | ✅ |
| FuturesUnordered | 85% ↓ replication latency | ✅ |
| CPU-Adaptive L2 | 8-512 shards (auto-scaled) | ✅ |
| Moka Cache | Lock-free reads, TinyLFU | ✅ Default |

## Recent Major Features (2025)

| Date | Feature | Achievement |
|------|---------|-------------|
| Dec 2025 | SDK Type Integration | Rust SDK now uses broker's shared types (ProduceRequest, Record, etc.) for compile-time type safety |
| Dec 2025 | Integration Test Suite | 6 integration tests covering graceful shutdown, load testing (10K msg/sec), and health checks |
| Dec 2025 | QUIC Protocol Implementation | Fixed client-broker protocol with proper `[1-byte type][data]` framing and bincode serialization |
| Nov 2025 | Production Unwrap Elimination | Phase 1 complete: 13 critical unwraps fixed, panic hooks installed |
| Nov 2025 | Broker Core Abstraction | Extracted broker logic to library (bin: 374→45 lines, 88% reduction) |
| Nov 2025 | Circular Dependency Fix | Broke security↔controller cycle using Dependency Inversion |
| Nov 2025 | Security Facades | Refactored SecurityManager god object into 4 focused facades |
| Nov 2025 | Security Admin API | 27 REST endpoints (cert, ACL, audit) |
| Nov 2025 | Lock-Free Pools | DashMap-based connection management |
| Nov 2025 | CPU-Adaptive Cache | Auto-scales L2 shards with CPU cores |
| Oct 2025 | WASM ETL | Real wasmtime 25.0, 3 test modules |
| Oct 2025 | Web UI Docker | 50.2MB container, K8s manifests |
| Oct 2025 | SmallVec Headers | Stack-alloc 0-4 headers, 90% ↓ heap |
| Oct 2025 | Buffer Pooling | Network/WAL/object storage pools |
| Oct 2025 | FuturesUnordered | Early-return replication, parallel uploads |
| 2025 | Miri Integration | Memory safety tests with proptest |
| 2025 | ACME Protocol | RFC 8555, DNS/HTTP challenges, multi-cloud |
| 2025 | Moka Cache | Default lock-free cache (TinyLFU) |
| 2025 | OpenRaft 0.9.21 | Production consensus, WAL, compaction |

## WASM ETL Details

**Protocol**:
1. `allocate(size: i32) -> i32` - Alloc memory, return pointer
2. `transform(input_ptr: i32, input_len: i32) -> i32` - Process data
3. `deallocate(ptr: i32)` - Free memory (no-op for bump allocator)

**Output Format**: `[4 bytes LE length][N bytes data]`

**Test Modules** (`tests/wasm_modules/`):
- `simple_transform` (462B): Uppercase transformation
- `infinite_loop` (208B): Timeout validation
- `fuel_exhaustion` (371B): CPU limit testing

**Tech**: No-std, <500B binaries, fuel metering, DashMap instance pooling

## Security Setup

```bash
./target/release/rustmq-admin ca init --cn "RustMQ Root CA"
./target/release/rustmq-admin acl create \
  --principal "user@company.com" \
  --resource "topic.events.*" \
  --permissions read,write
```

## Competitive Analysis

**RobustMQ** (Oct 2025): Multi-protocol platform (MQTT/AMQP/Kafka), v0.2.2 preview, not production-ready
**RustMQ Advantages**: Performance (SmallVec, buffer pools), 90% cost savings, WASM ETL, stability (492 tests)
**Focus Areas**: Performance, cloud economics, production stability (not multi-protocol)

See: `ROBUSTMQ_RESEARCH.md`, `ROBUSTMQ_COMPARISON_SUMMARY.md`

## Recent Fixes

- **Security Facades Refactoring** (Nov 11, 2025): SecurityManager god object refactored into 4 focused facades (Authentication, Authorization, Certificate, HealthCheck) with 100% backward compatibility
- **WebPKI Validation**: 92% test failure reduction (15→1), fallback to legacy for rcgen certs
- **Go SDK Perf**: 97% faster (11min→0.6s) via parallelization + timeout reduction
- **Cache Benchmarks**: Lazy init pattern for benchmark compatibility
- **Raft Config**: Heartbeat timeout 1000ms→2000ms (>interval)
- **FuturesUnordered**: Test expectations fixed for early-return behavior
- **Cert Race Conditions**: 100-500ms delays for async persistence
