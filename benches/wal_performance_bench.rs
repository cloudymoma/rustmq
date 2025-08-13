// Comprehensive benchmarks comparing DirectIOWal vs OptimizedDirectIOWal performance
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use rustmq::storage::{DirectIOWal, OptimizedDirectIOWal, WalFactory, AlignedBufferPool, traits::WriteAheadLog};
use rustmq::config::WalConfig;
use rustmq::types::{WalRecord, TopicPartition, Record};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn create_test_record(offset: u64, size: usize) -> WalRecord {
    WalRecord {
        topic_partition: TopicPartition {
            topic: "benchmark-topic".to_string(),
            partition: 0,
        },
        offset,
        record: Record {
            key: Some(format!("key-{}", offset).into_bytes().into()),
            value: vec![0u8; size].into(),
            headers: vec![],
            timestamp: chrono::Utc::now().timestamp_millis(),
        },
        crc32: 0,
    }
}

fn create_wal_config(temp_dir: &TempDir, fsync: bool, segment_size: u64) -> WalConfig {
    WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 100 * 1024 * 1024, // 100MB
        fsync_on_write: fsync,
        segment_size_bytes: segment_size,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    }
}

async fn setup_standard_wal(config: WalConfig) -> DirectIOWal {
    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 100));
    DirectIOWal::new(config, buffer_pool).await.unwrap()
}

async fn setup_optimized_wal(config: WalConfig) -> OptimizedDirectIOWal {
    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 100));
    OptimizedDirectIOWal::new(config, buffer_pool).await.unwrap()
}

fn bench_wal_append_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("wal_append_latency");
    
    // Test different record sizes
    for record_size in [256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(*record_size as u64));
        
        // Benchmark standard WAL
        group.bench_with_input(
            BenchmarkId::new("standard_wal", record_size),
            record_size,
            |b, &size| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 64 * 1024);
                        let wal = setup_standard_wal(config).await;
                        
                        let start = std::time::Instant::now();
                        for i in 0..iters {
                            let record = create_test_record(i, size);
                            black_box(wal.append(record).await.unwrap());
                        }
                        start.elapsed()
                    })
                });
            },
        );
        
        // Benchmark optimized WAL
        group.bench_with_input(
            BenchmarkId::new("optimized_wal", record_size),
            record_size,
            |b, &size| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 64 * 1024);
                        let wal = setup_optimized_wal(config).await;
                        
                        let start = std::time::Instant::now();
                        for i in 0..iters {
                            let record = create_test_record(i, size);
                            black_box(wal.append(record).await.unwrap());
                        }
                        start.elapsed()
                    })
                });
            },
        );
    }
    
    group.finish();
}

fn bench_wal_read_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("wal_read_latency");
    
    // Pre-populate WALs with data for reading
    let num_records = 1000;
    let record_size = 1024;
    
    group.bench_function("standard_wal_read", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let config = create_wal_config(&temp_dir, false, 64 * 1024);
                let wal = setup_standard_wal(config).await;
                
                // Pre-populate with data
                for i in 0..num_records {
                    let record = create_test_record(i, record_size);
                    wal.append(record).await.unwrap();
                }
                wal.sync().await.unwrap();
                
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    let offset = black_box(fastrand::u64(0..num_records));
                    black_box(wal.read(offset, 4096).await.unwrap());
                }
                start.elapsed()
            })
        });
    });
    
    group.bench_function("optimized_wal_read", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let config = create_wal_config(&temp_dir, false, 64 * 1024);
                let wal = setup_optimized_wal(config).await;
                
                // Pre-populate with data
                for i in 0..num_records {
                    let record = create_test_record(i, record_size);
                    wal.append(record).await.unwrap();
                }
                wal.sync().await.unwrap();
                
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    let offset = black_box(fastrand::u64(0..num_records));
                    black_box(wal.read(offset, 4096).await.unwrap());
                }
                start.elapsed()
            })
        });
    });
    
    group.finish();
}

fn bench_wal_sync_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("wal_sync_performance");
    
    // Test sync performance after writing data
    let batch_sizes = [10, 50, 100];
    
    for &batch_size in &batch_sizes {
        group.bench_with_input(
            BenchmarkId::new("standard_wal_sync", batch_size),
            &batch_size,
            |b, &size| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 64 * 1024);
                        let wal = setup_standard_wal(config).await;
                        
                        let start = std::time::Instant::now();
                        for _ in 0..iters {
                            // Write a batch of records
                            for i in 0..size {
                                let record = create_test_record(i, 1024);
                                wal.append(record).await.unwrap();
                            }
                            // Sync to disk
                            black_box(wal.sync().await.unwrap());
                        }
                        start.elapsed()
                    })
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("optimized_wal_sync", batch_size),
            &batch_size,
            |b, &size| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 64 * 1024);
                        let wal = setup_optimized_wal(config).await;
                        
                        let start = std::time::Instant::now();
                        for _ in 0..iters {
                            // Write a batch of records
                            for i in 0..size {
                                let record = create_test_record(i, 1024);
                                wal.append(record).await.unwrap();
                            }
                            // Sync to disk
                            black_box(wal.sync().await.unwrap());
                        }
                        start.elapsed()
                    })
                });
            },
        );
    }
    
    group.finish();
}

