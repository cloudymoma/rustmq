# RustMQ Deployment Procedure

This guide provides the complete, step-by-step procedure for deploying a production RustMQ cluster on Google Kubernetes Engine (GKE).

## 1. Environment Preparation

### GCP Project Setup
```bash
export PROJECT_ID="rustmq-production"
gcloud projects create $PROJECT_ID
gcloud config set project $PROJECT_ID
```

### Enable APIs
```bash
gcloud services enable \
    container.googleapis.com \
    compute.googleapis.com \
    secretmanager.googleapis.com \
    storage.googleapis.com \
    cloudkms.googleapis.com
```

## 2. Infrastructure Setup

### Create GKE Cluster
Use the provided script for an optimized cluster:
```bash
cd gke
./create-cluster.sh --environment prod
```

### Provision Storage
```bash
# Create message bucket
export GCS_BUCKET="rustmq-messages-$PROJECT_ID"
gsutil mb -l us-central1 gs://$GCS_BUCKET

# Apply StorageClasses
kubectl apply -f storage/storageclass.yaml
```

## 3. Security Infrastructure

### Initialize Root CA
```bash
# Use the admin CLI (locally or via helper pod)
rustmq-admin ca init --cn "RustMQ Production CA" --org "MyOrg"
```

### Setup Secrets
```bash
cd gke/secrets
./setup-secrets.sh
```

## 4. Docker Images

### Build and Push
```bash
cd docker
PROJECT_ID=$PROJECT_ID ./quick-deploy.sh production-images
```

## 5. Deployment

### Apply Manifests
```bash
cd gke
PROJECT_ID=$PROJECT_ID ./deploy-rustmq-gke.sh deploy --environment production
```

### Verify Deployment
```bash
./deploy-rustmq-gke.sh status --environment production
```

## 6. Post-Deployment

### Configure Monitoring
1. Verify `ServiceMonitor` resources are created in the `rustmq` namespace.
2. Access the Google Cloud Console "Monitoring" tab.
3. Import dashboards from `monitoring/dashboards/`.

### Load Testing
Run a sanity load test to verify performance:
```bash
cargo test --release --test integration_load_test -- --ignored
```
