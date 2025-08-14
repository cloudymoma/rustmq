//! Branchless Parsing Performance Benchmarks
//!
//! Comprehensive benchmarks comparing traditional branched parsing
//! vs. branchless SIMD-optimized parsing for WAL recovery scenarios.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rustmq::storage::wal::branchless_parser::{BranchlessRecordBatchParser, BranchlessValidation};
use rustmq::storage::wal::WalSegmentMetadata;
use std::time::Instant;

/// Generate test buffer with multiple records for benchmarking
fn generate_test_buffer(num_records: usize, record_size: usize) -> Vec<u8> {
    let mut buffer = Vec::new();
    
    for i in 0..num_records {
        // Record size (8 bytes)
        buffer.extend_from_slice(&(record_size as u64).to_le_bytes());
        
        // Record data
        let data = vec![(i % 256) as u8; record_size - 8];
        buffer.extend_from_slice(&data);
    }
    
    buffer
}

/// Traditional branched parsing implementation for comparison
fn parse_records_branched(
    buffer: &[u8],
    file_offset_start: u64,
    logical_offset_start: u64,
) -> Result<Vec<WalSegmentMetadata>, Box<dyn std::error::Error>> {
    let mut segments = Vec::new();
    let mut buffer_pos = 0;
    let mut logical_offset = logical_offset_start;
    
    while buffer_pos < buffer.len() {
        // Traditional branched bounds checking
        if buffer_pos + 8 > buffer.len() {
            break;
        }
        
        // Traditional record size reading with error handling
        let record_size = if let Ok(size) = read_record_size_traditional(&buffer[buffer_pos..]) {
            size
        } else {
            break;
        };
        
        // Traditional validation with branching
        if record_size < 8 || record_size > 64 * 1024 * 1024 {
            break;
        }
        
        // Traditional bounds checking for complete record
        if buffer_pos + record_size as usize > buffer.len() {
            break;
        }
        
        let segment = WalSegmentMetadata {
            start_offset: logical_offset,
            end_offset: logical_offset + 1,
            file_offset: file_offset_start + buffer_pos as u64,
            size_bytes: record_size,
            created_at: Instant::now(),
        };
        
        segments.push(segment);
        logical_offset += 1;
        buffer_pos += record_size as usize;
    }
    
    Ok(segments)
}

/// Traditional record size reading with bounds checking
fn read_record_size_traditional(buffer: &[u8]) -> Result<u64, Box<dyn std::error::Error>> {
    if buffer.len() < 8 {
        return Err("Buffer too small".into());
    }
    
    let size = u64::from_le_bytes([
        buffer[0], buffer[1], buffer[2], buffer[3],
        buffer[4], buffer[5], buffer[6], buffer[7],
    ]);
    
    Ok(size)
}

/// Benchmark record size reading: branched vs branchless
fn bench_record_size_reading(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_size_reading");
    
    // Create test buffer with many record sizes
    let mut buffer = Vec::new();
    for i in 0..1000 {
        buffer.extend_from_slice(&((i * 100 + 64) as u64).to_le_bytes());
    }
    
    group.throughput(Throughput::Elements(1000));
    
    // Traditional branched implementation
    group.bench_function("traditional_branched", |b| {
        b.iter(|| {
            let mut pos = 0;
            let mut results = Vec::new();
            
            while pos + 8 <= buffer.len() {
                let size = read_record_size_traditional(&buffer[pos..]).unwrap();
                results.push(black_box(size));
                pos += 8;
            }
            
            results
        })
    });
    
    // Branchless implementation
    group.bench_function("branchless_unsafe", |b| {
        let parser = BranchlessRecordBatchParser::new().unwrap();
        
        b.iter(|| {
            let mut pos = 0;
            let mut results = Vec::new();
            
            while pos + 8 <= buffer.len() {
                let size = unsafe {
                    parser.read_record_size_branchless(&buffer[pos..])
                };
                results.push(black_box(size));
                pos += 8;
            }
            
            results
        })
    });
    
    group.finish();
}

