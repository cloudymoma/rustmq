# RustMQ Environment Configuration Guide

**Phase 2: Configuration Management** (October 2025)

This directory contains environment-specific configuration for RustMQ GKE deployments.

## Files

- **`common.env`** - Base configuration shared across all environments
- **`dev.env`** - Development environment (low cost, fast iteration)
- **`staging.env`** - Staging environment (production-like testing)
- **`prod.env`** - Production environment (high availability, enterprise security)

## Configuration Hierarchy

Configuration values are loaded with the following priority (highest to lowest):

```
prod.env (or staging.env, dev.env)
    ↓
common.env
    ↓
Default values
```

**Example**: If `BROKER_CPU_REQUEST` is defined in both `common.env` and `prod.env`, the value from `prod.env` wins.

## Quick Start

### 1. Set Your GCP Project ID

Edit the environment file for your target environment:

```bash
# For development
vim gke/environments/dev.env
# Set: GCP_PROJECT_ID="your-project-id"

# For production
vim gke/environments/prod.env
# Set: GCP_PROJECT_ID="your-project-id"
```

### 2. Set Your GCS Bucket

```bash
# In dev.env, staging.env, or prod.env
GCS_BUCKET="your-bucket-name"
```

### 3. Validate Configuration

```bash
cd gke
./validate-config.sh dev    # For development
./validate-config.sh prod   # For production
```

### 4. Deploy

```bash
cd gke
./deploy.sh dev    # Deploy to development
./deploy.sh prod   # Deploy to production
```

## Environment Comparison

| Feature | Dev | Staging | Production |
|---------|-----|---------|------------|
| **Cost** | $50-100/mo | $200-400/mo | $500-1000/mo |
| **Controller Replicas** | 1 | 3 | 5 |
| **Broker Min/Max** | 1-3 | 2-10 | 5-50 |
| **Machine Type** | e2-medium | n2-standard-4 | n2-standard-8 |
| **TLS/mTLS** | Disabled | Enabled | Enabled |
| **Workload Identity** | Disabled | Enabled | Enabled |
| **Binary Authorization** | Disabled | Enabled | Enabled |
| **Auto-Shutdown** | Enabled | Optional | Disabled |
| **Preemptible Nodes** | 80% | 30% | 0% |
| **Local SSD** | No | Yes | Yes |
| **Monitoring** | Basic | Full | Full + Alerting |
| **HA Features** | None | Enabled | Enabled + DR |

## Configuration Categories

### Infrastructure
- GCP project, region, zones
- GKE cluster name and version
- Node pools and machine types

### RustMQ Services
- Controller: replicas, resources, Raft settings
- Broker: scaling, resources, connections
- Admin: replicas, resources

### Storage
- WAL: size, storage class, local SSD
- Object Storage: GCS bucket, multipart settings
- Cache: sizes, sharding

### Security
- TLS/mTLS configuration
- Secrets (references to Secret Manager)
- ACL, network policies
- Workload Identity, Binary Authorization

### Monitoring
- Prometheus, Grafana, Alerting
- Logging level and format
- Distributed tracing

### Networking
- VPC, subnets, load balancers
- Network policies

### High Availability
- Pod Disruption Budgets
- Pod anti-affinity
- Multi-zone deployment

### Operations
- Scaling (HPA, cluster autoscaler)
- Health checks
- Graceful shutdown

## Environment-Specific Notes

### Development (`dev.env`)

**Optimized for**: Fast iteration, low cost, easy debugging

**Key Features**:
- Single controller (Raft still works)
- 1-3 brokers (minimal scaling)
- Small machine types (e2-medium)
- TLS disabled (easier debugging)
- Auto-shutdown enabled (save costs at night)
- 80% preemptible nodes (cost savings)
- Debug logging enabled

**Use for**:
- Feature development
- Quick testing
- Debugging
- Cost-effective experimentation

**NOT suitable for**:
- Performance testing
- Load testing
- Security testing
- HA testing

### Staging (`staging.env`)

**Optimized for**: Production-like testing, pre-production validation

**Key Features**:
- 3 controllers (Raft quorum)
- 2-10 brokers (moderate scaling)
- Production machine types (n2-standard-4)
- Full TLS/mTLS (like prod)
- Workload Identity (like prod)
- Binary Authorization (like prod)
- Full monitoring stack
- Multi-zone deployment
- 30% preemptible nodes (some cost savings)

**Use for**:
- Load testing
- Performance validation
- Integration testing
- Security testing
- Pre-production validation
- Release candidate testing

### Production (`prod.env`)

**Optimized for**: Maximum reliability, enterprise security, production scale

**Key Features**:
- 5 controllers (Raft quorum, tolerates 2 failures)
- 5-50 brokers (production scaling)
- Large machine types (n2-standard-8)
- **FULL security** (mTLS, Binary Auth, Workload Identity, Network Policies)
- **NO preemptible nodes** (reliability over cost)
- **NO auto-shutdown** (24/7 operation)
- Local SSD for WAL (performance)
- Full monitoring + alerting
- Disaster recovery enabled
- Compliance ready (SOC2, GDPR, HIPAA)

