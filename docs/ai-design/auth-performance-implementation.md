# RustMQ Authentication Performance Implementation Guide

This document provides concrete implementation examples for the performance optimizations identified in the authentication design analysis.

## 1. Optimized Connection Pool with Principal Storage

```rust
// src/network/connection_pool.rs

use std::sync::Arc;
use std::time::Instant;
use quinn::Connection;
use dashmap::DashMap;
use lru::LruCache;
use parking_lot::RwLock;

#[derive(Clone)]
pub struct AuthenticatedConnection {
    pub connection: Connection,
    pub principal: Arc<str>,  // Interned string for memory efficiency
    pub created_at: Instant,
    pub last_activity: Arc<RwLock<Instant>>,
    // L1 cache - connection-specific ACL cache
    pub acl_cache: Arc<RwLock<LruCache<AclKey, bool>>>,
}

pub struct OptimizedConnectionPool {
    // Sharded to reduce lock contention
    connections: Arc<[DashMap<String, AuthenticatedConnection>; 16]>,
    max_connections_per_shard: usize,
    // Principal string interning pool
    principal_pool: Arc<DashMap<String, Arc<str>>>,
}

impl OptimizedConnectionPool {
    pub fn new(max_total_connections: usize) -> Self {
        let shards = (0..16)
            .map(|_| DashMap::new())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
            
        Self {
            connections: Arc::new(shards),
            max_connections_per_shard: max_total_connections / 16,
            principal_pool: Arc::new(DashMap::new()),
        }
    }
    
    pub fn add_authenticated_connection(
        &self,
        client_id: String,
        connection: Connection,
        principal: String,
    ) -> Arc<str> {
        // Intern the principal string
        let interned_principal = self.principal_pool
            .entry(principal.clone())
            .or_insert_with(|| Arc::from(principal.as_str()))
            .clone();
        
        let shard_idx = xxhash_rust::xxh3::xxh3_64(client_id.as_bytes()) as usize % 16;
        let shard = &self.connections[shard_idx];
        
        // Create connection with L1 cache
        let auth_conn = AuthenticatedConnection {
            connection,
            principal: interned_principal.clone(),
            created_at: Instant::now(),
            last_activity: Arc::new(RwLock::new(Instant::now())),
            acl_cache: Arc::new(RwLock::new(LruCache::new(1000))), // 1000 entries per connection
        };
        
        // LRU eviction within shard
        if shard.len() >= self.max_connections_per_shard {
            let oldest = shard
                .iter()
                .min_by_key(|entry| entry.created_at)
                .map(|entry| entry.key().clone());
                
            if let Some(oldest_id) = oldest {
                shard.remove(&oldest_id);
            }
        }
        
        shard.insert(client_id, auth_conn);
        interned_principal
    }
    
    pub fn get_connection(&self, client_id: &str) -> Option<AuthenticatedConnection> {
        let shard_idx = xxhash_rust::xxh3::xxh3_64(client_id.as_bytes()) as usize % 16;
        self.connections[shard_idx]
            .get(client_id)
            .map(|entry| {
                // Update last activity
                *entry.last_activity.write() = Instant::now();
                entry.value().clone()
            })
    }
}
```

## 2. High-Performance ACL Cache Implementation

