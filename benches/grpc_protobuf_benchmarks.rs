//! Performance benchmarks for gRPC protobuf operations
//! 
//! This benchmark suite measures the performance of critical protobuf operations:
//! - Type conversion performance (internal ↔ protobuf)
//! - Serialization/deserialization overhead
//! - Large data handling capabilities
//! - Concurrent operation throughput
//! - Memory allocation patterns

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use rustmq::{
    types::*,
    proto::{common, broker, controller},
    proto_convert,
};
use chrono::Utc;
use std::time::Duration;
use rand::{Rng, thread_rng};

// ============================================================================
// Benchmark Data Generators
// ============================================================================

fn generate_test_record(key_size: usize, value_size: usize, header_count: usize) -> Record {
    let mut rng = thread_rng();
    
    Record {
        key: if key_size > 0 {
            Some((0..key_size).map(|_| rng.gen()).collect())
        } else {
            None
        },
        value: (0..value_size).map(|_| rng.gen()).collect(),
        headers: (0..header_count).map(|i| Header {
            key: format!("header-{}", i),
            value: (0..20).map(|_| rng.gen()).collect(),
        }).collect(),
        timestamp: Utc::now().timestamp_millis(),
    }
}

fn generate_wal_record_batch(count: usize, record_size: usize) -> Vec<WalRecord> {
    (0..count).map(|i| WalRecord {
        topic_partition: TopicPartition {
            topic: "benchmark-topic".to_string(),
            partition: i as u32 % 10,
        },
        offset: i as u64,
        record: generate_test_record(50, record_size, 3),
        crc32: thread_rng().gen(),
    }).collect()
}

fn generate_log_entries(count: usize, data_size: usize) -> Vec<controller::LogEntry> {
    (0..count).map(|i| controller::LogEntry {
        index: i as u64,
        term: 1,
        r#type: controller::LogEntryType::BrokerMetadata as i32,
        data: (0..data_size).map(|_| thread_rng().gen()).collect(),
        timestamp: Some(prost_types::Timestamp {
            seconds: Utc::now().timestamp(),
            nanos: 0,
        }),
        node_id: "benchmark-node".to_string(),
        checksum: thread_rng().gen(),
        data_size: data_size as u32,
        correlation_id: format!("bench-{}", i),
        priority: 0,
        tags: vec!["benchmark".to_string()],
    }).collect()
}

// ============================================================================
// Type Conversion Benchmarks
// ============================================================================

fn bench_record_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_conversion");
    
    for record_size in [100, 1000, 10000, 100000].iter() {
        let record = generate_test_record(50, *record_size, 5);
        
        group.throughput(Throughput::Bytes(*record_size as u64));
        group.bench_with_input(
            BenchmarkId::new("internal_to_proto", record_size),
            record_size,
            |b, _| {
                b.iter(|| {
                    let _proto: common::Record = black_box(record.clone()).try_into().unwrap();
                });
            },
        );
        
        let proto_record: common::Record = record.try_into().unwrap();
        group.bench_with_input(
            BenchmarkId::new("proto_to_internal", record_size),
            record_size,
            |b, _| {
                b.iter(|| {
                    let _internal: Record = black_box(proto_record.clone()).try_into().unwrap();
                });
            },
        );
    }
    
    group.finish();
}

fn bench_wal_record_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_record_conversion");
    
    for batch_size in [1, 10, 100, 1000].iter() {
        let wal_records = generate_wal_record_batch(*batch_size, 1000);
        let total_bytes = wal_records.len() * 1000;
        
        group.throughput(Throughput::Bytes(total_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("batch_to_proto", batch_size),
            batch_size,
            |b, _| {
                b.iter(|| {
                    let _proto_records: Result<Vec<common::WalRecord>, _> = 
                        black_box(wal_records.clone()).into_iter()
                        .map(|r| r.try_into())
                        .collect();
                });
            },
        );
    }
    
    group.finish();
}

