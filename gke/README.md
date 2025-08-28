# RustMQ GKE Deployment

This directory handles **GKE cluster deployment and management**. For Docker image building, see [`../docker/`](../docker/).

## Directory Structure

```
gke/
├── base/                          # Base Kubernetes resources
│   ├── namespace.yaml             # Namespace and RBAC configuration
│   ├── storage.yaml               # Storage classes and Local SSD setup
│   ├── configmap-controller.yaml  # Controller configuration
│   ├── configmap-broker.yaml      # Broker configuration
│   ├── controller-statefulset.yaml # Controller StatefulSet
│   ├── broker-daemonset.yaml      # Broker DaemonSet
│   ├── services.yaml              # Kubernetes Services
│   ├── ingress.yaml               # Ingress and NetworkPolicies
│   ├── backend-config.yaml        # GCP BackendConfig and SecurityPolicy
│   ├── monitoring.yaml            # Prometheus monitoring setup
│   ├── hpa.yaml                   # Horizontal Pod Autoscaler
│   └── kustomization.yaml         # Base Kustomization
├── overlays/
│   ├── production/                # Production-specific configurations
│   │   └── kustomization.yaml
│   └── development/               # Development-specific configurations
│       └── kustomization.yaml
└── README.md                      # This file
```

## Base Components

### Core Infrastructure
- **namespace.yaml**: Creates `rustmq` namespace with RBAC and service accounts
- **storage.yaml**: Defines storage classes for pd-ssd and Local SSD with provisioner
- **controller-statefulset.yaml**: StatefulSet for 1-3 RustMQ controllers with persistent storage
- **broker-daemonset.yaml**: DaemonSet for brokers with Local SSD mounting

### Configuration
- **configmap-controller.yaml**: Controller configuration with OpenRaft settings
- **configmap-broker.yaml**: Broker configuration with WAL, cache, and object storage

### Networking
- **services.yaml**: Multiple services for different traffic types:
  - `controller-service`: Headless service for StatefulSet
  - `broker-service`: ClusterIP service for internal traffic
  - `broker-external`: LoadBalancer for external QUIC traffic
  - `controller-admin`: Internal LoadBalancer for admin API
- **ingress.yaml**: Ingress configuration with NetworkPolicies
- **backend-config.yaml**: GCP-specific backend configuration and security policies

### Monitoring & Autoscaling
- **monitoring.yaml**: Prometheus ServiceMonitors, alerts, and Grafana dashboard
- **hpa.yaml**: Horizontal Pod Autoscaler, VPA, and Pod Disruption Budgets

## Key Features

### Storage Architecture
- **Controllers**: Use regional pd-ssd for Raft consensus logs and metadata
- **Brokers**: Use Local SSD (375GB) for WAL with GCS for object storage
- **Auto-provisioning**: Automatic Local SSD formatting and mounting

### Security
- **mTLS**: Mutual TLS for all inter-service communication
- **Cloud Armor**: DDoS protection and geographic restrictions
- **Network Policies**: Strict ingress/egress rules
- **RBAC**: Least-privilege service accounts

### Performance
- **QUIC/HTTP3**: Low-latency client communication
- **Local SSD**: High IOPS storage for WAL (680K read, 360K write IOPS)
- **Auto-scaling**: CPU, memory, and custom metrics-based scaling
- **Load Balancing**: Geographic distribution with health checks

### Monitoring
- **Prometheus**: Comprehensive metrics collection
- **Grafana**: Pre-configured dashboards
- **Alerting**: Critical alerts for availability and performance
- **Observability**: Distributed tracing and log aggregation

## Environment Configurations

### Production Overlay
- **3 Controllers**: High availability Raft consensus
- **3-20 Brokers**: Auto-scaling based on load
- **Resource Limits**: Production-grade CPU/memory allocation
- **Security**: Full mTLS, Cloud Armor, strict policies
- **Monitoring**: Full observability stack

### Development Overlay
- **1 Controller**: Minimal setup for development
- **1-3 Brokers**: Limited scaling for cost efficiency
- **Reduced Resources**: Lower CPU/memory for development
- **Relaxed Security**: Simplified setup for iteration
- **Debug Logging**: Verbose logging for troubleshooting

## Deployment Variables

Key environment variables that need to be set before deployment:

