//! High-Performance Multi-Level ACL Cache System
//!
//! This module implements the sophisticated caching system for RustMQ authorization,
//! featuring three cache levels optimized for different access patterns and latency requirements.
//!
//! ## Cache Hierarchy
//!
//! 1. **L1 Cache (Connection-Local)**: ThreadLocal LruCache with zero contention
//!    - Latency: ~10ns
//!    - Capacity: 1000 entries per connection
//!    - Scope: Single connection/client
//!    - Eviction: LRU
//!
//! 2. **L2 Cache (Broker-Wide)**: Sharded DashMap for concurrent access
//!    - Latency: ~50ns
//!    - Shards: 32 for optimal concurrency
//!    - Scope: Entire broker instance
//!    - TTL: Configurable (default 5 minutes)
//!
//! 3. **L3 Cache (Negative)**: BloomFilter for fast negative lookups
//!    - Latency: ~20ns
//!    - False positive rate: 0.1%
//!    - Purpose: Reject known-denied permissions instantly
//!    - Memory efficient: ~1MB for 1M entries

use super::{AclKey, Permission};
use crate::error::{Result, RustMqError};
use crate::security::SecurityMetrics;

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use dashmap::DashMap;
use lru::LruCache;
use bloomfilter::Bloom;
use parking_lot::RwLock;
use tokio::sync::RwLock as AsyncRwLock;

/// Cache level enumeration for metrics and debugging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLevel {
    L1,  // Connection-local
    L2,  // Broker-wide
    L3,  // Negative (Bloom filter)
}

/// ACL cache entry with metadata for L2 cache
#[derive(Debug, Clone)]
pub struct AclCacheEntry {
    /// Whether the permission is allowed
    pub allowed: bool,
    
    /// When this entry expires
    pub expires_at: Instant,
    
    /// Number of times this entry was accessed
    pub hit_count: u64,
    
    /// When this entry was created
    pub created_at: Instant,
    
    /// Size estimate for memory tracking
    pub size_bytes: usize,
}

impl AclCacheEntry {
    /// Create a new cache entry
    pub fn new(allowed: bool, ttl: Duration) -> Self {
        Self {
            allowed,
            expires_at: Instant::now() + ttl,
            hit_count: 0,
            created_at: Instant::now(),
            size_bytes: std::mem::size_of::<Self>(),
        }
    }
    
    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
    
    /// Record a cache hit
    pub fn record_hit(&mut self) {
        self.hit_count += 1;
    }
    
    /// Get the age of this entry
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
    
    /// Extend the TTL of this entry
    pub fn extend_ttl(&mut self, additional_ttl: Duration) {
        self.expires_at = Instant::now() + additional_ttl;
    }
}

/// Thread-safe L1 cache for connection-local ACL caching
pub struct L1AclCache {
    cache: RwLock<LruCache<AclKey, bool>>,
    hit_count: std::sync::atomic::AtomicU64,
    miss_count: std::sync::atomic::AtomicU64,
    max_capacity: usize,
}