fn bench_topic_partition_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("topic_partition_conversion");
    
    // Test with different topic name lengths
    for topic_len in [10, 50, 100, 500].iter() {
        let topic_partition = TopicPartition {
            topic: "a".repeat(*topic_len),
            partition: 42,
        };
        
        group.throughput(Throughput::Bytes(*topic_len as u64));
        group.bench_with_input(
            BenchmarkId::new("roundtrip", topic_len),
            topic_len,
            |b, _| {
                b.iter(|| {
                    let proto: common::TopicPartition = black_box(topic_partition.clone()).into();
                    let _back: TopicPartition = proto.into();
                });
            },
        );
    }
    
    group.finish();
}

fn bench_broker_info_conversion(c: &mut Criterion) {
    let broker_info = BrokerInfo {
        id: "benchmark-broker-with-long-name".to_string(),
        host: "very-long-hostname.subdomain.domain.com".to_string(),
        port_quic: 9092,
        port_rpc: 9093,
        rack_id: Some("rack-with-very-long-descriptive-name".to_string()),
    };
    
    c.bench_function("broker_info_roundtrip", |b| {
        b.iter(|| {
            let proto: common::BrokerInfo = black_box(broker_info.clone()).try_into().unwrap();
            let _back: BrokerInfo = proto.try_into().unwrap();
        });
    });
}

fn bench_follower_state_conversion(c: &mut Criterion) {
    let follower_state = FollowerState {
        broker_id: "follower-broker-with-long-name".to_string(),
        last_known_offset: 12345678,
        last_heartbeat: Utc::now(),
        lag: 1000,
    };
    
    c.bench_function("follower_state_roundtrip", |b| {
        b.iter(|| {
            let proto: common::FollowerState = black_box(follower_state.clone()).try_into().unwrap();
            let _back: FollowerState = proto.try_into().unwrap();
        });
    });
}

// ============================================================================
// Request/Response Conversion Benchmarks
// ============================================================================

fn bench_replicate_data_request_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("replicate_data_request");
    
    for record_count in [1, 10, 100, 1000].iter() {
        let request = ReplicateDataRequest {
            leader_epoch: 5,
            topic_partition: TopicPartition {
                topic: "benchmark-topic".to_string(),
                partition: 0,
            },
            records: generate_wal_record_batch(*record_count, 500),
            leader_id: "leader-1".to_string(),
        };
        
        let total_bytes = record_count * 500;
        group.throughput(Throughput::Bytes(total_bytes as u64));
        
        group.bench_with_input(
            BenchmarkId::new("to_proto", record_count),
            record_count,
            |b, _| {
                b.iter(|| {
                    let _proto: broker::ReplicateDataRequest = 
                        black_box(request.clone()).try_into().unwrap();
                });
            },
        );
        
        let proto_request: broker::ReplicateDataRequest = request.try_into().unwrap();
        group.bench_with_input(
            BenchmarkId::new("from_proto", record_count),
            record_count,
            |b, _| {
                b.iter(|| {
                    let _internal: ReplicateDataRequest = 
                        black_box(proto_request.clone()).try_into().unwrap();
                });
            },
        );
    }
    
    group.finish();
}

fn bench_heartbeat_request_conversion(c: &mut Criterion) {
    let request = HeartbeatRequest {
        leader_epoch: 10,
        leader_id: "leader-with-very-long-identifier".to_string(),
        topic_partition: TopicPartition {
            topic: "heartbeat-topic-with-long-name".to_string(),
            partition: 5,
        },
        high_watermark: 987654321,
    };
    
    c.bench_function("heartbeat_request_roundtrip", |b| {
        b.iter(|| {
            let proto: broker::HeartbeatRequest = black_box(request.clone()).try_into().unwrap();
            let _back: HeartbeatRequest = proto.try_into().unwrap();
        });
    });
}

