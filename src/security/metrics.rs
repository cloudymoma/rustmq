//! Security Metrics Collection
//!
//! This module provides comprehensive metrics collection for RustMQ's security subsystem,
//! enabling monitoring of authentication, authorization, and cache performance.
//!
//! ## Metrics Categories
//!
//! - **Authentication Metrics**: mTLS handshake performance, certificate validation latency
//! - **Authorization Metrics**: Multi-level cache performance, ACL evaluation latency
//! - **Cache Metrics**: Hit rates, memory usage, eviction patterns across L1/L2/L3 caches
//! - **Security Events**: Failed attempts, certificate revocations, policy violations

use crate::error::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Comprehensive security metrics collector
pub struct SecurityMetrics {
    // Authentication metrics
    auth_success_count: AtomicU64,
    auth_failure_count: AtomicU64,
    auth_latency_sum_nanos: AtomicU64,
    auth_latency_count: AtomicU64,

    // Certificate metrics
    cert_validation_count: AtomicU64,
    cert_parse_latency_sum_nanos: AtomicU64,
    cert_cache_hits: AtomicU64,
    cert_cache_misses: AtomicU64,
    cert_revocations: AtomicU64,

    // Authorization metrics
    authz_success_count: AtomicU64,
    authz_failure_count: AtomicU64,
    authz_latency_sum_nanos: AtomicU64,
    authz_latency_count: AtomicU64,

    // L1 cache metrics (connection-local)
    l1_cache_hits: AtomicU64,
    l1_cache_misses: AtomicU64,

    // L2 cache metrics (broker-wide)
    l2_cache_hits: AtomicU64,
    l2_cache_misses: AtomicU64,
    l2_cache_expiries: AtomicU64,
    l2_cache_cleanups: AtomicU64,

    // L3 cache metrics (negative/bloom filter)
    l3_cache_hits: AtomicU64,
    l3_cache_inserts: AtomicU64,
    l3_cache_clears: AtomicU64,

    // Batch fetching metrics
    batch_fetch_count: AtomicU64,
    batch_fetch_size_sum: AtomicU64,
    batch_fetch_latency_sum_nanos: AtomicU64,

    // Security events
    principal_invalidations: AtomicU64,
    cache_invalidations: AtomicU64,
    security_violations: AtomicU64,

    // TLS/mTLS metrics
    mtls_handshake_count: AtomicU64,
    mtls_handshake_latency_sum_nanos: AtomicU64,
    mtls_handshake_failures: AtomicU64,

    // Connection metrics
    active_authenticated_connections: AtomicU64,
    connection_churn_count: AtomicU64,
}

impl SecurityMetrics {
    /// Create a new security metrics collector
    pub fn new() -> Result<Self> {
        Ok(Self {
            auth_success_count: AtomicU64::new(0),
            auth_failure_count: AtomicU64::new(0),
            auth_latency_sum_nanos: AtomicU64::new(0),
            auth_latency_count: AtomicU64::new(0),

            cert_validation_count: AtomicU64::new(0),
            cert_parse_latency_sum_nanos: AtomicU64::new(0),
            cert_cache_hits: AtomicU64::new(0),
            cert_cache_misses: AtomicU64::new(0),
            cert_revocations: AtomicU64::new(0),

            authz_success_count: AtomicU64::new(0),
            authz_failure_count: AtomicU64::new(0),
            authz_latency_sum_nanos: AtomicU64::new(0),
            authz_latency_count: AtomicU64::new(0),

            l1_cache_hits: AtomicU64::new(0),
            l1_cache_misses: AtomicU64::new(0),

            l2_cache_hits: AtomicU64::new(0),
            l2_cache_misses: AtomicU64::new(0),
            l2_cache_expiries: AtomicU64::new(0),
            l2_cache_cleanups: AtomicU64::new(0),

            l3_cache_hits: AtomicU64::new(0),
            l3_cache_inserts: AtomicU64::new(0),
            l3_cache_clears: AtomicU64::new(0),

            batch_fetch_count: AtomicU64::new(0),
            batch_fetch_size_sum: AtomicU64::new(0),
            batch_fetch_latency_sum_nanos: AtomicU64::new(0),

            principal_invalidations: AtomicU64::new(0),
            cache_invalidations: AtomicU64::new(0),
            security_violations: AtomicU64::new(0),

            mtls_handshake_count: AtomicU64::new(0),
            mtls_handshake_latency_sum_nanos: AtomicU64::new(0),
            mtls_handshake_failures: AtomicU64::new(0),

            active_authenticated_connections: AtomicU64::new(0),
            connection_churn_count: AtomicU64::new(0),
        })
    }

