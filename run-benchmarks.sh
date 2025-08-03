#!/bin/bash
set -e

echo "=== RustMQ High-Watermark Calculation Benchmarks ==="
echo ""
echo "Running quick performance tests..."
echo ""

# Run the quick unit test benchmarks
cargo test --lib benchmarks:: -- --nocapture 2>/dev/null | grep -A 50 "==="

echo ""
echo "For detailed Criterion benchmarks, run:"
echo "  cargo bench --bench replication_manager_benchmarks"
echo ""
echo "Benchmark results will be saved to target/criterion/"