fn bench_transfer_leadership_request_conversion(c: &mut Criterion) {
    let request = TransferLeadershipRequest {
        topic_partition: TopicPartition {
            topic: "leadership-transfer-topic".to_string(),
            partition: 3,
        },
        current_leader_id: "current-leader-with-long-name".to_string(),
        current_leader_epoch: 15,
        new_leader_id: "new-leader-with-long-name".to_string(),
    };
    
    c.bench_function("transfer_leadership_roundtrip", |b| {
        b.iter(|| {
            let proto: broker::TransferLeadershipRequest = 
                black_box(request.clone()).try_into().unwrap();
            let _back: TransferLeadershipRequest = proto.try_into().unwrap();
        });
    });
}

// ============================================================================
// Error Handling Benchmarks
// ============================================================================

fn bench_error_code_mapping(c: &mut Criterion) {
    let errors = vec![
        RustMqError::TopicNotFound("test-topic".to_string()),
        RustMqError::PartitionNotFound("test-partition".to_string()),
        RustMqError::BrokerNotFound("test-broker".to_string()),
        RustMqError::StaleLeaderEpoch { request_epoch: 1, current_epoch: 2 },
        RustMqError::NotLeader("test-broker".to_string()),
        RustMqError::OffsetOutOfRange("offset 100".to_string()),
        RustMqError::TopicAlreadyExists("existing-topic".to_string()),
        RustMqError::ResourceExhausted("memory".to_string()),
        RustMqError::PermissionDenied("access denied".to_string()),
        RustMqError::Timeout,
    ];
    
    c.bench_function("error_code_mapping", |b| {
        b.iter(|| {
            for error in &errors {
                let _code = proto_convert::error_to_code(black_box(error));
                let _message = proto_convert::error_to_message(black_box(error));
                let _retryable = proto_convert::error_is_retryable(black_box(error));
            }
        });
    });
}

// ============================================================================
// Timestamp Conversion Benchmarks
// ============================================================================

fn bench_timestamp_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("timestamp_conversion");
    
    let timestamps = (0..1000).map(|i| Utc::now().timestamp_millis() + i).collect::<Vec<_>>();
    
    group.bench_function("to_proto", |b| {
        b.iter(|| {
            for &timestamp in &timestamps {
                let _proto = proto_convert::timestamp_to_proto(black_box(timestamp)).unwrap();
            }
        });
    });
    
    let proto_timestamps: Vec<_> = timestamps.iter()
        .map(|&ts| proto_convert::timestamp_to_proto(ts).unwrap())
        .collect();
    
    group.bench_function("from_proto", |b| {
        b.iter(|| {
            for proto_ts in &proto_timestamps {
                let _millis = proto_convert::timestamp_from_proto(black_box(proto_ts.clone())).unwrap();
            }
        });
    });
    
    group.finish();
}

// ============================================================================
// Large Data Benchmarks
// ============================================================================

fn bench_large_record_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_record_conversion");
    group.sample_size(10); // Fewer samples for large data
    group.measurement_time(Duration::from_secs(10));
    
    for size_mb in [1, 5, 10, 50].iter() {
        let size_bytes = size_mb * 1024 * 1024;
        let record = generate_test_record(1024, size_bytes, 10);
        
        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("conversion", format!("{}MB", size_mb)),
            size_mb,
            |b, _| {
                b.iter(|| {
                    let proto: common::Record = black_box(record.clone()).try_into().unwrap();
                    let _back: Record = proto.try_into().unwrap();
                });
            },
        );
    }
    
    group.finish();
}

fn bench_large_log_entry_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_log_entry_batch");
    group.sample_size(10);
    
    for entry_count in [100, 500, 1000, 5000].iter() {
        let entries = generate_log_entries(*entry_count, 1024);
        let total_bytes = entry_count * 1024;
        
        group.throughput(Throughput::Bytes(total_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("processing", entry_count),
            entry_count,
            |b, _| {
                b.iter(|| {
                    // Simulate processing log entries
                    let _total_size: usize = black_box(&entries).iter()
                        .map(|entry| entry.data.len())
                        .sum();
                });
            },
        );
    }
    
    group.finish();
}

