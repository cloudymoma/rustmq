//! Ultra-Fast Authentication and Authorization System
//!
//! This module implements high-performance authentication and authorization
//! optimizations targeting sub-100ns authorization latency. It includes:
//!
//! - Lock-free L2 cache using RCU (Read-Copy-Update) semantics
//! - Thread-local L1 cache with zero contention
//! - SIMD-optimized permission evaluation
//! - Cache-aligned data structures for optimal CPU cache performance
//! - Hardware-accelerated cryptographic operations
//!
//! ## Performance Targets
//!
//! - Total authorization latency: <100ns (90th percentile)
//! - L1 cache hit: <5ns
//! - L2 cache hit: <25ns
//! - Throughput: >10M operations/second/core
//! - Memory usage: 50% reduction vs baseline

pub mod lockfree_cache;
pub mod thread_local_cache;
pub mod simd_evaluator;
pub mod compact_encoding;
pub mod metrics;

#[cfg(test)]
pub mod tests;

use crate::error::{Result, RustMqError};
use crate::security::{SecurityMetrics, AclConfig};
use crate::security::auth::{AclKey, Permission, AuthContext};

use std::sync::Arc;
use std::time::Instant;

pub use lockfree_cache::LockFreeAuthCache;
pub use thread_local_cache::ThreadLocalAuthCache;
pub use simd_evaluator::VectorizedPermissionEvaluator;
pub use compact_encoding::{CompactAuthEntry, CompactPermissionSet};

/// Ultra-fast authorization system targeting sub-100ns latency
pub struct UltraFastAuthSystem {
    /// Lock-free L2 cache with RCU semantics
    l2_cache: LockFreeAuthCache,
    
    /// Thread-local L1 cache factory
    l1_factory: ThreadLocalCacheFactory,
    
    /// SIMD-optimized permission evaluator
    evaluator: VectorizedPermissionEvaluator,
    
    /// Performance metrics integration
    metrics: Arc<SecurityMetrics>,
    
    /// Configuration
    config: UltraFastConfig,
}

/// Configuration for ultra-fast auth system
#[derive(Debug, Clone)]
pub struct UltraFastConfig {
    /// Enable ultra-fast optimizations
    pub enabled: bool,
    
    /// L1 cache size per thread
    pub l1_cache_size: usize,
    
    /// L2 cache size in MB
    pub l2_cache_size_mb: usize,
    
    /// Enable SIMD optimizations
    pub simd_enabled: bool,
    
    /// Enable cache prefetching
    pub prefetch_enabled: bool,
    
    /// SIMD batch size
    pub batch_size: usize,
    
    /// Performance targets for validation
    pub performance_targets: PerformanceTargets,
}

impl Default for UltraFastConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            l1_cache_size: 1024,
            l2_cache_size_mb: 100,
            simd_enabled: cfg!(target_feature = "avx2"),
            prefetch_enabled: true,
            batch_size: 8,
            performance_targets: PerformanceTargets::default(),
        }
    }
}

/// Performance targets for validation
#[derive(Debug, Clone)]
pub struct PerformanceTargets {
    /// Maximum L1 cache latency in nanoseconds
    pub max_l1_latency_ns: u64,
    
    /// Maximum L2 cache latency in nanoseconds
    pub max_l2_latency_ns: u64,
    
    /// Maximum total authorization latency in nanoseconds
    pub max_total_latency_ns: u64,
    
    /// Minimum throughput in operations per second
    pub min_throughput_ops_per_sec: u64,
}

impl Default for PerformanceTargets {
    fn default() -> Self {
        Self {
            max_l1_latency_ns: 5,
            max_l2_latency_ns: 25,
            max_total_latency_ns: 100,
            min_throughput_ops_per_sec: 10_000_000, // 10M ops/sec
        }
    }
}

/// Thread-local L1 cache factory
pub struct ThreadLocalCacheFactory {
    cache_size: usize,
    metrics: Arc<SecurityMetrics>,
}

impl ThreadLocalCacheFactory {
    pub fn new(cache_size: usize, metrics: Arc<SecurityMetrics>) -> Self {
        Self {
            cache_size,
            metrics,
        }
    }
    
    /// Get or create thread-local L1 cache
    pub fn get_cache(&self) -> &'static ThreadLocalAuthCache {
        ThreadLocalAuthCache::get_or_create(self.cache_size)
    }
}

/// Authorization result with performance metrics
#[derive(Debug, Clone)]
pub struct UltraFastAuthResult {
    /// Whether authorization was granted
    pub allowed: bool,
    
    /// Total latency in nanoseconds
    pub latency_ns: u64,
    
    /// Whether L1 cache was hit
    pub l1_hit: bool,
    
    /// Whether L2 cache was hit
    pub l2_hit: bool,
    
    /// Whether bloom filter was consulted
    pub bloom_checked: bool,
    
    /// Batch size used (for SIMD operations)
    pub batch_size: usize,
}

impl UltraFastAuthSystem {
    /// Create a new ultra-fast auth system
    pub async fn new(
        config: UltraFastConfig,
        acl_config: AclConfig,
        metrics: Arc<SecurityMetrics>,
    ) -> Result<Self> {
        let l2_cache = LockFreeAuthCache::new(
            config.l2_cache_size_mb * 1024 * 1024, // Convert MB to bytes
            metrics.clone(),
        )?;
        
        let l1_factory = ThreadLocalCacheFactory::new(
            config.l1_cache_size,
            metrics.clone(),
        );
        
        let evaluator = VectorizedPermissionEvaluator::new(
            config.simd_enabled,
            config.batch_size,
        )?;
        
        Ok(Self {
            l2_cache,
            l1_factory,
            evaluator,
            metrics,
            config,
        })
    }
    