fn bench_wal_concurrent_writes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("wal_concurrent_writes");
    
    // Test concurrent write performance
    let concurrency_levels = [1, 4, 8, 16];
    
    for &concurrency in &concurrency_levels {
        group.bench_with_input(
            BenchmarkId::new("standard_wal_concurrent", concurrency),
            &concurrency,
            |b, &conc| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 64 * 1024);
                        let wal = Arc::new(setup_standard_wal(config).await);
                        
                        let start = std::time::Instant::now();
                        
                        let mut tasks = Vec::new();
                        for _ in 0..conc {
                            let wal_clone = wal.clone();
                            let task = tokio::spawn(async move {
                                for i in 0..iters / conc as u64 {
                                    let record = create_test_record(i, 1024);
                                    black_box(wal_clone.append(record).await.unwrap());
                                }
                            });
                            tasks.push(task);
                        }
                        
                        for task in tasks {
                            task.await.unwrap();
                        }
                        
                        start.elapsed()
                    })
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("optimized_wal_concurrent", concurrency),
            &concurrency,
            |b, &conc| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 64 * 1024);
                        let wal = Arc::new(setup_optimized_wal(config).await);
                        
                        let start = std::time::Instant::now();
                        
                        let mut tasks = Vec::new();
                        for _ in 0..conc {
                            let wal_clone = wal.clone();
                            let task = tokio::spawn(async move {
                                for i in 0..iters / conc as u64 {
                                    let record = create_test_record(i, 1024);
                                    black_box(wal_clone.append(record).await.unwrap());
                                }
                            });
                            tasks.push(task);
                        }
                        
                        for task in tasks {
                            task.await.unwrap();
                        }
                        
                        start.elapsed()
                    })
                });
            },
        );
    }
    
    group.finish();
}

fn bench_wal_factory_selection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("wal_factory_selection");
    
    group.bench_function("factory_optimal_creation", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    let temp_dir = TempDir::new().unwrap();
                    let config = create_wal_config(&temp_dir, false, 64 * 1024);
                    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
                    
                    let _wal = black_box(
                        WalFactory::create_optimal_wal(config, buffer_pool).await.unwrap()
                    );
                }
                start.elapsed()
            })
        });
    });
    
    group.bench_function("platform_detection", |b| {
        b.iter(|| {
            black_box(WalFactory::get_platform_capabilities());
        });
    });
    
    group.finish();
}

fn bench_wal_backend_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("wal_backend_comparison");
    
    // Comprehensive comparison of backend performance characteristics
    let test_scenarios = [
        ("small_records", 256, 1000),
        ("medium_records", 4096, 500),
        ("large_records", 16384, 100),
    ];
    
    for (scenario_name, record_size, num_records) in test_scenarios.iter() {
        group.throughput(Throughput::Bytes((record_size * num_records) as u64));
        
        // Standard WAL performance
        group.bench_with_input(
            BenchmarkId::new(format!("standard_{}", scenario_name), ""),
            &(record_size, num_records),
            |b, &(size, count)| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 1024 * 1024);
                        let wal = setup_standard_wal(config).await;
                        
                        let start = std::time::Instant::now();
                        for _ in 0..iters {
                            for i in 0..*count {
                                let record = create_test_record(i as u64, *size);
                                black_box(wal.append(record).await.unwrap());
                            }
                            black_box(wal.sync().await.unwrap());
                        }
                        start.elapsed()
                    })
                });
            },
        );
        
        // Optimized WAL performance
        group.bench_with_input(
            BenchmarkId::new(format!("optimized_{}", scenario_name), ""),
            &(record_size, num_records),
            |b, &(size, count)| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = create_wal_config(&temp_dir, false, 1024 * 1024);
                        let wal = setup_optimized_wal(config).await;
                        
                        let start = std::time::Instant::now();
                        for _ in 0..iters {
                            for i in 0..*count {
                                let record = create_test_record(i as u64, *size);
                                black_box(wal.append(record).await.unwrap());
                            }
                            black_box(wal.sync().await.unwrap());
                        }
                        start.elapsed()
                    })
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    wal_benches,
    bench_wal_append_latency,
    bench_wal_read_latency,
    bench_wal_sync_performance,
    bench_wal_concurrent_writes,
    bench_wal_factory_selection,
    bench_wal_backend_comparison
);

criterion_main!(wal_benches);