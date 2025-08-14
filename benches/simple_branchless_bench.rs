//! Simple Branchless Parser Performance Test
//!
//! A minimal benchmark to verify the branchless parser performance improvements.

use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Instant;
use rustmq::storage::wal::branchless_parser::BranchlessRecordBatchParser;

fn generate_test_data(num_records: usize, record_size: usize) -> Vec<u8> {
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

fn traditional_parsing(buffer: &[u8]) -> usize {
    let mut count = 0;
    let mut pos = 0;
    
    while pos + 8 <= buffer.len() {
        // Traditional branched bounds checking and size reading
        if pos + 8 > buffer.len() {
            break;
        }
        
        let size_bytes = [
            buffer[pos], buffer[pos + 1], buffer[pos + 2], buffer[pos + 3],
            buffer[pos + 4], buffer[pos + 5], buffer[pos + 6], buffer[pos + 7],
        ];
        let record_size = u64::from_le_bytes(size_bytes) as usize;
        
        // Traditional validation with branching
        if record_size < 8 || record_size > 64 * 1024 * 1024 {
            break;
        }
        
        // Traditional bounds checking for complete record
        if pos + record_size > buffer.len() {
            break;
        }
        
        count += 1;
        pos += record_size;
    }
    
    count
}

fn bench_verification(c: &mut Criterion) {
    let buffer = generate_test_data(1000, 128);
    
    c.bench_function("traditional_parsing", |b| {
        b.iter(|| traditional_parsing(&buffer))
    });
    
    c.bench_function("branchless_parsing", |b| {
        let mut parser = BranchlessRecordBatchParser::new().unwrap();
        b.iter(|| {
            let segments = parser.parse_record_headers_batch(&buffer, 0, 0).unwrap();
            segments.len()
        })
    });
    
    // Print verification results
    eprintln!("\n🚀 Branchless Parser Performance Verification");
    eprintln!("==============================================");
    
    let mut parser = BranchlessRecordBatchParser::new().unwrap();
    let test_buffer = generate_test_data(10000, 128);
    
    let start = Instant::now();
    let segments = parser.parse_record_headers_batch(&test_buffer, 0, 0).unwrap();
    let parse_time = start.elapsed();
    
    let stats = parser.get_stats();
    let throughput = (test_buffer.len() as f64) / parse_time.as_secs_f64() / 1024.0 / 1024.0;
    
    eprintln!("✅ Test: 10,000 records parsed successfully");
    eprintln!("📊 Throughput: {:.2} MB/s", throughput);
    eprintln!("⚡ Latency:    {:?}", parse_time);
    eprintln!("🎯 Accuracy:   {}/{} records", segments.len(), 10000);
    eprintln!("💻 SIMD:       {}", if parser.is_simd_enabled() { "Enabled" } else { "Disabled" });
    eprintln!("🔧 Features:   {}", parser.get_cpu_features());
    
    if segments.len() == 10000 {
        eprintln!("\n🎉 SUCCESS: Branchless parser implementation verified!");
    }
}

criterion_group!(benches, bench_verification);
criterion_main!(benches);