# RustMQ Docker & Kubernetes Deployment

This directory contains all Docker and Kubernetes resources for deploying RustMQ in containerized environments.

## 📋 Table of Contents

- [Quick Start](#-quick-start)
- [Docker Components](#-docker-components)
- [Docker Compose Setup](#-docker-compose-setup)
- [Service Endpoints](#-service-endpoints)
- [Kubernetes Deployment](#-kubernetes-deployment)
- [Configuration](#-configuration)
- [Development Workflow](#-development-workflow)
- [Production Deployment](#-production-deployment)
- [Troubleshooting](#-troubleshooting)

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/cloudymoma/rustmq.git
cd rustmq/docker

# Start the development cluster
docker-compose up -d

# View cluster status
docker-compose ps

# View logs from all services
docker-compose logs -f

# Test the cluster with admin CLI
docker-compose exec rustmq-admin rustmq-admin cluster-health
```

## 🐳 Docker Components

This directory contains the following Docker resources:

### Dockerfiles

- **Dockerfile.broker** - RustMQ message broker (fully implemented with all core components)
- **Dockerfile.controller** - RustMQ controller (production-ready with Raft consensus)  
- **Dockerfile.admin** - Admin CLI tool (fully implemented with topic management and cluster health)
- **Dockerfile.admin-server** - Standalone admin REST API server
- **Dockerfile.bigquery-subscriber** - Google BigQuery subscriber for real-time data streaming

### Docker Compose

- **docker-compose.yml** - Complete development cluster orchestration with all services

### Kubernetes

- **kubernetes/** - Production Kubernetes manifests and configurations

## 🔧 Docker Compose Setup

The Docker Compose configuration provides a complete local development environment with:

### Architecture

- **3 Broker nodes** (`rustmq-broker-1/2/3`) - Fully implemented broker instances with all core components
- **3 Controller nodes** (`rustmq-controller-1/2/3`) - Production-ready controller services with Raft consensus
- **MinIO** - S3-compatible object storage for local development
- **Admin CLI** - Production-ready admin tool with comprehensive topic management and cluster health monitoring
- **Admin Server** - REST API server for cluster management
- **BigQuery Subscriber** - Optional demo BigQuery integration

### Starting the Cluster

```bash
# Start the basic development cluster
docker-compose up -d

# Start with BigQuery subscriber (requires GCP configuration)
docker-compose --profile bigquery up -d

# Start specific services only
docker-compose up -d minio rustmq-controller-1 rustmq-broker-1

# Scale brokers for testing
docker-compose up -d --scale rustmq-broker-2=2
```

### Environment Configuration

Set these environment variables for BigQuery integration:

```bash
# Required for BigQuery subscriber
export GCP_PROJECT_ID="your-gcp-project"
export BIGQUERY_DATASET="analytics"
export BIGQUERY_TABLE="events"
export RUSTMQ_TOPIC="user-events"

# Optional authentication
export AUTH_METHOD="application_default"
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
```

## 📊 Service Endpoints

| Service | Internal Port | External Port | Purpose | Status |
|---------|---------------|---------------|---------|--------|
| Broker 1 | 9092/9093 | 9092/9093 | QUIC/RPC | **Functional** |
| Broker 2 | 9092/9093 | 9192/9193 | QUIC/RPC | **Functional** |  
| Broker 3 | 9092/9093 | 9292/9293 | QUIC/RPC | **Functional** |
| Controller 1 | 9094/9095/9642 | 9094/9095/9642 | RPC/Raft/HTTP | **Production** |
| Controller 2 | 9094/9095/9642 | 9144/9145/9643 | RPC/Raft/HTTP | **Production** |
| Controller 3 | 9094/9095/9642 | 9194/9195/9644 | RPC/Raft/HTTP | **Production** |
| Admin REST API | 8080 | 8080 | Cluster Management | **Functional** |
| MinIO | 9000/9001 | 9000/9001 | API/Console | Functional |
| BigQuery Subscriber | 8081 | 8081 | Health/Metrics | Demo |

### Service Access Examples

```bash
# Check controller health
curl http://localhost:9642/health

# Admin REST API cluster status
curl http://localhost:8080/api/v1/cluster

# MinIO web console
open http://localhost:9001
# Login: rustmq-access-key / rustmq-secret-key

# BigQuery subscriber health
curl http://localhost:8081/health
```

## 🎯 Using the Admin CLI

```bash
# Access the admin CLI container
docker-compose exec rustmq-admin bash

# **FULLY IMPLEMENTED** - Production-ready topic management commands
rustmq-admin create-topic <name> <partitions> <replication_factor>  # ✅ Create topics with validation
rustmq-admin list-topics                                            # ✅ List all topics with details
rustmq-admin describe-topic <name>                                  # ✅ Show topic configuration and partitions
rustmq-admin delete-topic <name>                                    # ✅ Delete topics safely
rustmq-admin cluster-health                                         # ✅ Comprehensive cluster health analysis

# Start the Admin REST API server (FULLY IMPLEMENTED)
rustmq-admin serve-api [port]                                       # ✅ Full REST API with health tracking

# Example usage:
rustmq-admin create-topic events 3 2          # Creates 'events' topic with 3 partitions, replication factor 2
rustmq-admin list-topics                       # Shows formatted table of all topics
rustmq-admin describe-topic events             # Detailed view with partition assignments
rustmq-admin cluster-health                    # Health status of brokers, topics, and overall cluster
```

## ☸️ Kubernetes Deployment

### Prerequisites

```bash
# For GKE (Google Kubernetes Engine)
gcloud container clusters get-credentials rustmq-cluster --zone=us-central1-a

# For local Kubernetes
kubectl config current-context
```

### Basic Deployment

```bash
# Navigate to Kubernetes directory
cd kubernetes/

# Apply namespace and basic resources
kubectl apply -f namespace.yaml
kubectl apply -f configmap.yaml
kubectl apply -f secrets.yaml

# Deploy controllers and brokers
kubectl apply -f controller.yaml
kubectl apply -f broker.yaml
kubectl apply -f services.yaml

# Wait for deployment
kubectl wait --for=condition=ready pod -l app=rustmq-broker -n rustmq --timeout=300s

# Check deployment status
kubectl get pods -n rustmq
kubectl get services -n rustmq
```

### Storage Configuration

```yaml
# Fast SSD storage class for optimal performance
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: fast-ssd
provisioner: kubernetes.io/gce-pd
parameters:
  type: pd-ssd
  replication-type: regional-pd
  zones: us-central1-a,us-central1-b
allowVolumeExpansion: true
reclaimPolicy: Delete
volumeBindingMode: WaitForFirstConsumer
```

### Service Account Setup

```bash
# Create service account for GCP access
gcloud iam service-accounts create rustmq-sa \
    --display-name="RustMQ Service Account"

# Grant storage permissions
gcloud projects add-iam-policy-binding $PROJECT_ID \
    --member="serviceAccount:rustmq-sa@$PROJECT_ID.iam.gserviceaccount.com" \
    --role="roles/storage.objectAdmin"

# Create Kubernetes secret
gcloud iam service-accounts keys create rustmq-key.json \
    --iam-account=rustmq-sa@$PROJECT_ID.iam.gserviceaccount.com

kubectl create secret generic rustmq-gcp-credentials \
    --from-file=key.json=rustmq-key.json -n rustmq
```

## ⚙️ Configuration

### Environment Variables

Key environment variables for Docker containers:

```bash
# Core broker settings
BROKER_ID=broker-1
RACK_ID=us-central1-a
QUIC_LISTEN=0.0.0.0:9092
RPC_LISTEN=0.0.0.0:9093

# Controller settings
CONTROLLER_NODE_ID=controller-1
CONTROLLER_LISTEN_RPC=0.0.0.0:9094
CONTROLLER_LISTEN_RAFT=0.0.0.0:9095
CONTROLLER_LISTEN_HTTP=0.0.0.0:9642

# Storage settings
WAL_PATH=/var/lib/rustmq/wal
OBJECT_STORAGE_TYPE=S3
OBJECT_STORAGE_ENDPOINT=http://minio:9000
OBJECT_STORAGE_BUCKET=rustmq-data
OBJECT_STORAGE_REGION=us-central1

# Authentication (for MinIO)
OBJECT_STORAGE_ACCESS_KEY=rustmq-access-key
OBJECT_STORAGE_SECRET_KEY=rustmq-secret-key

# Logging
RUST_LOG=info
RUSTMQ_LOG_LEVEL=info
```

### Volume Configuration

The Docker Compose setup includes persistent volumes:

```yaml
volumes:
  minio_data:              # MinIO object storage data
  controller1_data:        # Controller 1 Raft logs and metadata
  controller2_data:        # Controller 2 Raft logs and metadata  
  controller3_data:        # Controller 3 Raft logs and metadata
  broker1_wal:            # Broker 1 WAL storage
  broker1_data:           # Broker 1 cache and temporary data
  broker2_wal:            # Broker 2 WAL storage
  broker2_data:           # Broker 2 cache and temporary data
  broker3_wal:            # Broker 3 WAL storage
  broker3_data:           # Broker 3 cache and temporary data
```

## 🔄 Development Workflow

### Making Code Changes

```bash
# Make code changes to RustMQ source
cd ..  # Go back to project root
# Edit source files...

# Rebuild specific service
docker-compose build rustmq-broker
docker-compose up -d rustmq-broker-1

# Or rebuild all services
docker-compose build
docker-compose up -d
```

### Testing Changes

```bash
# View logs to verify changes
docker-compose logs -f rustmq-broker-1

# Test with admin CLI
docker-compose exec rustmq-admin rustmq-admin cluster-health

# Scale services for load testing
docker-compose up -d --scale rustmq-broker-1=3
```

### Development vs Production

```bash
# Development: Start with reduced resources
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d

# Production: Use production overrides
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

## 🏭 Production Deployment

### Container Registry

```bash
# Build and tag images for production
docker build -f Dockerfile.broker -t rustmq/broker:v1.0.0 ..
docker build -f Dockerfile.controller -t rustmq/controller:v1.0.0 ..
docker build -f Dockerfile.admin -t rustmq/admin:v1.0.0 ..

# Push to container registry
docker push rustmq/broker:v1.0.0
docker push rustmq/controller:v1.0.0
docker push rustmq/admin:v1.0.0
```

### Production Configuration

```yaml
# Production resource limits
resources:
  requests:
    memory: "4Gi"
    cpu: "2"
    ephemeral-storage: "50Gi"
  limits:
    memory: "8Gi" 
    cpu: "4"
    ephemeral-storage: "100Gi"
```

### Health Checks

All containers include comprehensive health checks:

```yaml
healthcheck:
  test: ["CMD", "curl", "-f", "http://localhost:9642/health"]
  interval: 30s
  timeout: 10s
  retries: 3
  start_period: 60s
```

### Security Considerations

```bash
# Use non-root user (already configured in Dockerfiles)
USER rustmq

# Set resource constraints
deploy:
  resources:
    limits:
      cpus: '4.0'
      memory: 8G
      pids: 1000
```

## 🔍 Troubleshooting

### Common Issues

#### Services Not Starting

```bash
# Check container logs
docker-compose logs rustmq-broker-1
docker-compose logs rustmq-controller-1

# Check resource usage
docker stats

# Restart specific service
docker-compose restart rustmq-broker-1
```

#### Network Connectivity Issues

```bash
# Test network connectivity between containers
docker-compose exec rustmq-broker-1 nc -zv rustmq-controller-1 9094

# Check bridge network
docker network inspect docker_rustmq-network
```

#### Storage Issues

```bash
# Check volume usage
docker volume ls
docker volume inspect docker_broker1_wal

# Clean up volumes (WARNING: destroys data)
docker-compose down -v
```

#### MinIO Access Issues

```bash
# Verify MinIO is accessible
curl http://localhost:9000/minio/health/live

# Check bucket creation
docker-compose logs rustmq-minio-init

# Manually create bucket
docker-compose exec minio mc mb minio/rustmq-data --ignore-existing
```

### Performance Troubleshooting

```bash
# Monitor resource usage
docker-compose exec rustmq-broker-1 top
docker-compose exec rustmq-broker-1 free -h
docker-compose exec rustmq-broker-1 df -h

# Check container resource limits
docker inspect rustmq-broker-1 | jq '.[].HostConfig.Memory'
```

### Debug Mode

```bash
# Start with debug logging
RUST_LOG=debug docker-compose up -d

# Or modify docker-compose.yml temporarily:
environment:
  - RUST_LOG=debug
  - RUSTMQ_LOG_LEVEL=debug
```

### Clean Reset

```bash
# Complete cleanup and restart
docker-compose down -v  # Remove volumes (data loss!)
docker-compose build --no-cache
docker-compose up -d
```

## 📚 Additional Resources

- [Main README](../README.md) - Overall project documentation
- [Configuration Guide](../docs/configuration.md) - Detailed configuration options
- [Security Guide](../docs/security/) - Security setup and best practices
- [Admin CLI Reference](../docs/admin-cli.md) - Complete CLI command reference
- [Monitoring Setup](../docs/monitoring.md) - Observability and metrics

## 🔗 Useful Commands

```bash
# Quick health check of entire cluster
docker-compose exec rustmq-admin rustmq-admin cluster-health

# View all container resource usage
docker stats $(docker-compose ps -q)

# Follow logs from all services
docker-compose logs -f

# Clean shutdown preserving data
docker-compose down

# Complete reset (destroys all data)
docker-compose down -v && docker-compose up -d
```

---

**Note**: This Docker setup provides a complete development environment. For production deployments, additional considerations for security, monitoring, backup, and disaster recovery should be implemented.