// ============================================================================
// Memory Allocation Benchmarks
// ============================================================================

fn bench_memory_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");
    
    // Benchmark record creation with different header counts
    for header_count in [0, 10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("record_creation", header_count),
            header_count,
            |b, &header_count| {
                b.iter(|| {
                    let _record = generate_test_record(black_box(100), black_box(1000), black_box(header_count));
                });
            },
        );
    }
    
    // Benchmark batch operations
    for batch_size in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("batch_creation", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let _batch = generate_wal_record_batch(black_box(batch_size), black_box(500));
                });
            },
        );
    }
    
    group.finish();
}

// ============================================================================
// Concurrent Operations Benchmarks
// ============================================================================

fn bench_concurrent_conversions(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_conversions");
    
    let records: Vec<_> = (0..100).map(|_| generate_test_record(100, 1000, 5)).collect();
    
    group.bench_function("sequential", |b| {
        b.iter(|| {
            for record in &records {
                let proto: common::Record = black_box(record.clone()).try_into().unwrap();
                let _back: Record = proto.try_into().unwrap();
            }
        });
    });
    
    // Note: In a real benchmark, you'd use a thread pool or async runtime
    // This is a simplified version for demonstration
    group.bench_function("parallel_simulation", |b| {
        b.iter(|| {
            // Simulate parallel processing by chunking
            for chunk in records.chunks(10) {
                for record in chunk {
                    let proto: common::Record = black_box(record.clone()).try_into().unwrap();
                    let _back: Record = proto.try_into().unwrap();
                }
            }
        });
    });
    
    group.finish();
}

// ============================================================================
// Compression Impact Benchmarks
// ============================================================================

fn bench_compression_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_impact");
    
    // Create highly compressible data (repeated patterns)
    let compressible_data = vec![0u8; 10000];
    let compressible_record = Record {
        key: Some(compressible_data.clone()),
        value: compressible_data,
        headers: vec![],
        timestamp: Utc::now().timestamp_millis(),
    };
    
    // Create incompressible data (random)
    let random_data: Vec<u8> = (0..10000).map(|_| thread_rng().gen()).collect();
    let random_record = Record {
        key: Some(random_data.clone()),
        value: random_data,
        headers: vec![],
        timestamp: Utc::now().timestamp_millis(),
    };
    
    group.throughput(Throughput::Bytes(20000)); // 20KB per record
    
    group.bench_function("compressible_data", |b| {
        b.iter(|| {
            let proto: common::Record = black_box(compressible_record.clone()).try_into().unwrap();
            let _back: Record = proto.try_into().unwrap();
        });
    });
    
    group.bench_function("random_data", |b| {
        b.iter(|| {
            let proto: common::Record = black_box(random_record.clone()).try_into().unwrap();
            let _back: Record = proto.try_into().unwrap();
        });
    });
    
    group.finish();
}

// ============================================================================
// Benchmark Groups
// ============================================================================

criterion_group!(
    type_conversions,
    bench_record_conversion,
    bench_wal_record_conversion,
    bench_topic_partition_conversion,
    bench_broker_info_conversion,
    bench_follower_state_conversion
);

criterion_group!(
    request_response_conversions,
    bench_replicate_data_request_conversion,
    bench_heartbeat_request_conversion,
    bench_transfer_leadership_request_conversion
);

criterion_group!(
    error_and_timestamp,
    bench_error_code_mapping,
    bench_timestamp_conversion
);

criterion_group!(
    large_data,
    bench_large_record_conversion,
    bench_large_log_entry_batch
);

criterion_group!(
    memory_and_concurrency,
    bench_memory_allocation_patterns,
    bench_concurrent_conversions
);

criterion_group!(
    compression,
    bench_compression_impact
);

criterion_main!(
    type_conversions,
    request_response_conversions,
    error_and_timestamp,
    large_data,
    memory_and_concurrency,
    compression
);