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
- [Admin REST API](#-admin-rest-api)
- [BigQuery Subscriber](#-bigquery-subscriber)
- [Google Cloud Platform Setup](#-google-cloud-platform-setup)
- [Deployment](#-deployment)
- [Configuration](#-configuration)
- [Usage Examples](#-usage-examples)
- [Message Broker Core API](#-message-broker-core-api)
- [API Reference](#-api-reference)
- [Performance Tuning](#-performance-tuning)
- [Monitoring](#-monitoring)
- [Contributing](#-contributing)

## 🚧 Development Status

**RustMQ is actively under development with major core components now implemented and tested.** The current implementation includes:

### ✅ Fully Implemented Components
- **Configuration System**: Complete TOML-based configuration with validation and runtime updates
- **Storage Layer**: Production-ready implementations (DirectIO WAL, Object Storage, Tiered Cache)
- **Network Layer**: **FULLY IMPLEMENTED** QUIC/gRPC servers with connection pooling and request routing
- **Replication System**: **FULLY IMPLEMENTED** leader-follower replication with epoch validation and ISR tracking
- **Controller Service**: **FULLY IMPLEMENTED** with Raft consensus, metadata management, and decommission slots
- **ETL Processing**: **FULLY IMPLEMENTED** WebAssembly-based stream processing with resource limiting
- **BigQuery Subscriber**: Functional BigQuery integration with comprehensive configuration options
- **Scaling Operations**: Complete decommissioning slot management and broker scaling logic
- **Operational Management**: Rolling upgrades, Kubernetes deployment, volume recovery
- **Docker Environment**: Complete Docker Compose setup for development and testing
- **Message Broker Core**: **FULLY IMPLEMENTED** high-level produce/consume API with comprehensive integration tests
- **Admin REST API**: **FULLY IMPLEMENTED** cluster management API with real-time health tracking, topic/broker operations, and comprehensive monitoring
- **Go SDK**: **FULLY IMPLEMENTED** production-ready client library with advanced connection management, TLS/mTLS support, health checking, and robust reconnection logic
- **Rust SDK**: **FULLY IMPLEMENTED** complete client library with async/await, QUIC transport, and comprehensive producer API
- **Comprehensive Testing**: 99 passing unit tests + 11 admin API tests + 9 broker core integration tests + 11 Go SDK connection tests + additional integration tests covering all major components

### 🚧 In Development  
- **Advanced Client Features**: Additional language bindings and advanced streaming features

### ❌ Not Yet Implemented
- **Production Features**: Advanced monitoring dashboards, alerting rules
- **Advanced Features**: Stream processing pipelines, exactly-once semantics

### Current Capabilities
- **Full Network Stack**: QUIC/HTTP3 client connections and gRPC broker-to-broker communication
- **Complete Replication**: Leader-follower replication with automatic failover and consistency guarantees  
- **Distributed Coordination**: Raft-based controller cluster with leader election and metadata management
- **Real-time ETL**: WebAssembly module execution with memory/timeout limits and pipeline chaining
- **Production Storage**: Tiered storage with intelligent WAL uploads and object storage integration
- **Message Broker Core**: High-level producer/consumer APIs with automatic partitioning, offset management, and error handling
- **Admin REST API**: Comprehensive cluster management with real-time health monitoring, topic/broker operations, and operational metrics
- **Kubernetes Ready**: StatefulSet deployments with persistent volumes and service discovery
- **Operational Excellence**: Automated scaling, rolling upgrades, and configuration hot-reloading

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

# Start the Admin REST API server
./target/release/rustmq-admin serve-api 8080
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
| Admin REST API | 8080 | 8080 | Cluster Management | **Functional** |
| MinIO | 9000/9001 | 9000/9001 | API/Console | Functional |
| BigQuery Subscriber | 8081 | 8081 | Health/Metrics | Demo |

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

# Start the Admin REST API server (FULLY IMPLEMENTED)
rustmq-admin serve-api [port]

# Note: CLI commands show usage but are not yet implemented
# REST API is fully functional with health tracking
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

## 🛠️ Admin REST API

RustMQ provides a comprehensive REST API for cluster management, monitoring, and operations. The Admin API includes real-time health tracking, topic management, broker monitoring, and operational metrics.

### 🚀 Key Features

- **Real-time Health Monitoring**: Live broker health tracking with automatic timeout detection
- **Cluster Status**: Comprehensive cluster health assessment with leadership tracking
- **Topic Management**: CRUD operations for topics with partition and replication management
- **Broker Operations**: Broker listing with health status and rack awareness
- **Operational Metrics**: Uptime tracking and performance monitoring
- **Production Ready**: Comprehensive error handling and JSON API responses

### 🏃 Quick Start

Start the Admin API server:

```bash
# Start with default settings (port 8080)
./target/release/rustmq-admin serve-api

# Start on custom port
./target/release/rustmq-admin serve-api 9642

# Docker environment (included in docker-compose)
docker-compose up -d
# Admin API available at http://localhost:9642
```

### 📊 Health Monitoring

The Admin API provides comprehensive health monitoring with real-time broker tracking:

#### Health Endpoint
```bash
# Check service health and uptime
curl http://localhost:8080/health
```

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "is_leader": true,
  "raft_term": 1
}
```

#### Cluster Status
```bash
# Get comprehensive cluster status
curl http://localhost:8080/api/v1/cluster
```

**Response:**
```json
{
  "success": true,
  "data": {
    "brokers": [
      {
        "id": "broker-1",
        "host": "localhost",
        "port_quic": 9092,
        "port_rpc": 9093,
        "rack_id": "rack-1",
        "online": true
      },
      {
        "id": "broker-2",
        "host": "localhost",
        "port_quic": 9192,
        "port_rpc": 9193,
        "rack_id": "rack-2", 
        "online": false
      }
    ],
    "topics": [],
    "leader": "controller-1",
    "term": 1,
    "healthy": true
  },
  "error": null,
  "leader_hint": null
}
```

### 📋 Topic Management

#### List Topics
```bash
curl http://localhost:8080/api/v1/topics
```

#### Create Topic
```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{
    "name": "user-events",
    "partitions": 12,
    "replication_factor": 3,
    "retention_ms": 604800000,
    "segment_bytes": 1073741824,
    "compression_type": "lz4"
  }'
```

**Response:**
```json
{
  "success": true,
  "data": "Topic 'user-events' created",
  "error": null,
  "leader_hint": "controller-1"
}
```

#### Describe Topic
```bash
curl http://localhost:8080/api/v1/topics/user-events
```

**Response:**
```json
{
  "success": true,
  "data": {
    "name": "user-events",
    "partitions": 12,
    "replication_factor": 3,
    "config": {
      "retention_ms": 604800000,
      "segment_bytes": 1073741824,
      "compression_type": "lz4"
    },
    "created_at": "2024-01-15T10:30:00Z",
    "partition_assignments": [
      {
        "partition": 0,
        "leader": "broker-1",
        "replicas": ["broker-1", "broker-2", "broker-3"],
        "in_sync_replicas": ["broker-1", "broker-2"],
        "leader_epoch": 1
      }
    ]
  },
  "error": null,
  "leader_hint": "controller-1"
}
```

#### Delete Topic
```bash
curl -X DELETE http://localhost:8080/api/v1/topics/user-events
```

### 🖥️ Broker Management

#### List Brokers
```bash
curl http://localhost:8080/api/v1/brokers
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "broker-1",
      "host": "localhost",
      "port_quic": 9092,
      "port_rpc": 9093,
      "rack_id": "us-central1-a",
      "online": true
    },
    {
      "id": "broker-2", 
      "host": "localhost",
      "port_quic": 9192,
      "port_rpc": 9193,
      "rack_id": "us-central1-b",
      "online": true
    }
  ],
  "error": null,
  "leader_hint": "controller-1"
}
```

### 🔧 Health Tracking System

The Admin API includes a sophisticated health tracking system:

#### Features
- **Background Health Monitoring**: Automatic health checks every 15 seconds
- **Timeout-based Health Assessment**: Configurable 30-second health timeout
- **Intelligent Cluster Health**: Smart health calculation for small clusters
- **Real-time Updates**: Live health status in all broker-related endpoints
- **Stale Entry Cleanup**: Automatic cleanup of old health data

#### Health Check Logic
- **Healthy**: Last successful health check within 30 seconds
- **Unhealthy**: No successful health check or timeout exceeded
- **Cluster Health**: For ≤2 brokers: healthy if ≥1 broker healthy + leader exists
- **Large Clusters**: Healthy if majority of brokers healthy + leader exists

### 🚨 Error Handling

The Admin API provides comprehensive error handling with detailed responses:

#### Error Response Format
```json
{
  "success": false,
  "data": null,
  "error": "Detailed error message",
  "leader_hint": "controller-2"
}
```

#### Common Error Scenarios
- **Topic Not Found** (404): Topic doesn't exist
- **Insufficient Brokers**: Not enough brokers for replication factor
- **Leader Not Available**: Controller leader election in progress
- **Invalid Configuration**: Malformed request parameters

### 📈 Production Deployment

#### Kubernetes Integration
The Admin API integrates seamlessly with Kubernetes deployments:

```yaml
# Service exposure
apiVersion: v1
kind: Service
metadata:
  name: rustmq-admin
spec:
  type: LoadBalancer
  selector:
    app: rustmq-controller
  ports:
  - name: http
    port: 80
    targetPort: 8080
    protocol: TCP
```

#### Health Check Configuration
```yaml
# Pod health checks
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 30
  periodSeconds: 10
  
readinessProbe:
  httpGet:
    path: /health  
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 5
```

### 🧪 Testing

The Admin API includes comprehensive test coverage:

- **11 Unit Tests**: All API endpoints and health tracking functionality
- **Integration Testing**: End-to-end API workflows with mock backends
- **Error Scenario Testing**: Comprehensive error condition validation
- **Performance Testing**: Health tracking timeout and expiration behavior

#### Running Tests
```bash
# Run admin API tests
cargo test admin::api

# Run specific health tracking tests
cargo test test_broker_health_tracking test_cluster_health_calculation

# All admin tests pass
# test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured
```

### 🔧 Configuration

#### Environment Variables
```bash
# Admin API configuration
ADMIN_API_PORT=8080
HEALTH_CHECK_INTERVAL=15    # seconds
HEALTH_TIMEOUT=30          # seconds
```

#### TOML Configuration
```toml
[admin]
port = 8080
health_check_interval_ms = 15000
health_timeout_ms = 30000
enable_cors = true
log_requests = true
```

### 🔍 Monitoring Integration

The Admin API provides monitoring endpoints for observability:

#### Metrics Endpoint (Future)
```bash
# Prometheus metrics (planned)
curl http://localhost:8080/metrics
```

#### Log Analysis
```bash
# View API request logs
docker-compose logs rustmq-admin

# Filter for health check logs
docker-compose logs rustmq-admin | grep "Health check"
```

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

## 🔧 Message Broker Core API

RustMQ now includes a fully implemented high-level Message Broker Core that provides intuitive producer and consumer APIs with comprehensive error handling, automatic partition management, and flexible acknowledgment levels.

### Architecture Overview

The Message Broker Core is built on a modular architecture that integrates seamlessly with RustMQ's distributed storage and replication systems:

```rust
use rustmq::broker::core::*;

// Create a broker core instance with your storage backends
let core = MessageBrokerCore::new(
    wal,               // Write-Ahead Log implementation
    object_storage,    // Object storage backend (S3/GCS/Azure)
    cache,             // Distributed cache layer
    replication_manager, // Replication coordinator
    network_handler,   // Network communication handler
    broker_id,         // Unique broker identifier
);
```

### Producer API

The Producer trait provides a simple, high-performance interface for message production:

```rust
#[async_trait]
pub trait Producer {
    /// Send a single record to a topic-partition
    async fn send(&self, record: ProduceRecord) -> Result<ProduceResult>;
    
    /// Send a batch of records for optimized throughput
    async fn send_batch(&self, records: Vec<ProduceRecord>) -> Result<Vec<ProduceResult>>;
    
    /// Flush any pending records to ensure durability
    async fn flush(&self) -> Result<()>;
}
```

#### Single Message Production

```rust
let producer = core.create_producer();

let record = ProduceRecord {
    topic: "user-events".to_string(),
    partition: Some(0),                    // Optional: let RustMQ choose partition
    key: Some(b"user123".to_vec()),
    value: b"login_event".to_vec(),
    headers: vec![Header {
        key: "content-type".to_string(),
        value: b"application/json".to_vec(),
    }],
    acks: AcknowledgmentLevel::All,        // Wait for all replicas
    timeout_ms: 5000,
};

let result = producer.send(record).await?;
println!("Message produced at offset: {}", result.offset);
```

#### Batch Production for High Throughput

```rust
let mut batch = Vec::new();
for i in 0..1000 {
    batch.push(ProduceRecord {
        topic: "metrics".to_string(),
        partition: None,  // Auto-partition based on key hash
        key: Some(format!("sensor_{}", i % 10).into_bytes()),
        value: format!("{{\"value\": {}, \"timestamp\": {}}}", i, timestamp).into_bytes(),
        headers: vec![],
        acks: AcknowledgmentLevel::Leader,  // Faster acknowledgment
        timeout_ms: 1000,
    });
}

let results = producer.send_batch(batch).await?;
println!("Produced {} messages", results.len());
```

### Consumer API

The Consumer trait provides flexible message consumption with automatic offset management:

```rust
#[async_trait]
pub trait Consumer {
    /// Subscribe to one or more topics
    async fn subscribe(&mut self, topics: Vec<TopicName>) -> Result<()>;
    
    /// Poll for new records with configurable timeout
    async fn poll(&mut self, timeout_ms: u32) -> Result<Vec<ConsumeRecord>>;
    
    /// Commit specific offsets for durability
    async fn commit_offsets(&mut self, offsets: HashMap<TopicPartition, Offset>) -> Result<()>;
    
    /// Seek to a specific offset for replay scenarios
    async fn seek(&mut self, topic_partition: TopicPartition, offset: Offset) -> Result<()>;
}
```

#### Basic Consumer Usage

```rust
let mut consumer = core.create_consumer("analytics-group".to_string());

// Subscribe to topics
consumer.subscribe(vec!["user-events".to_string(), "orders".to_string()]).await?;

// Consume messages
loop {
    let records = consumer.poll(1000).await?;
    
    for record in records {
        println!("Received: topic={}, partition={}, offset={}", 
                 record.topic_partition.topic,
                 record.topic_partition.partition,
                 record.offset);
        
        // Process your message
        process_message(&record.value).await?;
        
        // Optional: Manual offset commit for exactly-once processing
        let mut offsets = HashMap::new();
        offsets.insert(record.topic_partition.clone(), record.offset + 1);
        consumer.commit_offsets(offsets).await?;
    }
}
```

#### Consumer Seek for Message Replay

```rust
// Replay messages from a specific point in time
let topic_partition = TopicPartition {
    topic: "user-events".to_string(),
    partition: 0,
};

// Seek to offset 1000 to replay messages
consumer.seek(topic_partition, 1000).await?;

// Continue normal polling - will start from offset 1000
let records = consumer.poll(5000).await?;
```

### Acknowledgment Levels

RustMQ supports flexible acknowledgment levels for different durability and performance requirements:

```rust
use rustmq::types::AcknowledgmentLevel;

// Maximum performance - fire and forget
acks: AcknowledgmentLevel::None,

// Fast acknowledgment - leader only
acks: AcknowledgmentLevel::Leader,

// High availability - majority of replicas
acks: AcknowledgmentLevel::Majority,

// Maximum durability - all replicas
acks: AcknowledgmentLevel::All,

// Custom requirement - specific number of replicas
acks: AcknowledgmentLevel::Custom(3),
```

### Error Handling

The Broker Core provides comprehensive error handling with detailed error types:

```rust
use rustmq::error::RustMqError;

match producer.send(record).await {
    Ok(result) => println!("Success: offset {}", result.offset),
    Err(RustMqError::NotLeader(partition)) => {
        println!("Not leader for partition: {}", partition);
        // Retry with updated metadata
    },
    Err(RustMqError::OffsetOutOfRange(msg)) => {
        println!("Offset out of range: {}", msg);
        // Seek to valid offset
    },
    Err(RustMqError::Timeout) => {
        println!("Request timed out");
        // Implement retry logic
    },
    Err(e) => println!("Other error: {}", e),
}
```

### Integration with Storage Layers

The Broker Core seamlessly integrates with RustMQ's tiered storage architecture:

- **Local WAL**: Recent messages are served from high-speed local NVMe storage
- **Cache Layer**: Frequently accessed messages are cached for optimal performance  
- **Object Storage**: Historical messages are automatically migrated to cost-effective cloud storage
- **Intelligent Routing**: The core automatically routes read requests to the optimal storage tier

### Testing and Validation

The Message Broker Core includes comprehensive test coverage:

- **Unit Tests**: Core functionality with 88 passing tests
- **Integration Tests**: End-to-end workflows with 9 comprehensive test scenarios
- **Mock Implementations**: Complete test doubles for all dependencies
- **Error Scenarios**: Comprehensive error condition testing

### Performance Characteristics

- **Low Latency**: Sub-millisecond produce latency for local WAL writes
- **High Throughput**: Batch production for maximum throughput scenarios
- **Automatic Partitioning**: Intelligent partition selection based on message keys
- **Zero-Copy Operations**: Efficient memory usage throughout the message path
- **Async Throughout**: Non-blocking I/O for maximum concurrency

## 📦 Client SDKs

RustMQ provides official client SDKs for multiple programming languages with production-ready features and comprehensive documentation.

### 🦀 Rust SDK
- **Location**: [`sdk/rust/`](sdk/rust/)
- **Status**: ✅ **Fully Implemented** - Production-ready client library with comprehensive producer API
- **Features**: 
  - **Advanced Producer API**: Builder pattern with intelligent batching, flush mechanisms, and configurable acknowledgment levels
  - **Async/Await**: Built on Tokio with zero-copy operations and streaming support
  - **QUIC Transport**: Modern HTTP/3 protocol for low-latency communication
  - **Comprehensive Error Handling**: Detailed error types with retry logic and timeout management
  - **Performance Monitoring**: Built-in metrics for messages sent/failed, batch sizes, and timing
- **Build**: `cargo build --release`
- **Install**: Add to `Cargo.toml`: `rustmq-client = { path = "sdk/rust" }`

#### Producer API

The Rust SDK provides a comprehensive Producer API with intelligent batching, flush mechanisms, and production-ready features. Based on the actual implementation:

##### Basic Producer Usage

```rust
use rustmq_client::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client connection
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("my-app-producer".to_string()),
        connect_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    
    let client = RustMqClient::new(config).await?;
    
    // Create producer with custom configuration
    let producer_config = ProducerConfig {
        batch_size: 100,                           // Batch up to 100 messages
        batch_timeout: Duration::from_millis(10),  // Or send after 10ms
        ack_level: AckLevel::All,                   // Wait for all replicas
        producer_id: Some("my-app-producer".to_string()),
        ..Default::default()
    };
    
    let producer = ProducerBuilder::new()
        .topic("user-events")
        .config(producer_config)
        .client(client)
        .build()
        .await?;
    
    // Send a single message and wait for acknowledgment
    let message = Message::builder()
        .topic("user-events")
        .payload("user logged in")
        .header("user-id", "12345")
        .header("event-type", "login")
        .build()?;
    
    let result = producer.send(message).await?;
    println!("Message sent to partition {} at offset {}", 
             result.partition, result.offset);
    
    Ok(())
}
```

##### Fire-and-Forget Messages

```rust
// High-throughput fire-and-forget sending
for i in 0..1000 {
    let message = Message::builder()
        .topic("metrics")
        .payload(format!("{{\"value\": {}, \"timestamp\": {}}}", i, timestamp))
        .header("sensor-id", &format!("sensor-{}", i % 10))
        .build()?;
    
    // Returns immediately after queuing - no waiting for broker
    producer.send_async(message).await?;
}

// Flush to ensure all messages are sent
producer.flush().await?;
```

##### Batch Operations

```rust
// Prepare a batch of messages
let messages: Vec<_> = (0..50).map(|i| {
    Message::builder()
        .topic("batch-topic")
        .payload(format!("message-{}", i))
        .header("batch-id", "batch-123")
        .build().unwrap()
}).collect();

// Send batch and wait for all acknowledgments
let results = producer.send_batch(messages).await?;

for result in results {
    println!("Message {} sent to offset {}", 
             result.message_id, result.offset);
}
```

##### Producer Configuration Options

```rust
let producer_config = ProducerConfig {
    // Batching configuration
    batch_size: 100,                           // Messages per batch
    batch_timeout: Duration::from_millis(10),  // Maximum wait time
    
    // Reliability configuration  
    ack_level: AckLevel::All,                   // All, Leader, or None
    max_message_size: 1024 * 1024,             // 1MB max message size
    idempotent: true,                           // Enable idempotent producer
    
    // Producer identification
    producer_id: Some("my-producer".to_string()),
    
    // Advanced configuration
    compression: CompressionConfig {
        enabled: true,
        algorithm: CompressionAlgorithm::Lz4,
        level: 6,
        min_size: 1024,
    },
    
    default_properties: HashMap::from([
        ("app".to_string(), "my-application".to_string()),
        ("version".to_string(), "1.0.0".to_string()),
    ]),
};
```

##### Error Handling

```rust
use rustmq_sdk::error::ClientError;

match producer.send(message).await {
    Ok(result) => {
        println!("Success: {} at offset {}", result.message_id, result.offset);
    }
    Err(ClientError::Timeout { timeout_ms }) => {
        println!("Request timed out after {}ms", timeout_ms);
        // Implement retry logic
    }
    Err(ClientError::Broker(msg)) => {
        println!("Broker error: {}", msg);
        // Handle broker-side errors
    }
    Err(ClientError::MessageTooLarge { size, max_size }) => {
        println!("Message too large: {} bytes (max: {})", size, max_size);
        // Reduce message size
    }
    Err(e) => {
        println!("Other error: {}", e);
    }
}
```

##### Monitoring and Metrics

```rust
// Get producer performance metrics
let metrics = producer.metrics().await;

println!("Messages sent: {}", 
         metrics.messages_sent.load(std::sync::atomic::Ordering::Relaxed));
println!("Messages failed: {}", 
         metrics.messages_failed.load(std::sync::atomic::Ordering::Relaxed));
println!("Batches sent: {}", 
         metrics.batches_sent.load(std::sync::atomic::Ordering::Relaxed));
println!("Average batch size: {:.2}", 
         *metrics.average_batch_size.read().await);

if let Some(last_send) = *metrics.last_send_time.read().await {
    println!("Last send: {:?} ago", last_send.elapsed());
}
```

##### Graceful Shutdown

```rust
// Proper producer shutdown
async fn shutdown_producer(producer: Producer) -> Result<(), ClientError> {
    // Flush all pending messages
    producer.flush().await?;
    
    // Close producer and cleanup resources
    producer.close().await?;
    
    println!("Producer shut down gracefully");
    Ok(())
}
```

### 🐹 Go SDK  
- **Location**: [`sdk/go/`](sdk/go/)
- **Status**: ✅ **Fully Implemented** - Production-ready client library with sophisticated connection layer
- **Features**: 
  - **Advanced Connection Management**: QUIC transport with intelligent connection pooling, round-robin load balancing, and automatic failover
  - **Comprehensive TLS/mTLS Support**: Full client certificate authentication with CA validation and configurable trust stores
  - **Health Check System**: Real-time broker health monitoring with JSON message exchange and automatic cleanup of failed connections
  - **Robust Reconnection Logic**: Exponential backoff with jitter, per-broker state tracking, and intelligent failure recovery
  - **Producer API with Batching**: Intelligent message batching with configurable size/timeout thresholds and compression support
  - **Extensive Statistics**: Connection metrics, health check tracking, error monitoring, traffic analytics, and reconnection statistics
  - **Production-Ready Features**: Concurrent-safe operations, goroutine-based processing, configurable timeouts, and comprehensive error handling
- **Build**: `go build ./...`
- **Install**: `import "github.com/rustmq/rustmq/sdk/go/rustmq"`

### Go SDK Connection Layer Highlights

The Go SDK features a sophisticated connection management system designed for production environments:

#### TLS/mTLS Configuration
```go
config := &rustmq.ClientConfig{
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ClientCert: "/etc/ssl/certs/client.pem",
        ClientKey:  "/etc/ssl/private/client.key",
        ServerName: "rustmq.example.com",
    },
}
```

#### Health Check & Reconnection
```go
// Automatic health monitoring with configurable intervals
config.KeepAliveInterval = 30 * time.Second

// Exponential backoff with jitter for reconnection
config.RetryConfig = &rustmq.RetryConfig{
    MaxRetries: 10,
    BaseDelay:  100 * time.Millisecond,
    MaxDelay:   30 * time.Second,
    Multiplier: 2.0,
    Jitter:     true,
}
```

#### Producer with Intelligent Batching
```go
// Create producer with batching configuration
producerConfig := &rustmq.ProducerConfig{
    BatchSize:    100,
    BatchTimeout: 100 * time.Millisecond,
    AckLevel:     rustmq.AckAll,
    Idempotent:   true,
}

producer, err := client.CreateProducer("topic", producerConfig)
if err != nil {
    log.Fatal(err)
}

// Send message with automatic batching
result, err := producer.Send(ctx, message)
```

#### Connection Statistics
```go
stats := client.Stats()
fmt.Printf("Active: %d/%d, Reconnects: %d, Health Checks: %d", 
    stats.ActiveConnections, stats.TotalConnections,
    stats.ReconnectAttempts, stats.HealthChecks)

// Additional statistics available
fmt.Printf("Bytes: Sent=%d, Received=%d, Errors=%d", 
    stats.BytesSent, stats.BytesReceived, stats.Errors)
```

### Common SDK Features
- **QUIC/HTTP3 Transport**: Low-latency, multiplexed connections
- **Producer APIs**: Sync/async sending, batching, compression
- **Consumer APIs**: Auto-commit, manual offset management, consumer groups
- **Stream Processing**: Real-time message transformation pipelines
- **Configuration**: Comprehensive client, producer, consumer settings
- **Monitoring**: Built-in metrics, health checks, observability
- **Error Handling**: Retry logic, circuit breakers, dead letter queues
- **Security**: TLS/mTLS, authentication, authorization

### Quick Start

#### Rust SDK
```bash
cd sdk/rust

# Basic producer example
cargo run --example simple_producer

# Advanced consumer with multi-partition support
cargo run --example advanced_consumer

# Stream processing example
cargo run --example stream_processor
```

#### Go SDK
```bash
cd sdk/go

# Basic producer example
go run examples/simple_producer.go

# Basic consumer example
go run examples/simple_consumer.go

# Advanced stream processing
go run examples/advanced_stream_processor.go
```

See individual SDK READMEs for detailed usage, configuration, performance tuning, and API documentation.

## 📚 Usage Examples

### Client Examples

**Note**: The following are examples of the intended client API. Current implementation is in early development stage and these clients are not yet available.

#### Rust Client Example

```rust
// Cargo.toml
[dependencies]
rustmq-client = { path = "sdk/rust" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }

// main.rs
use rustmq_client::*;
use serde::{Serialize, Deserialize};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct OrderEvent {
    order_id: String,
    customer_id: String,
    amount: f64,
    timestamp: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client connection
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("order-processor".to_string()),
        ..Default::default()
    };
    
    let client = RustMqClient::new(config).await?;
    
    // Create producer
    let producer = ProducerBuilder::new()
        .topic("orders")
        .client(client.clone())
        .build()
        .await?;

    // Produce messages
    let order = OrderEvent {
        order_id: "order-123".to_string(),
        customer_id: "customer-456".to_string(),
        amount: 99.99,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };

    let message = Message::builder()
        .topic("orders")
        .key(&order.order_id)
        .payload(serde_json::to_vec(&order)?)
        .header("content-type", "application/json")
        .build()?;

    let result = producer.send(message).await?;
    println!("Message produced at offset: {}", result.offset);

    // Create consumer
    let consumer = ConsumerBuilder::new()
        .topic("orders")
        .consumer_group("order-processors")
        .client(client)
        .build()
        .await?;
    
    // Consume with automatic offset management
    while let Some(consumer_message) = consumer.receive().await? {
        let message = &consumer_message.message;
        let order: OrderEvent = serde_json::from_slice(&message.payload)?;
        
        // Process the order
        process_order(order).await?;
        
        // Acknowledge message
        consumer_message.ack().await?;
    }
    
    Ok(())
}

async fn process_order(order: OrderEvent) -> Result<(), Box<dyn std::error::Error>> {
    println!("Processing order {} for customer {} amount ${}", 
             order.order_id, order.customer_id, order.amount);
    
    // Your business logic here
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    Ok(())
}
```

#### Go Client Example

```go
// go.mod
module rustmq-example

go 1.21

require (
    github.com/rustmq/rustmq/sdk/go v0.1.0
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
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

type OrderEvent struct {
    OrderID    string  `json:"order_id"`
    CustomerID string  `json:"customer_id"`
    Amount     float64 `json:"amount"`
    Timestamp  int64   `json:"timestamp"`
}

func main() {
    // Create client configuration
    config := &rustmq.ClientConfig{
        Brokers:  []string{"localhost:9092"},
        ClientID: "order-processor",
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal("Failed to create client:", err)
    }
    defer client.Close()

    // Producer example
    producer, err := client.CreateProducer("orders")
    if err != nil {
        log.Fatal("Failed to create producer:", err)
    }
    defer producer.Close()

    // Send some orders
    for i := 0; i < 10; i++ {
        order := OrderEvent{
            OrderID:    uuid.New().String(),
            CustomerID: fmt.Sprintf("customer-%d", i%5),
            Amount:     float64((i + 1) * 25),
            Timestamp:  time.Now().UnixMilli(),
        }

        orderBytes, _ := json.Marshal(order)
        
        message := rustmq.NewMessage().
            Topic("orders").
            KeyString(order.OrderID).
            Payload(orderBytes).
            Header("content-type", "application/json").
            Build()

        ctx := context.Background()
        result, err := producer.Send(ctx, message)
        if err != nil {
            log.Printf("Failed to send message: %v", err)
            continue
        }
        
        fmt.Printf("Message sent at offset: %d, partition: %d\n", 
            result.Offset, result.Partition)
    }

    // Consumer example
    consumer, err := client.CreateConsumer("orders", "order-processors")
    if err != nil {
        log.Fatal("Failed to create consumer:", err)
    }
    defer consumer.Close()

    // Consume messages
    for i := 0; i < 10; i++ {
        ctx := context.Background()
        message, err := consumer.Receive(ctx)
        if err != nil {
            log.Printf("Receive error: %v", err)
            continue
        }

        var order OrderEvent
        if err := json.Unmarshal(message.Message.Payload, &order); err != nil {
            log.Printf("Failed to unmarshal order: %v", err)
            message.Ack()
            continue
        }

        // Process the order
        if err := processOrder(order); err != nil {
            log.Printf("Failed to process order %s: %v", order.OrderID, err)
            message.Nack() // Retry
            continue
        }

        fmt.Printf("Processed order %s for customer %s amount $%.2f\n",
            order.OrderID, order.CustomerID, order.Amount)
            
        // Acknowledge successful processing
        message.Ack()
    }
}

func processOrder(order OrderEvent) error {
    // Your business logic here
    time.Sleep(100 * time.Millisecond)
    return nil
}
```

#### Admin Operations

**✅ Fully Implemented**: The Admin REST API is production-ready with comprehensive cluster management capabilities.

```bash
# Create topic with custom configuration
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{
    "name": "user-events",
    "partitions": 24,
    "replication_factor": 3,
    "retention_ms": 604800000,
    "segment_bytes": 1073741824,
    "compression_type": "lz4"
  }'

# List topics
curl http://localhost:8080/api/v1/topics

# Get topic details
curl http://localhost:8080/api/v1/topics/user-events

# Delete topic
curl -X DELETE http://localhost:8080/api/v1/topics/user-events

# Get cluster health and status
curl http://localhost:8080/api/v1/cluster

# List brokers with health status
curl http://localhost:8080/api/v1/brokers

# Check service health and uptime
curl http://localhost:8080/health

# Advanced features (future implementation)
# Partition rebalancing, ETL module management, and metrics endpoints
# will be available in future releases
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
# Unit tests (currently 88 tests passing)
cargo test --lib

# Integration tests (9 broker core tests + others)
cargo test --test integration_broker_core

# Run specific module tests
cargo test storage::
cargo test scaling::
cargo test broker::core

# Run with features
cargo test --features "io-uring,wasm"

# All tests
cargo test
```

## 📄 License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## 🔗 Links

- [Issue Tracker](https://github.com/cloudymoma/rustmq/issues)

---

**RustMQ** - Built with ❤️ in Rust for the cloud-native future. Optimized for Google Cloud Platform.
