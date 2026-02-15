# CLAUDE.md

## Build Commands

```bash
cargo build --release && cargo test --lib      # Debug build + tests
cargo test --release                            # Release tests (includes benchmarks)
cargo check && cargo clippy && cargo fmt        # Code quality
cd docker && ./build-and-push.sh build          # Docker images
kubectl kustomize gke/manifests/overlays/prod   # Validate K8s manifests
```

## Architecture

**RustMQ**: Cloud-native message queue with 90% storage cost savings via GCS/S3/Azure.

**Flow**: `Client → QUIC:9092 → Broker → WAL → Cache → Cloud Storage`

**Ports**:
- Controller: 9094 (gRPC), 9095 (Raft), 9642 (Admin HTTP)
- Broker: 9092 (QUIC/UDP), 9093 (gRPC), 9096 (ETL), 9643 (Admin HTTP)

**Key Dirs**: `src/storage/`, `src/network/`, `src/controller/`, `src/security/`, `src/etl/`

## 1.0 Release Status (Feb 12, 2026)

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Build & Compile Verification | ✅ Complete |
| 2 | Dead Code & Simplification | Pending |
| 3 | Security Hardening | Pending |
| 4 | Performance Review | Pending |
| 5 | GCS Integration | Pending |
| 6 | SDK Validation | Pending |
| 7 | Docker Build Process | Pending |
| 8 | GKE Deployment | Pending |
| 9 | Testing Gaps | Pending |
| 10 | Documentation | Pending |
| 11 | Cleanup | Pending |

**Details**: See `plan.md` for full review plan with priority matrix

## Key Resources

| Category | Files |
|----------|-------|
| GKE Manifests | `gke/manifests/base/`, `gke/manifests/overlays/{dev,staging,prod}/` |
| Docker | `docker/Dockerfile.*`, `docker/build-and-push.sh` |
| Config | `config/broker.toml`, `config/controller.toml` |
| Security | `scripts/scan-images.sh`, `gke/scripts/validate-secrets.sh` |
| Monitoring | `gke/manifests/base/monitoring/` (ServiceMonitors, PrometheusRules) |

## Tests

- **Unit**: 501 passing | **Integration**: 6 tests (graceful shutdown, 10K msg/sec load)
- **Load tests**: `cargo test --release -- --ignored`

## Performance (All Implemented)

SmallVec headers (90%↓ heap), Lock-free pools (547ns ACL), Buffer pooling (30-40%↓ alloc), Zero-copy API, FuturesUnordered replication, CPU-adaptive L2 cache, Moka TinyLFU cache

## Dependency Updates (Jan 2026)

| Package | Old | New | Notes |
|---------|-----|-----|-------|
| rustls | 0.21 | 0.23 | CVE-2024-32650 fix, Certificate→CertificateDer |
| aws-sdk-s3 | 0.35 | 1.68 | Major API update |
| quinn | 0.10 | 0.11 | finish() now sync |
| tonic | 0.11 | 0.12 | prost 0.13 |
| hyper | 0.14 | 1.5 | hyper-util, http-body-util |
| rustls-pemfile | 1.x | 2.2 | Iterator<Result> API |

## Quick Reference

```bash
# Security setup
./target/release/rustmq-admin ca init --cn "RustMQ Root CA"
./target/release/rustmq-admin acl create --principal "user@example.com" --resource "topic.*" --permissions read,write

# Run services
cargo run --bin rustmq-controller -- --config config/controller.toml
cargo run --bin rustmq-broker -- --config config/broker.toml
```