```rust
// src/auth/acl_cache.rs

use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use bloom::{BloomFilter, ASMS};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::timeout;

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct AclKey {
    principal: Arc<str>,
    topic: Arc<str>,
    permission: Permission,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum Permission {
    Read = 0b01,
    Write = 0b10,
    Admin = 0b100,
}

pub struct AclEntry {
    allowed: bool,
    expires_at: Instant,
}

pub struct OptimizedAclCache {
    // Bloom filter for negative caching
    negative_cache: Arc<RwLock<BloomFilter>>,
    
    // L2 cache - sharded for concurrent access
    l2_cache: Arc<[DashMap<AclKey, AclEntry>; 32]>,
    
    // Batch fetcher
    batch_fetcher: Arc<BatchedAclFetcher>,
    
    // Metrics
    metrics: Arc<AuthMetrics>,
}

impl OptimizedAclCache {
    pub fn new(
        expected_entries: usize,
        controller_client: Arc<ControllerClient>,
        metrics: Arc<AuthMetrics>,
    ) -> Self {
        let bloom = BloomFilter::with_rate(0.001, expected_entries);
        let shards = (0..32)
            .map(|_| DashMap::new())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
            
        Self {
            negative_cache: Arc::new(RwLock::new(bloom)),
            l2_cache: Arc::new(shards),
            batch_fetcher: Arc::new(BatchedAclFetcher::new(controller_client)),
            metrics,
        }
    }
    
    pub async fn check_permission(
        &self,
        connection: &AuthenticatedConnection,
        topic: Arc<str>,
        permission: Permission,
    ) -> Result<bool> {
        let start = Instant::now();
        let key = AclKey {
            principal: connection.principal.clone(),
            topic,
            permission,
        };
        
        // L1 check - connection cache (no async, ~10ns)
        if let Some(&allowed) = connection.acl_cache.read().peek(&key) {
            self.metrics.record_l1_hit();
            self.metrics.record_auth_latency(start.elapsed());
            return Ok(allowed);
        }
        
        // Negative cache check (~20ns)
        {
            let bloom = self.negative_cache.read().await;
            if bloom.contains(&key) {
                self.metrics.record_negative_hit();
                self.metrics.record_auth_latency(start.elapsed());
                return Ok(false);
            }
        }
        
        // L2 check - broker cache (~50ns)
        let shard_idx = xxhash_rust::xxh3::xxh3_64(&bincode::serialize(&key)?) as usize % 32;
        if let Some(entry) = self.l2_cache[shard_idx].get(&key) {
            if entry.expires_at > Instant::now() {
                let allowed = entry.allowed;
                drop(entry); // Release lock early
                
                // Update L1
                connection.acl_cache.write().put(key.clone(), allowed);
                
                self.metrics.record_l2_hit();
                self.metrics.record_auth_latency(start.elapsed());
                return Ok(allowed);
            }
        }
        
        // L3 - Controller fetch (batched)
        self.metrics.record_cache_miss();
        let allowed = self.batch_fetcher.fetch(key.clone()).await?;
        
        // Update caches
        self.update_caches(connection, key, allowed).await;
        
        self.metrics.record_auth_latency(start.elapsed());
        Ok(allowed)
    }
    
    async fn update_caches(
        &self,
        connection: &AuthenticatedConnection,
        key: AclKey,
        allowed: bool,
    ) {
        // Update L1
        connection.acl_cache.write().put(key.clone(), allowed);
        
        // Update L2
        let shard_idx = xxhash_rust::xxh3::xxh3_64(&bincode::serialize(&key).unwrap()) as usize % 32;
        self.l2_cache[shard_idx].insert(
            key.clone(),
            AclEntry {
                allowed,
                expires_at: Instant::now() + Duration::from_secs(300), // 5 min TTL
            },
        );
        
        // Update negative cache
        if !allowed {
            self.negative_cache.write().await.insert(&key);
        }
    }
}
```

## 3. Batched ACL Fetching

```rust
// src/auth/batch_fetcher.rs

use std::collections::HashMap;
use tokio::sync::{oneshot, Mutex};
use tokio::time::{interval, Duration};

struct PendingRequest {
    senders: Vec<oneshot::Sender<bool>>,
}

pub struct BatchedAclFetcher {
    pending: Arc<Mutex<HashMap<AclKey, PendingRequest>>>,
    controller_client: Arc<ControllerClient>,
    batch_size: usize,
    batch_timeout: Duration,
    fetch_semaphore: Arc<Semaphore>,
}

impl BatchedAclFetcher {
    pub fn new(controller_client: Arc<ControllerClient>) -> Self {
        let fetcher = Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            controller_client,
            batch_size: 100,
            batch_timeout: Duration::from_millis(10),
            fetch_semaphore: Arc::new(Semaphore::new(10)), // Max 10 concurrent fetches
        };
        
        // Start background batch processor
        fetcher.start_batch_processor();
        fetcher
    }
    
    pub async fn fetch(&self, key: AclKey) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        
        let should_trigger = {
            let mut pending = self.pending.lock().await;
            let request = pending.entry(key.clone()).or_insert_with(|| PendingRequest {
                senders: Vec::new(),
            });
            request.senders.push(tx);
            
            pending.len() >= self.batch_size
        };
        
        if should_trigger {
            self.trigger_batch_fetch().await;
        }
        
        // Wait for response with timeout
        match timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err(RustMqError::Internal("ACL fetch cancelled".into())),
            Err(_) => Err(RustMqError::Timeout("ACL fetch timeout".into())),
        }
    }
    
    async fn trigger_batch_fetch(&self) {
        let _permit = match self.fetch_semaphore.try_acquire() {
            Ok(permit) => permit,
            Err(_) => return, // Another fetch is in progress
        };
        
        let batch = {
            let mut pending = self.pending.lock().await;
            std::mem::take(&mut *pending)
        };
        
        if batch.is_empty() {
            return;
        }
        
        let keys: Vec<AclKey> = batch.keys().cloned().collect();
        
        // Fetch from controller
        match self.controller_client.batch_fetch_acls(keys).await {
            Ok(results) => {
                for (key, request) in batch {
                    let allowed = results.get(&key).copied().unwrap_or(false);
                    for sender in request.senders {
                        let _ = sender.send(allowed);
                    }
                }
            }
            Err(e) => {
                // Notify all waiters of the error
                for (_, request) in batch {
                    for sender in request.senders {
                        let _ = sender.send(false); // Fail closed
                    }
                }
                tracing::error!("Batch ACL fetch failed: {}", e);
            }
        }
    }
    
    fn start_batch_processor(&self) {
        let pending = self.pending.clone();
        let batch_timeout = self.batch_timeout;
        let fetcher = self.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(batch_timeout);
            
            loop {
                interval.tick().await;
                
                let should_fetch = {
                    let pending = pending.lock().await;
                    !pending.is_empty()
                };
                
                if should_fetch {
                    fetcher.trigger_batch_fetch().await;
                }
            }
        });
    }
}
```

