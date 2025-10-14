use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustmq::types::{Record, Header, Headers};
use bytes::Bytes;

/// Quick benchmark for header memory patterns
fn quick_bench(c: &mut Criterion) {
    // SmallVec inline (2 headers) - no heap allocation
    c.bench_function("smallvec_2_headers", |b| {
        b.iter(|| {
            let headers: Headers = vec![
                Header::new("h1".to_string(), vec![1]),
                Header::new("h2".to_string(), vec![2]),
            ].into_iter().collect();
            black_box(headers)
        });
    });

    // SmallVec inline (4 headers) - still no heap allocation
    c.bench_function("smallvec_4_headers", |b| {
        b.iter(|| {
            let headers: Headers = (0..4)
                .map(|i| Header::new(format!("h{}", i), vec![i]))
                .collect();
            black_box(headers)
        });
    });

    // SmallVec spilled (8 headers) - heap allocation
    c.bench_function("smallvec_8_headers", |b| {
        b.iter(|| {
            let headers: Headers = (0..8)
                .map(|i| Header::new(format!("h{}", i), vec![i]))
                .collect();
            black_box(headers)
        });
    });

    // Record creation with SmallVec (zero-copy)
    c.bench_function("record_with_headers_4", |b| {
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

    // Header cloning performance
    c.bench_function("clone_4_headers", |b| {
        let headers: Headers = (0..4)
            .map(|i| Header::new(format!("h{}", i), vec![i as u8; 100]))
            .collect();

        b.iter(|| black_box(headers.clone()));
    });
}

criterion_group!(benches, quick_bench);
criterion_main!(benches);
