# RustMQ Authentication & Authorization Performance Analysis

This document provides a comprehensive performance analysis of the proposed mTLS authentication and ACL-based authorization system, with specific optimizations to maintain sub-microsecond hot path performance.

## Executive Summary

The proposed design can achieve sub-microsecond authorization on the hot path with proper optimizations:
- **Hot Path Authorization**: ~50-100ns per request (hash map lookup)
- **mTLS Connection Overhead**: ~1-2ms (amortized over connection lifetime)
- **Cache Miss Penalty**: ~1-10ms (Controller RPC)
- **Memory Overhead**: ~1-10MB for typical ACL cache

## 1. Hot Path Performance Analysis

### Current State
- Request routing: ~500ns (bincode deserialization + handler dispatch)
- No authentication/authorization overhead

### With Authentication/Authorization
```
Per-Request Overhead = Principal Extraction + ACL Lookup + Permission Check
                    = 0ns + 50-100ns + 10ns
                    = 60-110ns total
```

**Key Insight**: Principal extraction is zero-cost after connection establishment because it's stored in the ConnectionPool entry.

### Optimization: Cached Principal in Connection State

```rust
struct ConnectionEntry {
    connection: Connection,
    created_at: Instant,
    principal: Arc<String>, // Add principal here
}

struct AuthorizedRequest<T> {
    principal: Arc<String>, // Zero-copy reference
    request: T,
}
```

## 2. Memory Usage Analysis

### ACL Cache Memory Model

```
Memory = (Principals × Topics × Permissions × Entry_Size) + Overhead

Where:
- Entry_Size = 8 bytes (hash) + 40 bytes (strings) + 8 bytes (permission) + 16 bytes (TTL) = 72 bytes
- HashMap overhead = ~1.5x

Example (1000 principals, 100 topics, 2 permissions/principal):
Memory = 1000 × 100 × 2 × 72 × 1.5 = ~21.6MB
```

### Memory Optimization Strategies

1. **Interned Strings**: Use string interning for principals and topics
   ```rust
   type InternedString = Arc<str>;
   type AclKey = (InternedString, InternedString, Permission);
   ```
   - Reduces memory by 60-80% for duplicate strings
   - Improves cache locality

2. **Compact Permission Representation**
   ```rust
   #[repr(u8)]
   enum Permission {
       Read = 0b01,
       Write = 0b10,
       ReadWrite = 0b11,
   }
   ```

3. **Bloom Filter Pre-check**
   ```rust
   struct AclCache {
       bloom: BloomFilter<AclKey>,  // 1 bit per entry
       cache: DashMap<AclKey, AclEntry>,
   }
   ```
   - Reduces negative lookup cost to ~10ns
   - Memory overhead: ~0.125 bytes per entry

## 3. Cache Performance Optimization

### Current Design Issues
- TTL-based eviction can cause cache storms
- No differentiation between hot/cold entries
- Single global cache contention

### Optimized Multi-Level Cache Design

```rust
struct OptimizedAclCache {
    // L1: Per-connection cache (thread-local, no locks)
    connection_cache: ThreadLocal<LruCache<AclKey, bool>>,
    
    // L2: Shared broker cache (concurrent, sharded)
    broker_cache: Arc<[DashMap<AclKey, AclEntry>; 16]>,
    
    // L3: Controller (source of truth)
    controller_client: Arc<ControllerClient>,
}

impl OptimizedAclCache {
    async fn check_permission(&self, key: AclKey) -> Result<bool> {
        // L1 check: ~10ns
        if let Some(allowed) = self.connection_cache.get().get(&key) {
            return Ok(*allowed);
        }
        
        // L2 check: ~50ns
        let shard = hash(&key) % 16;
        if let Some(entry) = self.broker_cache[shard].get(&key) {
            if !entry.is_expired() {
                self.connection_cache.get_mut().put(key.clone(), entry.allowed);
                return Ok(entry.allowed);
            }
        }
        
        // L3 fallback: ~1-10ms
        let result = self.fetch_from_controller(key).await?;
        self.update_caches(key, result);
        Ok(result)
    }
}
```

**Performance Characteristics**:
- L1 Hit Rate: ~95% (connection-affinity)
- L2 Hit Rate: ~99% (broker-wide)
- Average latency: 0.95×10ns + 0.04×50ns + 0.01×5ms = ~62ns

## 4. mTLS Connection Overhead Analysis

### QUIC + TLS 1.3 Performance

```
Connection Establishment = TCP (0 RTT) + TLS 1.3 (1 RTT) + Certificate Verification
                        = 0 + ~0.5ms + ~1.5ms
                        = ~2ms total
```

### Optimizations

1. **Session Resumption** (0-RTT)
   ```rust
   let mut server_config = ServerConfig::with_crypto(Arc::new(
       rustls::ServerConfig::builder()
           .with_safe_defaults()
           .with_client_cert_verifier(verifier)
           .with_single_cert(cert_chain, key)?
           .with_session_storage(ServerSessionMemoryCache::new(10_000))
   ));
   ```
   - Reduces reconnection to ~0.1ms
   - Memory: ~1KB per cached session

2. **Certificate Caching**
   ```rust
   struct CertificateCache {
       parsed_certs: DashMap<Vec<u8>, ParsedCertificate>,
   }
   ```
   - Avoids repeated X.509 parsing (~1ms saved)
   - Memory: ~2KB per unique certificate

3. **Connection Pooling Recommendations**
   - Clients should maintain persistent connections
   - Implement exponential backoff for reconnections
   - Monitor connection churn rate