## 4. Enhanced QUIC Server with mTLS

```rust
// src/network/quic_server_auth.rs

use rustls::{Certificate, PrivateKey, ServerConfig};
use std::sync::Arc;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig};

pub struct AuthenticatedQuicServer {
    endpoint: Endpoint,
    connection_pool: Arc<OptimizedConnectionPool>,
    acl_cache: Arc<OptimizedAclCache>,
    request_router: Arc<RequestRouter>,
    session_cache: Arc<DashMap<Vec<u8>, SessionData>>,
}

impl AuthenticatedQuicServer {
    pub async fn new(config: NetworkConfig) -> Result<Self> {
        let tls_config = Self::create_mtls_config(&config.tls)?;
        let quinn_config = QuinnServerConfig::with_crypto(Arc::new(tls_config));
        
        // Enable 0-RTT for performance
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
        transport.keep_alive_interval(Some(Duration::from_secs(10)));
        
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(tls_config));
        server_config.transport = Arc::new(transport);
        server_config.use_stateless_retry(true);
        
        // ... rest of initialization
    }
    
    fn create_mtls_config(tls_config: &TlsConfig) -> Result<rustls::ServerConfig> {
        let ca_cert = load_ca_cert(&tls_config.ca_cert_path)?;
        let server_cert = load_certs(&tls_config.server_cert_path)?;
        let server_key = load_private_key(&tls_config.server_key_path)?;
        
        // Custom client certificate verifier with caching
        let verifier = Arc::new(CachingClientCertVerifier::new(ca_cert));
        
        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_client_cert_verifier(verifier)
            .with_single_cert(server_cert, server_key)?;
            
        // Enable session resumption
        config.session_storage = Arc::new(ServerSessionMemoryCache::new(10_000));
        config.send_tls13_tickets = 4; // Send multiple tickets for redundancy
        
        Ok(config)
    }
    
    async fn handle_connection(&self, connection: quinn::Connection) -> Result<()> {
        // Extract principal from certificate
        let principal = self.extract_principal(&connection)?;
        
        // Get client ID from connection
        let client_id = format!("{}:{}", 
            connection.remote_address().ip(),
            connection.remote_address().port()
        );
        
        // Store authenticated connection
        let interned_principal = self.connection_pool.add_authenticated_connection(
            client_id.clone(),
            connection.clone(),
            principal,
        );
        
        // Pre-warm ACL cache for common topics
        self.warm_acl_cache(&interned_principal).await?;
        
        // Handle streams
        loop {
            match connection.accept_bi().await {
                Ok((send, recv)) => {
                    let auth_conn = self.connection_pool
                        .get_connection(&client_id)
                        .ok_or("Connection not found")?;
                        
                    tokio::spawn(self.handle_authenticated_stream(
                        auth_conn,
                        send,
                        recv,
                    ));
                }
                Err(e) => {
                    self.connection_pool.remove_connection(&client_id);
                    return Err(e.into());
                }
            }
        }
    }
    
    fn extract_principal(&self, connection: &quinn::Connection) -> Result<String> {
        let identity = connection
            .peer_identity()
            .ok_or("No peer identity")?;
            
        let certs = identity
            .downcast_ref::<Vec<Certificate>>()
            .ok_or("Invalid certificate type")?;
            
        let cert = certs
            .first()
            .ok_or("No client certificate")?;
            
        // Cache parsed certificates
        static CERT_CACHE: Lazy<DashMap<Vec<u8>, String>> = Lazy::new(DashMap::new);
        
        if let Some(principal) = CERT_CACHE.get(&cert.0) {
            return Ok(principal.clone());
        }
        
        // Parse certificate
        let parsed = X509Certificate::from_der(&cert.0)?;
        let principal = parsed
            .subject()
            .iter_common_name()
            .next()
            .ok_or("No CN in certificate")?
            .as_str()?
            .to_string();
            
        CERT_CACHE.insert(cert.0.clone(), principal.clone());
        Ok(principal)
    }
}
```

