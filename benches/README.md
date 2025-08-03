# RustMQ Performance Benchmarks

This directory contains performance benchmarks for critical components of RustMQ.

## High-Watermark Calculation Benchmarks

The `replication_manager_benchmarks.rs` file contains comprehensive benchmarks for the optimized high-watermark calculation algorithm in the ReplicationManager.

### Running the Benchmarks

To run the full benchmark suite with Criterion:

```bash
cargo bench --bench replication_manager_benchmarks
```

To run the quick performance tests included in the unit tests:

```bash
cargo test --lib benchmarks:: -- --nocapture
```

### What's Being Measured

The benchmarks compare three approaches for finding the k-th smallest element (used in high-watermark calculation):

1. **Original Approach**: Build a full min-heap and extract k elements - O(n log n)
2. **Naive Approach**: Sort the entire array and index - O(n log n)
3. **Optimized Approach**: Hybrid algorithm that adapts based on k:
   - k=1: Direct minimum search - O(n)
   - k=2: Two-pass linear scan - O(n)
   - k≥3: Bounded max-heap - O(n log k)

### Benchmark Scenarios

The benchmarks test various realistic scenarios:

- **Cluster Sizes**: 10, 100, 1000, 10000 brokers
- **Replication Factors (k)**: 2, 3, 5
- **Data Distributions**:
  - Sequential: Small variance, typical steady state
  - Spread: Wide variance, typical during recovery
  - Duplicates: Many identical offsets, common in stable clusters
  - Worst Case: All unique offsets

### Expected Results

The optimized approach shows significant improvements:

- **~6x speedup** for typical cases (n=1000, k=3)
- **Memory reduction** from O(n) to O(k) heap size
- **Better scaling** for large clusters

### Memory Efficiency

For a cluster of 10,000 brokers with k=3:
- Original: 80,000 bytes heap allocation
- Optimized: 24 bytes heap allocation
- Reduction: 99.97%

### Performance Characteristics

The algorithm complexity improvements:
- Original: O(n log n) operations
- Optimized: O(n log k) operations for k≥3, O(n) for k≤2

For n=10,000 and k=3:
- Original: ~132,000 operations
- Optimized: ~15,850 operations
- Reduction: ~88%

### Interpreting Results

The benchmarks output:
1. **Time per operation**: Average time in microseconds
2. **Memory allocations**: Peak heap memory usage
3. **Throughput**: Operations per second
4. **Speedup factor**: How much faster the optimized approach is

The results demonstrate that the optimization is particularly effective for:
- Large clusters (n > 1000)
- Typical replication factors (k = 2-5)
- Real-world offset distributions