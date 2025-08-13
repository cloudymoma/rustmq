use criterion::{criterion_group, criterion_main, Criterion};
use rustmq::storage::{cache::CacheManager, traits::Cache};
use rustmq::config::CacheConfig;
#[cfg(feature = "moka-cache")]
use rustmq::config::EvictionPolicy;
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

// Test creation without runtime (should work now)
fn bench_cache_creation_no_runtime(c: &mut Criterion) {
    c.bench_function("cache_creation_no_runtime", |b| {
        let config = create_cache_config(1024);
        b.iter(|| {
            // This should work without tokio runtime now
            CacheManager::new_without_maintenance(&config)
        });
    });
}

// Test creation with runtime (original method)
fn bench_cache_creation_with_runtime(c: &mut Criterion) {
    c.bench_function("cache_creation_with_runtime", |b| {
        let rt = Runtime::new().unwrap();
        let _guard = rt.enter();
        let config = create_cache_config(1024);
        b.iter(|| {
            CacheManager::new(&config)
        });
    });
}

criterion_group!(benches, bench_cache_creation_no_runtime, bench_cache_creation_with_runtime);
criterion_main!(benches);