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

## 1.0 Release Status (Jan 6, 2026)

| Week | Phase | Status |
|------|-------|--------|
| 1 | Docker Critical Fixes | ✅ Complete |
| 2 | GKE Critical Fixes | ✅ Complete |
| 3 | Security (RBAC, scanning) | ✅ Complete |
| 4 | Monitoring & Observability | ✅ Complete |
| 5 | Backup/DR & Cost Optimization | Pending |
| 6 | Load Testing & Hardening | Pending |
| 7 | Final Validation & Launch | Pending |

**Readiness**: 90% | **Target**: Jan 15, 2026 | **Blockers**: 17/19 resolved

**Details**: See `plan.md` for full 7-week roadmap

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

## Quick Reference

```bash
# Security setup
./target/release/rustmq-admin ca init --cn "RustMQ Root CA"
./target/release/rustmq-admin acl create --principal "user@example.com" --resource "topic.*" --permissions read,write

# Run services
cargo run --bin rustmq-controller -- --config config/controller.toml
cargo run --bin rustmq-broker -- --config config/broker.toml
```
