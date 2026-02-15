//! Ultra-Fast Authorization Metrics
//!
//! This module provides specialized metrics for tracking the performance
//! of the ultra-fast authorization system.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Metrics for ultra-fast authorization system
pub struct UltraFastMetrics {
    /// L1 cache hits
    l1_cache_hits: AtomicU64,

    /// L1 cache misses
    l1_cache_misses: AtomicU64,

    /// L2 cache hits
    l2_cache_hits: AtomicU64,

    /// L2 cache misses
    l2_cache_misses: AtomicU64,

    /// SIMD operations performed
    simd_operations: AtomicU64,

    /// Scalar operations performed
    scalar_operations: AtomicU64,

    /// Total authorization latency (nanoseconds)
    total_latency_ns: AtomicU64,

    /// Total operations performed
    total_operations: AtomicU64,

    /// Creation time for throughput calculation
    created_at: Instant,
}

impl UltraFastMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self {
            l1_cache_hits: AtomicU64::new(0),
            l1_cache_misses: AtomicU64::new(0),
            l2_cache_hits: AtomicU64::new(0),
            l2_cache_misses: AtomicU64::new(0),
            simd_operations: AtomicU64::new(0),
            scalar_operations: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            total_operations: AtomicU64::new(0),
            created_at: Instant::now(),
        }
    }

    /// Record L1 cache hit
    pub fn record_l1_hit(&self) {
        self.l1_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record L1 cache miss
    pub fn record_l1_miss(&self) {
        self.l1_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record L2 cache hit
    pub fn record_l2_hit(&self) {
        self.l2_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record L2 cache miss
    pub fn record_l2_miss(&self) {
        self.l2_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record SIMD operation
    pub fn record_simd_operation(&self, batch_size: usize) {
        self.simd_operations
            .fetch_add(batch_size as u64, Ordering::Relaxed);
    }

    /// Record scalar operation
    pub fn record_scalar_operation(&self) {
        self.scalar_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record authorization latency
    pub fn record_latency(&self, latency_ns: u64) {
        self.total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);
        self.total_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Get L1 cache hit rate
    pub fn l1_hit_rate(&self) -> f64 {
        let hits = self.l1_cache_hits.load(Ordering::Relaxed);
        let misses = self.l1_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get L2 cache hit rate
    pub fn l2_hit_rate(&self) -> f64 {
        let hits = self.l2_cache_hits.load(Ordering::Relaxed);
        let misses = self.l2_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get average latency in nanoseconds
    pub fn average_latency_ns(&self) -> u64 {
        let total_latency = self.total_latency_ns.load(Ordering::Relaxed);
        let total_ops = self.total_operations.load(Ordering::Relaxed);

        if total_ops == 0 {
            0
        } else {
            total_latency / total_ops
        }
    }

    /// Get throughput in operations per second
    pub fn throughput_ops_per_sec(&self) -> u64 {
        let total_ops = self.total_operations.load(Ordering::Relaxed);
        let duration = self.created_at.elapsed().as_secs_f64();

        if duration == 0.0 {
            0
        } else {
            (total_ops as f64 / duration) as u64
        }
    }

    /// Get SIMD utilization ratio
    pub fn simd_utilization(&self) -> f64 {
        let simd_ops = self.simd_operations.load(Ordering::Relaxed);
        let scalar_ops = self.scalar_operations.load(Ordering::Relaxed);
        let total = simd_ops + scalar_ops;

        if total == 0 {
            0.0
        } else {
            simd_ops as f64 / total as f64
        }
    }

    /// Get snapshot of all metrics
    pub fn snapshot(&self) -> UltraFastMetricsSnapshot {
        UltraFastMetricsSnapshot {
            l1_cache_hits: self.l1_cache_hits.load(Ordering::Relaxed),
            l1_cache_misses: self.l1_cache_misses.load(Ordering::Relaxed),
            l2_cache_hits: self.l2_cache_hits.load(Ordering::Relaxed),
            l2_cache_misses: self.l2_cache_misses.load(Ordering::Relaxed),
            simd_operations: self.simd_operations.load(Ordering::Relaxed),
            scalar_operations: self.scalar_operations.load(Ordering::Relaxed),
            total_latency_ns: self.total_latency_ns.load(Ordering::Relaxed),
            total_operations: self.total_operations.load(Ordering::Relaxed),
            l1_hit_rate: self.l1_hit_rate(),
            l2_hit_rate: self.l2_hit_rate(),
            average_latency_ns: self.average_latency_ns(),
            throughput_ops_per_sec: self.throughput_ops_per_sec(),
            simd_utilization: self.simd_utilization(),
        }
    }
}

impl Default for UltraFastMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of ultra-fast metrics at a point in time
#[derive(Debug, Clone)]
pub struct UltraFastMetricsSnapshot {
    pub l1_cache_hits: u64,
    pub l1_cache_misses: u64,
    pub l2_cache_hits: u64,
    pub l2_cache_misses: u64,
    pub simd_operations: u64,
    pub scalar_operations: u64,
    pub total_latency_ns: u64,
    pub total_operations: u64,
    pub l1_hit_rate: f64,
    pub l2_hit_rate: f64,
    pub average_latency_ns: u64,
    pub throughput_ops_per_sec: u64,
    pub simd_utilization: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_recording() {
        let metrics = UltraFastMetrics::new();

        // Record some operations
        metrics.record_l1_hit();
        metrics.record_l1_miss();
        metrics.record_l2_hit();
        metrics.record_latency(100);

        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.l1_cache_hits, 1);
        assert_eq!(snapshot.l1_cache_misses, 1);
        assert_eq!(snapshot.l2_cache_hits, 1);
        assert_eq!(snapshot.total_operations, 1);
        assert_eq!(snapshot.l1_hit_rate, 0.5);
    }

    #[test]
    fn test_hit_rate_calculation() {
        let metrics = UltraFastMetrics::new();

        // Record multiple hits and misses
        for _ in 0..8 {
            metrics.record_l1_hit();
        }
        for _ in 0..2 {
            metrics.record_l1_miss();
        }

        assert_eq!(metrics.l1_hit_rate(), 0.8);
    }

    #[test]
    fn test_simd_utilization() {
        let metrics = UltraFastMetrics::new();

        metrics.record_simd_operation(8); // 8 operations in SIMD batch
        metrics.record_scalar_operation();
        metrics.record_scalar_operation();

        // SIMD utilization should be 8/10 = 0.8
        assert_eq!(metrics.simd_utilization(), 0.8);
    }
}
