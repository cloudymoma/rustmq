# RustMQ Monitoring & Observability

RustMQ provides comprehensive monitoring through Prometheus metrics and Grafana dashboards, specifically optimized for GKE Managed Prometheus.

## 📊 Metrics Endpoints

Each RustMQ component exposes a `/metrics` HTTP endpoint for scraping:

- **Broker**: Port 9643 (Admin HTTP) - includes MetricsCollector data with 10s heartbeat to controller
- **Controller**: Port 9642 (Admin HTTP)
- **Admin Server**: Port 8080 (REST API)

## 🔍 Key Metrics to Watch

### Core Performance
- `rustmq_messages_produced_total`: Total number of messages received from producers.
- `rustmq_messages_consumed_total`: Total number of messages sent to consumers.
- `rustmq_produce_latency_ms`: Produce operation latency histogram.
- `rustmq_fetch_latency_ms`: Fetch operation latency histogram.

### Storage
> **Serving model:** the WAL is durability/recovery-only — consumer fetches are served
> from the **in-memory hot tier** (recent, un-tiered records) and **object storage** (cold),
> never from the WAL. The hot tier is bounded by `cache.write_cache_size_bytes`: when it
> fills, appends apply backpressure (soft limit force-seals + tiers to evict; hard limit
> blocks until tiering frees memory). Cold reads use a separate `cache.read_cache_size_bytes`
> download cache. Background tiering uploads sealed segments and compacts small cold objects;
> a durable broker-local cold index keeps cold data readable after restart/failover.

- `rustmq_wal_append_latency_us`: WAL write (durability) latency — Direct I/O on the active segment.
- `rustmq_object_storage_upload_bytes_total`: Data tiered (uploaded) to object storage.
- `rustmq_cache_hit_ratio`: Cold-read download-cache (LruCache) efficiency. A low ratio with
  rising fetch latency suggests the hot-tier budget is too small (records tier before consumers
  catch up, pushing reads to object storage).

### Reliability
- `rustmq_replication_lag_bytes`: Replication offset lag between leader and followers.
- `rustmq_raft_term`: Current Raft election term.
- `rustmq_connection_errors_total`: QUIC connection handshake failures.

### Broker Metrics (MetricsCollector)
- `rustmq_broker_cpu_usage` (Gauge): Current CPU usage percentage for the broker.
- `rustmq_broker_memory_usage` (Gauge): Current memory usage in bytes for the broker.
- `rustmq_broker_disk_usage` (Gauge): Current disk usage in bytes for the broker.
- `rustmq_broker_partition_count` (GaugeVec): Number of partitions by role (labels: role=leader|follower).
- `rustmq_broker_message_rate` (Counter): Total number of messages processed per second.
- `rustmq_broker_network_bytes_total` (CounterVec): Total network bytes transferred (labels: direction=tx|rx).
- `rustmq_broker_heartbeat_age_seconds` (Gauge): Time since last heartbeat sent to controller (updated every 10s).

### Consumer Group Metrics
- `rustmq_partition_high_watermark` (GaugeVec): High watermark offset for each partition (labels: topic, partition).
- `rustmq_group_committed_offset` (GaugeVec): Last committed offset for consumer group (labels: group, topic, partition).
- `rustmq_consumer_group_members` (GaugeVec): Number of active members in consumer group (label: group).
- `rustmq_consumer_group_rebalances_total` (CounterVec): Total number of rebalances per consumer group (label: group).

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
