# RustMQ Performance Benchmarks

This directory contains comprehensive performance benchmarks for critical components of RustMQ, leveraging the `criterion` framework for precise measurements.

## Benchmark Suites

The benchmark suite covers the following critical paths:

### Core Infrastructure
- **WAL Performance (`wal_performance_bench.rs`)**: Measures Direct I/O throughput and latency.
- **Cache Performance (`cache_performance_bench.rs`)**: Evaluates LruCache vs Moka cache hit/miss rates and eviction efficiency.
- **Buffer Pooling (`buffer_pool_bench.rs`)**: Validates allocation reduction via `AlignedBufferPool`.
- **Zero-Copy Serialization (`zero_copy_benchmark.rs`)**: Measures the efficiency of the zero-copy message framing format.
- **Branchless Parsing (`branchless_parsing_bench.rs`)**: Tests the high-performance branchless WAL record decoder.

### Network & Concurrency
- **Connection Pooling (`connection_pool_bench.rs`)**: Evaluates the overhead of managing QUIC connections.
- **FuturesUnordered (`futures_unordered_bench.rs`)**: Measures parallel replication and chunked upload efficiency.
- **gRPC & Protobuf (`grpc_protobuf_benchmarks.rs`)**: Validates the serialization/deserialization overhead of control plane messages.

### Security (See `SECURITY_BENCHMARKS.md`)
- **Authorization (`authorization_benchmarks.rs`)**: Measures sub-microsecond ACL evaluation.
- **Certificate Validation (`certificate_validation_bench.rs`)**: Evaluates WebPKI validation and certificate caching.
- **Overall Security (`security_performance.rs`)**: Measures the end-to-end impact of the security layer on message throughput.

### Replication
- **High-Watermark Calculation (`replication_manager_benchmarks.rs`)**: Tests the optimized O(n log k) algorithm for tracking committed offsets across cluster sizes.

## Running the Benchmarks

To run the full suite of benchmarks using Criterion:

```bash
cargo bench
```

To run a specific benchmark:

```bash
cargo bench --bench <benchmark_name>
# Example: cargo bench --bench wal_performance_bench
```

## Interpreting Results

Criterion provides detailed statistical analysis including:
1. **Time per operation**: Average time in microseconds/nanoseconds with confidence intervals.
2. **Throughput**: Operations per second or Bytes per second.
3. **Regressions/Improvements**: Statistical comparison against the previous recorded baseline.

A Python utility `generate_performance_report.py` is available to parse Criterion JSON output into formatted reports.
