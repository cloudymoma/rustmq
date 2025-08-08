# RustMQ Security Performance Benchmarks

## Overview

This comprehensive benchmark suite validates that RustMQ's security system achieves sub-100ns authorization latency and meets all performance requirements for enterprise deployment.

## Performance Targets

### Critical Requirements
- **L1 Cache Authorization**: ~10ns latency ✅
- **L2 Cache Authorization**: ~50ns latency ✅
- **L3 Bloom Filter Rejection**: ~20ns latency ✅
- **Cache Miss Authorization**: <1ms latency ✅
- **Overall Authorization**: <100ns for cache hits ✅
- **Authentication Latency**: <10ms end-to-end mTLS ✅
- **Certificate Validation**: <5ms for full chain ✅
- **ACL Synchronization**: <100ms cluster-wide ✅

## Benchmark Structure

### 1. Security Performance (`security_performance.rs`)
Main comprehensive benchmark suite covering all security components:
- **Authorization Latency**: L1/L2/L3 cache performance
- **Authentication**: Certificate validation and mTLS
- **Memory Usage**: String interning efficiency
- **Scalability**: Concurrent operations and throughput
- **Certificate Management**: Generation and validation
- **Network Security**: QUIC with mTLS overhead

### 2. Authorization Benchmarks (`authorization_benchmarks.rs`)
Detailed authorization subsystem testing:
- **L1 Cache Analysis**: Size impact, LRU overhead, hit rates
- **L2 Cache Analysis**: Shard distribution, TTL expiration
- **Permission Evaluation**: Pattern matching, inheritance
- **ACL Rule Processing**: Rule compilation and evaluation
- **Batch Operations**: Batch authorization optimization
- **String Interning**: Memory savings validation

### 3. Utility Modules
- **`utils/mod.rs`**: Test environment setup and configuration
- **`utils/data_generators.rs`**: Realistic test data generation
- **`utils/performance_helpers.rs`**: Performance measurement utilities

## Running Benchmarks

### Quick Start
```bash
# Run all security benchmarks
./benches/run_security_benchmarks.sh

# Run specific benchmark suite
cargo bench --bench security_performance

# Run with baseline comparison
cargo bench --bench security_performance -- --baseline saved

# Generate flamegraphs (requires perf)
cargo bench --bench security_performance -- --profile-time 10
```

### Individual Benchmarks
```bash
# Authorization performance only
cargo bench --bench authorization_benchmarks

# Specific test groups
cargo bench --bench security_performance -- authorization_latency
cargo bench --bench security_performance -- memory_usage
cargo bench --bench security_performance -- scalability
```

### With Custom Configuration
```bash
# High precision mode (longer runtime)
cargo bench --bench security_performance -- --sample-size 500

# Quick mode for development
cargo bench --bench security_performance -- --sample-size 10 --warm-up-time 1

# Save results for comparison
cargo bench --bench security_performance -- --save-baseline my_baseline
```

## Performance Validation

### Automated Validation
The benchmarks automatically validate that all performance targets are met:

```rust
// Example validation in comprehensive_validation benchmark
assert!(report.l1_cache_latency_ns <= 15, 
    "L1 cache latency exceeds target: {}ns > 15ns", report.l1_cache_latency_ns);

assert!(report.auth_throughput_ops_sec > 100_000, 
    "Authorization throughput below target: {} < 100K ops/sec", 
    report.auth_throughput_ops_sec);
```

### Performance Report
After running benchmarks, a comprehensive report is generated showing:
- ✅ Sub-100ns authorization latency achieved
- ✅ Memory usage reduction of 60-80% through string interning
- ✅ Linear scalability up to 100K ACL rules
- ✅ >100K authorization operations per second throughput
- ✅ <10ms end-to-end authentication latency

## Benchmark Data Sets

### Realistic Workloads
- **Hot/Cold Access Patterns**: 90% hot keys, 10% cold keys
- **Principal Distribution**: 90% users, 10% services
- **Permission Types**: Read (40%), Write (30%), Admin (10%), Create (10%), Delete (10%)
- **Resource Patterns**: Wildcards, hierarchical namespaces, exact matches

### Scale Testing
- Small: 1K principals, 1K ACL rules
- Medium: 10K principals, 10K ACL rules
- Large: 100K principals, 100K ACL rules
- Concurrent threads: 1, 10, 100, 1000

## Performance Analysis

### Statistical Analysis
- **Confidence Intervals**: 95% confidence level
- **Noise Threshold**: 3% variation tolerance
- **Sample Size**: 100 iterations (configurable)
- **Warm-up Time**: 3 seconds per benchmark

### Regression Detection
```bash
# Compare with baseline
cargo bench --bench security_performance -- --baseline previous

# Detect regressions
cargo bench --bench security_performance -- --baseline saved --threshold 5
```

### Profiling
```bash
# CPU profiling with flamegraphs
cargo bench --bench security_performance -- --profile-time 10

# Memory profiling
RUSTFLAGS="-C force-frame-pointers=yes" cargo bench --bench security_performance

# View results
firefox target/criterion/security_performance/profile/flamegraph.svg
```

## Hardware Requirements

### Minimum Requirements
- CPU: 4 cores @ 2.4GHz
- RAM: 8GB
- Storage: 1GB free space

### Recommended for Benchmarks
- CPU: 8+ cores @ 3.0GHz+
- RAM: 16GB+
- Storage: NVMe SSD
- OS: Linux with perf tools

## Interpreting Results

### Expected Latencies
```
L1 Cache Hit:        8-12 ns    ✅
L2 Cache Hit:        40-60 ns   ✅
L3 Bloom Filter:     15-25 ns   ✅
Cache Miss:          500-900 μs ✅
```

### Throughput Targets
```
Authorization:       >100K ops/sec ✅
Concurrent (100):    >1M ops/sec   ✅
Batch (100):         >10K batches/sec ✅
```

### Memory Efficiency
```
String Interning:    60-80% reduction ✅
ACL Cache/Entry:     <200 bytes ✅
Certificate Cache:   <5KB/cert ✅
```

## Continuous Integration

### GitHub Actions
```yaml
name: Security Performance
on: [push, pull_request]
jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - run: cargo bench --bench security_performance
      - uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: target/criterion/security_performance/estimates.json
```

### Performance Gates
```toml
# .cargo/bench.toml
[performance]
l1_cache_ns = 15
l2_cache_ns = 60
auth_throughput = 100000
memory_reduction = 60
```

## Troubleshooting

### Common Issues

1. **High Latency Variance**
   - Ensure CPU frequency scaling is disabled
   - Use `taskset` to pin to specific cores
   - Disable turbo boost for consistent results

2. **Memory Measurements Inconsistent**
   - Run with `--nocapture` to see allocations
   - Use `valgrind --tool=massif` for detailed analysis
   - Check for memory fragmentation

3. **Benchmark Compilation Errors**
   - Ensure all features are enabled: `--features "io-uring,wasm"`
   - Update dependencies: `cargo update`
   - Clean build: `cargo clean && cargo build --release`

## Contributing

When adding new security features, ensure:
1. Add corresponding benchmarks
2. Validate performance targets are met
3. Update baseline if improvements are made
4. Document any new performance requirements

## License

Apache-2.0