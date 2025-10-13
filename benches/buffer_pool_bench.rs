// Buffer Pool Performance Benchmarks
// Validates the expected 30-40% allocation overhead reduction from buffer pooling

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rustmq::storage::{AlignedBufferPool, BufferPool};
use std::sync::Arc;
use tokio::runtime::Runtime;

// Benchmark 1: Network Layer - 8KB Buffer Allocation (Baseline)
fn bench_network_alloc_baseline(c: &mut Criterion) {
    c.bench_function("network_alloc_baseline", |b| {
        b.iter(|| {
            let buffer = vec![0u8; 8192];
            black_box(buffer);
        });
    });
}

// Benchmark 2: Network Layer - 8KB Buffer from Pool
fn bench_network_buffer_pool(c: &mut Criterion) {
    let pool = Arc::new(AlignedBufferPool::new(4096, 100));

    c.bench_function("network_buffer_pool", |b| {
        b.iter(|| {
            let buffer = pool.get_aligned_buffer(8192).unwrap();
            black_box(&buffer);
            pool.return_buffer(buffer);
        });
    });
}

// Benchmark 3: Concurrent Network Buffer Pool Access
fn bench_concurrent_network_pool(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = Arc::new(AlignedBufferPool::new(4096, 200));

    let mut group = c.benchmark_group("concurrent_network_pool");

    for num_tasks in [10, 50, 100, 500].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(num_tasks), num_tasks, |b, &num_tasks| {
            b.iter(|| {
                rt.block_on(async {
                    let mut handles = vec![];
                    for _ in 0..num_tasks {
                        let pool_clone = pool.clone();
                        handles.push(tokio::spawn(async move {
                            let buffer = pool_clone.get_aligned_buffer(8192).unwrap();
                            tokio::time::sleep(tokio::time::Duration::from_micros(1)).await;
                            pool_clone.return_buffer(buffer);
                        }));
                    }
                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            });
        });
    }
    group.finish();
}

// Benchmark 4: WAL I/O - Variable Size Buffer Allocation (Baseline)
fn bench_wal_alloc_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_alloc_baseline");

    for size in [1024, 4096, 16384, 65536].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let buffer = vec![0u8; size];
                black_box(buffer);
            });
        });
    }
    group.finish();
}

// Benchmark 5: WAL I/O - Variable Size Buffer from Pool
fn bench_wal_buffer_pool(c: &mut Criterion) {
    let pool = Arc::new(AlignedBufferPool::new(4096, 100));
    let mut group = c.benchmark_group("wal_buffer_pool");

    for size in [1024, 4096, 16384, 65536].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let buffer = pool.get_aligned_buffer(size).unwrap();
                black_box(&buffer);
                pool.return_buffer(buffer);
            });
        });
    }
    group.finish();
}

// Benchmark 6: Object Storage Range Read - Buffer Allocation (Baseline)
fn bench_object_storage_alloc_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("object_storage_alloc_baseline");

    // Common range sizes: 1KB, 4KB, 16KB, 64KB, 256KB
    for size in [1024, 4096, 16384, 65536, 262144].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let buffer = vec![0u8; size];
                black_box(buffer);
            });
        });
    }
    group.finish();
}

// Benchmark 7: Object Storage Range Read - Buffer from Pool
fn bench_object_storage_buffer_pool(c: &mut Criterion) {
    let pool = Arc::new(AlignedBufferPool::new(4096, 100));
    let mut group = c.benchmark_group("object_storage_buffer_pool");

    for size in [1024, 4096, 16384, 65536, 262144].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let buffer = pool.get_aligned_buffer(size).unwrap();
                black_box(&buffer);
                pool.return_buffer(buffer);
            });
        });
    }
    group.finish();
}

// Benchmark 8: Buffer Pool Exhaustion Scenario
fn bench_buffer_pool_exhaustion(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("buffer_pool_exhaustion", |b| {
        b.iter(|| {
            // Small pool size to test exhaustion behavior
            let pool = Arc::new(AlignedBufferPool::new(4096, 10));

            rt.block_on(async {
                let mut handles = vec![];
                // Request more buffers than pool capacity
                for _ in 0..100 {
                    let pool_clone = pool.clone();
                    handles.push(tokio::spawn(async move {
                        let buffer = pool_clone.get_aligned_buffer(8192).unwrap();
                        tokio::time::sleep(tokio::time::Duration::from_micros(1)).await;
                        pool_clone.return_buffer(buffer);
                    }));
                }
                for handle in handles {
                    handle.await.unwrap();
                }
            });
        });
    });
}

// Benchmark 9: Allocation Overhead Comparison - Total Time
fn bench_allocation_overhead_comparison(c: &mut Criterion) {
    let pool = Arc::new(AlignedBufferPool::new(4096, 100));
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("allocation_overhead_comparison");

    // Baseline: Vec allocation for 1000 requests
    group.bench_function("vec_1000_requests", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let buffer = vec![0u8; 8192];
                black_box(buffer);
            }
        });
    });

    // Buffer pool: Same 1000 requests
    group.bench_function("pool_1000_requests", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let buffer = pool.get_aligned_buffer(8192).unwrap();
                black_box(&buffer);
                pool.return_buffer(buffer);
            }
        });
    });

    group.finish();
}

// Benchmark 10: Memory Reuse Pattern - Latency Impact
fn bench_memory_reuse_latency(c: &mut Criterion) {
    let pool = Arc::new(AlignedBufferPool::new(4096, 100));
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_reuse_latency");

    // Simulate realistic request pattern: get buffer, process, return
    group.bench_function("baseline_with_processing", |b| {
        b.iter(|| {
            let buffer = vec![0u8; 8192];
            // Simulate 10μs processing time
            std::thread::sleep(std::time::Duration::from_micros(10));
            black_box(buffer);
        });
    });

    group.bench_function("pool_with_processing", |b| {
        b.iter(|| {
            let buffer = pool.get_aligned_buffer(8192).unwrap();
            // Simulate 10μs processing time
            std::thread::sleep(std::time::Duration::from_micros(10));
            black_box(&buffer);
            pool.return_buffer(buffer);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_network_alloc_baseline,
    bench_network_buffer_pool,
    bench_concurrent_network_pool,
    bench_wal_alloc_baseline,
    bench_wal_buffer_pool,
    bench_object_storage_alloc_baseline,
    bench_object_storage_buffer_pool,
    bench_buffer_pool_exhaustion,
    bench_allocation_overhead_comparison,
    bench_memory_reuse_latency
);

criterion_main!(benches);
