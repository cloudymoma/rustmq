use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rustmq::types::{Record, Header, Headers};
use smallvec::SmallVec;
use bytes::Bytes;

/// Benchmark message creation with varying header counts
fn bench_header_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("header_operations");

    // Benchmark message creation with 0, 2, 4, 8 headers
    for num_headers in [0, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("create_message_with_smallvec", num_headers),
            num_headers,
            |b, &num| {
                let headers: Headers = (0..num)
                    .map(|i| Header::new(format!("header-{}", i), vec![i as u8]))
                    .collect();

                b.iter(|| {
                    Record::with_headers(
                        Some(Bytes::from(vec![1, 2, 3])),
                        Bytes::from(vec![4, 5, 6]),
                        black_box(headers.clone()),
                        1234567890,
                    )
                });
            },
        );
    }

    // Benchmark header cloning (SmallVec vs Vec would show difference)
    for num_headers in [0, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("clone_headers", num_headers),
            num_headers,
            |b, &num| {
                let headers: Headers = (0..num)
                    .map(|i| Header::new(format!("header-{}", i), vec![i as u8; 100]))
                    .collect();

                b.iter(|| black_box(headers.clone()));
            },
        );
    }

    // Benchmark header iteration
    for num_headers in [0, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new("iterate_headers", num_headers),
            num_headers,
            |b, &num| {
                let headers: Headers = (0..num)
                    .map(|i| Header::new(format!("header-{}", i), vec![i as u8; 10]))
                    .collect();

                b.iter(|| {
                    let mut sum = 0u64;
                    for header in black_box(&headers) {
                        sum += header.value.len() as u64;
                    }
                    black_box(sum)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark record creation patterns
fn bench_record_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_creation");

    // Zero-copy path: with_headers
    group.bench_function("create_with_headers_zero_copy", |b| {
        let headers: Headers = (0..4)
            .map(|i| Header::new(format!("h{}", i), vec![i]))
            .collect();

        b.iter(|| {
            Record::with_headers(
                Some(Bytes::from_static(b"key")),
                Bytes::from_static(b"value"),
                black_box(headers.clone()),
                1234567890,
            )
        });
    });

    // Traditional path: from_bytes with Vec conversion
    group.bench_function("create_from_bytes_with_vec", |b| {
        let headers_vec: Vec<Header> = (0..4)
            .map(|i| Header::new(format!("h{}", i), vec![i]))
            .collect();

        b.iter(|| {
            Record::from_bytes(
                Some(Bytes::from_static(b"key")),
                Bytes::from_static(b"value"),
                black_box(headers_vec.clone()),
                1234567890,
            )
        });
    });

    // Legacy path: new with Vec<u8>
    group.bench_function("create_new_with_vec_u8", |b| {
        let headers_vec: Vec<Header> = (0..4)
            .map(|i| Header::new(format!("h{}", i), vec![i]))
            .collect();

        b.iter(|| {
            Record::new(
                Some(b"key".to_vec()),
                b"value".to_vec(),
                black_box(headers_vec.clone()),
                1234567890,
            )
        });
    });

    group.finish();
}

/// Benchmark memory allocation patterns
fn bench_memory_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_patterns");

    // SmallVec inline (0-4 headers) - no heap allocation
    group.bench_function("smallvec_inline_2_headers", |b| {
        b.iter(|| {
            let mut headers: Headers = SmallVec::new();
            headers.push(black_box(Header::new("h1".to_string(), vec![1])));
            headers.push(black_box(Header::new("h2".to_string(), vec![2])));
            black_box(headers)
        });
    });

    // SmallVec inline (4 headers) - still no heap allocation
    group.bench_function("smallvec_inline_4_headers", |b| {
        b.iter(|| {
            let mut headers: Headers = SmallVec::new();
            for i in 0..4 {
                headers.push(black_box(Header::new(format!("h{}", i), vec![i])));
            }
            black_box(headers)
        });
    });

    // SmallVec spilled (8 headers) - heap allocation
    group.bench_function("smallvec_spilled_8_headers", |b| {
        b.iter(|| {
            let mut headers: Headers = SmallVec::new();
            for i in 0..8 {
                headers.push(black_box(Header::new(format!("h{}", i), vec![i])));
            }
            black_box(headers)
        });
    });

    // Vec (always heap allocated)
    group.bench_function("vec_2_headers", |b| {
        b.iter(|| {
            let mut headers: Vec<Header> = Vec::new();
            headers.push(black_box(Header::new("h1".to_string(), vec![1])));
            headers.push(black_box(Header::new("h2".to_string(), vec![2])));
            black_box(headers)
        });
    });

    group.finish();
}

/// Benchmark serialization performance
fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    for num_headers in [0, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("serialize_record", num_headers),
            num_headers,
            |b, &num| {
                let headers: Headers = (0..num)
                    .map(|i| Header::new(format!("header-{}", i), vec![i as u8; 50]))
                    .collect();

                let record = Record::with_headers(
                    Some(Bytes::from(b"test-key".to_vec())),
                    Bytes::from(b"test-value-with-some-data".to_vec()),
                    headers,
                    1234567890,
                );

                b.iter(|| bincode::serialize(black_box(&record)).unwrap());
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_header_operations,
    bench_record_creation,
    bench_memory_patterns,
    bench_serialization
);
criterion_main!(benches);