    /// Ultra-fast authorization check (target: <100ns)
    #[inline(always)]
    pub fn authorize_fast(&self, key: &AclKey) -> UltraFastAuthResult {
        let start = Instant::now();
        
        // Phase 1: Check thread-local L1 cache (~5ns target)
        let l1_cache = self.l1_factory.get_cache();
        if let Some(allowed) = l1_cache.get_fast(key) {
            let latency = start.elapsed().as_nanos() as u64;
            self.metrics.record_l1_cache_hit();
            
            return UltraFastAuthResult {
                allowed,
                latency_ns: latency,
                l1_hit: true,
                l2_hit: false,
                bloom_checked: false,
                batch_size: 1,
            };
        }
        
        // Phase 2: Check lock-free L2 cache (~25ns target)
        if let Some(allowed) = self.l2_cache.get_fast(key) {
            let latency = start.elapsed().as_nanos() as u64;
            
            // Update L1 cache for future hits
            l1_cache.insert_fast(key.clone(), allowed);
            
            self.metrics.record_l2_cache_hit();
            
            return UltraFastAuthResult {
                allowed,
                latency_ns: latency,
                l1_hit: false,
                l2_hit: true,
                bloom_checked: false,
                batch_size: 1,
            };
        }
        
        // Phase 3: Fallback to full authorization
        // This would integrate with the existing authorization system
        let allowed = self.authorize_full_fallback(key);
        let latency = start.elapsed().as_nanos() as u64;
        
        // Cache the result in both L1 and L2
        l1_cache.insert_fast(key.clone(), allowed);
        self.l2_cache.insert_fast(key.clone(), allowed);
        
        self.metrics.record_cache_miss();
        
        UltraFastAuthResult {
            allowed,
            latency_ns: latency,
            l1_hit: false,
            l2_hit: false,
            bloom_checked: false,
            batch_size: 1,
        }
    }
    
    /// Batch authorization for SIMD optimization
    pub fn authorize_batch(&self, keys: &[AclKey]) -> Vec<UltraFastAuthResult> {
        if self.config.simd_enabled && keys.len() >= self.config.batch_size {
            self.authorize_batch_simd(keys)
        } else {
            // Fall back to individual processing
            keys.iter().map(|key| self.authorize_fast(key)).collect()
        }
    }
    
    /// SIMD-optimized batch authorization
    fn authorize_batch_simd(&self, keys: &[AclKey]) -> Vec<UltraFastAuthResult> {
        let batch_size = self.config.batch_size;
        let mut results = Vec::with_capacity(keys.len());
        
        for chunk in keys.chunks(batch_size) {
            let batch_results = self.evaluator.evaluate_batch(chunk);
            results.extend(batch_results);
        }
        
        results
    }
    
    /// Fallback to existing authorization system
    fn authorize_full_fallback(&self, key: &AclKey) -> bool {
        // This would integrate with the existing SecurityManager
        // For now, return a default value
        // TODO: Integrate with existing AuthorizationManager
        false
    }
    
    /// Get performance statistics
    pub fn get_performance_stats(&self) -> UltraFastPerformanceStats {
        UltraFastPerformanceStats {
            l1_hit_rate: self.l1_factory.get_cache().hit_rate(),
            l2_hit_rate: self.l2_cache.hit_rate(),
            average_latency_ns: 0, // TODO: Implement in SecurityMetrics
            throughput_ops_per_sec: 0, // TODO: Implement in SecurityMetrics
            memory_usage_bytes: self.l2_cache.memory_usage_bytes(),
        }
    }
    
    /// Validate performance against targets
    pub fn validate_performance(&self) -> Result<()> {
        let stats = self.get_performance_stats();
        let targets = &self.config.performance_targets;
        
        if stats.average_latency_ns > targets.max_total_latency_ns {
            return Err(RustMqError::Performance(format!(
                "Average latency {}ns exceeds target {}ns",
                stats.average_latency_ns,
                targets.max_total_latency_ns
            )));
        }
        
        if stats.throughput_ops_per_sec < targets.min_throughput_ops_per_sec {
            return Err(RustMqError::Performance(format!(
                "Throughput {} ops/sec below target {} ops/sec",
                stats.throughput_ops_per_sec,
                targets.min_throughput_ops_per_sec
            )));
        }
        
        Ok(())
    }
    
    /// Enable or disable ultra-fast optimizations at runtime
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }
    
    /// Check if ultra-fast optimizations are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Performance statistics for ultra-fast auth system
#[derive(Debug, Clone)]
pub struct UltraFastPerformanceStats {
    /// L1 cache hit rate (0.0 to 1.0)
    pub l1_hit_rate: f64,
    
    /// L2 cache hit rate (0.0 to 1.0)
    pub l2_hit_rate: f64,
    
    /// Average authorization latency in nanoseconds
    pub average_latency_ns: u64,
    
    /// Throughput in operations per second
    pub throughput_ops_per_sec: u64,
    
    /// Total memory usage in bytes
    pub memory_usage_bytes: usize,
}

/// Error type for performance validation failures
impl RustMqError {
    pub fn Performance(message: String) -> Self {
        RustMqError::Internal(format!("Performance validation failed: {}", message))
    }
}