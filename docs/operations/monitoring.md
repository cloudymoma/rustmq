# RustMQ Monitoring & Observability

RustMQ provides comprehensive monitoring through Prometheus metrics and Grafana dashboards, specifically optimized for GKE Managed Prometheus.

## 📊 Metrics Endpoints

Each RustMQ component exposes a `/metrics` HTTP endpoint for scraping:

- **Broker**: Port 9643 (Admin HTTP)
- **Controller**: Port 9642 (Admin HTTP)
- **Admin Server**: Port 8080 (REST API)

## 🔍 Key Metrics to Watch

### Core Performance
- `rustmq_messages_produced_total`: Total number of messages received from producers.
- `rustmq_messages_consumed_total`: Total number of messages sent to consumers.
- `rustmq_produce_latency_ms`: Produce operation latency histogram.
- `rustmq_fetch_latency_ms`: Fetch operation latency histogram.

### Storage
- `rustmq_wal_append_latency_us`: WAL write latency (Direct I/O).
- `rustmq_object_storage_upload_bytes_total`: Data uploaded to GCS.
- `rustmq_cache_hit_ratio`: LruCache efficiency.

### Reliability
- `rustmq_replication_lag_bytes`: Replication offset lag between leader and followers.
- `rustmq_raft_term`: Current Raft election term.
- `rustmq_connection_errors_total`: QUIC connection handshake failures.

## 🚨 Alerting Rules

Critical alerts are defined in `gke/manifests/base/monitoring/prometheusrule.yaml`:

1. **RustMQDiskFull**: Fired when WAL storage available space is < 15%.
2. **RustMQBrokerDown**: Fired if any broker instance is unreachable.
3. **RustMQRaftLeaderChanges**: Fired on excessive leader elections (indicates network instability).

## 📈 Dashboards

Grafana dashboards are available in the `monitoring/dashboards/` directory:

- `rustmq-overview.json`: High-level cluster health and throughput.
- `rustmq-storage.json`: Detailed WAL and Object Storage metrics.
- `rustmq-security.json`: Authentication/Authorization performance and audit stats.

## 🛠️ GKE Managed Prometheus

RustMQ is configured to use GKE Managed Prometheus by default:
- `ServiceMonitor` resources are used for target discovery.
- Use the Google Cloud Console "Monitoring" tab for built-in visualizations.