## 5. Scalability Analysis

### Controller RPC Load

```
RPC_Load = (Unique_Principals × Topics × Cache_Miss_Rate) / TTL
         = (1000 × 100 × 0.01) / 300s
         = ~3.3 RPC/s
```

### Optimization: Batch ACL Fetching

```rust
struct BatchedAclFetcher {
    pending: Arc<Mutex<HashMap<AclKey, Vec<oneshot::Sender<bool>>>>>,
    batch_timeout: Duration,
    batch_size: usize,
}

impl BatchedAclFetcher {
    async fn fetch(&self, key: AclKey) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        
        let should_trigger = {
            let mut pending = self.pending.lock().unwrap();
            pending.entry(key).or_default().push(tx);
            pending.len() >= self.batch_size
        };
        
        if should_trigger {
            self.trigger_batch_fetch().await;
        }
        
        timeout(self.batch_timeout, rx).await?
    }
}
```

**Benefits**:
- Reduces RPC count by 10-100x
- Improves cache warming efficiency
- Handles thundering herd scenarios

## 6. Alternative Data Structures

### 1. Radix Tree for Hierarchical Permissions

```rust
struct RadixAclTree {
    root: RadixNode,
}

impl RadixAclTree {
    fn check_permission(&self, principal: &str, topic: &str) -> Option<Permission> {
        // O(k) where k = length of topic
        // Supports wildcards: "payments/*" matches "payments/orders"
        self.root.lookup(principal, topic)
    }
}
```

**Performance**: ~100-200ns (slightly slower than hash map but supports patterns)

### 2. Cuckoo Filter for Negative Caching

```rust
struct NegativeCache {
    filter: CuckooFilter<AclKey>,
}
```

**Benefits**:
- False positive rate: 0.001%
- Memory: 12 bits per entry
- Lookup: ~20ns

## 7. Benchmark Recommendations

### Key Metrics to Track

```rust
#[derive(Debug)]
struct AuthMetrics {
    // Latency metrics (histograms)
    auth_check_latency_ns: Histogram,
    cache_hit_rate: Gauge,
    
    // Connection metrics
    mtls_handshake_duration_ms: Histogram,
    active_connections: Gauge,
    connection_churn_rate: Counter,
    
    // Cache metrics
    cache_size_bytes: Gauge,
    cache_evictions_per_sec: Counter,
    controller_rpc_latency_ms: Histogram,
    
    // Business metrics
    unauthorized_requests_per_sec: Counter,
}
```

### Benchmark Scenarios

1. **Hot Path Benchmark**
   ```rust
   #[bench]
   fn bench_authorized_produce(b: &mut Bencher) {
       let cache = setup_warmed_cache();
       let principal = Arc::new("payment-service".to_string());
       
       b.iter(|| {
           cache.check_permission(
               principal.clone(),
               "payments",
               Permission::Write
           )
       });
   }
   ```

2. **Cache Miss Benchmark**
   ```rust
   #[bench]
   fn bench_cache_miss_recovery(b: &mut Bencher) {
       let cache = setup_empty_cache();
       
       b.iter(|| {
           runtime.block_on(async {
               cache.check_permission(
                   random_principal(),
                   random_topic(),
                   Permission::Read
               ).await
           })
       });
   }
   ```

## 8. Production Deployment Recommendations

### 1. Gradual Rollout
```yaml
# ConfigMap for feature flags
auth:
  enabled: true
  enforcement_mode: "log_only"  # Start with logging
  cache_ttl_seconds: 300
  batch_fetch_size: 100
  negative_cache_enabled: true
```

### 2. Cache Warming on Startup
```rust
impl Broker {
    async fn initialize(&mut self) -> Result<()> {
        // Warm cache with common patterns
        let common_acls = self.controller_client
            .fetch_common_acls()
            .await?;
        
        for acl in common_acls {
            self.acl_cache.preload(acl);
        }
        
        Ok(())
    }
}
```

### 3. Monitoring Dashboard Queries

```promql
# Auth performance
histogram_quantile(0.99, auth_check_latency_ns) < 1000  # 99th percentile < 1μs

# Cache effectiveness  
cache_hit_rate > 0.99  # 99% cache hit rate

# Controller load
rate(controller_rpc_total[5m]) < 100  # RPC rate < 100/s
```

## 9. Cost-Benefit Analysis

### Performance Impact Summary

| Operation | Without Auth | With Auth (Optimized) | Overhead |
|-----------|-------------|----------------------|----------|
| Produce (hot path) | 500ns | 560-610ns | +12-22% |
| Connection setup | 1ms | 2-3ms | +100-200% |
| Memory per broker | - | 10-50MB | - |

### Scaling Characteristics

- **Linear with principals**: Cache size grows linearly
- **Sub-linear with topics**: Due to pattern matching
- **Constant per request**: No scaling degradation

## 10. Final Recommendations

1. **Implement Multi-Level Caching**: Critical for sub-microsecond performance
2. **Use String Interning**: Reduces memory by 60-80%
3. **Enable Batch Fetching**: Reduces Controller load by 10-100x
4. **Add Negative Caching**: Prevents DoS via invalid requests
5. **Implement Connection-Affinity**: Improves L1 cache hit rate
6. **Monitor Cache Metrics**: Set alerts for hit rate < 95%
7. **Use Session Resumption**: Critical for client reconnection performance

With these optimizations, RustMQ can maintain its sub-microsecond promise while adding robust security.