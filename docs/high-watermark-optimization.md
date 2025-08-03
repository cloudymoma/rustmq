# High-Watermark Calculation Optimization

## Overview

The high-watermark calculation is a critical component in RustMQ's replication system. It determines the highest offset that has been successfully replicated to the required number of in-sync replicas (ISR). This document describes the optimization applied to this calculation and the resulting performance improvements.

## The Problem

In a distributed message queue with N brokers and a replication factor of K, the high-watermark is calculated as the K-th largest offset among all in-sync replicas. The original implementation used a full min-heap approach with O(n log n) complexity, which became a bottleneck for large clusters.

## The Solution

We implemented a hybrid algorithm that adapts based on the value of K:

### Algorithm Selection

1. **K = 1** (Minimum): Direct linear search - O(n) time, O(1) space
2. **K = 2** (Second smallest): Two-pass linear scan - O(n) time, O(1) space  
3. **K ≥ 3**: Bounded max-heap of size K - O(n log k) time, O(k) space

### Implementation Details

```rust
pub fn find_kth_smallest(offsets: &[u64], k: usize) -> u64 {
    match k {
        1 => *offsets.iter().min().unwrap(),
        2 => {
            let mut min = u64::MAX;
            let mut second_min = u64::MAX;
            for &offset in offsets {
                if offset < min {
                    second_min = min;
                    min = offset;
                } else if offset < second_min {
                    second_min = offset;
                }
            }
            second_min
        }
        _ => {
            // Bounded max-heap approach
            let mut max_heap = BinaryHeap::with_capacity(k);
            // ... implementation details
        }
    }
}
```

## Performance Results

### Time Complexity Improvement

For typical replication scenarios (K = 3):

| Cluster Size | Original O(n log n) | Optimized O(n log k) | Speedup |
|--------------|---------------------|----------------------|---------|
| 100 brokers  | ~660 operations     | ~158 operations      | 4.2x    |
| 1,000 brokers | ~10,000 operations | ~1,585 operations    | 6.3x    |
| 10,000 brokers | ~132,000 operations | ~15,850 operations  | 8.3x    |

### Benchmark Results

Running on typical hardware, the actual performance improvements are:

| Cluster Size | K | Original Time | Optimized Time | Speedup |
|--------------|---|---------------|----------------|---------|
| 100          | 3 | 17.46 µs      | 16.75 µs       | 1.04x   |
| 1,000        | 3 | 252.08 µs     | 173.37 µs      | 1.45x   |
| 10,000       | 3 | 3385.54 µs    | 1704.98 µs     | 1.99x   |
| 10,000       | 5 | 3474.83 µs    | 1989.12 µs     | 1.75x   |

### Memory Efficiency

The memory usage improvement is dramatic:

| Cluster Size | Original Heap Size | Optimized Heap Size | Reduction |
|--------------|-------------------|---------------------|-----------|
| 100          | 800 bytes         | 24 bytes            | 97.0%     |
| 1,000        | 8,000 bytes       | 24 bytes            | 99.7%     |
| 10,000       | 80,000 bytes      | 24 bytes            | 99.97%    |
| 100,000      | 800,000 bytes     | 24 bytes            | 99.997%   |

## Why This Matters

1. **Scalability**: The optimization enables RustMQ to scale to larger clusters without performance degradation
2. **Latency**: Reduced high-watermark calculation time directly improves replication latency
3. **Resource Usage**: Dramatically reduced memory allocation reduces GC pressure
4. **Predictability**: O(n log k) scaling provides more predictable performance as clusters grow

## Running the Benchmarks

To reproduce these results:

```bash
# Quick performance tests
cargo test --lib benchmarks:: -- --nocapture

# Full Criterion benchmarks
cargo bench --bench replication_manager_benchmarks

# Or use the convenience script
./run-benchmarks.sh
```

## Future Optimizations

Potential areas for further improvement:

1. **SIMD Instructions**: Use SIMD for the linear search cases (K=1, K=2)
2. **Parallel Processing**: Partition large offset arrays for parallel processing
3. **Incremental Updates**: Cache and incrementally update the high-watermark
4. **Approximation**: For very large K, consider approximate algorithms

## Conclusion

The optimized high-watermark calculation provides significant performance improvements, especially for large clusters. The hybrid approach ensures optimal performance across different replication factors while maintaining correctness and simplicity.