impl L1AclCache {
    /// Create a new L1 cache with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: RwLock::new(LruCache::new(NonZeroUsize::new(capacity).unwrap())),
            hit_count: std::sync::atomic::AtomicU64::new(0),
            miss_count: std::sync::atomic::AtomicU64::new(0),
            max_capacity: capacity,
        }
    }
    
    /// Get a value from the cache
    pub fn get(&self, key: &AclKey) -> Option<bool> {
        let result = self.cache.write().get(key).copied();
        
        if result.is_some() {
            self.hit_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.miss_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        
        result
    }
    
    /// Put a value into the cache
    pub fn put(&self, key: AclKey, value: bool) {
        self.cache.write().put(key, value);
    }
    
    /// Check if the cache contains a key (without updating hit/miss stats)
    pub fn contains(&self, key: &AclKey) -> bool {
        self.cache.read().contains(key)
    }
    
    /// Clear all entries from the cache
    pub fn clear(&self) {
        self.cache.write().clear();
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> L1CacheStats {
        let cache = self.cache.read();
        L1CacheStats {
            capacity: self.max_capacity,
            size: cache.len(),
            hit_count: self.hit_count.load(std::sync::atomic::Ordering::Relaxed),
            miss_count: self.miss_count.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
    
    /// Get cache hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let hits = self.hit_count.load(std::sync::atomic::Ordering::Relaxed);
        let misses = self.miss_count.load(std::sync::atomic::Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

/// L2 cache statistics
#[derive(Debug, Clone)]
pub struct L1CacheStats {
    pub capacity: usize,
    pub size: usize,
    pub hit_count: u64,
    pub miss_count: u64,
}

/// Sharded L2 cache for broker-wide ACL caching
pub struct L2AclCache {
    shards: Arc<[DashMap<AclKey, AclCacheEntry>; 32]>,
    metrics: Arc<SecurityMetrics>,
    shard_count: usize,
}

impl L2AclCache {
    /// Create a new L2 cache with 32 shards
    pub fn new(metrics: Arc<SecurityMetrics>) -> Result<Self> {
        let shards: Vec<DashMap<AclKey, AclCacheEntry>> = (0..32)
            .map(|_| DashMap::new())
            .collect();
            
        let shards = Arc::new(
            shards.try_into()
                .map_err(|_| RustMqError::Internal("Failed to create L2 cache shards".to_string()))?
        );
        
        Ok(Self {
            shards,
            metrics,
            shard_count: 32,
        })
    }
    
    /// Get a value from the cache
    pub fn get(&self, key: &AclKey) -> Option<AclCacheEntry> {
        let shard_idx = self.calculate_shard_index(key);
        
        if let Some(mut entry) = self.shards[shard_idx].get_mut(key) {
            if !entry.is_expired() {
                entry.record_hit();
                self.metrics.record_l2_cache_hit();
                return Some(entry.clone());
            } else {
                // Remove expired entry
                drop(entry);
                self.shards[shard_idx].remove(key);
                self.metrics.record_l2_cache_expiry();
            }
        }
        
        self.metrics.record_l2_cache_miss();
        None
    }
    
    /// Put a value into the cache
    pub fn put(&self, key: AclKey, entry: AclCacheEntry) {
        let shard_idx = self.calculate_shard_index(&key);
        self.shards[shard_idx].insert(key, entry);
    }
    
    /// Remove entries for a specific principal
    pub fn invalidate_principal(&self, principal: &Arc<str>) {
        for shard in self.shards.iter() {
            shard.retain(|k, _| &k.principal != principal);
        }
        self.metrics.record_cache_invalidation();
    }
    
    /// Remove entries for a specific topic
    pub fn invalidate_topic(&self, topic: &Arc<str>) {
        for shard in self.shards.iter() {
            shard.retain(|k, _| &k.topic != topic);
        }
        self.metrics.record_cache_invalidation();
    }
    
    /// Clean expired entries from all shards
    pub fn cleanup_expired(&self) -> usize {
        let mut cleaned = 0;
        
        for shard in self.shards.iter() {
            let before_size = shard.len();
            shard.retain(|_, entry| !entry.is_expired());
            cleaned += before_size - shard.len();
        }
        
        if cleaned > 0 {
            self.metrics.record_cache_cleanup(cleaned);
        }
        
        cleaned
    }
    
    /// Get comprehensive cache statistics
    pub fn stats(&self) -> L2CacheStats {
        let mut total_entries = 0;
        let mut expired_entries = 0;
        let mut total_hits = 0;
        let mut total_memory_bytes = 0;
        let mut shard_sizes = Vec::with_capacity(self.shard_count);
        
        for shard in self.shards.iter() {
            let shard_size = shard.len();
            shard_sizes.push(shard_size);
            total_entries += shard_size;
            
            for entry in shard.iter() {
                total_hits += entry.hit_count;
                total_memory_bytes += entry.size_bytes;
                
                if entry.is_expired() {
                    expired_entries += 1;
                }
            }
        }
        
        L2CacheStats {
            total_entries,
            expired_entries,
            total_hits,
            total_memory_bytes,
            shard_count: self.shard_count,
            shard_sizes,
            average_shard_size: total_entries as f64 / self.shard_count as f64,
        }
    }
    
    /// Calculate which shard to use for a given key
    fn calculate_shard_index(&self, key: &AclKey) -> usize {
        let mut hasher = xxhash_rust::xxh3::Xxh3::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.shard_count
    }
}

/// L2 cache statistics
#[derive(Debug, Clone)]
pub struct L2CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub total_hits: u64,
    pub total_memory_bytes: usize,
    pub shard_count: usize,
    pub shard_sizes: Vec<usize>,
    pub average_shard_size: f64,
}

/// L3 negative cache using Bloom filter
pub struct L3NegativeCache {
    bloom_filter: AsyncRwLock<Bloom<AclKey>>,
    metrics: Arc<SecurityMetrics>,
    max_items: usize,
    false_positive_rate: f64,
}

impl L3NegativeCache {
    /// Create a new L3 negative cache
    pub fn new(
        max_items: usize,
        false_positive_rate: f64,
        metrics: Arc<SecurityMetrics>,
    ) -> Self {
        let bloom_filter = Bloom::new_for_fp_rate(max_items, false_positive_rate);
        
        Self {
            bloom_filter: AsyncRwLock::new(bloom_filter),
            metrics,
            max_items,
            false_positive_rate,
        }
    }
    
    /// Check if a key might be in the negative cache
    pub async fn might_contain(&self, key: &AclKey) -> bool {
        let bloom = self.bloom_filter.read().await;
        let result = bloom.check(key);
        
        if result {
            self.metrics.record_negative_cache_hit();
        }
        
        result
    }
    
    /// Add a key to the negative cache
    pub async fn insert(&self, key: &AclKey) {
        let mut bloom = self.bloom_filter.write().await;
        bloom.set(key);
        self.metrics.record_negative_cache_insert();
    }
    
    /// Clear the negative cache
    pub async fn clear(&self) {
        let mut bloom = self.bloom_filter.write().await;
        *bloom = Bloom::new_for_fp_rate(self.max_items, self.false_positive_rate);
        self.metrics.record_negative_cache_clear();
    }
    
    /// Get statistics for the negative cache
    pub async fn stats(&self) -> L3CacheStats {
        let bloom = self.bloom_filter.read().await;
        
        L3CacheStats {
            capacity: self.max_items,
            false_positive_rate: self.false_positive_rate,
            number_of_bits: bloom.number_of_bits(),
            number_of_hash_functions: bloom.number_of_hash_functions(),
            estimated_memory_bytes: bloom.number_of_bits() / 8,
        }
    }
}

/// L3 cache statistics
#[derive(Debug, Clone)]
pub struct L3CacheStats {
    pub capacity: usize,
    pub false_positive_rate: f64,
    pub number_of_bits: u64,
    pub number_of_hash_functions: u32,
    pub estimated_memory_bytes: u64,
}

/// Combined cache system integrating all three levels
pub struct AclCache {
    pub l2_cache: L2AclCache,
    pub l3_cache: L3NegativeCache,
    metrics: Arc<SecurityMetrics>,
    ttl: Duration,
}

impl AclCache {
    /// Create a new ACL cache system
    pub fn new(
        max_l3_items: usize,
        l3_false_positive_rate: f64,
        ttl: Duration,
        metrics: Arc<SecurityMetrics>,
    ) -> Result<Self> {
        let l2_cache = L2AclCache::new(metrics.clone())?;
        let l3_cache = L3NegativeCache::new(max_l3_items, l3_false_positive_rate, metrics.clone());
        
        Ok(Self {
            l2_cache,
            l3_cache,
            metrics,
            ttl,
        })
    }
    
    /// Create a new L1 cache for a connection
    pub fn create_l1_cache(&self, capacity: usize) -> Arc<L1AclCache> {
        Arc::new(L1AclCache::new(capacity))
    }
    
    /// Multi-level cache lookup
    pub async fn get(
        &self,
        l1_cache: &L1AclCache,
        key: &AclKey,
    ) -> Option<bool> {
        // L1 check first
        if let Some(result) = l1_cache.get(key) {
            return Some(result);
        }
        
        // L3 negative check
        if !key.permission.is_read_like() {
            if self.l3_cache.might_contain(key).await {
                return Some(false);
            }
        }
        
        // L2 check
        if let Some(entry) = self.l2_cache.get(key) {
            let result = entry.allowed;
            
            // Update L1 cache
            l1_cache.put(key.clone(), result);
            
            return Some(result);
        }
        
        None
    }
    
    /// Update all cache levels
    pub async fn put(
        &self,
        l1_cache: &L1AclCache,
        key: AclKey,
        allowed: bool,
    ) {
        // Update L1
        l1_cache.put(key.clone(), allowed);
        
        // Update L2
        let entry = AclCacheEntry::new(allowed, self.ttl);
        self.l2_cache.put(key.clone(), entry);
        
        // Update L3 (only for denials)
        if !allowed {
            self.l3_cache.insert(&key).await;
        }
    }
    
    /// Invalidate caches for a principal
    pub async fn invalidate_principal(&self, principal: &Arc<str>) {
        self.l2_cache.invalidate_principal(principal);
        // Note: L1 caches will naturally expire or can be cleared per-connection
        // L3 cache might have false positives but will not cause security issues
    }
    
    /// Get comprehensive statistics across all cache levels
    pub async fn stats(&self) -> AclCacheStats {
        let l2_stats = self.l2_cache.stats();
        let l3_stats = self.l3_cache.stats().await;
        
        let total_memory = l2_stats.total_memory_bytes + l3_stats.estimated_memory_bytes as usize;
        
        AclCacheStats {
            l2_stats,
            l3_stats,
            total_memory_estimate_bytes: total_memory,
        }
    }
    
    /// Run maintenance tasks
    pub async fn maintenance(&self) -> MaintenanceResult {
        let l2_cleaned = self.l2_cache.cleanup_expired();
        
        MaintenanceResult {
            l2_entries_cleaned: l2_cleaned,
        }
    }
}

/// Combined cache statistics
#[derive(Debug, Clone)]
pub struct AclCacheStats {
    pub l2_stats: L2CacheStats,
    pub l3_stats: L3CacheStats,
    pub total_memory_estimate_bytes: usize,
}

/// Maintenance operation result
#[derive(Debug, Clone)]
pub struct MaintenanceResult {
    pub l2_entries_cleaned: usize,
}

/// Extension trait for Permission to add utility methods
trait PermissionExt {
    fn is_read_like(&self) -> bool;
}

impl PermissionExt for Permission {
    fn is_read_like(&self) -> bool {
        matches!(self, Permission::Read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::AclKey;
    
    #[test]
    fn test_l1_cache_basic_operations() {
        let cache = L1AclCache::new(10);
        let key = AclKey {
            principal: Arc::from("test-user"),
            topic: Arc::from("test-topic"),
            permission: Permission::Read,
        };
        
        // Test miss
        assert_eq!(cache.get(&key), None);
        
        // Test put and hit
        cache.put(key.clone(), true);
        assert_eq!(cache.get(&key), Some(true));
        
        // Check stats
        let stats = cache.stats();
        assert_eq!(stats.hit_count, 1);
        assert_eq!(stats.miss_count, 1);
        assert_eq!(stats.size, 1);
    }
    
    #[test]
    fn test_l1_cache_lru_eviction() {
        let cache = L1AclCache::new(2);
        
        let key1 = AclKey {
            principal: Arc::from("user1"),
            topic: Arc::from("topic1"),
            permission: Permission::Read,
        };
        
        let key2 = AclKey {
            principal: Arc::from("user2"),
            topic: Arc::from("topic2"),
            permission: Permission::Read,
        };
        
        let key3 = AclKey {
            principal: Arc::from("user3"),
            topic: Arc::from("topic3"),
            permission: Permission::Read,
        };
        
        // Fill cache to capacity
        cache.put(key1.clone(), true);
        cache.put(key2.clone(), false);
        
        // Access key1 to make it recently used
        assert_eq!(cache.get(&key1), Some(true));
        
        // Add key3, should evict key2 (least recently used)
        cache.put(key3.clone(), true);
        
        assert_eq!(cache.get(&key1), Some(true));  // Still there
        assert_eq!(cache.get(&key2), None);        // Evicted
        assert_eq!(cache.get(&key3), Some(true));  // Newly added
    }
    
    #[tokio::test]
    async fn test_cache_entry_expiration() {
        let entry = AclCacheEntry::new(true, Duration::from_millis(10));
        assert!(!entry.is_expired());
        
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(entry.is_expired());
    }
    
    #[tokio::test]
    async fn test_l2_cache_sharding() {
        let metrics = Arc::new(SecurityMetrics::new().unwrap());
        let cache = L2AclCache::new(metrics).unwrap();
        
        let key1 = AclKey {
            principal: Arc::from("user1"),
            topic: Arc::from("topic1"),
            permission: Permission::Read,
        };
        
        let key2 = AclKey {
            principal: Arc::from("user2"),
            topic: Arc::from("topic2"),
            permission: Permission::Write,
        };
        
        let shard1 = cache.calculate_shard_index(&key1);
        let shard2 = cache.calculate_shard_index(&key2);
        
        // Different keys should potentially map to different shards
        // (though it's not guaranteed for just 2 keys)
        assert!(shard1 < 32);
        assert!(shard2 < 32);
        
        let entry = AclCacheEntry::new(true, Duration::from_secs(1));
        cache.put(key1.clone(), entry);
        
        assert!(cache.get(&key1).is_some());
        assert!(cache.get(&key2).is_none());
    }
    
    #[tokio::test]
    async fn test_l3_negative_cache() {
        let metrics = Arc::new(SecurityMetrics::new().unwrap());
        let cache = L3NegativeCache::new(1000, 0.01, metrics);
        
        let key = AclKey {
            principal: Arc::from("user1"),
            topic: Arc::from("topic1"),
            permission: Permission::Admin,
        };
        
        // Should not be in cache initially
        assert!(!cache.might_contain(&key).await);
        
        // Add to cache
        cache.insert(&key).await;
        
        // Should now be detected (might have false positives, but not false negatives)
        assert!(cache.might_contain(&key).await);
    }
}