/// Benchmark validation: branched vs branchless
fn bench_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");
    
    // Create test data with mix of valid and invalid sizes
    let sizes: Vec<u64> = (0..1000)
        .map(|i| match i % 5 {
            0 => 4,                    // Invalid: too small
            1 => 100 * 1024 * 1024,    // Invalid: too large  
            2 => 0,                    // Invalid: zero
            _ => (i * 50 + 64) as u64, // Valid sizes
        })
        .collect();
    
    group.throughput(Throughput::Elements(sizes.len() as u64));
    
    // Traditional branched validation
    group.bench_function("traditional_branched", |b| {
        b.iter(|| {
            let mut valid_count = 0;
            
            for &size in &sizes {
                if size >= 8 && size <= 64 * 1024 * 1024 {
                    valid_count += 1;
                }
            }
            
            black_box(valid_count)
        })
    });
    
    // Branchless validation
    group.bench_function("branchless_batch", |b| {
        b.iter(|| {
            let results = BranchlessValidation::validate_sizes_batch(&sizes);
            let valid_count = results.iter().filter(|&&x| x).count();
            black_box(valid_count)
        })
    });
    
    group.finish();
}

/// Benchmark full record parsing: traditional vs branchless
fn bench_full_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_parsing");
    
    // Test with different scenarios
    let scenarios = [
        ("small_records_1k", 1000, 64),      // 1000 small records
        ("medium_records_500", 500, 256),    // 500 medium records
        ("large_records_100", 100, 1024),    // 100 large records
        ("mixed_sizes", 1000, 128),          // Mixed sizes (will be varied)
    ];
    
    for (scenario_name, num_records, base_size) in scenarios.iter() {
        let buffer = if *scenario_name == "mixed_sizes" {
            // Create mixed size records
            let mut buffer = Vec::new();
            for i in 0..*num_records {
                let size = match i % 4 {
                    0 => 64,
                    1 => 128,
                    2 => 256, 
                    3 => 512,
                    _ => unreachable!(),
                };
                buffer.extend_from_slice(&(size as u64).to_le_bytes());
                buffer.extend_from_slice(&vec![(i % 256) as u8; size - 8]);
            }
            buffer
        } else {
            generate_test_buffer(*num_records, *base_size)
        };
        
        group.throughput(Throughput::Bytes(buffer.len() as u64));
        
        // Traditional branched parsing
        group.bench_with_input(
            BenchmarkId::new("traditional_branched", scenario_name),
            &buffer,
            |b, buffer| {
                b.iter(|| {
                    let result = parse_records_branched(black_box(buffer), 0, 0);
                    black_box(result.unwrap().len())
                })
            },
        );
        
        // Branchless scalar parsing
        group.bench_with_input(
            BenchmarkId::new("branchless_scalar", scenario_name),
            &buffer,
            |b, buffer| {
                let mut parser = BranchlessRecordBatchParser::new().unwrap();
                
                b.iter(|| {
                    let result = parser.parse_record_headers_batch(black_box(buffer), 0, 0);
                    black_box(result.unwrap().len())
                })
            },
        );
        
        // Branchless SIMD parsing (if available)
        group.bench_with_input(
            BenchmarkId::new("branchless_simd", scenario_name),
            &buffer,
            |b, buffer| {
                let mut parser = BranchlessRecordBatchParser::new().unwrap();
                
                b.iter(|| {
                    let result = parser.parse_record_headers_batch(black_box(buffer), 0, 0);
                    black_box(result.unwrap().len())
                })
            },
        );
    }
    
    group.finish();
}

