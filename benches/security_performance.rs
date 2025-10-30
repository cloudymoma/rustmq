//! Security Performance Benchmarks
//!
//! Validates RustMQ security system performance using actual implemented APIs.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;

use rustmq::security::auth::{AuthorizationManager, AclKey, Permission};
use rustmq::security::{AclConfig, SecurityMetrics};

/// Generate test ACL keys for benchmarking
fn generate_test_keys(count: usize) -> Vec<AclKey> {
    (0..count).map(|i| {
        AclKey::new(
            Arc::from(format!("user_{}", i % 100)),
            Arc::from(format!("topic_{}", i % 50)),
            match i % 3 {
                0 => Permission::Read,
                1 => Permission::Write,
                _ => Permission::Admin,
            }
        )
    }).collect()
}

/// Create test authorization manager
async fn create_auth_manager() -> Arc<AuthorizationManager> {
    let config = AclConfig {
        enabled: true,
        cache_size_mb: 50,
        cache_ttl_seconds: 300,
        l2_shard_count: 32,
        bloom_filter_size: 1_000_000,
        batch_fetch_size: 100,
        enable_audit_logging: false,
        negative_cache_enabled: true,
    };
    
    let metrics = Arc::new(SecurityMetrics::new().unwrap());
    Arc::new(AuthorizationManager::new(config, metrics, None).await.unwrap())
}

/// L1 Cache Performance Benchmarks
/// Target: <10ns per operation
fn l1_cache_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("l1_cache");
    group.measurement_time(Duration::from_secs(5));
    
    let rt = Runtime::new().unwrap();
    let auth_manager = rt.block_on(create_auth_manager());
    let connection_cache = auth_manager.create_connection_cache();
    
    // Test L1 cache hit performance
    group.bench_function("cache_hit", |b| {
        let keys = generate_test_keys(1000);
        
        // Pre-populate cache for hits
        for (i, key) in keys.iter().enumerate() {
            connection_cache.insert(key.clone(), i % 2 == 0);
        }
        
        let mut key_iter = keys.iter().cycle();
        
        b.iter(|| {
            let key = key_iter.next().unwrap();
            let result = connection_cache.get(key);
            black_box(result)
        });
    });
    
    // Test L1 cache miss performance
    group.bench_function("cache_miss", |b| {
        let keys = generate_test_keys(1000);
        let mut key_iter = keys.iter().cycle();
        
        b.iter(|| {
            let key = key_iter.next().unwrap();
            let result = connection_cache.get(key);
            black_box(result)
        });
    });
    
    // Test L1 cache insertion performance
    group.bench_function("cache_insert", |b| {
        let keys = generate_test_keys(1000);
        let mut key_iter = keys.iter().cycle();
        
        b.iter(|| {
            let key = key_iter.next().unwrap();
            connection_cache.insert(key.clone(), true);
        });
    });
    
    group.finish();
}

/// String Interning Performance Benchmarks
/// Target: 60-80% memory reduction
fn string_interning_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_interning");
    group.measurement_time(Duration::from_secs(5));
    
    let rt = Runtime::new().unwrap();
    let auth_manager = rt.block_on(create_auth_manager());
    
    // Test frequently used strings (high reuse)
    group.bench_function("frequent_strings", |b| {
        let common_strings = [
            "admin", "user", "service", "broker", "controller",
            "topic.events", "topic.metrics", "topic.logs",
        ];
        let mut string_iter = common_strings.iter().cycle();
        
        b.iter(|| {
            let string = string_iter.next().unwrap();
            let interned = auth_manager.intern_string(string);
            black_box(interned)
        });
    });
    
    // Test unique strings (low reuse)
    group.bench_function("unique_strings", |b| {
        let mut counter = 0u32;
        
        b.iter(|| {
            counter += 1;
            let string = format!("unique_string_{}", counter);
            let interned = auth_manager.intern_string(&string);
            black_box(interned)
        });
    });
    
    group.finish();
}

/// Authorization Check Performance Benchmarks
/// Target: <100ns total authorization latency
fn authorization_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("authorization");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let auth_manager = rt.block_on(create_auth_manager());
    let connection_cache = auth_manager.create_connection_cache();
    
    // Test single authorization check
    group.bench_function("single_check", |b| {
        let principal: Arc<str> = Arc::from("benchmark_user");
        
        b.to_async(&rt).iter(|| async {
            let result = auth_manager.check_permission(
                &connection_cache,
                &principal,
                "benchmark_topic",
                Permission::Read
            ).await;
            black_box(result)
        });
    });
    
    // Test authorization with different principals
    group.bench_function("different_principals", |b| {
        let principals: Vec<Arc<str>> = (0..100)
            .map(|i| Arc::from(format!("user_{}", i)))
            .collect();
        
        b.to_async(&rt).iter(|| {
            let principals = principals.clone();
            let auth_manager = auth_manager.clone();
            let connection_cache = connection_cache.clone();
            async move {
                let principal_idx = fastrand::usize(0..principals.len());
                let principal = &principals[principal_idx];
                let result = auth_manager.check_permission(
                    &connection_cache,
                    principal,
                    "benchmark_topic",
                    Permission::Read
                ).await;
                black_box(result)
            }
        });
    });
    
    // Test authorization with different topics
    group.bench_function("different_topics", |b| {
        let topics: Vec<String> = (0..50)
            .map(|i| format!("topic_{}", i))
            .collect();
        let principal: Arc<str> = Arc::from("benchmark_user");
        
        b.to_async(&rt).iter(|| {
            let topics = topics.clone();
            let principal = principal.clone();
            let auth_manager = auth_manager.clone();
            let connection_cache = connection_cache.clone();
            async move {
                let topic_idx = fastrand::usize(0..topics.len());
                let topic = &topics[topic_idx];
                let result = auth_manager.check_permission(
                    &connection_cache,
                    &principal,
                    topic,
                    Permission::Read
                ).await;
                black_box(result)
            }
        });
    });
    
    // Test authorization with different permissions
    group.bench_function("different_permissions", |b| {
        let permissions = [Permission::Read, Permission::Write, Permission::Admin];
        let principal: Arc<str> = Arc::from("benchmark_user");
        
        b.to_async(&rt).iter(|| {
            let principal = principal.clone();
            let auth_manager = auth_manager.clone();
            let connection_cache = connection_cache.clone();
            async move {
                let permission_idx = fastrand::usize(0..permissions.len());
                let permission = permissions[permission_idx];
                let result = auth_manager.check_permission(
                    &connection_cache,
                    &principal,
                    "benchmark_topic",
                    permission
                ).await;
                black_box(result)
            }
        });
    });
    
    group.finish();
}