**CRITICAL REQUIREMENTS**:
- ✅ CONTROLLER_REPLICAS must be odd (5 recommended)
- ✅ Binary Authorization REQUIRED
- ✅ Workload Identity REQUIRED
- ✅ Network Policies REQUIRED
- ✅ TLS/mTLS REQUIRED
- ✅ Monitoring REQUIRED
- ✅ Backups REQUIRED

**BEFORE deploying to production**:
1. Test in staging first
2. Run load tests
3. Verify disaster recovery
4. Security audit
5. Team approval
6. Rollback plan ready

## Common Configuration Tasks

### Change Controller Replicas

```bash
# Edit environment file
vim gke/environments/prod.env

# Change CONTROLLER_REPLICAS (MUST be odd: 1, 3, 5, 7)
CONTROLLER_REPLICAS="5"

# Validate
./validate-config.sh prod

# Deploy
./deploy.sh prod
```

### Scale Broker Resources

```bash
# Edit environment file
vim gke/environments/prod.env

# Increase broker resources
BROKER_CPU_REQUEST="8000m"
BROKER_MEMORY_REQUEST="16Gi"

# Adjust scaling limits
BROKER_MIN_NODES="10"
BROKER_MAX_NODES="100"

# Validate and deploy
./validate-config.sh prod && ./deploy.sh prod
```

### Enable/Disable Features

```bash
# Edit environment file
vim gke/environments/staging.env

# Toggle features
ENABLE_TRACING="true"
ENABLE_BINARY_AUTHORIZATION="true"
ENABLE_AUTO_SHUTDOWN="false"

# Validate and deploy
./validate-config.sh staging && ./deploy.sh staging
```

## Configuration Validation

The `validate-config.sh` script checks:

1. **Syntax**: Valid shell syntax, no duplicate keys
2. **Types**: Numbers are numeric, booleans are true/false
3. **Semantics**: MIN <= MAX, CPU/memory reasonable
4. **GCP**: Project and region exist
5. **Raft**: CONTROLLER_REPLICAS is odd
6. **Resources**: Values within node capacity
7. **Secrets**: Secret names defined

Example validation output:

```bash
$ ./validate-config.sh prod

✓ Loading configuration...
✓ Validating syntax...
✓ Validating types...
✓ Validating GCP project...
✓ Validating Raft configuration...
ERROR: CONTROLLER_REPLICAS must be odd for Raft quorum
  Current value: 4
  Suggested values: 3, 5, 7
  File: gke/environments/prod.env:42
```

## Security Best Practices

### Secrets Management

**DO NOT** put actual secrets in `.env` files!

```bash
# ✅ CORRECT: Reference secrets by name
TLS_CERT_SECRET_NAME="rustmq-prod-tls-cert"

# ❌ WRONG: Never put actual secrets here
TLS_CERT="-----BEGIN CERTIFICATE-----..."  # NEVER DO THIS!
```

Secrets are stored in **Google Secret Manager** and synced to Kubernetes via **External Secrets Operator**.

### Committing Configuration

**Safe to commit**:
- ✅ `common.env` - No secrets
- ✅ `dev.env` - No secrets
- ✅ `staging.env` - No secrets
- ✅ `prod.env` - No secrets (references only)

**NEVER commit**:
- ❌ Files with `.secret` extension
- ❌ Files with actual credentials
- ❌ Private keys or certificates

## Troubleshooting

### Configuration Not Loading

```bash
# Check file permissions
ls -la gke/environments/

# Verify syntax
bash -n gke/environments/prod.env

# Check for duplicate keys
sort gke/environments/prod.env | uniq -d
```

### Validation Failures

```bash
# Run validation in verbose mode
VERBOSE=true ./validate-config.sh prod

# Skip specific checks (use with caution)
SKIP_GCP_CHECKS=true ./validate-config.sh prod
```

### Configuration Drift

```bash
# Check if deployed config differs from files
./drift-detect.sh prod

# Show differences
./drift-detect.sh prod --show-diff
```

## Advanced Topics

### Multi-Region Deployment

```bash
# In prod.env, add secondary region
GCP_SECONDARY_REGION="us-east1"
ENABLE_DISASTER_RECOVERY="true"
```

### Custom Raft Tuning

```bash
# Adjust Raft timeouts (in milliseconds)
CONTROLLER_RAFT_HEARTBEAT_INTERVAL="1000"
CONTROLLER_RAFT_HEARTBEAT_TIMEOUT="2000"   # Must be > interval
CONTROLLER_RAFT_ELECTION_TIMEOUT="5000"
```

### Resource Optimization

```bash
# Use Vertical Pod Autoscaler recommendations
ENABLE_VPA="true"
VPA_MODE="Recreate"  # Apply VPA recommendations
```

## References

- [GKE Best Practices](https://cloud.google.com/kubernetes-engine/docs/best-practices)
- [OpenRaft Configuration](https://docs.rs/openraft/)
- [RustMQ Documentation](../GKE_DEPLOYMENT_ANALYSIS.md)
- [Phase 2 Validation Schema](../config-schema.json)

---

**Version**: 1.0
**Last Updated**: October 2025
**Phase**: Phase 2 - Configuration Management
