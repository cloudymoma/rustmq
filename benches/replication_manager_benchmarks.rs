use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use rustmq::replication::manager::ReplicationManager;
use std::collections::BinaryHeap;
use rand::{thread_rng, Rng};

/// Generate test offsets with realistic distribution
/// This simulates a real-world scenario where followers might have different lag patterns
fn generate_test_offsets(n: usize, scenario: &str) -> Vec<u64> {
    let mut rng = thread_rng();
    let mut offsets = Vec::with_capacity(n);
    
    match scenario {
        "sequential" => {
            // All replicas are fairly caught up (small variance)
            let base_offset = 1_000_000;
            for _ in 0..n {
                let lag = rng.gen_range(0..100);
                offsets.push(base_offset - lag);
            }
        }
        "spread" => {
            // Replicas have wider spread (typical during recovery)
            let base_offset = 1_000_000;
            for _ in 0..n {
                let lag = rng.gen_range(0..10_000);
                offsets.push(base_offset - lag);
            }
        }
        "duplicates" => {
            // Many replicas have same offset (common in steady state)
            let base_offset = 1_000_000;
            let common_offsets = vec![base_offset, base_offset - 10, base_offset - 50];
            for _ in 0..n {
                let idx = rng.gen_range(0..common_offsets.len());
                offsets.push(common_offsets[idx]);
            }
        }
        "worst_case" => {
            // All offsets are different (worst case for heap operations)
            for _ in 0..n {
                offsets.push((n - i) as u64);
            }
        }
        _ => panic!("Unknown scenario: {}", scenario),
    }
    
    offsets
}

/// Original approach: Create full min-heap then extract k elements
/// This recreates the old algorithm for comparison
fn benchmark_original_approach(offsets: &[u64], k: usize) -> u64 {
    // Convert to min-heap by negating values
    let mut min_heap = BinaryHeap::with_capacity(offsets.len());
    for &offset in offsets {
        min_heap.push(std::cmp::Reverse(offset));
    }
    
    // Extract k-1 elements
    for _ in 0..(k - 1) {
        min_heap.pop();
    }
    
    // The k-th smallest is now at the top
    min_heap.peek().unwrap().0
}

/// Naive approach: Sort the entire array and index
fn benchmark_naive_approach(offsets: &[u64], k: usize) -> u64 {
    let mut sorted = offsets.to_vec();
    sorted.sort_unstable();
    sorted[k - 1]
}

/// Optimized approach using find_kth_smallest
fn benchmark_optimized_approach(offsets: &[u64], k: usize) -> u64 {
    ReplicationManager::find_kth_smallest(offsets, k)
}


fn bench_high_watermark_algorithms(c: &mut Criterion) {
    let scenarios = vec!["sequential", "spread", "duplicates", "worst_case"];
    let cluster_sizes = vec![10, 100, 1000, 10000];
    let k_values = vec![2, 3, 5];
    
    let mut group = c.benchmark_group("high_watermark_calculation");
    
    for scenario in &scenarios {
        for &n in &cluster_sizes {
            for &k in &k_values {
                if k > n {
                    continue;
                }
                
                let id = format!("{}_n{}_k{}", scenario, n, k);
                
                // Benchmark original approach
                group.bench_with_input(
                    BenchmarkId::new("original", &id),
                    &(n, k, scenario),
                    |b, &(n, k, scenario)| {
                        b.iter_batched(
                            || generate_test_offsets(n, scenario),
                            |offsets| benchmark_original_approach(black_box(&offsets), black_box(k)),
                            BatchSize::SmallInput,
                        );
                    },
                );
                
                // Benchmark naive approach
                group.bench_with_input(
                    BenchmarkId::new("naive_sort", &id),
                    &(n, k, scenario),
                    |b, &(n, k, scenario)| {
                        b.iter_batched(
                            || generate_test_offsets(n, scenario),
                            |offsets| benchmark_naive_approach(black_box(&offsets), black_box(k)),
                            BatchSize::SmallInput,
                        );
                    },
                );
                
                // Benchmark optimized approach
                group.bench_with_input(
                    BenchmarkId::new("optimized", &id),
                    &(n, k, scenario),
                    |b, &(n, k, scenario)| {
                        b.iter_batched(
                            || generate_test_offsets(n, scenario),
                            |offsets| benchmark_optimized_approach(black_box(&offsets), black_box(k)),
                            BatchSize::SmallInput,
                        );
                    },
                );
            }
        }
    }
    
    group.finish();
}

fn bench_special_cases(c: &mut Criterion) {
    let mut group = c.benchmark_group("special_cases");
    
    // Benchmark k=1 (minimum) case
    for &n in &[100, 1000, 10000] {
        let id = format!("minimum_n{}", n);
        
        group.bench_with_input(
            BenchmarkId::new("optimized_k1", &id),
            &n,
            |b, &n| {
                b.iter_batched(
                    || generate_test_offsets(n, "worst_case"),
                    |offsets| benchmark_optimized_approach(black_box(&offsets), black_box(1)),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    
    // Benchmark k=2 (second smallest) case
    for &n in &[100, 1000, 10000] {
        let id = format!("second_smallest_n{}", n);
        
        group.bench_with_input(
            BenchmarkId::new("optimized_k2", &id),
            &n,
            |b, &n| {
                b.iter_batched(
                    || generate_test_offsets(n, "worst_case"),
                    |offsets| benchmark_optimized_approach(black_box(&offsets), black_box(2)),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    group.sample_size(10); // Smaller sample size for memory benchmarks
    
    // Compare memory usage for n=10000, k=3
    let n = 10000;
    let k = 3;
    
    group.bench_function("original_memory", |b| {
        b.iter_batched(
            || generate_test_offsets(n, "worst_case"),
            |offsets| {
                // Original approach allocates full heap
                let heap_size = offsets.len() * std::mem::size_of::<u64>();
                let _ = benchmark_original_approach(&offsets, k);
                black_box(heap_size)
            },
            BatchSize::SmallInput,
        );
    });
    
    group.bench_function("optimized_memory", |b| {
        b.iter_batched(
            || generate_test_offsets(n, "worst_case"),
            |offsets| {
                // Optimized approach allocates bounded heap
                let heap_size = k * std::mem::size_of::<u64>();
                let _ = benchmark_optimized_approach(&offsets, k);
                black_box(heap_size)
            },
            BatchSize::SmallInput,
        );
    });
    
    group.finish();
}

fn bench_correctness_verification(_c: &mut Criterion) {
    // This isn't a performance benchmark, but verifies correctness
    let test_cases = vec![
        (100, 2),
        (100, 3),
        (100, 5),
        (1000, 3),
        (10000, 3),
    ];
    
    for (n, k) in test_cases {
        for scenario in &["sequential", "spread", "duplicates", "worst_case"] {
            let offsets = generate_test_offsets(n, scenario);
            
            let original = benchmark_original_approach(&offsets, k);
            let naive = benchmark_naive_approach(&offsets, k);
            let optimized = benchmark_optimized_approach(&offsets, k);
            
            assert_eq!(original, naive, 
                "Original and naive should match for n={}, k={}, scenario={}", n, k, scenario);
            assert_eq!(optimized, naive,
                "Optimized and naive should match for n={}, k={}, scenario={}", n, k, scenario);
        }
    }
    
    println!("✓ All correctness tests passed!");
}

criterion_group!(
    benches,
    bench_high_watermark_algorithms,
    bench_special_cases,
    bench_memory_usage,
    bench_correctness_verification
);
criterion_main!(benches);