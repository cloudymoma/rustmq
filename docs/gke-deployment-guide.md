# RustMQ GKE Deployment Guide

## Overview

This guide provides a streamlined approach to deploying RustMQ on Google Kubernetes Engine (GKE). RustMQ's cloud-native architecture with storage-compute separation makes it ideal for GKE deployment.

## Prerequisites

### 📋 Required Setup
1. **Google Cloud Project** with billing enabled
2. **GKE API** enabled: `gcloud services enable container.googleapis.com`
3. **`gcloud` CLI** authenticated and configured
4. **`kubectl`** installed and configured

## Two-Step Deployment

### 1. Build Images
```bash
cd docker/
PROJECT_ID=your-project-id ./quick-deploy.sh build-core prod
```

### 2. Deploy Cluster
```bash
cd ../gke/
PROJECT_ID=your-project-id ./deploy-rustmq-gke.sh deploy --environment production
```

That's it! The deployment script handles all infrastructure setup, cluster creation, and application deployment automatically.

## Environment Options

### Development Cluster (Single Zone)
```bash
# Build images
cd docker/ && ./quick-deploy.sh dev-build

# Deploy development cluster
cd ../gke/ && ./deploy-rustmq-gke.sh deploy --environment development
```

**Features**: 1 controller, 1-3 brokers, reduced resources, simplified security

### Production Cluster (Single Zone)  
```bash
# Build and push production images
cd docker/ && PROJECT_ID=your-project ./quick-deploy.sh production-images

# Deploy production cluster
cd ../gke/ && PROJECT_ID=your-project ./deploy-rustmq-gke.sh deploy --environment production
```

**Features**: 3 controllers, 3-20 brokers, full security, monitoring, auto-scaling

## Verification

### Check Deployment Status
```bash
# Check deployment status
./deploy-rustmq-gke.sh status

# Check pods
kubectl get pods -n rustmq

# View logs
kubectl logs -f statefulset/rustmq-controller -n rustmq

# Port forward admin API
kubectl port-forward svc/controller-admin 9642:9642 -n rustmq
curl http://localhost:9642/health
```

### Test Cluster
```bash
# Access admin CLI (if running locally)
docker-compose exec rustmq-admin bash
rustmq-admin cluster-health
rustmq-admin create-topic test 3 2
rustmq-admin list-topics
```

## Cleanup

```bash
# Remove entire deployment
./deploy-rustmq-gke.sh cleanup
```

⚠️ **Warning**: This removes all data and resources. Use with caution.

---

## Advanced Configuration

For detailed configuration options, troubleshooting, and advanced features, see:

- **[gke/README.md](../gke/README.md)** - Complete deployment details, architecture, and configurations
- **[docker/README.md](../docker/README.md)** - Image building and local development setup
- **[README.md](../README.md)** - Project overview and getting started