/// Cache Statistics Performance Benchmarks
fn cache_stats_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_stats");
    group.measurement_time(Duration::from_secs(5));
    
    let rt = Runtime::new().unwrap();
    let auth_manager = rt.block_on(create_auth_manager());
    
    // Test cache statistics collection performance
    group.bench_function("get_cache_stats", |b| {
        b.iter(|| {
            let stats = auth_manager.get_cache_stats();
            black_box(stats)
        });
    });
    
    // Test cache statistics after heavy usage
    group.bench_function("stats_after_usage", |b| {
        let connection_cache = auth_manager.create_connection_cache();
        let keys = generate_test_keys(1000);
        
        // Generate cache activity
        for (i, key) in keys.iter().enumerate() {
            connection_cache.insert(key.clone(), i % 2 == 0);
            connection_cache.get(key);
        }
        
        b.iter(|| {
            let stats = auth_manager.get_cache_stats();
            black_box(stats)
        });
    });
    
    group.finish();
}

/// Principal Invalidation Performance Benchmarks
fn invalidation_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("invalidation");
    group.measurement_time(Duration::from_secs(5));
    
    let rt = Runtime::new().unwrap();
    let auth_manager = rt.block_on(create_auth_manager());
    
    // Test single principal invalidation
    group.bench_function("single_principal", |b| {
        let principals = [
            "user1", "user2", "user3", "user4", "user5",
            "admin1", "admin2", "service1", "service2", "broker1"
        ];
        
        b.to_async(&rt).iter(|| {
            let auth_manager = auth_manager.clone();
            async move {
                let principal_idx = fastrand::usize(0..principals.len());
                let principal = principals[principal_idx];
                let result = auth_manager.invalidate_principal(principal).await;
                black_box(result)
            }
        });
    });
    
    group.finish();
}

/// Concurrent Access Performance Benchmarks
fn concurrent_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let auth_manager = rt.block_on(create_auth_manager());
    
    // Test concurrent cache access
    for concurrency in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("cache_access", concurrency),
            concurrency,
            |b, &conc| {
                b.to_async(&rt).iter(|| async {
                    let mut handles = Vec::new();
                    
                    for i in 0..conc {
                        let auth_manager = auth_manager.clone();
                        let connection_cache = auth_manager.create_connection_cache();
                        
                        let handle = tokio::spawn(async move {
                            let key = AclKey::new(
                                Arc::from(format!("concurrent_user_{}", i)),
                                Arc::from("concurrent_topic"),
                                Permission::Read
                            );
                            
                            // Mix of cache operations
                            connection_cache.insert(key.clone(), true);
                            let result = connection_cache.get(&key);
                            black_box(result)
                        });
                        
                        handles.push(handle);
                    }
                    
                    // Wait for all concurrent operations
                    for handle in handles {
                        let result = handle.await;
                        black_box(result);
                    }
                });
            },
        );
    }
    
    group.finish();
}

/// Memory Efficiency Benchmarks
fn memory_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");
    group.measurement_time(Duration::from_secs(5));
    
    let rt = Runtime::new().unwrap();
    let auth_manager = rt.block_on(create_auth_manager());
    
    // Test memory usage with many cache entries
    group.bench_function("large_cache", |b| {
        b.iter(|| {
            let connection_cache = auth_manager.create_connection_cache();
            
            // Add many entries to test memory efficiency
            for i in 0..5000 {
                let key = AclKey::new(
                    Arc::from(format!("memory_user_{}", i)),
                    Arc::from(format!("memory_topic_{}", i % 100)),
                    Permission::Read
                );
                connection_cache.insert(key, i % 2 == 0);
            }
            
            black_box(connection_cache)
        });
    });
    
    // Test string interning memory efficiency
    group.bench_function("string_interning_memory", |b| {
        let common_strings = [
            "admin", "user", "service", "broker", "controller",
            "topic.events", "topic.metrics", "topic.logs",
            "read", "write", "admin", "subscribe", "publish"
        ];
        
        b.iter(|| {
            let mut interned_strings = Vec::new();
            
            // Intern the same strings many times to test memory sharing
            for _ in 0..1000 {
                for string in &common_strings {
                    interned_strings.push(auth_manager.intern_string(string));
                }
            }
            
            black_box(interned_strings)
        });
    });
    
    group.finish();
}

criterion_group!(
    security_benches,
    l1_cache_benchmarks,
    string_interning_benchmarks,
    authorization_benchmarks,
    cache_stats_benchmarks,
    invalidation_benchmarks,
    concurrent_benchmarks,
    memory_benchmarks
);

criterion_main!(security_benches);