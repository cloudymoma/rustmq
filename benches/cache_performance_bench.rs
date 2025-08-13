use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rustmq::storage::{cache::CacheManager, traits::Cache};
use rustmq::config::CacheConfig;
#[cfg(feature = "moka-cache")]
use rustmq::config::EvictionPolicy;
use bytes::Bytes;
use std::sync::Arc;
use tokio::runtime::Runtime;

// Helper function to create a cache config
fn create_cache_config(size_bytes: u64) -> CacheConfig {
    CacheConfig {
        write_cache_size_bytes: size_bytes,
        read_cache_size_bytes: size_bytes,
        #[cfg(feature = "moka-cache")]
        eviction_policy: EvictionPolicy::Moka,
        #[cfg(not(feature = "moka-cache"))]
        eviction_policy: rustmq::config::EvictionPolicy::Lru,
    }
}

// Benchmark cache manager creation
fn bench_cache_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_creation");
    
    for &size in &[1024, 10_240, 102_400, 1_048_576] {
        // Test creation without maintenance tasks (for benchmarks)
        group.bench_with_input(
            BenchmarkId::new("cache_manager_new_no_maintenance", size),
            &size,
            |b, &size| {
                let config = create_cache_config(size);
                b.iter(|| {
                    CacheManager::new_without_maintenance(&config)
                });
            },
        );
        
        // Test creation with maintenance tasks (requires runtime)
        group.bench_with_input(
            BenchmarkId::new("cache_manager_new_with_maintenance", size),
            &size,
            |b, &size| {
                let rt = Runtime::new().unwrap();
                let _guard = rt.enter();  // Ensure runtime is active
                let config = create_cache_config(size);
                b.iter(|| {
                    CacheManager::new(&config)
                });
            },
        );
    }
    group.finish();
}

// Benchmark single-threaded cache operations
fn bench_cache_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = create_cache_config(1_048_576); // 1MB cache
    let cache_manager = Arc::new(CacheManager::new_without_maintenance(&config));
    
    let mut group = c.benchmark_group("cache_operations");
    group.throughput(Throughput::Elements(1));
    
    // Benchmark cache writes
    group.bench_function("cache_write", |b| {
        let cache = cache_manager.clone();
        b.to_async(&rt).iter(|| async {
            let key = format!("key_{}", fastrand::u64(..));
            let value = Bytes::from(vec![0u8; 256]); // 256 byte value
            cache.cache_write(&key, value).await.unwrap();
        });
    });
    
    // Benchmark cache reads (setup data first)
    rt.block_on(async {
        for i in 0..1000 {
            let key = format!("read_key_{}", i);
            let value = Bytes::from(vec![i as u8; 256]);
            cache_manager.cache_write(&key, value).await.unwrap();
        }
    });
    
    group.bench_function("cache_read_hit", |b| {
        let cache = cache_manager.clone();
        b.to_async(&rt).iter(|| async {
            let key = format!("read_key_{}", fastrand::usize(..1000));
            cache.serve_read(&key).await.unwrap();
        });
    });
    
    group.bench_function("cache_read_miss", |b| {
        let cache = cache_manager.clone();
        b.to_async(&rt).iter(|| async {
            let key = format!("miss_key_{}", fastrand::u64(..));
            cache.serve_read(&key).await.unwrap();
        });
    });
    
    group.finish();
}