/// Benchmark WAL recovery simulation
fn bench_wal_recovery_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_recovery_simulation");
    
    // Simulate realistic WAL recovery scenarios
    let recovery_scenarios = [
        ("startup_recovery_10k", 10_000, 128),     // Typical startup
        ("large_recovery_50k", 50_000, 256),      // Large system recovery
        ("tiny_records_100k", 100_000, 32),       // Many tiny records
        ("mixed_workload", 20_000, 256),          // Mixed record sizes
    ];
    
    for (scenario_name, num_records, avg_size) in recovery_scenarios.iter() {
        // Generate realistic WAL data
        let buffer = if *scenario_name == "mixed_workload" {
            let mut buffer = Vec::new();
            let sizes = [32, 64, 128, 256, 512, 1024];
            
            for i in 0..*num_records {
                let size = sizes[i % sizes.len()];
                buffer.extend_from_slice(&(size as u64).to_le_bytes());
                
                // Add realistic record data (not just padding)
                let mut record_data = Vec::new();
                record_data.extend_from_slice(&(i as u64).to_le_bytes());    // ID
                record_data.extend_from_slice(&[0u8; 8]);                    // Timestamp placeholder
                record_data.extend_from_slice(&vec![(i % 256) as u8; size - 24]); // Payload
                
                buffer.extend_from_slice(&record_data);
            }
            buffer
        } else {
            generate_test_buffer(*num_records, *avg_size)
        };
        
        group.throughput(Throughput::Elements(*num_records as u64));
        
        // Simulate traditional WAL recovery
        group.bench_with_input(
            BenchmarkId::new("traditional_recovery", scenario_name),
            &buffer,
            |b, buffer| {
                b.iter(|| {
                    let start_time = Instant::now();
                    let segments = parse_records_branched(black_box(buffer), 0, 0).unwrap();
                    let parse_time = start_time.elapsed();
                    
                    black_box((segments.len(), parse_time))
                })
            },
        );
        
        // Simulate branchless WAL recovery
        group.bench_with_input(
            BenchmarkId::new("branchless_recovery", scenario_name),
            &buffer,
            |b, buffer| {
                let mut parser = BranchlessRecordBatchParser::new().unwrap();
                
                b.iter(|| {
                    let start_time = Instant::now();
                    let segments = parser.parse_record_headers_batch(black_box(buffer), 0, 0).unwrap();
                    let parse_time = start_time.elapsed();
                    
                    black_box((segments.len(), parse_time))
                })
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory access patterns
fn bench_memory_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_patterns");
    
    // Test different memory access patterns
    let access_patterns = [
        ("sequential_1mb", 1024 * 1024, true),   // Sequential 1MB
        ("random_1mb", 1024 * 1024, false),     // Random access 1MB
        ("cache_friendly_64k", 64 * 1024, true), // Cache-friendly 64KB
    ];
    
    for (pattern_name, buffer_size, sequential) in access_patterns.iter() {
        let buffer = vec![0u8; *buffer_size];
        
        // Fill buffer with record size headers
        let mut test_buffer = Vec::new();
        let num_records = *buffer_size / 128; // Assume 128-byte average records
        
        for i in 0..num_records {
            test_buffer.extend_from_slice(&(128u64).to_le_bytes());
            test_buffer.extend_from_slice(&vec![(i % 256) as u8; 120]);
        }
        
        group.throughput(Throughput::Bytes(test_buffer.len() as u64));
        
        // Traditional parsing
        group.bench_with_input(
            BenchmarkId::new("traditional", pattern_name),
            &test_buffer,
            |b, buffer| {
                b.iter(|| {
                    let segments = parse_records_branched(black_box(buffer), 0, 0).unwrap();
                    black_box(segments.len())
                })
            },
        );
        
        // Branchless parsing
        group.bench_with_input(
            BenchmarkId::new("branchless", pattern_name),
            &test_buffer,
            |b, buffer| {
                let mut parser = BranchlessRecordBatchParser::new().unwrap();
                
                b.iter(|| {
                    let segments = parser.parse_record_headers_batch(black_box(buffer), 0, 0).unwrap();
                    black_box(segments.len())
                })
            },
        );
    }
    
    group.finish();
}

/// Benchmark error handling overhead
fn bench_error_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_handling");
    
    // Create buffer with mix of valid and invalid records
    let mut buffer = Vec::new();
    
    for i in 0..1000 {
        if i % 10 == 0 {
            // Invalid record (too small size)
            buffer.extend_from_slice(&(4u64).to_le_bytes());
            buffer.extend_from_slice(&[0u8; 4]);
        } else {
            // Valid record
            buffer.extend_from_slice(&(128u64).to_le_bytes());
            buffer.extend_from_slice(&vec![(i % 256) as u8; 120]);
        }
    }
    
    group.throughput(Throughput::Elements(1000));
    
    // Traditional error handling with Result propagation
    group.bench_function("traditional_result_propagation", |b| {
        b.iter(|| {
            let segments = parse_records_branched(black_box(&buffer), 0, 0).unwrap();
            black_box(segments.len())
        })
    });
    
    // Branchless error handling with arithmetic
    group.bench_function("branchless_arithmetic", |b| {
        let mut parser = BranchlessRecordBatchParser::new().unwrap();
        
        b.iter(|| {
            let segments = parser.parse_record_headers_batch(black_box(&buffer), 0, 0).unwrap();
            black_box(segments.len())
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_record_size_reading,
    bench_validation,
    bench_full_parsing,
    bench_wal_recovery_simulation,
    bench_memory_patterns,
    bench_error_handling
);

criterion_main!(benches);