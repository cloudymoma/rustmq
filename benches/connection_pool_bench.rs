use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rustmq::network::ConnectionPool;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Benchmark concurrent reads from the connection pool
fn bench_concurrent_reads(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("connection_pool_reads");

    for num_threads in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}threads", num_threads)),
            num_threads,
            |b, &num_threads| {
                let pool = Arc::new(ConnectionPool::new(10000));

                b.to_async(&rt).iter(|| async {
                    let pool = pool.clone();
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let pool = pool.clone();
                            tokio::spawn(async move {
                                for j in 0..100 {
                                    let client_id = format!("client-{}-{}", i, j);
                                    black_box(pool.get_connection(&client_id).await);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

/// Benchmark pool with mixed read/write operations
fn bench_mixed_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("connection_pool_mixed");

    for num_threads in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}threads", num_threads)),
            num_threads,
            |b, &num_threads| {
                let pool = Arc::new(ConnectionPool::new(1000));

                b.to_async(&rt).iter(|| async {
                    let pool = pool.clone();
                    let handles: Vec<_> = (0..num_threads)
                        .map(|i| {
                            let pool = pool.clone();
                            tokio::spawn(async move {
                                for j in 0..50 {
                                    let client_id = format!("client-{}-{}", i, j);
                                    // Mix of reads
                                    black_box(pool.get_connection(&client_id).await);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

/// Benchmark pool statistics collection under load
fn bench_pool_stats(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = Arc::new(ConnectionPool::new(10000));

    c.bench_function("pool_stats", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = pool.clone();
            black_box(pool.len());
            black_box(pool.is_empty());
        });
    });
}

/// Benchmark lock-free read performance
fn bench_dashmap_read_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = Arc::new(ConnectionPool::new(10000));

    c.bench_function("dashmap_concurrent_reads", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = pool.clone();
            let handles: Vec<_> = (0..16)
                .map(|i| {
                    let pool = pool.clone();
                    tokio::spawn(async move {
                        for j in 0..1000 {
                            let client_id = format!("client-{}-{}", i, j);
                            black_box(pool.get_connection(&client_id).await);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.await.unwrap();
            }
        });
    });
}

criterion_group!(
    benches,
    bench_concurrent_reads,
    bench_mixed_operations,
    bench_pool_stats,
    bench_dashmap_read_performance,
);
criterion_main!(benches);
