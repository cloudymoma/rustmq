# RustMQ: Cloud-Native Distributed Message Queue System

[![Build Status](https://github.com/cloudymoma/rustmq/workflows/Rust/badge.svg)](https://github.com/cloudymoma/rustmq/actions)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust Version](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)

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

## 🏃 Quick Start

### Prerequisites

- Rust 1.70+ and Cargo
- Docker and Docker Compose
- Google Cloud SDK (for GCP deployment)
- kubectl (for Kubernetes deployment)

### Local Development Setup

```bash
# Clone the repository
git clone https://github.com/cloudymoma/rustmq.git
cd rustmq

# Build the project
cargo build --release

# Run tests
cargo test

# Start local development cluster with Docker Compose
docker-compose up -d

# Or run individual components locally:
# Run broker
./target/release/rustmq-broker --config config/broker.toml

# Run controller
./target/release/rustmq-controller --config config/controller.toml
```

## 🐳 Docker Development Setup

RustMQ provides a complete Docker-based development environment with proper container orchestration.

### Docker Components

The following Docker containers are available with the proper naming schema:

- **Dockerfile.broker** - RustMQ message broker with optimized multi-stage build
- **Dockerfile.controller** - RustMQ controller with Raft consensus support  
- **Dockerfile.admin** - Interactive admin CLI with helper commands
- **Dockerfile.bigquery-subscriber** - Google BigQuery subscriber for streaming data to BigQuery

### Starting the Cluster

```bash
# Start the complete RustMQ cluster (3 controllers + 3 brokers + dependencies)
docker-compose up -d

# View cluster status
docker-compose ps

# View logs from all services
docker-compose logs -f

# View logs from specific service
docker-compose logs -f rustmq-broker-1
```

### Cluster Architecture

The Docker Compose setup includes:

- **3 Controller nodes** (`rustmq-controller-1/2/3`) - Raft consensus cluster
- **3 Broker nodes** (`rustmq-broker-1/2/3`) - Stateless message brokers
- **etcd** - Distributed metadata store
- **MinIO** - S3-compatible object storage for local development
- **Admin CLI** - Interactive management interface
- **BigQuery Subscriber** - Optional Google BigQuery streaming subscriber

### Service Endpoints

| Service | Internal Port | External Port | Purpose |
|---------|---------------|---------------|---------|
| Broker 1 | 9092/9093 | 9092/9093 | QUIC/RPC |
| Broker 2 | 9092/9093 | 9192/9193 | QUIC/RPC |  
| Broker 3 | 9092/9093 | 9292/9293 | QUIC/RPC |
| Controller 1 | 9094/9095/9642 | 9094/9095/9642 | RPC/Raft/HTTP |
| Controller 2 | 9094/9095/9642 | 9144/9145/9643 | RPC/Raft/HTTP |
| Controller 3 | 9094/9095/9642 | 9194/9195/9644 | RPC/Raft/HTTP |
| etcd | 2379/2380 | 2379/2380 | Client/Peer |
| MinIO | 9000/9001 | 9000/9001 | API/Console |
| BigQuery Subscriber | 8080 | 8080 | Health/Metrics |

### Using the Admin CLI

```bash
# Access the admin CLI container
docker-compose exec rustmq-admin bash

# Use the convenient CLI wrapper
rustmq-cli topics list
rustmq-cli brokers status
rustmq-cli cluster status

# Or use the full admin API
rustmq-admin --help
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

    [metastore]
    type = "etcd"
    endpoints = ["etcd:2379"]

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

[wal]
path = "/dev/nvme1n1"               # WAL storage path (preferably NVMe)
capacity_bytes = 10737418240        # 10GB WAL capacity
fsync_on_write = true               # Force sync on write (durability)
segment_size_bytes = 1073741824     # 1GB segment size
buffer_size = 65536                 # 64KB buffer size

[cache]
write_cache_size_bytes = 1073741824  # 1GB hot data cache
read_cache_size_bytes = 2147483648   # 2GB cold data cache
eviction_policy = "Lru"              # Cache eviction policy

[object_storage]
type = "Gcs"                        # Storage backend (S3/Gcs/Azure/Local)
bucket = "rustmq-data"              # Storage bucket name
region = "us-central1"              # Storage region
endpoint = "https://storage.googleapis.com"
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

### Rust Client Example

```rust
// Cargo.toml
[dependencies]
rustmq-client = "0.1.0"
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

### Go Client Example

```go
// go.mod
module rustmq-example

go 1.21

require (
    github.com/rustmq/rustmq-go v0.1.0
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

### Admin Operations

```bash
# Create topic with custom configuration
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

# List topics
curl http://rustmq-admin/api/v1/topics

# Get topic details
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

## 📊 Performance Tuning

### Broker Optimization

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

### Kubernetes Resource Tuning

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

## 📈 Monitoring

### Prometheus Configuration

```yaml
# prometheus-config.yaml
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

### Grafana Dashboards

Key metrics to monitor:

- **Throughput**: `rate(messages_produced_total[5m])`, `rate(messages_consumed_total[5m])`
- **Latency**: `produce_latency_seconds`, `consume_latency_seconds`
- **Storage**: `wal_size_bytes`, `cache_hit_ratio`, `object_storage_upload_rate`
- **Replication**: `replication_lag`, `in_sync_replicas_count`
- **System**: `cpu_usage`, `memory_usage`, `disk_iops`, `network_throughput`

### Alerting Rules

```yaml
# alerts.yaml
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

## 🔧 Troubleshooting

### Common Issues

1. **High Memory Usage**
```bash
# Check cache configuration
kubectl logs rustmq-broker-0 -n rustmq | grep cache

# Reduce cache sizes in configuration
[cache]
write_cache_size_bytes = 536870912   # 512MB
read_cache_size_bytes = 1073741824   # 1GB
```

2. **Slow Object Storage Uploads**
```bash
# Check bandwidth limiting
curl http://rustmq-admin/api/v1/metrics | grep upload

# Increase concurrent uploads
[object_storage]
max_concurrent_uploads = 20
```

3. **Replication Lag**
```bash
# Check follower states
curl http://rustmq-admin/api/v1/cluster | jq '.followers'

# Adjust replication settings
[replication]
ack_timeout_ms = 3000
max_replication_lag = 5000
```

### Log Analysis

```bash
# View broker logs
kubectl logs -f rustmq-broker-0 -n rustmq

# Check for specific errors
kubectl logs rustmq-broker-0 -n rustmq | grep ERROR

# Tail logs from all brokers
kubectl logs -f -l app=rustmq-broker -n rustmq
```

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

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
# Unit tests
cargo test

# Integration tests
cargo test --test integration

# Benchmark tests
cargo bench

# Stress tests
cargo test --release --test stress -- --ignored
```

## 📄 License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## 🔗 Links

- [Issue Tracker](https://github.com/cloudymoma/rustmq/issues)

---

**RustMQ** - Built with ❤️ in Rust for the cloud-native future. Optimized for Google Cloud Platform.
