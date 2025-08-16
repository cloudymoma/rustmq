# RustMQ Security Performance Tuning Guide

## Overview

This guide provides comprehensive tuning recommendations for optimizing RustMQ's security module performance in production environments. It covers configuration parameters, deployment strategies, monitoring setup, and troubleshooting techniques.

## Table of Contents

- [Quick Start Tuning](#quick-start-tuning)
- [Configuration Reference](#configuration-reference)
- [Workload-Specific Optimization](#workload-specific-optimization)
- [Memory Tuning](#memory-tuning)
- [Cache Optimization](#cache-optimization)
- [Concurrency Tuning](#concurrency-tuning)
- [Monitoring and Alerting](#monitoring-and-alerting)
- [Troubleshooting Guide](#troubleshooting-guide)
- [Performance Patterns](#performance-patterns)

## Quick Start Tuning

### **High-Performance Configuration**

For maximum throughput with moderate memory usage:

```toml
# config/broker.toml
[security.acl]
cache_size_mb = 512              # Large cache for high hit rates
l2_shard_count = 64              # High concurrency
bloom_filter_size = 500_000      # Large negative cache
enable_string_interning = true   # Memory optimization

[security.ultra_fast]
l1_cache_size = 8192            # Large per-connection cache
l1_cache_ttl_seconds = 600      # 10 minutes
enable_statistics = true         # Performance monitoring

[security.performance_targets]
max_l1_latency_ns = 1200        # Current target
max_l2_latency_ns = 5000        # Production target
min_throughput_ops_per_sec = 1_000_000  # 1M ops/sec
```

### **Memory-Optimized Configuration**

For minimal memory usage with good performance:

```toml
# config/broker.toml
[security.acl]
cache_size_mb = 64              # Smaller cache
l2_shard_count = 16             # Fewer shards
bloom_filter_size = 50_000      # Smaller filter
enable_string_interning = true  # Essential for memory savings

[security.ultra_fast]
l1_cache_size = 2048           # Smaller per-connection cache
l1_cache_ttl_seconds = 300     # 5 minutes
cleanup_interval_seconds = 120  # Frequent cleanup

[security.acl]
cache_cleanup_interval_seconds = 180
expired_entry_cleanup_threshold = 500
```

### **Latency-Optimized Configuration**

For ultra-low latency at any cost:

```toml
# config/broker.toml
[security.acl]
cache_size_mb = 1024            # Very large cache
l2_shard_count = 128            # Maximum sharding
bloom_filter_size = 1_000_000   # Large bloom filter
enable_string_interning = true  # Reduce allocation overhead

[security.ultra_fast]
l1_cache_size = 16384          # Maximum per-connection cache
l1_cache_ttl_seconds = 1800    # 30 minutes
enable_prefetch = true          # Predictive loading

[security.performance_targets]
max_l1_latency_ns = 500        # Aggressive target
max_l2_latency_ns = 2000       # Tight constraint
```

## Configuration Reference

### **Security Module Configuration**

#### **Core Security Settings**
```toml
[security]
# Global security settings
enabled = true
fail_open = false               # Fail-safe: deny on errors
audit_enabled = true            # Enable security auditing
metrics_enabled = true          # Performance metrics
log_level = "info"             # Security event logging

# TLS Configuration
[security.tls]
enabled = true
cert_path = "/etc/rustmq/certs/server.crt"
key_path = "/etc/rustmq/certs/server.key"
ca_path = "/etc/rustmq/certs/ca.crt"
verify_client_cert = true      # mTLS authentication
```

#### **ACL Configuration**
```toml
[security.acl]
# Cache Settings
cache_size_mb = 100            # Total L2 cache memory
l2_shard_count = 32            # Concurrent access shards
cache_ttl_seconds = 3600       # 1 hour default TTL
enable_string_interning = true # Memory optimization

# Bloom Filter Settings  
bloom_filter_size = 100_000    # Element capacity
bloom_filter_fp_rate = 0.01    # 1% false positive rate
bloom_rebuild_interval = 86400  # 24 hours

# Cleanup and Maintenance
cache_cleanup_interval_seconds = 300    # 5 minutes
expired_entry_cleanup_threshold = 1000  # Batch cleanup size
max_rule_length = 1024                  # Security limit
```

#### **Ultra-Fast Cache Configuration**
```toml
[security.ultra_fast]
# L1 Cache (Thread-Local)
l1_cache_size = 4096           # Entries per connection
l1_cache_ttl_seconds = 300     # 5 minutes
enable_statistics = true        # Performance tracking

# Performance Targets
[security.performance_targets]
max_l1_latency_ns = 1200       # L1 cache target
max_l2_latency_ns = 5000       # L2 cache target
max_total_latency_ns = 10000   # End-to-end target
min_throughput_ops_per_sec = 500_000  # Minimum performance
```

#### **Certificate Management**
```toml
[security.certificate_management]
# Storage and Caching
cert_cache_size = 1000         # Certificate cache entries
cert_cache_ttl_seconds = 7200  # 2 hours
validation_cache_size = 5000   # Validation result cache

# Performance Settings
enable_ocsp_checking = false   # Disable for performance
enable_crl_checking = false    # Disable for performance
cert_chain_max_depth = 5       # Limit chain validation
```

#### **Rate Limiting Configuration**
```toml
[security.rate_limiting]
# Global Limits
global_requests_per_second = 10000
global_burst_capacity = 20000

# Per-IP Limits
per_ip_requests_per_second = 100
per_ip_burst_capacity = 200
per_ip_tracking_size = 10000   # Max tracked IPs

# Endpoint-Specific Limits
[security.rate_limiting.endpoints]
health_rps = 1000              # Health check endpoints
read_rps = 500                 # Read operations
write_rps = 100                # Write operations
admin_rps = 50                 # Administrative operations
```

## Workload-Specific Optimization

### **High-Throughput Messaging**

**Characteristics**: Millions of messages/sec, high cache hit rates, stable principals

**Optimization Strategy**:
```toml
[security.acl]
cache_size_mb = 1024          # Large cache for hot data
l2_shard_count = 64           # High concurrency support
bloom_filter_size = 1_000_000 # Avoid false positives

[security.ultra_fast]
l1_cache_size = 16384         # Large per-connection cache
l1_cache_ttl_seconds = 1800   # Long TTL for stable workload
```

**Expected Performance**: 2M+ authorizations/sec, <1μs latency

### **Multi-Tenant Environment**

**Characteristics**: Many different principals, diverse topic access patterns

**Optimization Strategy**:
```toml
[security.acl]
cache_size_mb = 512           # Balanced cache size
l2_shard_count = 128          # High sharding for diversity
enable_string_interning = true # Essential for many principals
bloom_filter_size = 500_000   # Large negative cache

[security.ultra_fast]
l1_cache_size = 4096          # Moderate per-connection cache
l1_cache_ttl_seconds = 300    # Shorter TTL for changing patterns
```

**Expected Performance**: 1M+ authorizations/sec, <2μs latency

### **Batch Processing Workload**

**Characteristics**: Periodic high-volume bursts, predictable access patterns

**Optimization Strategy**:
```toml
[security.acl]
cache_size_mb = 256           # Smaller cache (burst-tolerant)
l2_shard_count = 32           # Standard sharding
cache_ttl_seconds = 7200      # Long TTL for batch jobs

[security.ultra_fast]
l1_cache_size = 8192          # Large cache for burst periods
l1_cache_ttl_seconds = 600    # Persist through batch duration
```

**Expected Performance**: 500K+ authorizations/sec during bursts

### **Real-Time Analytics**

**Characteristics**: Frequent permission changes, low latency requirements

**Optimization Strategy**:
```toml
[security.acl]
cache_size_mb = 128           # Smaller cache (frequent invalidation)
l2_shard_count = 64           # High concurrency
cache_ttl_seconds = 300       # Short TTL for fresh data

[security.ultra_fast]
l1_cache_size = 2048          # Smaller cache (changing permissions)
l1_cache_ttl_seconds = 60     # Very short TTL
```

**Expected Performance**: 200K+ authorizations/sec, <5μs latency

## Memory Tuning

### **Memory Sizing Guidelines**

#### **Cache Size Calculation**
```
L1 Cache Memory = connections × l1_cache_size × 32 bytes
L2 Cache Memory = cache_size_mb (configured)
Bloom Filter Memory = bloom_filter_size × 1.2 bits ÷ 8
String Interner Memory = unique_strings × avg_string_length × 0.3

Total Memory ≈ L1 + L2 + Bloom + Interner + 20% overhead
```

#### **Example Sizing**
```
Small Deployment (1K connections):
  L1: 1K × 4K × 32B = 128MB
  L2: 100MB (configured)
  Bloom: 100K × 1.2 ÷ 8 = 15KB
  Interner: 10MB (estimated)
  Total: ~253MB

Large Deployment (50K connections):
  L1: 50K × 4K × 32B = 6.4GB
  L2: 1GB (configured)
  Bloom: 1M × 1.2 ÷ 8 = 150KB
  Interner: 100MB (estimated)
  Total: ~7.5GB
```

### **Memory Optimization Techniques**

#### **String Interning Configuration**
```toml
[security.acl]
enable_string_interning = true
interner_cleanup_interval = 3600    # 1 hour
interner_max_entries = 100_000      # Prevent unbounded growth
interner_memory_limit_mb = 100      # Memory cap
```

#### **Cache Eviction Tuning**
```toml
[security.acl]
# L2 Cache Eviction (when size-limited)
eviction_policy = "lru"            # Least Recently Used
eviction_batch_size = 100          # Batch evictions for efficiency
eviction_threshold = 0.85          # Start eviction at 85% full

# Expired Entry Cleanup
expired_entry_cleanup_threshold = 1000  # Batch size
cleanup_interval_seconds = 300          # 5 minutes
max_cleanup_duration_ms = 50            # Limit cleanup time
```

### **Memory Monitoring**

#### **Key Memory Metrics**
```
rustmq_security_l1_cache_memory_bytes
rustmq_security_l2_cache_memory_bytes
rustmq_security_bloom_filter_memory_bytes
rustmq_security_string_interner_memory_bytes
rustmq_security_total_memory_bytes
```

#### **Memory Alerts**
```yaml
- alert: SecurityMemoryHigh
  expr: rustmq_security_total_memory_bytes > 1073741824  # 1GB
  for: 5m
  annotations:
    summary: "Security module memory usage high"

- alert: SecurityMemoryGrowth
  expr: increase(rustmq_security_total_memory_bytes[1h]) > 104857600  # 100MB/hour
  for: 10m
  annotations:
    summary: "Security module memory growing rapidly"
```

## Cache Optimization

### **Advanced Certificate Caching**

RustMQ features advanced certificate caching capabilities that significantly improve certificate validation performance:

```toml
[security.certificate_caching]
# Certificate cache settings
cert_cache_size = 10000              # Maximum cached certificates
cert_cache_ttl_hours = 24           # Certificate cache TTL
ca_cache_size = 100                 # Maximum cached CA chains
ca_cache_ttl_hours = 168            # CA cache TTL (1 week)

# Performance tuning
enable_batch_validation = true      # Enable batch operations
enable_certificate_prefetch = true  # Enable prefetching
prefetch_recently_used_count = 50   # Prefetch top N recently used

# Cache invalidation
auto_invalidate_expired = true      # Auto-invalidate expired certificates
invalidation_check_interval_minutes = 30  # Check interval

# WebPKI integration
webpki_cache_enabled = true         # Enable WebPKI-based caching
webpki_fallback_cache_enabled = true # Cache fallback results
trust_anchor_cache_size = 50       # Trust anchor cache size

# Metrics collection
enable_certificate_metrics = true   # Enable detailed metrics
parse_timing_enabled = true        # Track certificate parsing times
cache_hit_rate_target = 0.90       # Target 90% hit rate
```

### **L1 Cache Tuning**

#### **Size Optimization**
```toml
# Conservative (low memory)
l1_cache_size = 1024              # 32KB per connection

# Balanced (good performance/memory)
l1_cache_size = 4096              # 128KB per connection

# Aggressive (maximum performance)
l1_cache_size = 16384             # 512KB per connection
```

#### **TTL Optimization**
```toml
# Fast-changing permissions
l1_cache_ttl_seconds = 60         # 1 minute

# Stable permissions  
l1_cache_ttl_seconds = 300        # 5 minutes

# Very stable permissions
l1_cache_ttl_seconds = 1800       # 30 minutes
```

### **L2 Cache Tuning**

#### **Sharding Optimization**
```
Shard Count Guidelines:
- CPU Cores ≤ 8: shard_count = 16
- CPU Cores 8-16: shard_count = 32  
- CPU Cores 16-32: shard_count = 64
- CPU Cores > 32: shard_count = 128

Rule: shard_count = max(16, cpu_cores × 2)
```

#### **Memory vs. Performance Trade-offs**
```toml
# Memory-constrained
[security.acl]
cache_size_mb = 64
eviction_policy = "lru"
eviction_threshold = 0.75

# Performance-focused
[security.acl]  
cache_size_mb = 1024
eviction_policy = "lru"
eviction_threshold = 0.95
```

### **Bloom Filter Tuning**

#### **Size vs. Accuracy Trade-offs**
```
Bloom Filter Sizing:
- 10K elements: 17KB memory, 0.01% FP rate
- 100K elements: 120KB memory, 0.007% FP rate  
- 1M elements: 1.2MB memory, 0.0007% FP rate

False Positive Impact:
- Each FP causes ~5ms ACL database lookup
- Target: <0.1% FP rate for production workloads
```

#### **Rebuild Strategy**
```toml
[security.acl]
bloom_rebuild_interval = 86400    # 24 hours
bloom_rebuild_threshold = 0.05    # Rebuild at 5% FP rate
bloom_size_growth_factor = 1.5    # Grow by 50% on rebuild
```

## Concurrency Tuning

### **Thread Pool Configuration**

#### **Security Worker Threads**
```toml
[security.workers]
# Authorization worker pool
auth_worker_threads = 16          # 2x CPU cores recommended
auth_queue_size = 10000           # Pending request buffer

# Certificate validation pool  
cert_worker_threads = 4           # I/O bound, fewer threads
cert_queue_size = 1000            # Smaller queue

# ACL database pool
acl_worker_threads = 8            # Database connections
acl_connection_pool_size = 16     # Database connection pool
```

#### **Lock-Free Optimization**
```toml
[security.concurrency]
# L2 Cache Sharding
prefer_lock_free = true           # Use DashMap over Mutex<HashMap>
shard_load_factor = 0.75          # Balance memory vs. contention

# Atomic Operations
use_atomic_counters = true        # Metrics without locks
atomic_relaxed_ordering = true    # Performance over strict ordering
```

### **CPU Affinity Tuning**

#### **NUMA Awareness**
```bash
# Bind security worker threads to specific CPU cores
export RUSTMQ_SECURITY_AFFINITY="0-7"    # Use cores 0-7
export RUSTMQ_MEMORY_AFFINITY="node0"    # Use NUMA node 0

# Alternative: use numactl
numactl --cpunodebind=0 --membind=0 ./rustmq-broker
```

#### **CPU Cache Optimization**
```toml
[security.cpu_optimization]
# Cache-friendly data layout
enable_cache_line_padding = true   # Avoid false sharing
prefer_sequential_access = true    # CPU cache prefetching
batch_operations = true            # Reduce cache misses
```

## Monitoring and Alerting

### **Performance Metrics**

#### **Key Performance Indicators (KPIs)**
```
# Latency Metrics
rustmq_security_authorization_duration_seconds
rustmq_security_l1_cache_latency_seconds  
rustmq_security_l2_cache_latency_seconds
rustmq_security_bloom_filter_latency_seconds

# Throughput Metrics
rustmq_security_authorizations_per_second
rustmq_security_cache_operations_per_second
rustmq_security_total_operations_per_second

# Cache Efficiency Metrics
rustmq_security_l1_cache_hit_rate
rustmq_security_l2_cache_hit_rate
rustmq_security_bloom_filter_accuracy
```

#### **Prometheus Configuration**
```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'rustmq-security'
    static_configs:
      - targets: ['broker1:9091', 'broker2:9091']
    metrics_path: '/metrics'
    scrape_interval: 5s
```

### **Grafana Dashboards**

#### **Security Performance Dashboard**
```json
{
  "dashboard": {
    "title": "RustMQ Security Performance",
    "panels": [
      {
        "title": "Authorization Latency",
        "type": "graph",
        "targets": [
          {
            "expr": "histogram_quantile(0.99, rustmq_security_authorization_duration_seconds_bucket)",
            "legendFormat": "P99 Latency"
          }
        ]
      },
      {
        "title": "Cache Hit Rates", 
        "type": "stat",
        "targets": [
          {
            "expr": "rustmq_security_l1_cache_hit_rate * 100",
            "legendFormat": "L1 Hit Rate %"
          }
        ]
      }
    ]
  }
}
```

### **Alert Rules**

#### **Performance Degradation Alerts**
```yaml
groups:
  - name: rustmq_security_performance
    rules:
      - alert: SecurityLatencyHigh
        expr: histogram_quantile(0.99, rustmq_security_authorization_duration_seconds_bucket) > 0.005
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "Security authorization latency is high"
          description: "P99 latency is {{ $value }}s, exceeding 5ms threshold"

      - alert: SecurityCacheHitRateLow
        expr: rustmq_security_l1_cache_hit_rate < 0.8
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Security cache hit rate is low"
          description: "L1 cache hit rate is {{ $value | humanizePercentage }}"

      - alert: SecurityThroughputLow
        expr: rate(rustmq_security_authorizations_total[5m]) < 100000
        for: 3m
        labels:
          severity: critical
        annotations:
          summary: "Security authorization throughput is low"
          description: "Authorization rate is {{ $value }} ops/sec"
```

## Troubleshooting Guide

### **Common Performance Issues**

#### **High L1 Cache Miss Rate**

**Symptoms**:
- L1 hit rate < 70%
- Increased L2 cache load
- Higher authorization latency

**Diagnosis**:
```bash
# Check L1 cache configuration
grep l1_cache_size config/broker.toml
grep l1_cache_ttl_seconds config/broker.toml

# Monitor L1 metrics
curl localhost:9091/metrics | grep l1_cache_hit_rate
```

**Solutions**:
```toml
# Increase cache size
l1_cache_size = 8192              # Double the size

# Increase TTL for stable workloads
l1_cache_ttl_seconds = 600        # 10 minutes

# Check for memory pressure
memory_pressure_threshold = 0.85  # Monitor memory usage
```

#### **L2 Cache Contention**

**Symptoms**:
- L2 latency > 5ms
- High CPU usage in cache operations
- Lock contention in metrics

**Diagnosis**:
```bash
# Check shard count vs. CPU cores
nproc
grep l2_shard_count config/broker.toml

# Monitor contention metrics
curl localhost:9091/metrics | grep cache_contention
```

**Solutions**:
```toml
# Increase sharding
l2_shard_count = 128              # Higher than CPU count

# Reduce cache size if memory-bound
cache_size_mb = 256               # Smaller cache, less contention

# Enable lock-free optimizations
prefer_lock_free = true
```

#### **Memory Pressure Issues**

**Symptoms**:
- High GC pause times
- Out-of-memory errors
- Degraded cache performance

**Diagnosis**:
```bash
# Check memory usage
cat /proc/meminfo | grep Available
ps aux | grep rustmq-broker

# Monitor security memory metrics
curl localhost:9091/metrics | grep security_memory_bytes
```

**Solutions**:
```toml
# Reduce cache sizes
cache_size_mb = 64                # Smaller L2 cache
l1_cache_size = 2048              # Smaller L1 cache
bloom_filter_size = 50_000        # Smaller bloom filter

# Enable aggressive cleanup
cleanup_interval_seconds = 120    # More frequent cleanup
eviction_threshold = 0.7          # Earlier eviction
```

#### **Bloom Filter False Positives**

**Symptoms**:
- Unnecessary ACL database lookups
- Higher than expected latency for cache misses
- Poor cache efficiency

**Diagnosis**:
```bash
# Check false positive rate
curl localhost:9091/metrics | grep bloom_filter_false_positive_rate

# Monitor database lookup frequency
curl localhost:9091/metrics | grep acl_database_lookups_total
```

**Solutions**:
```toml
# Increase bloom filter size
bloom_filter_size = 500_000       # 5x larger filter

# Decrease false positive rate
bloom_filter_fp_rate = 0.001      # 0.1% instead of 1%

# Enable automatic rebuilding
bloom_rebuild_threshold = 0.01    # Rebuild at 1% FP rate
```

### **Performance Debugging Tools**

#### **Built-in Profiling**
```bash
# Enable detailed performance profiling
export RUSTMQ_SECURITY_PROFILE=true
export RUSTMQ_SECURITY_TRACE=debug

# Run broker with profiling
./rustmq-broker --config config/broker.toml --profile
```

#### **External Profiling Tools**
```bash
# CPU profiling with perf
perf record -g ./rustmq-broker --config config/broker.toml
perf report

# Memory profiling with valgrind
valgrind --tool=massif ./rustmq-broker --config config/broker.toml

# Rust-specific profiling
cargo install flamegraph
cargo flamegraph --bin rustmq-broker
```

### **Load Testing**

#### **Security-Specific Load Tests**
```bash
# Authorization throughput test
cargo test test_authorization_latency_requirements --release

# Concurrent security operations test  
cargo test test_concurrent_security_operations_stress --release

# Memory pressure test
cargo test test_memory_usage_and_string_interning --release

# Cache efficiency test
cargo test test_cache_performance_under_load --release
```

#### **Production Load Simulation**
```bash
# Use rustmq-admin for load testing
./rustmq-admin benchmark-security \
  --connections 1000 \
  --requests-per-second 100000 \
  --duration 300s \
  --principals 10000 \
  --topics 1000
```

## Performance Patterns

### **Cache Warming Strategies**

#### **Predictive Cache Population**
```rust
// Warm critical caches on startup
async fn warm_security_caches() {
    // Pre-populate with common principals
    let common_principals = vec!["producer-1", "consumer-1", "admin"];
    let common_topics = vec!["orders", "events", "metrics"];
    
    for principal in common_principals {
        for topic in common_topics {
            let key = AclKey::new(principal, topic, Permission::Read);
            security_manager.preload_authorization(&key).await;
        }
    }
}
```

#### **Background Cache Refresh**
```toml
[security.cache_warming]
enable_background_refresh = true
refresh_interval_seconds = 3600     # 1 hour
refresh_threshold = 0.1             # Refresh when 10% capacity reached
warmup_on_startup = true            # Pre-populate critical entries
```

### **Adaptive Performance Tuning**

#### **Dynamic Cache Sizing**
```toml
[security.adaptive_tuning]
enable_auto_sizing = true
min_cache_size_mb = 64              # Minimum cache size
max_cache_size_mb = 1024            # Maximum cache size  
size_adjustment_factor = 1.2        # Growth/shrink factor
adjustment_interval_seconds = 3600   # 1 hour evaluation
```

#### **Load-Based Configuration**
```rust
// Adjust cache parameters based on load
if current_ops_per_sec > 1_000_000 {
    // High load: prioritize performance
    l1_cache_size = 16384;
    l2_shard_count = 128;
    cache_ttl = 1800;
} else if current_ops_per_sec < 100_000 {
    // Low load: optimize memory
    l1_cache_size = 2048;
    l2_shard_count = 16;
    cache_ttl = 300;
}
```

### **Best Practices Summary**

#### **Configuration Best Practices**
1. **Start with conservative settings** and tune based on metrics
2. **Monitor cache hit rates** as primary performance indicator
3. **Size caches based on working set** not total data size
4. **Use string interning** for any deployment with >1K principals
5. **Tune shard count** to match CPU core count × 2

#### **Operational Best Practices**
1. **Set up comprehensive monitoring** before production deployment
2. **Establish performance baselines** for regression detection
3. **Test configuration changes** in staging environment first
4. **Monitor memory usage trends** to prevent out-of-memory issues
5. **Plan for growth** with headroom in cache sizes

#### **Troubleshooting Best Practices**
1. **Check cache hit rates first** when investigating performance issues
2. **Use built-in metrics** before external profiling tools
3. **Test with realistic data** that matches production patterns
4. **Consider workload characteristics** when tuning parameters
5. **Validate changes** with load testing before production rollout

---

*Tuning Guide Version: 1.0.0*  
*Last Updated: December 2024*  
*Compatible with: RustMQ 1.0.0, Rust 2024 Edition*