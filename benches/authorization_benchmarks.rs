//! Authorization-specific performance benchmarks
//!
//! This module focuses on detailed authorization performance testing,
//! including cache hierarchy, permission evaluation, and ACL rule processing.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;

mod utils;
use utils::{data_generators::*, performance_helpers::*, setup_authorization_manager};

/// Detailed L1 cache performance analysis
fn l1_cache_detailed_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("l1_cache_detailed");
    group.measurement_time(Duration::from_secs(10));
    
    use rustmq::security::auth::ConnectionAclCache;
    
    // Cache sizes impact on performance
    for cache_size in [100, 500, 1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("cache_size", cache_size),
            cache_size,
            |b, &size| {
                let cache = ConnectionAclCache::new(size);
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
    
    // LRU eviction impact
    group.bench_function("lru_eviction_overhead", |b| {
        let cache = ConnectionAclCache::new(100);
        let keys = generate_acl_key_batch(200); // More than cache size
        
        let mut key_iter = keys.iter().cycle();
        
        b.iter(|| {
            let key = key_iter.next().unwrap();
            cache.insert(key.clone(), true);
            let result = cache.get(key);
            black_box(result)
        });
    });
    
    // Hit rate analysis
    group.bench_function("hit_rate_analysis", |b| {
        let cache = ConnectionAclCache::new(1000);
        let hot_keys = generate_acl_key_batch(100); // Hot set
        let cold_keys = generate_acl_key_batch(900); // Cold set
        
        // Pre-populate with hot keys
        for key in hot_keys.iter() {
            cache.insert(key.clone(), true);
        }
        
        let mut rng = rand::thread_rng();
        
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            
            for _ in 0..iters {
                // 90% hot keys, 10% cold keys (realistic workload)
                let key = if rand::Rng::gen_bool(&mut rng, 0.9) {
                    &hot_keys[rng.gen_range(0..hot_keys.len())]
                } else {
                    &cold_keys[rng.gen_range(0..cold_keys.len())]
                };
                
                let _ = cache.get(key);
            }
            
            start.elapsed()
        });
    });
    
    group.finish();
}

/// Detailed L2 cache performance analysis
fn l2_cache_detailed_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("l2_cache_detailed");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(async {
        setup_authorization_manager().await.0
    });
    
    // Shard distribution analysis
    for shard_count in [8, 16, 32, 64].iter() {
        group.bench_with_input(
            BenchmarkId::new("shard_count", shard_count),
            shard_count,
            |b, &shards| {
                let keys = generate_acl_key_batch(1000);
                
                // Pre-populate L2 cache
                rt.block_on(async {
                    for key in keys.iter() {
                        auth_mgr.populate_l2_cache_with_shards(key.clone(), true, shards).await;
                    }
                });
                
                let mut key_iter = keys.iter().cycle();
                
                b.to_async(&rt).iter(|| async {
                    let key = key_iter.next().unwrap();
                    let result = auth_mgr.check_l2_cache(key).await;
                    black_box(result)
                });
            },
        );
    }
    
    // TTL expiration impact
    group.bench_function("ttl_expiration_overhead", |b| {
        let keys = generate_acl_key_batch(100);
        
        b.to_async(&rt).iter(|| async {
            // Add with short TTL
            for key in keys.iter() {
                auth_mgr.populate_l2_cache_with_ttl(
                    key.clone(),
                    true,
                    Duration::from_millis(10)
                ).await;
            }
            
            // Wait for expiration
            tokio::time::sleep(Duration::from_millis(15)).await;
            
            // Access expired entries
            for key in keys.iter() {
                let _ = auth_mgr.check_l2_cache(key).await;
            }
        });
    });
    
    group.finish();
}

/// Permission evaluation performance
fn permission_evaluation_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("permission_evaluation");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(async {
        setup_authorization_manager().await.0
    });
    
    // Simple permission check
    group.bench_function("simple_permission", |b| {
        let principal = Principal {
            name: Arc::from("user1"),
            principal_type: PrincipalType::User,
            groups: vec![],
            attributes: Default::default(),
        };
        
        b.to_async(&rt).iter(|| async {
            let result = auth_mgr.check_permission(
                &principal,
                "topic:test",
                Permission::Read
            ).await;
            black_box(result)
        });
    });
    
    // Group-based permission check
    group.bench_function("group_permission", |b| {
        let principal = Principal {
            name: Arc::from("user2"),
            principal_type: PrincipalType::User,
            groups: vec![Arc::from("admin"), Arc::from("operators")],
            attributes: Default::default(),
        };
        
        b.to_async(&rt).iter(|| async {
            let result = auth_mgr.check_permission(
                &principal,
                "topic:admin-only",
                Permission::Admin
            ).await;
            black_box(result)
        });
    });
    
    // Wildcard pattern matching
    group.bench_function("wildcard_matching", |b| {
        let patterns = vec![
            "topic:*",
            "topic:logs.*",
            "topic:metrics.*.cpu",
            "topic:events.{app1,app2}.*",
        ];
        
        let resources = vec![
            "topic:test",
            "topic:logs.app1",
            "topic:metrics.server1.cpu",
            "topic:events.app1.errors",
        ];
        
        b.iter(|| {
            for pattern in patterns.iter() {
                for resource in resources.iter() {
                    let matches = auth_mgr.match_pattern(pattern, resource);
                    black_box(matches);
                }
            }
        });
    });
    
    // Permission inheritance
    group.bench_function("permission_inheritance", |b| {
        let principal = Principal {
            name: Arc::from("service1"),
            principal_type: PrincipalType::Service,
            groups: vec![Arc::from("services"), Arc::from("monitoring")],
            attributes: Default::default(),
        };
        
        b.to_async(&rt).iter(|| async {
            let result = auth_mgr.check_inherited_permission(
                &principal,
                "topic:metrics",
                Permission::Read
            ).await;
            black_box(result)
        });
    });
    
    group.finish();
}