## 5. Performance Monitoring

```rust
// src/auth/metrics.rs

use prometheus::{Histogram, Counter, Gauge, HistogramOpts, register_histogram};
use std::time::Duration;

pub struct AuthMetrics {
    // Latency tracking
    auth_check_latency: Histogram,
    
    // Cache metrics
    l1_hits: Counter,
    l2_hits: Counter,
    negative_hits: Counter,
    cache_misses: Counter,
    cache_size_bytes: Gauge,
    
    // Connection metrics
    active_connections: Gauge,
    connection_churn: Counter,
    mtls_handshake_duration: Histogram,
}

impl AuthMetrics {
    pub fn new() -> Result<Self> {
        Ok(Self {
            auth_check_latency: register_histogram!(HistogramOpts::new(
                "rustmq_auth_check_duration_nanoseconds",
                "Authorization check latency in nanoseconds"
            ).buckets(vec![50.0, 100.0, 200.0, 500.0, 1000.0, 5000.0]))?,
            
            l1_hits: register_counter!("rustmq_acl_l1_hits_total", "L1 cache hits")?,
            l2_hits: register_counter!("rustmq_acl_l2_hits_total", "L2 cache hits")?,
            negative_hits: register_counter!("rustmq_acl_negative_hits_total", "Negative cache hits")?,
            cache_misses: register_counter!("rustmq_acl_cache_misses_total", "Cache misses")?,
            
            cache_size_bytes: register_gauge!(
                "rustmq_acl_cache_size_bytes",
                "Total ACL cache size in bytes"
            )?,
            
            active_connections: register_gauge!(
                "rustmq_active_connections",
                "Number of active authenticated connections"
            )?,
            
            connection_churn: register_counter!(
                "rustmq_connection_churn_total",
                "Total connection establishments and terminations"
            )?,
            
            mtls_handshake_duration: register_histogram!(HistogramOpts::new(
                "rustmq_mtls_handshake_duration_milliseconds",
                "mTLS handshake duration"
            ).buckets(vec![0.5, 1.0, 2.0, 5.0, 10.0, 20.0]))?,
        })
    }
    
    pub fn record_auth_latency(&self, duration: Duration) {
        self.auth_check_latency.observe(duration.as_nanos() as f64);
    }
    
    pub fn record_l1_hit(&self) {
        self.l1_hits.inc();
    }
    
    pub fn record_l2_hit(&self) {
        self.l2_hits.inc();
    }
    
    pub fn record_negative_hit(&self) {
        self.negative_hits.inc();
    }
    
    pub fn record_cache_miss(&self) {
        self.cache_misses.inc();
    }
}
```

## 6. Benchmark Suite

```rust
// benches/auth_performance.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rustmq::auth::*;

fn benchmark_auth_hot_path(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let cache = setup_warmed_cache();
    let connection = create_test_connection("test-principal");
    
    c.bench_function("auth_check_l1_hit", |b| {
        b.iter(|| {
            rt.block_on(async {
                cache.check_permission(
                    &connection,
                    Arc::from("test-topic"),
                    Permission::Write,
                ).await.unwrap()
            })
        })
    });
}

fn benchmark_connection_establishment(c: &mut Criterion) {
    let mut group = c.benchmark_group("connection_establishment");
    
    for scenario in ["cold_start", "with_session_resumption", "with_cert_cache"] {
        group.bench_with_input(
            BenchmarkId::from_parameter(scenario),
            &scenario,
            |b, &scenario| {
                b.iter(|| {
                    establish_connection(scenario)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_cache_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_scalability");
    
    for size in [1000, 10_000, 100_000, 1_000_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |b, &size| {
                let cache = create_cache_with_entries(size);
                b.iter(|| {
                    cache.random_lookup()
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    benchmark_auth_hot_path,
    benchmark_connection_establishment,
    benchmark_cache_scalability
);
criterion_main!(benches);
```

This implementation provides:

1. **Multi-level caching** with L1 (connection-local) and L2 (broker-wide) caches
2. **String interning** for memory efficiency
3. **Sharded data structures** to reduce lock contention
4. **Batch fetching** to reduce Controller load
5. **Negative caching** with Bloom filters
6. **Session resumption** for faster reconnections
7. **Comprehensive metrics** for production monitoring
8. **Benchmark suite** for performance validation

With these optimizations, RustMQ can achieve the target sub-microsecond authorization latency while maintaining scalability.