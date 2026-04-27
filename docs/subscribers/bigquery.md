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

# Start the cluster with BigQuery subscriber (from docker/ directory)
cd docker && docker-compose --profile bigquery up -d

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