    // Authentication metrics

    pub fn record_authentication_success(&self, latency: Duration) {
        self.auth_success_count.fetch_add(1, Ordering::Relaxed);
        self.auth_latency_sum_nanos
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
        self.auth_latency_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_authentication_failure(&self, _reason: &str) {
        self.auth_failure_count.fetch_add(1, Ordering::Relaxed);
        self.security_violations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_certificate_validation(&self) {
        self.cert_validation_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_certificate_parse_latency(&self, latency: Duration) {
        self.cert_parse_latency_sum_nanos
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn record_certificate_cache_hit(&self) {
        self.cert_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_certificate_cache_miss(&self) {
        self.cert_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_certificate_revocation(&self) {
        self.cert_revocations.fetch_add(1, Ordering::Relaxed);
    }

    // Authorization metrics

    pub fn record_authorization_success(&self, latency: Duration) {
        self.authz_success_count.fetch_add(1, Ordering::Relaxed);
        self.record_authorization_latency(latency);
    }

    pub fn record_authorization_failure(&self, latency: Duration) {
        self.authz_failure_count.fetch_add(1, Ordering::Relaxed);
        self.record_authorization_latency(latency);
        self.security_violations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_authorization_latency(&self, latency: Duration) {
        self.authz_latency_sum_nanos
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
        self.authz_latency_count.fetch_add(1, Ordering::Relaxed);
    }

    // Cache metrics

    pub fn record_l1_cache_hit(&self) {
        self.l1_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_l1_cache_miss(&self) {
        self.l1_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_l2_cache_hit(&self) {
        self.l2_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_l2_cache_miss(&self) {
        self.l2_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_l2_cache_expiry(&self) {
        self.l2_cache_expiries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_cleanup(&self, cleaned_count: usize) {
        self.l2_cache_cleanups
            .fetch_add(cleaned_count as u64, Ordering::Relaxed);
    }

    pub fn record_negative_cache_hit(&self) {
        self.l3_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_negative_cache_insert(&self) {
        self.l3_cache_inserts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_negative_cache_clear(&self) {
        self.l3_cache_clears.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        // General cache miss counter
        self.l2_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    // Batch fetching metrics

    pub fn record_batch_fetch(&self, batch_size: usize) {
        self.batch_fetch_count.fetch_add(1, Ordering::Relaxed);
        self.batch_fetch_size_sum
            .fetch_add(batch_size as u64, Ordering::Relaxed);
    }

    pub fn record_batch_fetch_latency(&self, latency: Duration) {
        self.batch_fetch_latency_sum_nanos
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
    }

    // Cache invalidation metrics

    pub fn record_principal_invalidation(&self) {
        self.principal_invalidations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_invalidation(&self) {
        self.cache_invalidations.fetch_add(1, Ordering::Relaxed);
    }

    // mTLS metrics

    pub fn record_mtls_handshake_success(&self, latency: Duration) {
        self.mtls_handshake_count.fetch_add(1, Ordering::Relaxed);
        self.mtls_handshake_latency_sum_nanos
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn record_mtls_handshake_failure(&self) {
        self.mtls_handshake_failures.fetch_add(1, Ordering::Relaxed);
    }

    // Connection metrics

    pub fn record_connection_established(&self) {
        self.active_authenticated_connections
            .fetch_add(1, Ordering::Relaxed);
        self.connection_churn_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_connection_closed(&self) {
        self.active_authenticated_connections
            .fetch_sub(1, Ordering::Relaxed);
        self.connection_churn_count.fetch_add(1, Ordering::Relaxed);
    }

    // Computed metrics (getters)

    pub fn authentication_success_rate(&self) -> f64 {
        let success = self.auth_success_count.load(Ordering::Relaxed);
        let failure = self.auth_failure_count.load(Ordering::Relaxed);
        let total = success + failure;

        if total == 0 {
            1.0
        } else {
            success as f64 / total as f64
        }
    }

    pub fn authorization_success_rate(&self) -> f64 {
        let success = self.authz_success_count.load(Ordering::Relaxed);
        let failure = self.authz_failure_count.load(Ordering::Relaxed);
        let total = success + failure;

        if total == 0 {
            1.0
        } else {
            success as f64 / total as f64
        }
    }

    pub fn average_authentication_latency_nanos(&self) -> f64 {
        let sum = self.auth_latency_sum_nanos.load(Ordering::Relaxed);
        let count = self.auth_latency_count.load(Ordering::Relaxed);

        if count == 0 {
            0.0
        } else {
            sum as f64 / count as f64
        }
    }

    pub fn average_authorization_latency_nanos(&self) -> f64 {
        let sum = self.authz_latency_sum_nanos.load(Ordering::Relaxed);
        let count = self.authz_latency_count.load(Ordering::Relaxed);

        if count == 0 {
            0.0
        } else {
            sum as f64 / count as f64
        }
    }

    pub fn l1_cache_hit_rate(&self) -> f64 {
        let hits = self.l1_cache_hits.load(Ordering::Relaxed);
        let misses = self.l1_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    pub fn l2_cache_hit_rate(&self) -> f64 {
        let hits = self.l2_cache_hits.load(Ordering::Relaxed);
        let misses = self.l2_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    pub fn certificate_cache_hit_rate(&self) -> f64 {
        let hits = self.cert_cache_hits.load(Ordering::Relaxed);
        let misses = self.cert_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    pub fn average_batch_size(&self) -> f64 {
        let sum = self.batch_fetch_size_sum.load(Ordering::Relaxed);
        let count = self.batch_fetch_count.load(Ordering::Relaxed);

        if count == 0 {
            0.0
        } else {
            sum as f64 / count as f64
        }
    }

    pub fn average_batch_fetch_latency_nanos(&self) -> f64 {
        let sum = self.batch_fetch_latency_sum_nanos.load(Ordering::Relaxed);
        let count = self.batch_fetch_count.load(Ordering::Relaxed);

        if count == 0 {
            0.0
        } else {
            sum as f64 / count as f64
        }
    }

    pub fn average_mtls_handshake_latency_nanos(&self) -> f64 {
        let sum = self
            .mtls_handshake_latency_sum_nanos
            .load(Ordering::Relaxed);
        let count = self.mtls_handshake_count.load(Ordering::Relaxed);

        if count == 0 {
            0.0
        } else {
            sum as f64 / count as f64
        }
    }

    // Raw counter getters

    pub fn active_connections(&self) -> u64 {
        self.active_authenticated_connections
            .load(Ordering::Relaxed)
    }

    pub fn total_security_violations(&self) -> u64 {
        self.security_violations.load(Ordering::Relaxed)
    }

    pub fn total_certificate_revocations(&self) -> u64 {
        self.cert_revocations.load(Ordering::Relaxed)
    }

    pub fn total_cache_invalidations(&self) -> u64 {
        self.cache_invalidations.load(Ordering::Relaxed)
    }

    pub fn total_mtls_handshake_failures(&self) -> u64 {
        self.mtls_handshake_failures.load(Ordering::Relaxed)
    }

    /// Get a comprehensive snapshot of all security metrics
    pub fn snapshot(&self) -> SecurityMetricsSnapshot {
        SecurityMetricsSnapshot {
            // Authentication
            auth_success_count: self.auth_success_count.load(Ordering::Relaxed),
            auth_failure_count: self.auth_failure_count.load(Ordering::Relaxed),
            auth_success_rate: self.authentication_success_rate(),
            avg_auth_latency_nanos: self.average_authentication_latency_nanos(),

            // Authorization
            authz_success_count: self.authz_success_count.load(Ordering::Relaxed),
            authz_failure_count: self.authz_failure_count.load(Ordering::Relaxed),
            authz_success_rate: self.authorization_success_rate(),
            avg_authz_latency_nanos: self.average_authorization_latency_nanos(),

            // Cache performance
            l1_cache_hits: self.l1_cache_hits.load(Ordering::Relaxed),
            l1_cache_misses: self.l1_cache_misses.load(Ordering::Relaxed),
            l1_cache_hit_rate: self.l1_cache_hit_rate(),

            l2_cache_hits: self.l2_cache_hits.load(Ordering::Relaxed),
            l2_cache_misses: self.l2_cache_misses.load(Ordering::Relaxed),
            l2_cache_hit_rate: self.l2_cache_hit_rate(),

            l3_cache_hits: self.l3_cache_hits.load(Ordering::Relaxed),

            // Certificates
            cert_cache_hit_rate: self.certificate_cache_hit_rate(),
            cert_revocations: self.cert_revocations.load(Ordering::Relaxed),

            // Batch fetching
            batch_fetch_count: self.batch_fetch_count.load(Ordering::Relaxed),
            avg_batch_size: self.average_batch_size(),
            avg_batch_latency_nanos: self.average_batch_fetch_latency_nanos(),

            // Security events
            security_violations: self.security_violations.load(Ordering::Relaxed),
            cache_invalidations: self.cache_invalidations.load(Ordering::Relaxed),

            // Connections
            active_connections: self
                .active_authenticated_connections
                .load(Ordering::Relaxed),
            connection_churn: self.connection_churn_count.load(Ordering::Relaxed),

            // mTLS
            mtls_handshake_failures: self.mtls_handshake_failures.load(Ordering::Relaxed),
            avg_mtls_handshake_latency_nanos: self.average_mtls_handshake_latency_nanos(),
        }
    }
}

/// Snapshot of security metrics at a point in time
#[derive(Debug, Clone)]
pub struct SecurityMetricsSnapshot {
    // Authentication metrics
    pub auth_success_count: u64,
    pub auth_failure_count: u64,
    pub auth_success_rate: f64,
    pub avg_auth_latency_nanos: f64,

    // Authorization metrics
    pub authz_success_count: u64,
    pub authz_failure_count: u64,
    pub authz_success_rate: f64,
    pub avg_authz_latency_nanos: f64,

    // Cache metrics
    pub l1_cache_hits: u64,
    pub l1_cache_misses: u64,
    pub l1_cache_hit_rate: f64,

    pub l2_cache_hits: u64,
    pub l2_cache_misses: u64,
    pub l2_cache_hit_rate: f64,

    pub l3_cache_hits: u64,

    // Certificate metrics
    pub cert_cache_hit_rate: f64,
    pub cert_revocations: u64,

    // Batch fetching metrics
    pub batch_fetch_count: u64,
    pub avg_batch_size: f64,
    pub avg_batch_latency_nanos: f64,

    // Security events
    pub security_violations: u64,
    pub cache_invalidations: u64,

    // Connection metrics
    pub active_connections: u64,
    pub connection_churn: u64,

    // mTLS metrics
    pub mtls_handshake_failures: u64,
    pub avg_mtls_handshake_latency_nanos: f64,
}

impl SecurityMetricsSnapshot {
    /// Check if authorization latency meets performance targets
    pub fn meets_latency_targets(&self) -> bool {
        // Target: sub-100ns authorization latency
        self.avg_authz_latency_nanos < 100.0
    }

    /// Check if cache hit rates are healthy
    pub fn has_healthy_cache_performance(&self) -> bool {
        // Target: >=90% L1 hit rate, >=80% L2 hit rate
        self.l1_cache_hit_rate >= 0.9 && self.l2_cache_hit_rate >= 0.8
    }

    /// Check if security violation rate is acceptable
    pub fn has_acceptable_security_posture(&self) -> bool {
        let total_operations = self.auth_success_count
            + self.auth_failure_count
            + self.authz_success_count
            + self.authz_failure_count;

        if total_operations == 0 {
            return true;
        }

        // Target: <1% security violations
        let violation_rate = self.security_violations as f64 / total_operations as f64;
        violation_rate < 0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_basic_operations() {
        let metrics = SecurityMetrics::new().unwrap();

        // Test authentication metrics
        metrics.record_authentication_success(Duration::from_nanos(50));
        metrics.record_authentication_failure("test_failure");

        assert_eq!(metrics.authentication_success_rate(), 0.5);
        assert_eq!(metrics.average_authentication_latency_nanos(), 50.0);

        // Test cache metrics
        metrics.record_l1_cache_hit();
        metrics.record_l1_cache_miss();

        assert_eq!(metrics.l1_cache_hit_rate(), 0.5);
    }

    #[test]
    fn test_metrics_snapshot() {
        let metrics = SecurityMetrics::new().unwrap();

        metrics.record_authorization_success(Duration::from_nanos(75));

        // Record enough cache hits to exceed the 90% L1 and 80% L2 thresholds
        // Need >90% for L1 and >80% for L2
        for _ in 0..19 {
            metrics.record_l1_cache_hit();
            metrics.record_l2_cache_hit();
        }
        // Add one miss to get 95% hit rate (19/20 = 0.95)
        metrics.record_l1_cache_miss();
        metrics.record_l2_cache_miss();

        let snapshot = metrics.snapshot();

        assert!(snapshot.meets_latency_targets());
        assert!(
            snapshot.has_healthy_cache_performance(),
            "L1 hit rate: {}, L2 hit rate: {}",
            snapshot.l1_cache_hit_rate,
            snapshot.l2_cache_hit_rate
        );
        assert!(snapshot.has_acceptable_security_posture());
    }

    #[test]
    fn test_connection_metrics() {
        let metrics = SecurityMetrics::new().unwrap();

        metrics.record_connection_established();
        metrics.record_connection_established();
        assert_eq!(metrics.active_connections(), 2);

        metrics.record_connection_closed();
        assert_eq!(metrics.active_connections(), 1);
    }
}
