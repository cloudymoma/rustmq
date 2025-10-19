# RustMQ GKE Deployment Guide

**Phase 3: GKE Infrastructure Optimization** (October 2025)

Complete guide for deploying RustMQ to Google Kubernetes Engine (GKE).

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Quick Start](#quick-start)
3. [Deployment Workflow](#deployment-workflow)
4. [Environment Configuration](#environment-configuration)
5. [Manual Deployment Steps](#manual-deployment-steps)
6. [Deployment Scripts](#deployment-scripts)
7. [Post-Deployment Tasks](#post-deployment-tasks)
8. [Monitoring and Operations](#monitoring-and-operations)
9. [Troubleshooting](#troubleshooting)
10. [Cost Estimation](#cost-estimation)

## Prerequisites

### Required Tools

Install the following tools on your local machine:

```bash
# Google Cloud SDK
curl https://sdk.cloud.google.com | bash
exec -l $SHELL
gcloud init

# kubectl
gcloud components install kubectl

# kustomize
curl -s "https://raw.githubusercontent.com/kubernetes-sigs/kustomize/master/hack/install_kustomize.sh" | bash
sudo mv kustomize /usr/local/bin/

# Helm 3
curl https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash
```

### GCP Project Setup

1. **Create GCP Project** (if not exists):
   ```bash
   gcloud projects create rustmq-project --name="RustMQ"
   gcloud config set project rustmq-project
   ```

2. **Enable Required APIs**:
   ```bash
   gcloud services enable container.googleapis.com
   gcloud services enable compute.googleapis.com
   gcloud services enable secretmanager.googleapis.com
   gcloud services enable storage-api.googleapis.com
   gcloud services enable cloudresourcemanager.googleapis.com
   ```

3. **Set Up Billing**:
   ```bash
   # Link billing account (required)
   gcloud beta billing accounts list
   gcloud beta billing projects link rustmq-project \
     --billing-account=BILLING_ACCOUNT_ID
   ```

4. **Grant Permissions**:
   ```bash
   # Your user needs these roles:
   # - roles/container.admin (GKE cluster management)
   # - roles/compute.admin (Network/LB management)
   # - roles/secretmanager.admin (Secret Manager)
   # - roles/storage.admin (GCS buckets)
   ```

### Docker Images

Build and push Docker images to Artifact Registry:

```bash
# Create Artifact Registry repository
gcloud artifacts repositories create rustmq \
  --repository-format=docker \
  --location=us \
  --project=PROJECT_ID

# Configure Docker authentication
gcloud auth configure-docker us-docker.pkg.dev

# Build and push images (from project root)
cd docker
./build-multiplatform.sh broker --push
./build-multiplatform.sh controller --push
./build-multiplatform.sh admin-server --push
```

## Quick Start

### Development Environment (Single Command)

```bash
# Navigate to gke directory
cd gke

# Deploy everything
./deploy.sh dev
```

This single command will:
1. Validate configuration
2. Create GKE cluster
3. Install External Secrets Operator
4. Set up Google Secret Manager integration
5. Deploy RustMQ (controllers, brokers, admin server)
6. Configure networking
7. Verify deployment

**Time**: ~15-20 minutes

### Production Environment (With Confirmations)

```bash
# Step-by-step with validation
./deploy.sh prod
```

## Deployment Workflow

```
┌──────────────────────────────────────────────────────────────┐
│ Phase 1: Preparation                                         │
├──────────────────────────────────────────────────────────────┤
│ 1. Configure environment (environments/prod.env)             │
│ 2. Validate configuration (validate-config.sh)               │
│ 3. Build Docker images                                       │
└──────────────────────────────────────────────────────────────┘
                         ↓
┌──────────────────────────────────────────────────────────────┐
│ Phase 2: Infrastructure                                      │
├──────────────────────────────────────────────────────────────┤
│ 1. Create GKE cluster (create-cluster.sh)                    │
│ 2. Configure kubectl context                                 │
│ 3. Set up Workload Identity                                  │
│ 4. Create GCS bucket                                         │
└──────────────────────────────────────────────────────────────┘
                         ↓
┌──────────────────────────────────────────────────────────────┐
│ Phase 3: Secrets Management                                  │
├──────────────────────────────────────────────────────────────┤
│ 1. Install External Secrets Operator                         │
│ 2. Create secrets in Google Secret Manager                   │
│ 3. Apply SecretStore and ExternalSecrets                     │
│ 4. Verify secrets synced to K8s                              │
└──────────────────────────────────────────────────────────────┘
                         ↓
┌──────────────────────────────────────────────────────────────┐
│ Phase 4: Application Deployment                              │
├──────────────────────────────────────────────────────────────┤
│ 1. Apply StorageClass                                        │
│ 2. Deploy RustMQ with Kustomize                              │
│ 3. Wait for pods to be ready                                 │
│ 4. Configure networking (Ingress, load balancers)            │
└──────────────────────────────────────────────────────────────┘
                         ↓
┌──────────────────────────────────────────────────────────────┐
│ Phase 5: Verification                                        │
├──────────────────────────────────────────────────────────────┤
│ 1. Check pod health                                          │
│ 2. Verify services have endpoints                            │
│ 3. Run drift detection                                       │
│ 4. Test connectivity                                         │
└──────────────────────────────────────────────────────────────┘
```

## Environment Configuration

### Development (`dev`)

**Purpose**: Local development and testing

**Configuration**:
- 1 controller (no HA)
- 1-3 brokers (autoscaling)
- e2-medium nodes
- 80% preemptible nodes
- Zonal cluster (single zone)
- TLS disabled (faster iteration)

**Cost**: ~$50-100/month

**Use Cases**:
- Feature development
- Testing
- Learning RustMQ

### Staging (`staging`)

**Purpose**: Pre-production testing

**Configuration**:
- 3 controllers (tolerates 1 failure)
- 2-10 brokers (autoscaling)
- n2-standard-4 nodes
- 30% preemptible nodes
- Regional cluster (3 zones)
- Full TLS/mTLS enabled

**Cost**: ~$200-400/month

**Use Cases**:
- Integration testing
- Load testing
- Client SDK testing
- Pre-production validation

### Production (`prod`)

**Purpose**: Production workloads

**Configuration**:
- 5 controllers (tolerates 2 failures)
- 5-50 brokers (autoscaling)
- n2-standard-8 nodes
- 0% preemptible nodes (reliability)
- Regional cluster (3 zones)
- Full security stack (TLS, mTLS, Binary Authorization)
- Pod anti-affinity (spread across zones)

**Cost**: ~$500-1000/month

**Use Cases**:
- Production message processing
- Mission-critical workloads
- High availability requirements

## Manual Deployment Steps

If you prefer step-by-step deployment instead of using `deploy.sh`:

### Step 1: Configure Environment

```bash
# Edit environment configuration
vim gke/environments/prod.env

# Update these variables:
# - GCP_PROJECT_ID
# - GCP_REGION
# - CLUSTER_NAME
# - GCS_BUCKET
```

### Step 2: Validate Configuration

```bash
./gke/validate-config.sh prod
```

### Step 3: Create GKE Cluster

```bash
./gke/create-cluster.sh prod
```

### Step 4: Install External Secrets Operator

```bash
helm repo add external-secrets https://charts.external-secrets.io
helm repo update

helm install external-secrets \
  external-secrets/external-secrets \
  --namespace external-secrets-system \
  --create-namespace \
  --set installCRDs=true
```

### Step 5: Create Secrets in Google Secret Manager

```bash
./gke/secrets/setup-secrets.sh prod create
./gke/secrets/setup-secrets.sh prod grant-access
```

### Step 6: Apply Storage Configuration

```bash
kubectl apply -f gke/storage/storageclass.yaml
```

### Step 7: Deploy RustMQ

```bash
# Deploy with Kustomize
cd gke/manifests/overlays/prod

# Replace PROJECT_ID and apply
kustomize build . | sed "s/PROJECT_ID/YOUR_PROJECT_ID/g" | kubectl apply -f -
```

### Step 8: Apply Secrets Configuration

```bash
kubectl apply -f gke/secrets/external-secrets/secret-store.yaml
kubectl apply -f gke/secrets/external-secrets/external-secrets.yaml
```

### Step 9: Verify Deployment

```bash
# Check pod status
kubectl get pods -n rustmq

# Check services
kubectl get svc -n rustmq

# Check secrets synced
kubectl get externalsecrets -n rustmq

# Run drift detection
./gke/drift-detect.sh prod
```

## Deployment Scripts

### Primary Scripts

| Script | Purpose | Example |
|--------|---------|---------|
| `deploy.sh` | Master deployment orchestration | `./deploy.sh prod` |
| `create-cluster.sh` | Create GKE cluster | `./create-cluster.sh staging` |
| `delete-cluster.sh` | Delete GKE cluster safely | `./delete-cluster.sh dev` |
| `validate-config.sh` | Validate environment config | `./validate-config.sh prod` |
| `drift-detect.sh` | Detect configuration drift | `./drift-detect.sh prod --show-diff` |

### Secrets Management Scripts

| Script | Purpose | Example |
|--------|---------|---------|
| `secrets/setup-secrets.sh` | Manage Google Secret Manager | `./secrets/setup-secrets.sh prod create` |

### Configuration Scripts

| Script | Purpose | Example |
|--------|---------|---------|
| `scripts/load-config.sh` | Load environment configuration | `source scripts/load-config.sh prod` |

## Post-Deployment Tasks

### 1. Verify Pod Health

```bash
# Check all pods are running
kubectl get pods -n rustmq

# Watch pod status
kubectl get pods -n rustmq -w

# Check specific components
kubectl get pods -l app.kubernetes.io/component=controller -n rustmq
kubectl get pods -l app.kubernetes.io/component=broker -n rustmq
```

### 2. Test Connectivity

```bash
# Port forward to broker
kubectl port-forward svc/rustmq-broker 9092:9092 -n rustmq

# Test with RustMQ CLI (from another terminal)
rustmq-cli produce --topic test --message "Hello from GKE!"

# Port forward to admin API
kubectl port-forward svc/rustmq-admin-server 8080:8080 -n rustmq

# Test admin API
curl http://localhost:8080/health
```

### 3. Configure Monitoring (Optional)

```bash
# Install Prometheus Operator
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
helm install prometheus prometheus-community/kube-prometheus-stack \
  --namespace monitoring \
  --create-namespace

# Port forward to Grafana
kubectl port-forward svc/prometheus-grafana 3000:80 -n monitoring

# Access Grafana at http://localhost:3000 (admin/prom-operator)
```

### 4. Set Up Ingress (Production Only)

```bash
# Reserve static IP
gcloud compute addresses create rustmq-admin-ip --global

# Get IP address
gcloud compute addresses describe rustmq-admin-ip --global --format="get(address)"

# Configure DNS A record pointing to this IP

# Update ingress.yaml with your domain
vim gke/network/ingress.yaml

# Apply ingress
kubectl apply -f gke/network/ingress.yaml

# Wait for certificate provisioning (15-30 minutes)
kubectl get managedcertificate rustmq-admin-cert -n rustmq -w
```

## Monitoring and Operations

### Viewing Logs

```bash
# Controller logs
kubectl logs -f rustmq-controller-0 -n rustmq

# Broker logs (all replicas)
kubectl logs -f deployment/rustmq-broker -n rustmq

# Admin server logs
kubectl logs -f deployment/rustmq-admin-server -n rustmq

# Logs from last hour
kubectl logs --since=1h -l app.kubernetes.io/name=rustmq -n rustmq
```

### Scaling

```bash
# Scale brokers manually
kubectl scale deployment rustmq-broker -n rustmq --replicas=10

# Check HPA status
kubectl get hpa -n rustmq

# Edit HPA limits
kubectl edit hpa rustmq-broker -n rustmq
```

### Updates and Rollbacks

```bash
# Update image tag
kubectl set image deployment/rustmq-broker \
  broker=us-docker.pkg.dev/PROJECT_ID/rustmq/rustmq-broker:v1.1.0 \
  -n rustmq

# Check rollout status
kubectl rollout status deployment/rustmq-broker -n rustmq

# Rollback if needed
kubectl rollout undo deployment/rustmq-broker -n rustmq

# Check rollout history
kubectl rollout history deployment/rustmq-broker -n rustmq
```

### Configuration Drift

```bash
# Detect drift
./gke/drift-detect.sh prod

# Show detailed differences
./gke/drift-detect.sh prod --show-diff

# Auto-fix drift (use with caution!)
./gke/drift-detect.sh prod --auto-fix
```

## Troubleshooting

### Pods Not Starting

**Problem**: Pods stuck in `Pending` or `CrashLoopBackOff`

**Diagnosis**:
```bash
kubectl describe pod rustmq-controller-0 -n rustmq
kubectl logs rustmq-controller-0 -n rustmq
```

**Common Causes**:
- Insufficient resources (check node capacity)
- Image pull errors (check image exists in registry)
- PVC binding issues (check StorageClass)
- Secrets not synced (check ExternalSecrets)

### Secrets Not Syncing

**Problem**: ExternalSecrets showing "SecretSyncedError"

**Diagnosis**:
```bash
kubectl describe externalsecret rustmq-tls-cert -n rustmq
kubectl logs -n external-secrets-system deployment/external-secrets -f
```

**Solutions**:
- Verify Workload Identity annotation on service account
- Check IAM permissions for service account
- Verify secrets exist in Google Secret Manager
- Check SecretStore configuration

### Load Balancer Not Working

**Problem**: Service stuck in `<pending>` for external IP

**Diagnosis**:
```bash
kubectl describe svc rustmq-broker -n rustmq
kubectl get events -n rustmq
```

**Solutions**:
- Check quota limits in GCP (load balancers, IP addresses)
- Verify firewall rules allow health checks
- Check service selector matches pod labels

### High Pod Latency

**Problem**: Slow response times from pods

**Diagnosis**:
```bash
# Check pod CPU/memory usage
kubectl top pods -n rustmq

# Check node resources
kubectl top nodes

# Check for throttling
kubectl describe pod rustmq-broker-0 -n rustmq | grep -i throttl
```

**Solutions**:
- Increase resource limits
- Scale horizontally (more replicas)
- Check for I/O bottlenecks (storage)

## Cost Estimation

### Monthly Costs by Environment

**Development**:
| Resource | Cost |
|----------|------|
| GKE cluster | $30-50 |
| Compute (e2-medium × 1-3 nodes, 80% preemptible) | $20-40 |
| Storage (10GB SSD) | $2 |
| Load balancers (2× internal) | $20 |
| GCS bucket (<100GB) | $2 |
| Networking | $5-10 |
| **Total** | **$79-124/month** |

**Staging**:
| Resource | Cost |
|----------|------|
| GKE cluster | $30-50 |
| Compute (n2-standard-4 × 3-10 nodes, 30% preemptible) | $150-400 |
| Storage (150GB SSD) | $26 |
| Load balancers (2× internal + 1× external) | $35-50 |
| GCS bucket (<500GB) | $10 |
| Cloud Armor | $6 |
| Networking | $20-40 |
| **Total** | **$277-582/month** |

**Production**:
| Resource | Cost |
|----------|------|
| GKE cluster | $30-50 |
| Compute (n2-standard-8 × 5-50 nodes, 0% preemptible) | $500-3000 |
| Storage (500GB SSD) | $85 |
| Load balancers (2× internal + 1× external) | $35-50 |
| GCS bucket (5TB) | $100 |
| Cloud Armor | $6 |
| Networking | $100-300 |
| Snapshots/backups | $20-50 |
| **Total** | **$876-3641/month** |

### Cost Optimization Tips

1. **Use Preemptible Nodes** (dev/staging): 60-80% cost reduction
2. **Right-size Nodes**: Monitor usage and adjust machine types
3. **Enable Cluster Autoscaler**: Scale down during low traffic
4. **Use Standard Storage Tier**: For non-critical GCS data
5. **Clean Up Unused Resources**: Delete old snapshots, unused load balancers
6. **Use Committed Use Discounts**: 37-57% discount for 1-3 year commitment

## Implementation Progress

### Phase 1: Docker Build Optimization ✅ Completed (October 2025)

**Objective**: Optimize Docker image building for faster CI/CD and smaller production images.

**Key Achievements:**
- ✅ cargo-chef dependency caching (60-70% faster builds)
- ✅ Distroless runtime base (40-50% smaller images)
- ✅ Multi-platform builds (AMD64 + ARM64)
- ✅ Security hardening (non-root, minimal attack surface)

**Deliverables:**
- Optimized Dockerfiles for all components (broker, controller, admin, admin-server)
- Multi-platform build scripts
- Build cache configuration
- Image size reduction from ~250MB to ~150MB

**For Details**: See [Docker Build Documentation](../docker/README.md)

---

### Phase 2: Configuration Management ✅ Completed (October 2025)

**Objective**: Implement robust environment-specific configuration with Google Cloud native secrets management.

**Key Achievements:**
- ✅ Environment-based configuration (dev, staging, prod)
- ✅ Google Secret Manager integration with External Secrets Operator
- ✅ Workload Identity for keyless authentication
- ✅ Configuration validation with drift detection
- ✅ Automated secret rotation

**Deliverables:**
- Environment configuration files (`environments/*.env`)
- Configuration validation script (`validate-config.sh`)
- External Secrets Operator manifests (`secrets/external-secrets/`)
- Secret management scripts (`secrets/setup-secrets.sh`)
- Drift detection script (`drift-detect.sh`)

**Configuration Structure:**
```
gke/
├── environments/          # Environment-specific configs
│   ├── dev.env           # Development configuration
│   ├── staging.env       # Staging configuration
│   └── prod.env          # Production configuration
├── secrets/              # Secrets management
│   ├── setup-secrets.sh  # Secret provisioning script
│   └── external-secrets/ # K8s ExternalSecret manifests
└── scripts/
    └── load-config.sh    # Config loader utility
```

**Security Features:**
- Zero service account keys (Workload Identity)
- Automatic secret rotation
- Centralized secret storage (Google Secret Manager)
- Audit logging for secret access

---

### Phase 3: GKE Infrastructure Optimization ✅ Completed (October 2025)

**Objective**: Deploy production-ready RustMQ cluster to GKE with full automation.

**Key Achievements:**
- ✅ Automated GKE cluster creation with VPC-native networking
- ✅ Kustomize-based multi-environment manifests (base + overlays)
- ✅ Storage optimization (Regional SSD + GCS hybrid)
- ✅ Network security (NetworkPolicies, Cloud Armor, mTLS)
- ✅ Single-command deployment (`deploy.sh`)
- ✅ Comprehensive monitoring and operations tooling

**Deliverables:**

**Cluster Infrastructure:**
- Automated cluster creation (`create-cluster.sh`)
- Safe cluster deletion (`delete-cluster.sh`)
- Master deployment orchestration (`deploy.sh`)

**Kubernetes Manifests:**
- Base manifests (namespace, serviceaccounts, configmaps, deployments, services, HPA, NetworkPolicies)
- Environment overlays (dev, staging, prod) with patches for:
  - Replica counts (1 controller for dev → 5 for prod)
  - Resource limits (e2-medium → n2-standard-8)
  - Security policies (Pod Security Standards)
  - Pod anti-affinity for HA

**Storage Architecture:**
- Regional SSD for controller Raft WAL (HA, low latency)
- GCS for message storage (90% cost savings)
- Workload Identity for keyless GCS access

**Network Architecture:**
- Internal Load Balancers for broker and controller
- External HTTPS Load Balancer for admin API
- Cloud Armor for DDoS protection
- NetworkPolicies for default-deny security
- VPC-native cluster with private nodes

**Operations:**
- Configuration drift detection
- Automated scaling (HPA)
- Health monitoring
- Log aggregation

**Cost Optimization:**
- Development: ~$79-124/month
- Staging: ~$277-582/month
- Production: ~$876-3641/month

---

## Architecture Overview

### Storage Layers
1. **WAL (Write-Ahead Log)**: Regional SSD for Raft consensus
2. **L1 Cache**: In-memory hot data
3. **Object Storage**: GCS for message persistence (90% cost reduction)

### Network Architecture
```
Internet → Cloud Armor → GKE Ingress → Admin API (ClusterIP)

VPC (Private Network)
  ├── Internal LB (Broker) → Broker Pods (5-50 replicas)
  └── Internal LB (Controller) → Controller StatefulSet (3-5 replicas)
```

### Security
- **Authentication**: mTLS with WebPKI validation
- **Authorization**: Fast ACL (547ns lookups)
- **Secrets**: Google Secret Manager + External Secrets Operator
- **Network**: Default-deny NetworkPolicies
- **Container**: Non-root user (UID 65532), read-only filesystem

---

## References

- [Docker Build Documentation](../docker/README.md)
- [Storage Architecture](storage/README.md)
- [Network Architecture](network/README.md)
- [Secrets Management](secrets/README.md)

## Support

For issues or questions:
1. Review logs: `kubectl logs -l app.kubernetes.io/name=rustmq -n rustmq`
2. Run diagnostics: `./drift-detect.sh <env> --show-diff`
3. Check pod status: `kubectl get pods -n rustmq`
4. Verify secrets: `kubectl get externalsecrets -n rustmq`

---

**Version**: 1.0
**Last Updated**: October 2025
**Status**: All phases completed and production-ready