/// ACL rule processing performance
fn acl_rule_processing_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("acl_rule_processing");
    group.measurement_time(Duration::from_secs(15));
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(async {
        setup_authorization_manager().await.0
    });
    
    // Rule evaluation with different complexities
    for rule_count in [10, 100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*rule_count as u64));
        group.bench_with_input(
            BenchmarkId::new("rule_count", rule_count),
            rule_count,
            |b, &count| {
                let rules = generate_acl_rules(count);
                
                b.to_async(&rt).iter(|| async {
                    let result = auth_mgr.evaluate_rules(&rules).await;
                    black_box(result)
                });
            },
        );
    }
    
    // Deny rules priority
    group.bench_function("deny_rules_priority", |b| {
        let mut rules = generate_acl_rules(1000);
        // Add some deny rules
        for i in 0..100 {
            rules[i].permission_type = "DENY".to_string();
        }
        
        b.to_async(&rt).iter(|| async {
            let result = auth_mgr.evaluate_rules_with_priority(&rules).await;
            black_box(result)
        });
    });
    
    // Rule compilation and caching
    group.bench_function("rule_compilation", |b| {
        let rules = generate_acl_rules(100);
        
        b.to_async(&rt).iter(|| async {
            let compiled = auth_mgr.compile_rules(&rules).await;
            black_box(compiled)
        });
    });
    
    group.finish();
}

/// Batch authorization operations
fn batch_authorization_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_authorization");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(async {
        setup_authorization_manager().await.0
    });
    
    // Batch size impact
    for batch_size in [10, 50, 100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            batch_size,
            |b, &size| {
                let keys = generate_acl_key_batch(size);
                
                b.to_async(&rt).iter(|| async {
                    let results = auth_mgr.authorize_batch(&keys).await;
                    black_box(results)
                });
            },
        );
    }
    
    // Batch fetch optimization
    group.bench_function("batch_fetch_optimization", |b| {
        let keys = generate_acl_key_batch(100);
        
        b.to_async(&rt).iter(|| async {
            // Clear caches to force fetch
            auth_mgr.clear_all_caches().await;
            
            let results = auth_mgr.authorize_batch_with_fetch(&keys).await;
            black_box(results)
        });
    });
    
    group.finish();
}

/// String interning effectiveness
fn string_interning_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_interning");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(async {
        setup_authorization_manager().await.0
    });
    
    // Interning with different reuse patterns
    for reuse_factor in [10, 50, 90].iter() {
        group.bench_with_input(
            BenchmarkId::new("reuse_factor_percent", reuse_factor),
            reuse_factor,
            |b, &reuse| {
                let unique_count = 100;
                let total_count = 10000;
                
                b.to_async(&rt).iter(|| async {
                    let memory = auth_mgr.measure_interning_efficiency(
                        unique_count,
                        total_count,
                        reuse
                    ).await;
                    black_box(memory)
                });
            },
        );
    }
    
    // Interning lookup performance
    group.bench_function("interning_lookup", |b| {
        // Pre-populate intern pool
        rt.block_on(async {
            for i in 0..1000 {
                auth_mgr.intern_string(&format!("principal_{}", i)).await;
            }
        });
        
        b.to_async(&rt).iter(|| async {
            let s = format!("principal_{}", rand::random::<u32>() % 1000);
            let interned = auth_mgr.intern_string(&s).await;
            black_box(interned)
        });
    });
    
    group.finish();
}

// Configure benchmark groups
criterion_group! {
    name = authorization_benchmarks;
    config = Criterion::default()
        .significance_level(0.05)
        .noise_threshold(0.03);
    targets = 
        l1_cache_detailed_benchmarks,
        l2_cache_detailed_benchmarks,
        permission_evaluation_benchmarks,
        acl_rule_processing_benchmarks,
        batch_authorization_benchmarks,
        string_interning_benchmarks
}

criterion_main!(authorization_benchmarks);