use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rustmq::types::{Record, Header};
use bytes::Bytes;
use std::time::Instant;

fn benchmark_record_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Record Creation");
    
    let key_data = b"test_key".to_vec();
    let value_data = vec![0u8; 1000]; // 1KB value
    let headers = vec![Header::new("content-type".to_string(), b"application/json".to_vec())];
    let timestamp = chrono::Utc::now().timestamp_millis();
    
    group.bench_function("Vec<u8> to Bytes conversion", |b| {
        b.iter(|| {
            Record::new(
                black_box(Some(key_data.clone())),
                black_box(value_data.clone()),
                black_box(headers.clone()),
                black_box(timestamp),
            )
        })
    });
    
    let key_bytes = Bytes::from(key_data.clone());
    let value_bytes = Bytes::from(value_data.clone());
    
    group.bench_function("Zero-copy Bytes construction", |b| {
        b.iter(|| {
            Record::from_bytes(
                black_box(Some(key_bytes.clone())),
                black_box(value_bytes.clone()),
                black_box(headers.clone()),
                black_box(timestamp),
            )
        })
    });
    
    group.finish();
}

fn benchmark_record_cloning(c: &mut Criterion) {
    let mut group = c.benchmark_group("Record Cloning");
    
    // Create a record with substantial data
    let large_value = vec![0u8; 10000]; // 10KB value
    let record = Record::new(
        Some(b"test_key".to_vec()),
        large_value,
        vec![Header::new("header1".to_string(), b"value1".to_vec())],
        chrono::Utc::now().timestamp_millis(),
    );
    
    group.bench_function("Clone Record with Bytes (reference counted)", |b| {
        b.iter(|| {
            let _cloned = black_box(record.clone());
        })
    });
    
    group.finish();
}

fn benchmark_protobuf_conversion_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Protobuf Conversion Simulation");
    
    // Simulate the old way: Vec<u8> -> Record -> Vec<u8> (with copying)
    group.bench_function("Old way (with copying)", |b| {
        b.iter(|| {
            let protobuf_data = black_box(vec![0u8; 1000]);
            
            // Simulate old protobuf deserialization (creates new Vec)
            let key = Some(protobuf_data[0..8].to_vec());
            let value = protobuf_data[8..].to_vec();
            
            // Create record (would copy again in old implementation)
            let record = Record::new(
                key,
                value,
                vec![],
                chrono::Utc::now().timestamp_millis(),
            );
            
            // Simulate serialization back to Vec<u8> (copies again)
            let _serialized_key = record.key_as_vec();
            let _serialized_value = record.value_as_vec();
        })
    });
    
    // Simulate the new way: protobuf buffer -> Bytes (zero-copy)
    group.bench_function("New way (zero-copy with Bytes)", |b| {
        b.iter(|| {
            let protobuf_data = black_box(vec![0u8; 1000]);
            let protobuf_bytes = Bytes::from(protobuf_data);
            
            // Zero-copy slicing
            let key_bytes = Some(protobuf_bytes.slice(0..8));
            let value_bytes = protobuf_bytes.slice(8..);
            
            // Create record with zero-copy
            let record = Record::from_bytes(
                key_bytes,
                value_bytes,
                vec![],
                chrono::Utc::now().timestamp_millis(),
            );
            
            // Zero-copy cloning for serialization
            let _key_ref = record.key.clone();
            let _value_ref = record.value.clone();
        })
    });
    
    group.finish();
}

fn benchmark_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("Allocation Patterns");
    
    group.bench_function("1000 records - Vec<u8> approach", |b| {
        b.iter(|| {
            let mut records = Vec::new();
            for i in 0..1000 {
                // Each record creates new allocations
                let record = Record::new(
                    Some(format!("key{}", i).into_bytes()),
                    format!("value{}", i).repeat(100).into_bytes(), // ~500 bytes
                    vec![Header::new("index".to_string(), i.to_string().into_bytes())],
                    chrono::Utc::now().timestamp_millis(),
                );
                records.push(record);
            }
            black_box(records);
        })
    });
    
    group.bench_function("1000 records - Bytes approach with sharing", |b| {
        b.iter(|| {
            let mut records = Vec::new();
            
            // Pre-allocate a large buffer to share
            let shared_buffer = Bytes::from(vec![0u8; 100000]);
            
            for i in 0..1000 {
                let start = (i * 100) % (shared_buffer.len() - 500);
                
                // Zero-copy slices from shared buffer
                let key_bytes = Some(shared_buffer.slice(start..start + 20));
                let value_bytes = shared_buffer.slice(start + 20..start + 520);
                
                let record = Record::from_bytes(
                    key_bytes,
                    value_bytes,
                    vec![Header::new("index".to_string(), i.to_string().into_bytes())],
                    chrono::Utc::now().timestamp_millis(),
                );
                records.push(record);
            }
            black_box(records);
        })
    });
    
    group.finish();
}

fn benchmark_replication_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Replication Simulation");
    
    // Simulate high-throughput broker-to-broker replication
    let batch_size = 100;
    
    group.bench_function("Replicate 100 messages - old approach", |b| {
        b.iter(|| {
            let start = Instant::now();
            
            for i in 0..batch_size {
                // Simulate network receive -> allocation
                let network_data = vec![0u8; 1024]; // 1KB message
                
                // Simulate protobuf deserialization -> more allocations
                let key = Some(format!("key-{}", i).into_bytes());
                let value = network_data;
                
                // Create internal record -> potential copying
                let record = Record::new(
                    key,
                    value,
                    vec![],
                    chrono::Utc::now().timestamp_millis(),
                );
                
                // Simulate storage write -> more copying
                let _serialized = (record.key_as_vec(), record.value_as_vec());
            }
            
            black_box(start.elapsed());
        })
    });
    
    group.bench_function("Replicate 100 messages - zero-copy approach", |b| {
        b.iter(|| {
            let start = Instant::now();
            
            for i in 0..batch_size {
                // Simulate network receive -> single allocation
                let network_data = Bytes::from(vec![0u8; 1024]);
                
                // Simulate zero-copy protobuf deserialization
                let key_bytes = Some(network_data.slice(0..20));
                let value_bytes = network_data.slice(20..);
                
                // Create internal record with zero-copy
                let record = Record::from_bytes(
                    key_bytes,
                    value_bytes,
                    vec![],
                    chrono::Utc::now().timestamp_millis(),
                );
                
                // Zero-copy sharing for storage (reference counting)
                let _shared_key = record.key.clone();
                let _shared_value = record.value.clone();
            }
            
            black_box(start.elapsed());
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_record_creation,
    benchmark_record_cloning,
    benchmark_protobuf_conversion_simulation,
    benchmark_allocation_patterns,
    benchmark_replication_simulation
);
criterion_main!(benches);