```bash
# Google Cloud Configuration
PROJECT_ID="your-project-id"
SERVICE_ACCOUNT="rustmq-sa@${PROJECT_ID}.iam.gserviceaccount.com"
REGION="us-central1"
ZONE="us-central1-a"

# GKE Cluster Configuration
CLUSTER_NAME="rustmq-cluster"
NETWORK_NAME="rustmq-network"
SUBNET_NAME="rustmq-subnet"

# Storage Configuration
GCS_BUCKET="${PROJECT_ID}-rustmq-storage"
GCS_REGION="us-central1"

# Security Configuration
CERT_DOMAIN="rustmq.your-domain.com"
STATIC_IP_NAME="rustmq-static-ip"

# Controller Configuration
CONTROLLER_COUNT="3"
CONTROLLER_CPU_REQUEST="2"
CONTROLLER_CPU_LIMIT="4"
CONTROLLER_MEMORY_REQUEST="4Gi"
CONTROLLER_MEMORY_LIMIT="8Gi"
CONTROLLER_DISK_SIZE="100Gi"

# Broker Configuration
BROKER_MIN_COUNT="3"
BROKER_MAX_COUNT="20"
BROKER_CPU_REQUEST="4"
BROKER_CPU_LIMIT="8"
BROKER_MEMORY_REQUEST="8Gi"
BROKER_MEMORY_LIMIT="16Gi"
```

## Quick Start

### Automated Deployment (Recommended)
```bash
# Complete production deployment with infrastructure setup
./deploy-rustmq-gke.sh deploy --environment production

# Development deployment
./deploy-rustmq-gke.sh deploy --environment development

# Deploy only Kubernetes resources (skip infrastructure)
./deploy-rustmq-gke.sh deploy-k8s --environment production

# Check deployment status
./deploy-rustmq-gke.sh status

# Cleanup resources
./deploy-rustmq-gke.sh cleanup
```

### Manual Deployment with Kustomize
```bash
# Production deployment
kubectl apply -k overlays/production

# Development deployment  
kubectl apply -k overlays/development
```

## Deployment Script Features

The `deploy-rustmq-gke.sh` script provides complete automation:

### Infrastructure Setup
- **VPC Network**: Creates `rustmq-network` with custom subnet
- **GCS Bucket**: Sets up object storage with lifecycle policies  
- **Service Accounts**: Creates IAM roles and permissions
- **Firewall Rules**: Configures security policies

### GKE Cluster Creation
- **Regional Cluster**: High availability across zones
- **Node Pools**: Separate pools for controllers (pd-ssd) and brokers (Local SSD)
- **Auto-scaling**: Dynamic scaling based on workload
- **Security**: Workload Identity, shielded nodes

### Kubernetes Deployment
- **Environment Overlays**: Production vs development configurations
- **Image Validation**: Checks required images exist in registry
- **Resource Management**: CPU, memory, and storage allocation
- **Health Monitoring**: Waits for deployment readiness

### Script Options
```bash
# Command options
--environment production|development   # Environment configuration
--dry-run                             # Preview without applying
--skip-infra                          # Skip infrastructure setup
--skip-images                         # Skip image validation

# Environment variables
PROJECT_ID=my-project                 # GCP project
CLUSTER_NAME=rustmq-cluster          # Cluster name
REGION=us-central1                   # GCP region
IMAGE_TAG=latest                     # Docker image tag
```

## Verification Commands
```bash
# Check all resources
kubectl get all -n rustmq

# Check controllers
kubectl get statefulset rustmq-controller -n rustmq

# Check brokers  
kubectl get daemonset rustmq-broker -n rustmq

# View logs
kubectl logs -f statefulset/rustmq-controller -n rustmq
kubectl logs -f daemonset/rustmq-broker -n rustmq

# Port forward admin API
kubectl port-forward svc/controller-admin 9642:9642 -n rustmq
curl http://localhost:9642/health
```

---

## Complete Workflow

1. **Build Images**: `cd ../docker && ./quick-deploy.sh build-core prod`
2. **Deploy Cluster**: `./deploy-rustmq-gke.sh deploy --environment production`
3. **Verify Status**: `./deploy-rustmq-gke.sh status`

For detailed setup guide, see [`../docs/gke-deployment-guide.md`](../docs/gke-deployment-guide.md).