// Benchmark concurrent cache operations
fn bench_concurrent_cache(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = create_cache_config(10_485_760); // 10MB cache
    let cache_manager = Arc::new(CacheManager::new_without_maintenance(&config));
    
    let mut group = c.benchmark_group("concurrent_cache");
    
    // Setup test data
    rt.block_on(async {
        for i in 0..10000 {
            let key = format!("concurrent_key_{}", i);
            let value = Bytes::from(vec![(i % 256) as u8; 128]);
            cache_manager.cache_write(&key, value).await.unwrap();
        }
    });
    
    for &concurrency in &[1, 2, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("concurrent_reads", concurrency),
            &concurrency,
            |b, &concurrency| {
                let cache = cache_manager.clone();
                b.to_async(&rt).iter(|| async {
                    let mut handles = Vec::with_capacity(concurrency);
                    
                    for _ in 0..concurrency {
                        let cache_clone = cache.clone();
                        let handle = tokio::spawn(async move {
                            for _ in 0..100 {
                                let key = format!("concurrent_key_{}", fastrand::usize(..10000));
                                cache_clone.serve_read(&key).await.unwrap();
                            }
                        });
                        handles.push(handle);
                    }
                    
                    // Wait for all tasks to complete
                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            },
        );
    }
    
    for &concurrency in &[1, 2, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("concurrent_mixed", concurrency),
            &concurrency,
            |b, &concurrency| {
                let cache = cache_manager.clone();
                b.to_async(&rt).iter(|| async {
                    let mut handles = Vec::with_capacity(concurrency);
                    
                    for thread_id in 0..concurrency {
                        let cache_clone = cache.clone();
                        let handle = tokio::spawn(async move {
                            for i in 0..50 {
                                // Mix of reads and writes
                                if i % 3 == 0 {
                                    // Write
                                    let key = format!("mixed_key_{}_{}", thread_id, i);
                                    let value = Bytes::from(vec![(i % 256) as u8; 128]);
                                    cache_clone.cache_write(&key, value).await.unwrap();
                                } else {
                                    // Read
                                    let key = format!("concurrent_key_{}", fastrand::usize(..10000));
                                    cache_clone.serve_read(&key).await.unwrap();
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    // Wait for all tasks to complete
                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            },
        );
    }
    
    group.finish();
}

// Benchmark cache under memory pressure (eviction scenarios)
fn bench_cache_eviction(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("cache_eviction");
    
    // Small cache to trigger frequent evictions
    let config = create_cache_config(32_768); // 32KB cache
    let cache_manager = Arc::new(CacheManager::new_without_maintenance(&config));
    
    group.bench_function("eviction_pressure", |b| {
        let cache = cache_manager.clone();
        b.to_async(&rt).iter(|| async {
            // Write entries larger than cache capacity to trigger evictions
            for i in 0..100 {
                let key = format!("eviction_key_{}", i);
                let value = Bytes::from(vec![(i % 256) as u8; 512]); // 512 byte values
                cache.cache_write(&key, value).await.unwrap();
            }
        });
    });
    
    group.finish();
}

// Benchmark cache with different value sizes
fn bench_cache_value_sizes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = create_cache_config(10_485_760); // 10MB cache
    let cache_manager = Arc::new(CacheManager::new_without_maintenance(&config));
    
    let mut group = c.benchmark_group("cache_value_sizes");
    
    for &size in &[64, 256, 1024, 4096, 16384] {
        group.bench_with_input(
            BenchmarkId::new("write_value_size", size),
            &size,
            |b, &size| {
                let cache = cache_manager.clone();
                b.to_async(&rt).iter(|| async {
                    let key = format!("size_key_{}", fastrand::u64(..));
                    let value = Bytes::from(vec![0u8; size]);
                    cache.cache_write(&key, value).await.unwrap();
                });
            },
        );
    }
    
    // Setup data for read tests
    rt.block_on(async {
        for &size in &[64, 256, 1024, 4096, 16384] {
            for i in 0..100 {
                let key = format!("read_size_{}_{}", size, i);
                let value = Bytes::from(vec![(i % 256) as u8; size]);
                cache_manager.cache_write(&key, value).await.unwrap();
            }
        }
    });
    
    for &size in &[64, 256, 1024, 4096, 16384] {
        group.bench_with_input(
            BenchmarkId::new("read_value_size", size),
            &size,
            |b, &size| {
                let cache = cache_manager.clone();
                b.to_async(&rt).iter(|| async {
                    let key = format!("read_size_{}_{}", size, fastrand::usize(..100));
                    cache.serve_read(&key).await.unwrap();
                });
            },
        );
    }
    
    group.finish();
}

// Benchmark comparing cache with and without maintenance overhead
fn bench_cache_performance_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = create_cache_config(1_048_576); // 1MB cache
    
    let cache_no_maintenance = Arc::new(CacheManager::new_without_maintenance(&config));
    let cache_with_maintenance = Arc::new({
        let _guard = rt.enter();
        CacheManager::new(&config)
    });
    
    let mut group = c.benchmark_group("cache_performance_comparison");
    group.throughput(Throughput::Elements(100));
    
    // Benchmark without maintenance overhead
    group.bench_function("no_maintenance_overhead", |b| {
        let cache = cache_no_maintenance.clone();
        b.to_async(&rt).iter(|| async {
            for i in 0..100 {
                let key = format!("perf_key_{}", i);
                let value = Bytes::from(vec![i as u8; 256]);
                cache.cache_write(&key, value).await.unwrap();
                cache.serve_read(&key).await.unwrap();
            }
        });
    });
    
    // Benchmark with maintenance overhead
    group.bench_function("with_maintenance_overhead", |b| {
        let cache = cache_with_maintenance.clone();
        b.to_async(&rt).iter(|| async {
            for i in 0..100 {
                let key = format!("perf_key_{}", i);
                let value = Bytes::from(vec![i as u8; 256]);
                cache.cache_write(&key, value).await.unwrap();
                cache.serve_read(&key).await.unwrap();
            }
        });
    });
    
    // Benchmark manual maintenance triggering
    #[cfg(feature = "moka-cache")]
    group.bench_function("manual_maintenance", |b| {
        let cache = cache_no_maintenance.clone();
        b.to_async(&rt).iter(|| async {
            for i in 0..50 {
                let key = format!("maint_key_{}", i);
                let value = Bytes::from(vec![i as u8; 256]);
                cache.cache_write(&key, value).await.unwrap();
            }
            // Manually trigger maintenance
            cache.run_maintenance().await;
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_cache_creation,
    bench_cache_operations,
    bench_concurrent_cache,
    bench_cache_eviction,
    bench_cache_value_sizes,
    bench_cache_performance_comparison
);

criterion_main!(benches);