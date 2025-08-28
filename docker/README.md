# RustMQ Docker Image Building

This directory handles **Docker image building and testing only**. For cluster deployment, see [`../gke/`](../gke/).

## 📋 Table of Contents

- [Quick Start](#-quick-start)
- [Build Scripts](#-build-scripts)
- [Docker Components](#-docker-components)
- [Local Development](#-local-development)
- [Image Registry](#-image-registry)
- [Build Configuration](#-build-configuration)

## 🚀 Quick Start

```bash
# Build core components for GKE deployment
./quick-deploy.sh build-core prod

# Build all components for development
./quick-deploy.sh dev-build

# Build and push production images
./quick-deploy.sh production-images

# For detailed building options
./build-and-push.sh help
```

## 🛠️ Build Scripts

Two master scripts handle all image operations:

### `quick-deploy.sh` - High-Level Workflows
```bash
./quick-deploy.sh build-core prod     # Build controller + broker for GKE
./quick-deploy.sh build-all dev       # Build all components 
./quick-deploy.sh production-images   # Production build with validation
./quick-deploy.sh hotfix-images prod controller  # Emergency hotfix
./quick-deploy.sh dev-build           # Local development (no push)
./quick-deploy.sh status              # Show images and config
```

### `build-and-push.sh` - Granular Control
```bash
./build-and-push.sh build controller  # Build specific component
./build-and-push.sh push broker       # Push specific component
./build-and-push.sh all               # Build and push all
./build-and-push.sh list              # Show components
./build-and-push.sh clean             # Remove local images
```

## 🐳 Docker Components

### Dockerfiles
- **Dockerfile.broker** - Message broker with QUIC and tiered storage
- **Dockerfile.controller** - Controller with OpenRaft consensus  
- **Dockerfile.admin** - Admin CLI tool
- **Dockerfile.admin-server** - Admin REST API server
- **Dockerfile.bigquery-subscriber** - BigQuery real-time streaming

### Local Development
- **docker-compose.yml** - Complete local cluster for development and testing

## 🧪 Local Development

Complete local cluster for development and testing:

```bash
# Start local development cluster
docker-compose up -d

# View cluster status
docker-compose ps

# Test cluster health
docker-compose exec rustmq-admin rustmq-admin cluster-health

# View logs
docker-compose logs -f

# Clean shutdown
docker-compose down
```

### Local Cluster Architecture
- **3 Controllers** - Raft consensus cluster
- **3 Brokers** - Message brokers with local storage
- **MinIO** - S3-compatible object storage
- **Admin CLI** - Management tools

For detailed local setup, see [Docker Compose Configuration](#docker-compose-configuration) section.

## 📦 Image Registry

Images are built and pushed to container registries for deployment:

### Registry Configuration
```bash
# Google Container Registry (default)
PROJECT_ID=my-project ./quick-deploy.sh build-core prod

# Custom registry
REGISTRY_HOST=my-registry.com PROJECT_ID=my-project ./build-and-push.sh all

# Docker Hub
REGISTRY_HOST=docker.io ./build-and-push.sh push controller
```

### Image Tags
- `latest` - Latest build
- `v1.0.0` - Version tag
- `abc123` - Git commit SHA
- `prod` - Production builds

### Image Names
- `gcr.io/PROJECT_ID/rustmq-controller:TAG`
- `gcr.io/PROJECT_ID/rustmq-broker:TAG`
- `gcr.io/PROJECT_ID/rustmq-admin:TAG`

## ⚙️ Build Configuration

### Environment Variables
```bash
# Core Configuration
PROJECT_ID=your-project-id        # GCP project ID
REGISTRY_HOST=gcr.io              # Container registry
IMAGE_TAG=latest                  # Image tag
VERSION=v1.0.0                    # Version label

# Build Configuration  
CARGO_BUILD_PROFILE=release       # Rust build profile
RUST_TARGET=x86_64-unknown-linux-gnu  # Target architecture
DOCKER_BUILDKIT=1                 # Enable BuildKit

# Advanced Options
MULTI_ARCH=false                  # Multi-architecture builds
SCAN_IMAGES=false                 # Vulnerability scanning
PARALLEL_BUILDS=4                 # Concurrent builds
```

### Build Context
All builds use the project root as context to access source code and dependencies.

### Registry Authentication
```bash
# GCR authentication (automatic)
gcloud auth configure-docker

# Docker Hub authentication
docker login

# Custom registry
docker login my-registry.com
```

## Docker Compose Configuration

<details>
<summary>Click to expand local development details</summary>

### Architecture
- **3 Controllers** - Raft consensus cluster  
- **3 Brokers** - Message brokers with local storage
- **MinIO** - S3-compatible object storage
- **Admin CLI** - Management tools

### Service Endpoints
| Service | External Port | Purpose |
|---------|---------------|---------|
| Broker 1 | 9092/9093 | QUIC/RPC |
| Controller 1 | 9094/9095/9642 | RPC/Raft/HTTP |
| MinIO | 9000/9001 | API/Console |

### Admin CLI Usage
```bash
# Access admin container
docker-compose exec rustmq-admin bash

# Cluster management commands
rustmq-admin create-topic events 3 2
rustmq-admin list-topics
rustmq-admin cluster-health
```

### Environment Variables
```bash
# Core settings
BROKER_ID=broker-1
RACK_ID=us-central1-a
OBJECT_STORAGE_ENDPOINT=http://minio:9000

# Authentication (MinIO)
OBJECT_STORAGE_ACCESS_KEY=rustmq-access-key
OBJECT_STORAGE_SECRET_KEY=rustmq-secret-key
```

### Troubleshooting
```bash
# Check logs
docker-compose logs -f rustmq-broker-1

# Restart services
docker-compose restart rustmq-controller-1

# Complete reset (destroys data)
docker-compose down -v && docker-compose up -d
```

</details>

---

## Next Steps

- **For GKE Deployment**: See [`../gke/README.md`](../gke/README.md)
- **For Production Setup**: See [`../docs/gke-deployment-guide.md`](../docs/gke-deployment-guide.md)
- **For Project Overview**: See [`../README.md`](../README.md)