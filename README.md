# RustMQ: Cloud-Native Distributed Message Queue System

[![Build Status](https://github.com/cloudymoma/rustmq/workflows/Rust/badge.svg)](https://github.com/cloudymoma/rustmq/actions)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust Version](https://img.shields.io/badge/rust-1.73+-blue.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.1.0-green.svg)](https://github.com/rustmq/rustmq)

RustMQ is a next-generation, cloud-native distributed message queue system that combines the high-performance characteristics of Apache Kafka with the cost-effectiveness and operational simplicity of modern cloud architectures. Built from the ground up in Rust, RustMQ leverages a shared-storage architecture that decouples compute from storage, enabling unprecedented elasticity, cost savings, and operational efficiency.

**Optimized for Google Cloud Platform**: RustMQ is designed with Google Cloud services as the default target, leveraging Google Cloud Storage for cost-effective object storage and Google Kubernetes Engine for orchestration, with all configurations defaulting to the `us-central1` region for optimal performance and cost efficiency.

## 🚀 Key Features

- **10x Cost Reduction**: 90% storage cost savings through single-copy storage in Google Cloud Storage
- **100x Elasticity**: Instant scaling with stateless brokers and metadata-only operations  
- **Single-Digit Millisecond Latency**: Optimized write path with local NVMe WAL and zero-copy data movement
- **QUIC/HTTP3 Protocol**: Reduced connection overhead and head-of-line blocking elimination
- **WebAssembly ETL**: Real-time data processing with secure sandboxing
- **Auto-Balancing**: Continuous load distribution optimization
- **Google Cloud Native**: Default configurations optimized for GCP services 

## 📋 Table of Contents

- [Quick Start](#-quick-start)
- [Docker Development Setup](#-docker-development-setup)
- [BigQuery Subscriber](#-bigquery-subscriber)
- [Google Cloud Platform Setup](#-google-cloud-platform-setup)
- [Deployment](#-deployment)
- [Configuration](#-configuration)
- [Usage Examples](#-usage-examples)
- [API Reference](#-api-reference)
- [Performance Tuning](#-performance-tuning)
- [Monitoring](#-monitoring)
- [Contributing](#-contributing)

## 🚧 Development Status

**RustMQ is currently in early development stage.** The current implementation includes:

### ✅ Implemented Components
- **Configuration System**: Complete TOML-based configuration with validation
- **Storage Abstractions**: Trait-based storage layer design (WAL, Object Storage, Cache)
- **BigQuery Subscriber**: Functional BigQuery integration demo
- **Scaling Operations**: Decommissioning slot management and scaling logic
- **Docker Environment**: Complete Docker Compose setup for development
- **Project Structure**: Well-organized module structure and build system

### 🚧 In Development
- **Network Layer**: QUIC/gRPC server implementations (basic structure)
- **Replication System**: Leader-follower replication logic (traits defined)
- **Controller Service**: Cluster coordination (placeholder implementation)
- **ETL Processing**: WebAssembly-based stream processing (framework)

### ❌ Not Yet Implemented
- **Message Broker Core**: Actual message producing/consuming functionality
- **Distributed Coordination**: Raft consensus and metadata management
- **Client Libraries**: Rust, Go, and other language clients
- **Admin API**: REST API for cluster management
- **Production Features**: Monitoring, metrics, health checks

### Current Capabilities
- Build and run placeholder broker/controller services
- Load and validate configuration files
- Demonstrate BigQuery subscriber functionality
- Test scaling operation logic with mock implementations

## 🏃 Quick Start

### Prerequisites

- Rust 1.73+ and Cargo
- Docker and Docker Compose
- Google Cloud SDK (for BigQuery subscriber demo)
- kubectl (for future Kubernetes deployment)

### Local Development Setup

```bash
# Clone the repository
git clone https://github.com/cloudymoma/rustmq.git
cd rustmq

# Build the project
cargo build --release

# Run tests
cargo test

# Start local development environment with Docker Compose
docker-compose up -d

# Or run individual components locally (placeholder implementations):
# Run broker (loads config and sleeps)
./target/release/rustmq-broker --config config/broker.toml

# Run controller (loads config and sleeps) 
./target/release/rustmq-controller --config config/controller.toml

# Run admin CLI (shows available commands)
./target/release/rustmq-admin create-topic test-topic 3 2
```

## 🐳 Docker Development Setup

RustMQ provides a Docker-based development environment for local testing and development.

### Docker Components

The following Docker containers are available:

- **Dockerfile.broker** - RustMQ message broker (early development stage)
- **Dockerfile.controller** - RustMQ controller (placeholder implementation)  
- **Dockerfile.admin** - Admin CLI tool (basic command structure)
- **Dockerfile.bigquery-subscriber** - Google BigQuery subscriber demo

### Starting the Development Environment

```bash
# Start the basic development cluster
docker-compose up -d

# View cluster status
docker-compose ps

# View logs from all services
docker-compose logs -f

# View logs from specific service
docker-compose logs -f rustmq-broker-1
```

### Development Architecture

The Docker Compose setup includes:

- **3 Broker nodes** (`rustmq-broker-1/2/3`) - Early-stage broker implementations
- **3 Controller nodes** (`rustmq-controller-1/2/3`) - Placeholder controller services
- **MinIO** - S3-compatible object storage for local development
- **Admin CLI** - Basic admin tool with command structure
- **BigQuery Subscriber** - Demo BigQuery integration

**Note**: The current implementation is in early development. Most services are placeholder implementations that load configuration and demonstrate the intended architecture.

### Service Endpoints

| Service | Internal Port | External Port | Purpose | Status |
|---------|---------------|---------------|---------|--------|
| Broker 1 | 9092/9093 | 9092/9093 | QUIC/RPC | Placeholder |
| Broker 2 | 9092/9093 | 9192/9193 | QUIC/RPC | Placeholder |  
| Broker 3 | 9092/9093 | 9292/9293 | QUIC/RPC | Placeholder |
| Controller 1 | 9094/9095/9642 | 9094/9095/9642 | RPC/Raft/HTTP | Placeholder |
| Controller 2 | 9094/9095/9642 | 9144/9145/9643 | RPC/Raft/HTTP | Placeholder |
| Controller 3 | 9094/9095/9642 | 9194/9195/9644 | RPC/Raft/HTTP | Placeholder |
| MinIO | 9000/9001 | 9000/9001 | API/Console | Functional |
| BigQuery Subscriber | 8080 | 8080 | Health/Metrics | Demo |

### Using the Admin CLI

```bash
# Access the admin CLI container
docker-compose exec rustmq-admin bash

# Available commands (placeholder implementations)
rustmq-admin create-topic <name> <partitions> <replication_factor>
rustmq-admin list-topics
rustmq-admin describe-topic <name>
rustmq-admin delete-topic <name>
rustmq-admin cluster-health

# Note: Commands show usage but are not yet implemented
```

### Development Workflow

```bash
# Make code changes and rebuild specific service
docker-compose build rustmq-broker
docker-compose up -d rustmq-broker-1

# Scale brokers for testing
docker-compose up -d --scale rustmq-broker-2=2

# Clean shutdown
docker-compose down

# Clean shutdown with volume cleanup
docker-compose down -v
```

### Container Features

Each Dockerfile includes:
- **Multi-stage builds** for optimized image size
- **Security** - Non-root user execution with gosu
- **Health checks** - Proper container health monitoring
- **Configuration** - Environment variable templating
- **Logging** - Structured logging with configurable levels
- **Dependencies** - Proper startup ordering and readiness checks

## 📊 BigQuery Subscriber

RustMQ includes a configurable Google Cloud BigQuery subscriber that can stream messages from RustMQ topics directly to BigQuery tables with high throughput and reliability.

### Key Features

- **Streaming Inserts**: Direct streaming to BigQuery using the insertAll API
- **Storage Write API**: Future support for BigQuery Storage Write API (higher throughput)
- **Configurable Batching**: Optimize for latency vs throughput with flexible batching
- **Schema Mapping**: Direct, custom, or nested JSON field mapping
- **Error Handling**: Comprehensive retry logic with dead letter handling
- **Monitoring**: Built-in health checks and metrics endpoints
- **Authentication**: Support for service account, metadata server, and application default credentials

### Quick Start with BigQuery

```bash
# Set required environment variables
export GCP_PROJECT_ID="your-gcp-project"
export BIGQUERY_DATASET="analytics"
export BIGQUERY_TABLE="events"
export RUSTMQ_TOPIC="user-events"

# Optional: Set authentication method
export AUTH_METHOD="application_default"  # or "service_account"
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"

# Start the cluster with BigQuery subscriber
docker-compose --profile bigquery up -d

# Check BigQuery subscriber health
curl http://localhost:8080/health

# View metrics
curl http://localhost:8080/metrics
```

### Configuration Options

The BigQuery subscriber supports extensive configuration through environment variables:

#### Required Configuration
- `GCP_PROJECT_ID` - Google Cloud Project ID
- `BIGQUERY_DATASET` - BigQuery dataset name
- `BIGQUERY_TABLE` - BigQuery table name
- `RUSTMQ_TOPIC` - RustMQ topic to subscribe to

#### Authentication
- `AUTH_METHOD` - Authentication method (`application_default`, `service_account`, `metadata_server`)
- `GOOGLE_APPLICATION_CREDENTIALS` - Path to service account key file

#### Batching & Performance
- `MAX_ROWS_PER_BATCH` - Maximum rows per batch (default: 1000)
- `MAX_BATCH_SIZE_BYTES` - Maximum batch size in bytes (default: 10MB)
- `MAX_BATCH_LATENCY_MS` - Maximum time to wait before sending partial batch (default: 1000ms)
- `MAX_CONCURRENT_BATCHES` - Maximum concurrent batches (default: 10)

#### Schema Mapping
- `SCHEMA_MAPPING` - Mapping strategy (`direct`, `custom`, `nested`)
- `AUTO_CREATE_TABLE` - Whether to auto-create table if not exists (default: false)

#### Error Handling
- `MAX_RETRIES` - Maximum retry attempts (default: 3)
- `DEAD_LETTER_ACTION` - Action for failed messages (`log`, `drop`, `dead_letter_queue`, `file`)
- `RETRY_BASE_MS` - Base retry delay in milliseconds (default: 1000)
- `RETRY_MAX_MS` - Maximum retry delay in milliseconds (default: 30000)

### Usage Examples

#### Basic Streaming Configuration

```bash
# Start with basic streaming inserts
docker run --rm \
  -e GCP_PROJECT_ID="my-project" \
  -e BIGQUERY_DATASET="analytics" \
  -e BIGQUERY_TABLE="events" \
  -e RUSTMQ_TOPIC="user-events" \
  -e RUSTMQ_BROKERS="rustmq-broker:9092" \
  rustmq/bigquery-subscriber
```

#### High-Throughput Configuration

```bash
# Optimized for high throughput
docker run --rm \
  -e GCP_PROJECT_ID="my-project" \
  -e BIGQUERY_DATASET="telemetry" \
  -e BIGQUERY_TABLE="metrics" \
  -e RUSTMQ_TOPIC="telemetry-data" \
  -e MAX_ROWS_PER_BATCH="5000" \
  -e MAX_BATCH_SIZE_BYTES="52428800" \
  -e MAX_BATCH_LATENCY_MS="500" \
  -e MAX_CONCURRENT_BATCHES="50" \
  rustmq/bigquery-subscriber
```

#### Custom Schema Mapping

Create a custom configuration file:

```toml
# bigquery-config.toml
project_id = "my-project"
dataset = "transformed_data"
table = "processed_events"

[write_method.streaming_inserts]
skip_invalid_rows = true
ignore_unknown_values = true

[subscription]
topic = "raw-events"
broker_endpoints = ["rustmq-broker:9092"]

[schema]
mapping = "custom"

[schema.column_mappings]
"event_id" = "id"
"event_timestamp" = "timestamp" 
"user_data.user_id" = "user_id"
"event_data.action" = "action"

[schema.default_values]
"processed_at" = "CURRENT_TIMESTAMP()"
"version" = "1.0"
```

```bash
# Use custom configuration
docker run --rm \
  -v $(pwd)/bigquery-config.toml:/etc/rustmq/custom-config.toml \
  -e CONFIG_FILE="/etc/rustmq/custom-config.toml" \
  rustmq/bigquery-subscriber
```

### Monitoring and Observability

The BigQuery subscriber exposes health and metrics endpoints:

```bash
# Health check endpoint
curl http://localhost:8080/health
# Response: {"status":"healthy","last_successful_insert":"2023-...", ...}

# Metrics endpoint  
curl http://localhost:8080/metrics
# Response: {"messages_received":1500,"messages_processed":1487, ...}
```

### Error Handling and Reliability

The subscriber includes comprehensive error handling:

- **Automatic Retries**: Configurable exponential backoff for transient errors
- **Dead Letter Handling**: Failed messages can be logged, dropped, or sent to dead letter queue
- **Health Monitoring**: Continuous health checks with degraded/unhealthy states
- **Graceful Shutdown**: Ensures all pending batches are processed during shutdown

### Production Deployment

For production deployments:

1. **Use Service Account Authentication**:
   ```bash
   export AUTH_METHOD="service_account"
   export GOOGLE_APPLICATION_CREDENTIALS="/etc/gcp/service-account.json"
   ```

2. **Optimize Batching for Your Workload**:
   - High volume: Increase batch size and reduce latency
   - Low latency: Reduce batch size and latency threshold
   - Mixed workload: Use default settings

3. **Monitor Key Metrics**:
   - Messages processed per second
   - Error rate and retry counts
   - BigQuery insertion latency
   - Backlog size

4. **Set Up Alerting**:
   - Health endpoint failures
   - High error rates (>5%)
   - Growing backlog size
   - BigQuery quota issues

## ☁️ Google Cloud Platform Setup

### Step 1: GCP Project Setup

```bash
# Set your project ID
export PROJECT_ID="your-rustmq-project"
export REGION="us-central1"
export ZONE="us-central1-a"

# Create and configure project
gcloud projects create $PROJECT_ID
gcloud config set project $PROJECT_ID
gcloud auth login

# Enable required APIs
gcloud services enable container.googleapis.com
gcloud services enable storage-api.googleapis.com
gcloud services enable compute.googleapis.com
gcloud services enable cloudresourcemanager.googleapis.com
```

### Step 2: GKE Cluster Setup

```bash
# Create GKE cluster with optimized node pools
gcloud container clusters create rustmq-cluster \
    --zone=$ZONE \
    --machine-type=n2-standard-4 \
    --num-nodes=3 \
    --enable-autorepair \
    --enable-autoupgrade \
    --enable-network-policy \
    --enable-ip-alias \
    --disk-type=pd-ssd \
    --disk-size=50GB \
    --max-nodes=10 \
    --min-nodes=3 \
    --enable-autoscaling

# Get credentials
gcloud container clusters get-credentials rustmq-cluster --zone=$ZONE

# Create storage class for fast SSD
kubectl apply -f - <<EOF
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
EOF
```

### Step 3: Cloud Storage Setup

```bash
# Create bucket for object storage
gsutil mb -c STANDARD -l $REGION gs://$PROJECT_ID-rustmq-data

# Enable versioning and lifecycle management
gsutil versioning set on gs://$PROJECT_ID-rustmq-data

# Create lifecycle policy for cost optimization
cat > lifecycle.json <<EOF
{
  "lifecycle": {
    "rule": [
      {
        "action": {"type": "SetStorageClass", "storageClass": "NEARLINE"},
        "condition": {"age": 30}
      },
      {
        "action": {"type": "SetStorageClass", "storageClass": "COLDLINE"},
        "condition": {"age": 90}
      },
      {
        "action": {"type": "Delete"},
        "condition": {"age": 365}
      }
    ]
  }
}
EOF

gsutil lifecycle set lifecycle.json gs://$PROJECT_ID-rustmq-data
```

### Step 4: Service Account Setup

```bash
# Create service account for RustMQ
gcloud iam service-accounts create rustmq-sa \
    --display-name="RustMQ Service Account" \
    --description="Service account for RustMQ cluster operations"

# Grant necessary permissions
gcloud projects add-iam-policy-binding $PROJECT_ID \
    --member="serviceAccount:rustmq-sa@$PROJECT_ID.iam.gserviceaccount.com" \
    --role="roles/storage.objectAdmin"

gcloud projects add-iam-policy-binding $PROJECT_ID \
    --member="serviceAccount:rustmq-sa@$PROJECT_ID.iam.gserviceaccount.com" \
    --role="roles/monitoring.writer"

# Create and download key
gcloud iam service-accounts keys create rustmq-key.json \
    --iam-account=rustmq-sa@$PROJECT_ID.iam.gserviceaccount.com

# Create Kubernetes secret
kubectl create secret generic rustmq-gcp-credentials \
    --from-file=key.json=rustmq-key.json
```

### Step 5: Networking Setup

```bash
# Create firewall rules for RustMQ
gcloud compute firewall-rules create rustmq-quic \
    --allow tcp:9092,udp:9092 \
    --source-ranges 0.0.0.0/0 \
    --description "RustMQ QUIC traffic"

gcloud compute firewall-rules create rustmq-rpc \
    --allow tcp:9093 \
    --source-ranges 10.0.0.0/8 \
    --description "RustMQ internal RPC traffic"

gcloud compute firewall-rules create rustmq-admin \
    --allow tcp:9642 \
    --source-ranges 0.0.0.0/0 \
    --description "RustMQ admin API"
```

## 🚀 Deployment

### Kubernetes Deployment

Create the deployment manifests:

```yaml
# k8s/namespace.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: rustmq
---
# k8s/configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: rustmq-config
  namespace: rustmq
data:
  broker.toml: |
    [broker]
    id = "${HOSTNAME}"
    rack_id = "${NODE_ZONE}"

    [network]
    quic_listen = "0.0.0.0:9092"
    rpc_listen = "0.0.0.0:9093"
    max_connections = 10000
    connection_timeout_ms = 30000

    [wal]
    path = "/var/lib/rustmq/wal"
    capacity_bytes = 10737418240
    fsync_on_write = true
    segment_size_bytes = 1073741824
    buffer_size = 65536

    [cache]
    write_cache_size_bytes = 1073741824
    read_cache_size_bytes = 2147483648
    eviction_policy = "Lru"

    [object_storage]
    type = "Gcs"
    bucket = "${GCS_BUCKET}"
    region = "${GCP_REGION}"
    endpoint = "https://storage.googleapis.com"
    multipart_threshold = 104857600
    max_concurrent_uploads = 10

    [controller]
    endpoints = ["rustmq-controller:9094"]
    election_timeout_ms = 5000
    heartbeat_interval_ms = 1000

    [replication]
    min_in_sync_replicas = 2
    ack_timeout_ms = 5000
    max_replication_lag = 1000

  controller.toml: |
    [controller]
    node_id = "${HOSTNAME}"
    raft_listen = "0.0.0.0:9095"
    rpc_listen = "0.0.0.0:9094"
    http_listen = "0.0.0.0:9642"

    [raft]
    peers = [
      "rustmq-controller-0@rustmq-controller-0.rustmq-controller:9095",
      "rustmq-controller-1@rustmq-controller-1.rustmq-controller:9095",
      "rustmq-controller-2@rustmq-controller-2.rustmq-controller:9095"
    ]

    # Note: Metastore configuration not yet implemented
    # Future versions will include distributed coordination

    [autobalancer]
    enabled = true
    cpu_threshold = 0.80
    memory_threshold = 0.75
    cooldown_seconds = 300
---
# k8s/broker.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: rustmq-broker
  namespace: rustmq
spec:
  serviceName: rustmq-broker
  replicas: 3
  selector:
    matchLabels:
      app: rustmq-broker
  template:
    metadata:
      labels:
        app: rustmq-broker
    spec:
      serviceAccountName: rustmq
      containers:
      - name: broker
        image: rustmq/broker:latest
        ports:
        - containerPort: 9092
          name: quic
        - containerPort: 9093
          name: rpc
        env:
        - name: HOSTNAME
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: NODE_ZONE
          valueFrom:
            fieldRef:
              fieldPath: metadata.labels['topology.kubernetes.io/zone']
        - name: GCS_BUCKET
          value: "${PROJECT_ID}-rustmq-data"
        - name: GCP_REGION
          value: "${REGION}"
        - name: GOOGLE_APPLICATION_CREDENTIALS
          value: "/var/secrets/google/key.json"
        volumeMounts:
        - name: config
          mountPath: /etc/rustmq
        - name: wal-storage
          mountPath: /var/lib/rustmq/wal
        - name: gcp-credentials
          mountPath: /var/secrets/google
          readOnly: true
        resources:
          requests:
            memory: "4Gi"
            cpu: "2"
          limits:
            memory: "8Gi"
            cpu: "4"
        livenessProbe:
          httpGet:
            path: /health
            port: 9642
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 9642
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: config
        configMap:
          name: rustmq-config
      - name: gcp-credentials
        secret:
          secretName: rustmq-gcp-credentials
  volumeClaimTemplates:
  - metadata:
      name: wal-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 50Gi
---
# k8s/service.yaml
apiVersion: v1
kind: Service
metadata:
  name: rustmq-broker
  namespace: rustmq
spec:
  type: LoadBalancer
  selector:
    app: rustmq-broker
  ports:
  - name: quic
    port: 9092
    targetPort: 9092
    protocol: TCP
  - name: rpc
    port: 9093
    targetPort: 9093
    protocol: TCP
---
apiVersion: v1
kind: Service
metadata:
  name: rustmq-admin
  namespace: rustmq
spec:
  type: LoadBalancer
  selector:
    app: rustmq-controller
  ports:
  - name: http
    port: 80
    targetPort: 9642
    protocol: TCP
```

Deploy to Kubernetes:

```bash
# Apply configurations
envsubst < k8s/configmap.yaml | kubectl apply -f -
kubectl apply -f k8s/namespace.yaml
kubectl apply -f k8s/broker.yaml
kubectl apply -f k8s/service.yaml

# Wait for deployment
kubectl wait --for=condition=ready pod -l app=rustmq-broker -n rustmq --timeout=300s

# Get external IP
kubectl get service rustmq-broker -n rustmq
```

## ⚙️ Configuration

**Note**: This is the intended configuration structure. Current implementation includes placeholder services that load and validate this configuration but don't fully implement the functionality.

### Broker Configuration (`broker.toml`)

```toml
[broker]
id = "broker-001"                    # Unique broker identifier
rack_id = "us-central1-a"            # Availability zone for rack awareness

[network]
quic_listen = "0.0.0.0:9092"        # QUIC/HTTP3 client endpoint
rpc_listen = "0.0.0.0:9093"         # Internal gRPC endpoint
max_connections = 10000              # Maximum concurrent connections
connection_timeout_ms = 30000        # Connection timeout

# QUIC-specific configuration
[network.quic_config]
max_concurrent_uni_streams = 1000
max_concurrent_bidi_streams = 1000
max_idle_timeout_ms = 30000
max_stream_data = 1024000
max_connection_data = 10240000

[wal]
path = "/var/lib/rustmq/wal"        # WAL storage path
capacity_bytes = 10737418240        # 10GB WAL capacity
fsync_on_write = true               # Force sync on write (durability)
segment_size_bytes = 1073741824     # 1GB segment size
buffer_size = 65536                 # 64KB buffer size
upload_interval_ms = 600000         # 10 minutes upload interval
flush_interval_ms = 1000            # 1 second flush interval

[cache]
write_cache_size_bytes = 1073741824  # 1GB hot data cache
read_cache_size_bytes = 2147483648   # 2GB cold data cache
eviction_policy = "Lru"              # Cache eviction policy (Lru/Lfu/Random)

[object_storage]
storage_type = "S3"                 # Storage backend (S3/Gcs/Azure/Local)
bucket = "rustmq-data"              # Storage bucket name
region = "us-central1"              # Storage region
endpoint = "http://minio:9000"      # Storage endpoint
access_key = "rustmq-access-key"    # Optional: Access key
secret_key = "rustmq-secret-key"    # Optional: Secret key
multipart_threshold = 104857600     # 100MB multipart upload threshold
max_concurrent_uploads = 10         # Concurrent upload limit

[controller]
endpoints = ["controller-1:9094", "controller-2:9094", "controller-3:9094"]
election_timeout_ms = 5000          # Leader election timeout
heartbeat_interval_ms = 1000        # Heartbeat frequency

[replication]
min_in_sync_replicas = 2            # Minimum replicas for acknowledgment
ack_timeout_ms = 5000               # Replication acknowledgment timeout
max_replication_lag = 1000          # Maximum acceptable lag

[etl]
enabled = true                      # Enable WebAssembly ETL processing
memory_limit_bytes = 67108864       # 64MB memory limit per module
execution_timeout_ms = 5000         # Execution timeout
max_concurrent_executions = 100     # Concurrent execution limit

[scaling]
max_concurrent_additions = 3        # Max brokers added simultaneously
max_concurrent_decommissions = 1    # Max brokers decommissioned simultaneously
rebalance_timeout_ms = 300000       # Partition rebalancing timeout
traffic_migration_rate = 0.1        # Traffic migration rate per minute
health_check_timeout_ms = 30000     # Health check timeout

[operations]
allow_runtime_config_updates = true # Enable runtime config updates
upgrade_velocity = 3                # Brokers upgraded per minute
graceful_shutdown_timeout_ms = 30000 # Graceful shutdown timeout

[operations.kubernetes]
use_stateful_sets = true            # Use StatefulSets for deployment
pvc_storage_class = "fast-ssd"      # Storage class for persistent volumes
wal_volume_size = "50Gi"            # WAL volume size
enable_pod_affinity = true          # Enable pod affinity for volume attachment
```

### Environment Variables

```bash
# Core settings
RUSTMQ_BROKER_ID=broker-001
RUSTMQ_RACK_ID=us-central1-a
RUSTMQ_LOG_LEVEL=info

# Storage settings
RUSTMQ_WAL_PATH=/var/lib/rustmq/wal
RUSTMQ_STORAGE_BUCKET=rustmq-data
RUSTMQ_STORAGE_REGION=us-central1

# GCP settings
GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json
GCP_PROJECT_ID=your-project-id

# Performance tuning
RUSTMQ_CACHE_SIZE=2147483648
RUSTMQ_MAX_CONNECTIONS=10000
RUSTMQ_BATCH_SIZE=1000
```

## 📚 Usage Examples

### Client Examples

**Note**: The following are examples of the intended client API. Current implementation is in early development stage and these clients are not yet available.

#### Rust Client Example

```rust
// Cargo.toml
[dependencies]
rustmq-client = "0.1.0"  # Not yet published
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }

// main.rs
use rustmq_client::{RustMqClient, ProduceRequest, FetchRequest, Record};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct OrderEvent {
    order_id: String,
    customer_id: String,
    amount: f64,
    timestamp: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to RustMQ cluster
    let client = RustMqClient::new("quic://rustmq-broker:9092").await?;

    // Create topic
    client.create_topic("orders", 12, 3).await?;
    println!("Topic 'orders' created with 12 partitions");

    // Produce messages
    let order = OrderEvent {
        order_id: "order-123".to_string(),
        customer_id: "customer-456".to_string(),
        amount: 99.99,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    let record = Record {
        key: Some(order.order_id.as_bytes().to_vec()),
        value: serde_json::to_vec(&order)?,
        headers: vec![],
        timestamp: order.timestamp,
    };

    let request = ProduceRequest {
        topic: "orders".to_string(),
        partition_id: 0,
        records: vec![record],
        acks: rustmq_client::AcknowledgmentLevel::All,
        timeout_ms: 5000,
    };

    let response = client.produce(request).await?;
    println!("Message produced at offset: {}", response.offset);

    // Consume messages
    let fetch_request = FetchRequest {
        topic: "orders".to_string(),
        partition_id: 0,
        fetch_offset: 0,
        max_bytes: 1024 * 1024, // 1MB
        timeout_ms: 5000,
    };

    let fetch_response = client.fetch(fetch_request).await?;
    for record in fetch_response.records {
        let order: OrderEvent = serde_json::from_slice(&record.value)?;
        println!("Received order: {:?}", order);
    }

    // Create consumer group
    let mut consumer = client.create_consumer("order-processors", &["orders"]).await?;
    
    // Consume with automatic offset management
    loop {
        let records = consumer.poll(std::time::Duration::from_millis(1000)).await?;
        for (record, partition, offset) in records {
            let order: OrderEvent = serde_json::from_slice(&record.value)?;
            
            // Process the order
            process_order(order).await?;
            
            // Commit offset
            consumer.commit_offset("orders", partition, offset).await?;
        }
    }
}

async fn process_order(order: OrderEvent) -> Result<(), Box<dyn std::error::Error>> {
    println!("Processing order {} for customer {} amount ${}", 
             order.order_id, order.customer_id, order.amount);
    
    // Your business logic here
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    Ok(())
}
```

#### Go Client Example

```go
// go.mod
module rustmq-example

go 1.21

require (
    github.com/rustmq/rustmq-go v0.1.0  // Not yet available
    github.com/google/uuid v1.3.0
)

// main.go
package main

import (
    "context"
    "encoding/json"
    "fmt"
    "log"
    "time"

    "github.com/google/uuid"
    "github.com/rustmq/rustmq-go"
)

type OrderEvent struct {
    OrderID    string  `json:"order_id"`
    CustomerID string  `json:"customer_id"`
    Amount     float64 `json:"amount"`
    Timestamp  int64   `json:"timestamp"`
}

func main() {
    // Connect to RustMQ cluster
    client, err := rustmq.NewClient(rustmq.Config{
        Brokers: []string{"rustmq-broker:9092"},
        Timeout: 30 * time.Second,
    })
    if err != nil {
        log.Fatal("Failed to create client:", err)
    }
    defer client.Close()

    // Create topic
    err = client.CreateTopic(context.Background(), "orders", 12, 3)
    if err != nil {
        log.Fatal("Failed to create topic:", err)
    }
    fmt.Println("Topic 'orders' created with 12 partitions")

    // Producer example
    go func() {
        producer := client.NewProducer(rustmq.ProducerConfig{
            Topic:        "orders",
            Acknowledgment: rustmq.AckAll,
            Timeout:      5 * time.Second,
        })
        defer producer.Close()

        for i := 0; i < 1000; i++ {
            order := OrderEvent{
                OrderID:    uuid.New().String(),
                CustomerID: fmt.Sprintf("customer-%d", i%100),
                Amount:     float64(i * 10),
                Timestamp:  time.Now().UnixMilli(),
            }

            orderBytes, _ := json.Marshal(order)
            
            record := rustmq.Record{
                Key:       []byte(order.OrderID),
                Value:     orderBytes,
                Headers:   map[string][]byte{},
                Timestamp: order.Timestamp,
            }

            offset, err := producer.Send(context.Background(), record)
            if err != nil {
                log.Printf("Failed to send message: %v", err)
                continue
            }
            
            fmt.Printf("Message sent at offset: %d\n", offset)
            time.Sleep(100 * time.Millisecond)
        }
    }()

    // Consumer example
    consumer := client.NewConsumer(rustmq.ConsumerConfig{
        GroupID:     "order-processors",
        Topics:      []string{"orders"},
        AutoCommit:  true,
        StartOffset: rustmq.OffsetEarliest,
    })
    defer consumer.Close()

    // Subscribe to topics
    err = consumer.Subscribe(context.Background())
    if err != nil {
        log.Fatal("Failed to subscribe:", err)
    }

    // Consume messages
    for {
        records, err := consumer.Poll(context.Background(), time.Second)
        if err != nil {
            log.Printf("Poll error: %v", err)
            continue
        }

        for _, record := range records {
            var order OrderEvent
            if err := json.Unmarshal(record.Value, &order); err != nil {
                log.Printf("Failed to unmarshal order: %v", err)
                continue
            }

            // Process the order
            if err := processOrder(order); err != nil {
                log.Printf("Failed to process order %s: %v", order.OrderID, err)
                continue
            }

            fmt.Printf("Processed order %s for customer %s amount $%.2f\n",
                order.OrderID, order.CustomerID, order.Amount)
        }
    }
}

func processOrder(order OrderEvent) error {
    // Your business logic here
    time.Sleep(100 * time.Millisecond)
    return nil
}
```

#### Admin Operations

**Note**: Admin API is not yet implemented. The following shows the intended API structure.

```bash
# Create topic with custom configuration (not yet implemented)
curl -X POST http://rustmq-admin/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{
    "name": "user-events",
    "partition_count": 24,
    "replication_factor": 3,
    "retention_policy": {"Time": {"retention_ms": 604800000}},
    "compression": "Lz4",
    "etl_modules": ["pii_scrubber", "fraud_detector"]
  }'

# List topics (not yet implemented)
curl http://rustmq-admin/api/v1/topics

# Get topic details (not yet implemented)
curl http://rustmq-admin/api/v1/topics/user-events

# Update topic configuration
curl -X PUT http://rustmq-admin/api/v1/topics/user-events \
  -H "Content-Type: application/json" \
  -d '{
    "retention_policy": {"Time": {"retention_ms": 1209600000}},
    "etl_modules": ["pii_scrubber", "fraud_detector", "analytics_enricher"]
  }'

# Get cluster health
curl http://rustmq-admin/api/v1/cluster/health

# List brokers
curl http://rustmq-admin/api/v1/brokers

# Trigger partition rebalancing
curl -X POST http://rustmq-admin/api/v1/rebalance \
  -H "Content-Type: application/json" \
  -d '{
    "topic_names": ["user-events", "orders"],
    "strategy": "BALANCE_LOAD",
    "max_concurrent_moves": 5
  }'

# Deploy WebAssembly ETL module
curl -X POST http://rustmq-admin/api/v1/wasm/modules \
  -H "Content-Type: application/octet-stream" \
  -H "X-Module-Name: analytics_enricher" \
  --data-binary @analytics_enricher.wasm

# Get metrics
curl http://rustmq-admin/api/v1/metrics
```

## 📊 Future Performance Tuning (Not Yet Implemented)

### Planned Broker Optimization

```toml
# High-throughput configuration
[wal]
capacity_bytes = 53687091200        # 50GB for high-volume topics
fsync_on_write = false              # Disable for maximum throughput
segment_size_bytes = 2147483648     # 2GB segments
buffer_size = 1048576               # 1MB buffer

[cache]
write_cache_size_bytes = 8589934592  # 8GB hot cache
read_cache_size_bytes = 17179869184  # 16GB cold cache

[network]
max_connections = 50000             # Increase connection limit
connection_timeout_ms = 60000       # Longer timeout for slow clients

[object_storage]
max_concurrent_uploads = 50         # More concurrent uploads
multipart_threshold = 52428800      # 50MB threshold

# Low-latency configuration
[wal]
fsync_on_write = true               # Enable for durability
buffer_size = 4096                  # Smaller buffers for low latency

[replication]
min_in_sync_replicas = 1            # Reduce for lower latency
ack_timeout_ms = 1000               # Faster timeouts
```

### Planned Kubernetes Resource Tuning

```yaml
# High-performance broker configuration
resources:
  requests:
    memory: "16Gi"
    cpu: "8"
    ephemeral-storage: "100Gi"
  limits:
    memory: "32Gi"
    cpu: "16"
    ephemeral-storage: "200Gi"

# Node affinity for performance
nodeSelector:
  cloud.google.com/gke-nodepool: high-performance
  
affinity:
  podAntiAffinity:
    requiredDuringSchedulingIgnoredDuringExecution:
    - labelSelector:
        matchLabels:
          app: rustmq-broker
      topologyKey: kubernetes.io/hostname

# Volume configuration for maximum IOPS
volumeClaimTemplates:
- metadata:
    name: wal-storage
  spec:
    accessModes: ["ReadWriteOnce"]
    storageClassName: fast-ssd
    resources:
      requests:
        storage: 500Gi
```

## 📈 Future Monitoring (Not Yet Implemented)

### Planned Prometheus Configuration

```yaml
# prometheus-config.yaml - future monitoring setup
apiVersion: v1
kind: ConfigMap
metadata:
  name: prometheus-config
data:
  prometheus.yml: |
    global:
      scrape_interval: 15s
      
    scrape_configs:
    - job_name: 'rustmq-brokers'
      kubernetes_sd_configs:
      - role: pod
        namespaces:
          names: [rustmq]
      relabel_configs:
      - source_labels: [__meta_kubernetes_pod_label_app]
        action: keep
        regex: rustmq-broker
      - source_labels: [__meta_kubernetes_pod_ip]
        target_label: __address__
        replacement: ${1}:9642
        
    - job_name: 'rustmq-controllers'
      kubernetes_sd_configs:
      - role: pod
        namespaces:
          names: [rustmq]
      relabel_configs:
      - source_labels: [__meta_kubernetes_pod_label_app]
        action: keep
        regex: rustmq-controller
      - source_labels: [__meta_kubernetes_pod_ip]
        target_label: __address__
        replacement: ${1}:9642
```

### Future Monitoring (Not Yet Implemented)

Planned metrics to monitor:

- **Throughput**: `rate(messages_produced_total[5m])`, `rate(messages_consumed_total[5m])`
- **Latency**: `produce_latency_seconds`, `consume_latency_seconds`
- **Storage**: `wal_size_bytes`, `cache_hit_ratio`, `object_storage_upload_rate`
- **Replication**: `replication_lag`, `in_sync_replicas_count`
- **System**: `cpu_usage`, `memory_usage`, `disk_iops`, `network_throughput`

### Future Alerting (Not Yet Implemented)

```yaml
# alerts.yaml - planned alerting rules
groups:
- name: rustmq.rules
  rules:
  - alert: HighProduceLatency
    expr: histogram_quantile(0.95, produce_latency_seconds) > 0.1
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: "High produce latency detected"
      
  - alert: ReplicationLagHigh
    expr: replication_lag > 10000
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "Replication lag is too high"
      
  - alert: BrokerDown
    expr: up{job="rustmq-brokers"} == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "RustMQ broker is down"
```

## 🔧 Development & Troubleshooting

### Current Development Issues

**Note**: Since RustMQ is in early development, most "issues" are actually missing implementations.

1. **Services Not Responding**
```bash
# Current broker/controller services are placeholders that just load config and sleep
# Check if they started successfully
docker-compose logs rustmq-broker-1
docker-compose logs rustmq-controller-1

# Look for configuration loading messages
# Services should log "started successfully" then sleep
```

2. **Build Issues**
```bash
# Ensure Rust toolchain is up to date
rustup update

# Clean build if needed
cargo clean
cargo build --release

# Run tests to verify implementation
cargo test
```

3. **Configuration Issues**
```bash
# Validate configuration
cargo run --bin rustmq-broker -- --config config/broker.toml

# Check configuration structure in src/config.rs
# All fields must be present in TOML files
```

### Log Analysis

```bash
# View service logs (placeholder implementations)
docker-compose logs rustmq-broker-1
docker-compose logs rustmq-controller-1

# Check for configuration validation errors
docker-compose logs | grep ERROR

# Monitor BigQuery subscriber demo
docker-compose logs rustmq-bigquery-subscriber
```

## 🤝 Contributing

We welcome contributions to help implement the remaining features! 

### Current Development Priorities

1. **Message Broker Core**: Implement actual produce/consume functionality
2. **Network Layer**: Complete QUIC/gRPC server implementations
3. **Distributed Coordination**: Implement Raft consensus and metadata management
4. **Client Libraries**: Build Rust and Go client libraries
5. **Admin API**: Implement REST API for cluster management

### Development Setup

```bash
# Clone and setup
git clone https://github.com/cloudymoma/rustmq.git
cd rustmq

# Install development dependencies
cargo install cargo-watch cargo-audit cargo-tarpaulin

# Run tests with coverage
cargo tarpaulin --out Html

# Watch for changes during development
cargo watch -x test -x clippy
```

### Testing

```bash
# Unit tests (currently 43 tests passing)
cargo test

# Run specific module tests
cargo test storage::
cargo test scaling::

# Run with features
cargo test --features "io-uring,wasm"

# Integration tests (placeholder)
cargo test --test integration
```

## 📄 License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## 🔗 Links

- [Issue Tracker](https://github.com/cloudymoma/rustmq/issues)

---

**RustMQ** - Built with ❤️ in Rust for the cloud-native future. Optimized for Google Cloud Platform.
