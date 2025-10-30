//! Authorization-specific performance benchmarks
//!
//! This module focuses on detailed authorization performance testing,
//! including cache hierarchy, permission evaluation, and ACL rule processing.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;
use rustmq::security::auth::{AuthorizationManager, AclKey, Permission};
use rustmq::security::{AclConfig, SecurityMetrics};

/// Generate ACL keys for benchmarking
fn generate_acl_key_batch(count: usize) -> Vec<AclKey> {
    let mut keys = Vec::with_capacity(count);
    for i in 0..count {
        let principal = Arc::from(format!("user_{}", i % 100));
        let topic = Arc::from(format!("topic_{}", i % 50));
        let permission = match i % 3 {
            0 => Permission::Read,
            1 => Permission::Write,
            _ => Permission::Admin,
        };
        keys.push(AclKey::new(principal, topic, permission));
    }
    keys
}

/// Setup authorization manager for benchmarks
async fn setup_authorization_manager() -> Arc<AuthorizationManager> {
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

/// Basic authorization performance tests
fn authorization_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("authorization");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(setup_authorization_manager());
    let connection_cache = auth_mgr.create_connection_cache();
    
    // Simple permission check using the actual API
    group.bench_function("simple_permission", |b| {
        let principal = Arc::from("user1");
        
        b.to_async(&rt).iter(|| async {
            let result = auth_mgr.check_permission(
                &connection_cache,
                &principal,
                "topic:test",
                Permission::Read
            ).await;
            black_box(result)
        });
    });
    
    // String interning performance
    group.bench_function("string_interning", |b| {
        let strings = vec![
            "user_frequent",
            "topic_common", 
            "admin_user",
            "service_account",
        ];
        
        let mut string_iter = strings.iter().cycle();
        
        b.iter(|| {
            let s = string_iter.next().unwrap();
            let interned = auth_mgr.intern_string(s);
            black_box(interned)
        });
    });
    
    // Cache statistics tracking
    group.bench_function("cache_stats", |b| {
        b.iter(|| {
            let stats = auth_mgr.get_cache_stats();
            black_box(stats)
        });
    });
    
    // More string interning patterns
    group.bench_function("varied_string_patterns", |b| {
        let patterns = vec![
            "user_{}", "topic_{}", "admin_{}", "service_{}",
            "role_{}", "group_{}", "tenant_{}", "app_{}",
        ];
        let mut counter = 0;
        
        b.iter(|| {
            for pattern in &patterns {
                let s = pattern.replace("{}", &counter.to_string());
                let interned = auth_mgr.intern_string(&s);
                black_box(interned);
            }
            counter += 1;
        });
    });
    
    // Permission invalidation
    group.bench_function("principal_invalidation", |b| {
        let principals = ["user1", "user2", "admin", "service"];
        
        b.to_async(&rt).iter(|| async {
            let principal = &principals[rand::random::<usize>() % principals.len()];
            let result = auth_mgr.invalidate_principal(principal).await;
            black_box(result)
        });
    });
    
    group.finish();
}

/// Connection cache performance tests
fn connection_cache_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("connection_cache");
    group.measurement_time(Duration::from_secs(5));
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(setup_authorization_manager());
    
    // Cache creation performance
    group.bench_function("cache_creation", |b| {
        b.iter(|| {
            let cache = auth_mgr.create_connection_cache();
            black_box(cache)
        });
    });
    
    // Cache operations
    for cache_size in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("cache_operations", cache_size),
            cache_size,
            |b, &size| {
                let cache = auth_mgr.create_connection_cache();
                let keys = generate_acl_key_batch(size);
                
                // Pre-populate cache
                for (i, key) in keys.iter().enumerate() {
                    cache.insert(key.clone(), i % 2 == 0);
                }
                
                let mut key_iter = keys.iter().cycle();
                
                b.iter(|| {
                    let key = key_iter.next().unwrap();
                    let result = cache.get(key);
                    black_box(result)
                });
            },
        );
    }
    
    group.finish();
}

// Configure benchmark groups
criterion_group! {
    name = authorization_performance;
    config = Criterion::default()
        .significance_level(0.05)
        .noise_threshold(0.03);
    targets = 
        authorization_benchmarks,
        connection_cache_benchmarks
}

criterion_main!(authorization_performance);