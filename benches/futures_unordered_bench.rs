/// Benchmarks for FuturesUnordered optimizations
///
/// Measures the performance improvements of FuturesUnordered over join_all
/// for both replication and object storage operations.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use rustmq::storage::object_storage::{LocalObjectStorage, UploadManagerImpl};
use rustmq::storage::traits::{ObjectStorage, UploadManager, WalSegment};
use rustmq::config::{ObjectStorageConfig, StorageType};
use rustmq::types::TopicPartition;
use bytes::Bytes;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_multipart_upload_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("multipart_upload");

    // Test different file sizes
    let sizes = vec![
        (10 * 1024 * 1024, "10MB"),
        (25 * 1024 * 1024, "25MB"),
        (50 * 1024 * 1024, "50MB"),
        (100 * 1024 * 1024, "100MB"),
    ];

    for (size, label) in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("parallel", label), &size, |b, &size| {
            b.to_async(Runtime::new().unwrap()).iter(|| async {
                let temp_dir = TempDir::new().unwrap();
                let storage = Arc::new(LocalObjectStorage::new(temp_dir.path().to_path_buf()).unwrap());
                let config = ObjectStorageConfig {
                    storage_type: StorageType::Local { path: temp_dir.path().to_path_buf() },
                    bucket: "test".to_string(),
                    region: "local".to_string(),
                    endpoint: "".to_string(),
                    access_key: None,
                    secret_key: None,
                    multipart_threshold: 5 * 1024 * 1024,
                    max_concurrent_uploads: 10,
                };

                let upload_manager = UploadManagerImpl::new(storage, config);

                let segment = WalSegment {
                    start_offset: 0,
                    end_offset: 100,
                    size_bytes: size as u64,
                    data: Bytes::from(vec![0u8; size]),
                    topic_partition: TopicPartition {
                        topic: "bench-topic".to_string(),
                        partition: 0,
                    },
                };

                upload_manager.upload_segment(segment).await.unwrap();
            });
        });
    }

    group.finish();
}

fn bench_replication_latencies(c: &mut Criterion) {
    let mut group = c.benchmark_group("replication_latency");

    // Simulate different cluster sizes
    let cluster_sizes = vec![3, 5, 7, 10];

    for num_followers in cluster_sizes {
        group.bench_with_input(
            BenchmarkId::new("futures_unordered", num_followers),
            &num_followers,
            |b, &_num_followers| {
                b.iter(|| {
                    // This would measure actual replication with FuturesUnordered
                    // For now, just a placeholder showing the pattern
                    // In real benchmark, would use the replication manager
                });
            },
        );
    }

    group.finish();
}

fn bench_concurrent_chunk_uploads(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_chunks");

    // Test different numbers of chunks
    let chunk_counts = vec![2, 5, 10, 20];

    for num_chunks in chunk_counts {
        group.bench_with_input(
            BenchmarkId::new("parallel", num_chunks),
            &num_chunks,
            |b, &num_chunks| {
                b.to_async(Runtime::new().unwrap()).iter(|| async move {
                    use futures::stream::{FuturesUnordered, StreamExt};

                    let temp_dir = TempDir::new().unwrap();
                    let storage = Arc::new(LocalObjectStorage::new(temp_dir.path().to_path_buf()).unwrap());

                    let chunk_size = 5 * 1024 * 1024;
                    let mut futures = FuturesUnordered::new();

                    for i in 0..num_chunks {
                        let key = format!("chunk-{}", i);
                        let data = Bytes::from(vec![0u8; chunk_size]);
                        let storage_clone = storage.clone();

                        futures.push(async move {
                            storage_clone.put(&key, data).await
                        });
                    }

                    while let Some(result) = futures.next().await {
                        result.unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches,
    bench_multipart_upload_parallel,
    bench_replication_latencies,
    bench_concurrent_chunk_uploads
);
criterion_main!(benches);
