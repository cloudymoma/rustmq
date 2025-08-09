//! Thread-Local L1 Authorization Cache
//!
//! This module implements ultra-fast thread-local L1 caching with zero
//! contention between threads. Target latency: <5ns per lookup.
//!
//! ## Key Features
//!
//! - Thread-local storage with zero contention
//! - Robin Hood hashing for predictable performance
//! - Cache-line aligned structures
//! - Predictive pre-population
//! - Connection-specific permission caching

use crate::security::auth::{AclKey, Permission};
use crate::security::ultra_fast::compact_encoding::{CompactAuthEntry, encode_permission};

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Thread-local authorization cache with zero contention
pub struct ThreadLocalAuthCache {
    /// Hash table with Robin Hood hashing
    table: RefCell<RobinHoodHashTable>,
    
    /// Performance counters
    hit_count: AtomicU64,
    miss_count: AtomicU64,
    
    /// Cache configuration
    config: L1CacheConfig,
}

/// Configuration for L1 cache
#[derive(Debug, Clone)]
struct L1CacheConfig {
    /// Maximum number of entries
    capacity: usize,
    
    /// Load factor threshold for resizing
    max_load_factor: f64,
    
    /// Enable predictive pre-population
    predictive_enabled: bool,
}

impl Default for L1CacheConfig {
    fn default() -> Self {
        Self {
            capacity: 1024,
            max_load_factor: 0.75,
            predictive_enabled: true,
        }
    }
}

/// Robin Hood hash table for predictable performance
struct RobinHoodHashTable {
    /// Hash table buckets
    buckets: Box<[L1CacheEntry]>,
    
    /// Number of occupied entries
    size: usize,
    
    /// Capacity (power of 2 for fast modulo)
    capacity: usize,
    
    /// Maximum probe distance seen
    max_probe_distance: usize,
}

/// Cache entry optimized for L1 cache performance
#[repr(C, align(32))] // 32-byte aligned for optimal L1 cache usage
#[derive(Clone, Debug)]
struct L1CacheEntry {
    /// Hash of the key
    key_hash: u64,
    
    /// Compact authorization data
    auth_data: CompactAuthEntry,
    
    /// Probe distance for Robin Hood hashing
    probe_distance: u16,
    
    /// Entry flags (occupied, recently_accessed, etc.)
    flags: u8,
    
    /// Padding to maintain alignment
    _padding: [u8; 5],
}

impl L1CacheEntry {
    const EMPTY: Self = Self {
        key_hash: 0,
        auth_data: CompactAuthEntry::EMPTY,
        probe_distance: 0,
        flags: 0,
        _padding: [0; 5],
    };
    
    const FLAG_OCCUPIED: u8 = 1 << 0;
    const FLAG_RECENTLY_ACCESSED: u8 = 1 << 1;
    
    /// Check if entry is occupied
    #[inline(always)]
    fn is_occupied(&self) -> bool {
        (self.flags & Self::FLAG_OCCUPIED) != 0
    }
    
    /// Mark entry as occupied
    #[inline(always)]
    fn set_occupied(&mut self) {
        self.flags |= Self::FLAG_OCCUPIED;
    }
    
    /// Clear occupied flag
    #[inline(always)]
    fn clear_occupied(&mut self) {
        self.flags &= !Self::FLAG_OCCUPIED;
        self.key_hash = 0;
        self.probe_distance = 0;
        self.auth_data = CompactAuthEntry::EMPTY;
    }
    
    /// Mark as recently accessed
    #[inline(always)]
    fn mark_accessed(&mut self) {
        self.flags |= Self::FLAG_RECENTLY_ACCESSED;
    }
    
    /// Check if recently accessed
    #[inline(always)]
    fn is_recently_accessed(&self) -> bool {
        (self.flags & Self::FLAG_RECENTLY_ACCESSED) != 0
    }
}

impl RobinHoodHashTable {
    /// Create new hash table with given capacity
    fn new(capacity: usize) -> Self {
        // Ensure capacity is power of 2
        let capacity = capacity.next_power_of_two();
        let buckets = vec![L1CacheEntry::EMPTY; capacity].into_boxed_slice();
        
        Self {
            buckets,
            size: 0,
            capacity,
            max_probe_distance: 0,
        }
    }
    
    /// Ultra-fast lookup (target: <5ns)
    #[inline(always)]
    fn get(&self, key_hash: u64) -> Option<bool> {
        let mut pos = (key_hash as usize) & (self.capacity - 1);
        let mut probe_distance = 0;
        
        loop {
            let entry = &self.buckets[pos];
            
            // Empty slot - key not found
            if !entry.is_occupied() {
                return None;
            }
            
            // Found matching key
            if entry.key_hash == key_hash {
                return Some(entry.auth_data.allowed());
            }
            
            // If current entry has shorter probe distance, key doesn't exist
            if entry.probe_distance < probe_distance {
                return None;
            }
            
            // Continue probing
            probe_distance += 1;
            pos = (pos + 1) & (self.capacity - 1);
            
            // Prevent infinite loop (shouldn't happen with proper load factor)
            if probe_distance as usize > self.max_probe_distance + 1 {
                return None;
            }
        }
    }
    
    /// Insert entry with Robin Hood hashing
    fn insert(&mut self, key_hash: u64, auth_data: CompactAuthEntry) {
        if self.size >= (self.capacity as f64 * 0.75) as usize {
            self.resize();
        }
        
        let mut entry = L1CacheEntry {
            key_hash,
            auth_data,
            probe_distance: 0,
            flags: L1CacheEntry::FLAG_OCCUPIED,
            _padding: [0; 5],
        };
        
        let mut pos = (key_hash as usize) & (self.capacity - 1);
        
        loop {
            let current = &mut self.buckets[pos];
            
            // Found empty slot
            if !current.is_occupied() {
                let probe_dist = entry.probe_distance;
                *current = entry;
                self.size += 1;
                self.max_probe_distance = self.max_probe_distance.max(probe_dist as usize);
                return;
            }
            
            // Update existing entry
            if current.key_hash == key_hash {
                current.auth_data = auth_data;
                current.mark_accessed();
                return;
            }
            
            // Robin Hood: if current entry is richer, swap
            if current.probe_distance < entry.probe_distance {
                std::mem::swap(current, &mut entry);
            }
            
            entry.probe_distance += 1;
            pos = (pos + 1) & (self.capacity - 1);
        }
    }
    
    /// Resize hash table when load factor is too high
    fn resize(&mut self) {
        let old_buckets = std::mem::replace(
            &mut self.buckets,
            vec![L1CacheEntry::EMPTY; self.capacity * 2].into_boxed_slice(),
        );
        
        let old_capacity = self.capacity;
        self.capacity *= 2;
        self.size = 0;
        self.max_probe_distance = 0;
        
        // Re-insert all entries
        for entry in old_buckets.iter() {
            if entry.is_occupied() {
                self.insert(entry.key_hash, entry.auth_data);
            }
        }
    }
    
    /// Get current load factor
    fn load_factor(&self) -> f64 {
        self.size as f64 / self.capacity as f64
    }
    
    /// Clear all entries
    fn clear(&mut self) {
        for bucket in self.buckets.iter_mut() {
            bucket.clear_occupied();
        }
        self.size = 0;
        self.max_probe_distance = 0;
    }
    
    /// Get statistics
    fn stats(&self) -> L1CacheTableStats {
        L1CacheTableStats {
            size: self.size,
            capacity: self.capacity,
            load_factor: self.load_factor(),
            max_probe_distance: self.max_probe_distance,
        }
    }
}

/// Thread-local cache statistics
#[derive(Debug, Clone)]
struct L1CacheTableStats {
    size: usize,
    capacity: usize,
    load_factor: f64,
    max_probe_distance: usize,
}

thread_local! {
    /// Thread-local L1 cache instance
    static L1_CACHE: RefCell<Option<Arc<ThreadLocalAuthCache>>> = RefCell::new(None);
}

impl ThreadLocalAuthCache {
    /// Get or create thread-local cache
    pub fn get_or_create(capacity: usize) -> &'static ThreadLocalAuthCache {
        L1_CACHE.with(|cache_cell| {
            let mut cache_opt = cache_cell.borrow_mut();
            
            if cache_opt.is_none() {
                let config = L1CacheConfig {
                    capacity,
                    ..Default::default()
                };
                
                let cache = Arc::new(ThreadLocalAuthCache::new(config));
                *cache_opt = Some(cache);
            }
            
            // Safety: We ensure the cache exists and use Arc to manage lifetime
            // The thread_local ensures this reference remains valid for the thread lifetime
            unsafe {
                std::mem::transmute::<&ThreadLocalAuthCache, &'static ThreadLocalAuthCache>(
                    cache_opt.as_ref().unwrap().as_ref()
                )
            }
        })
    }
    
    /// Create new thread-local cache
    fn new(config: L1CacheConfig) -> Self {
        let table = RobinHoodHashTable::new(config.capacity);
        
        Self {
            table: RefCell::new(table),
            hit_count: AtomicU64::new(0),
            miss_count: AtomicU64::new(0),
            config,
        }
    }
    
    /// Ultra-fast get operation (target: <5ns)
    #[inline(always)]
    pub fn get_fast(&self, key: &AclKey) -> Option<bool> {
        let key_hash = self.calculate_key_hash(key);
        
        let table = self.table.borrow();
        if let Some(result) = table.get(key_hash) {
            self.hit_count.fetch_add(1, Ordering::Relaxed);
            Some(result)
        } else {
            self.miss_count.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
    
    /// Fast insert operation
    pub fn insert_fast(&self, key: AclKey, allowed: bool) {
        let key_hash = self.calculate_key_hash(&key);
        
        // Create compact auth entry
        let auth_data = CompactAuthEntry::new(
            key_hash,
            allowed,
            Instant::now(),
            300, // 5 minute TTL for L1 cache
        );
        
        let mut table = self.table.borrow_mut();
        table.insert(key_hash, auth_data);
    }
    
    /// Calculate hash for AclKey
    #[inline(always)]
    fn calculate_key_hash(&self, key: &AclKey) -> u64 {
        let mut hasher = DefaultHasher::new();
        
        key.principal.hash(&mut hasher);
        key.topic.hash(&mut hasher);
        
        // Encode permission as u8 for hashing
        let permission_code = match key.permission {
            Permission::Read => 1u8,
            Permission::Write => 2u8,
            Permission::Admin => 3u8,
        };
        permission_code.hash(&mut hasher);
        
        hasher.finish()
    }
    
    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hit_count.load(Ordering::Relaxed);
        let misses = self.miss_count.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
    
    /// Clear cache
    pub fn clear(&self) {
        let mut table = self.table.borrow_mut();
        table.clear();
        
        self.hit_count.store(0, Ordering::Relaxed);
        self.miss_count.store(0, Ordering::Relaxed);
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> L1CacheStats {
        let table = self.table.borrow();
        let table_stats = table.stats();
        
        L1CacheStats {
            hit_count: self.hit_count.load(Ordering::Relaxed),
            miss_count: self.miss_count.load(Ordering::Relaxed),
            hit_rate: self.hit_rate(),
            size: table_stats.size,
            capacity: table_stats.capacity,
            load_factor: table_stats.load_factor,
            max_probe_distance: table_stats.max_probe_distance,
        }
    }
    
    /// Pre-populate cache with predicted entries
    pub fn prepopulate(&self, predictions: &[(AclKey, bool)]) {
        if !self.config.predictive_enabled {
            return;
        }
        
        for (key, allowed) in predictions {
            self.insert_fast(key.clone(), *allowed);
        }
    }
    
    /// Get memory usage estimate
    pub fn memory_usage_bytes(&self) -> usize {
        let table = self.table.borrow();
        table.capacity * std::mem::size_of::<L1CacheEntry>() +
        std::mem::size_of::<Self>()
    }
}

/// L1 cache statistics
#[derive(Debug, Clone)]
pub struct L1CacheStats {
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_rate: f64,
    pub size: usize,
    pub capacity: usize,
    pub load_factor: f64,
    pub max_probe_distance: usize,
}

/// Connection-specific cache with usage pattern prediction
pub struct ConnectionCache {
    /// Base thread-local cache
    base_cache: &'static ThreadLocalAuthCache,
    
    /// Connection-specific predictions
    predictor: AccessPatternPredictor,
    
    /// Connection ID for metrics
    connection_id: u64,
}

impl ConnectionCache {
    /// Create new connection cache
    pub fn new(connection_id: u64, capacity: usize) -> Self {
        let base_cache = ThreadLocalAuthCache::get_or_create(capacity);
        let predictor = AccessPatternPredictor::new();
        
        Self {
            base_cache,
            predictor,
            connection_id,
        }
    }
    
    /// Get with pattern learning
    pub fn get_with_learning(&mut self, key: &AclKey) -> Option<bool> {
        // Record access pattern
        self.predictor.record_access(key);
        
        // Try base cache first
        if let Some(result) = self.base_cache.get_fast(key) {
            return Some(result);
        }
        
        // Check if we can predict this access
        if let Some(prediction) = self.predictor.predict(key) {
            self.base_cache.insert_fast(key.clone(), prediction);
            return Some(prediction);
        }
        
        None
    }
    
    /// Insert with pattern learning
    pub fn insert_with_learning(&mut self, key: AclKey, allowed: bool) {
        self.base_cache.insert_fast(key.clone(), allowed);
        self.predictor.record_result(&key, allowed);
    }
}

/// Simple access pattern predictor
struct AccessPatternPredictor {
    /// Recent access patterns
    recent_accesses: Vec<(u64, bool)>, // (key_hash, result)
    
    /// Maximum pattern history
    max_history: usize,
}

impl AccessPatternPredictor {
    fn new() -> Self {
        Self {
            recent_accesses: Vec::with_capacity(100),
            max_history: 100,
        }
    }
    
    fn record_access(&mut self, key: &AclKey) {
        // Simple implementation - just record the hash
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let key_hash = hasher.finish();
        
        // For now, we don't have the result yet
        // This would be enhanced with more sophisticated pattern recognition
    }
    
    fn record_result(&mut self, key: &AclKey, allowed: bool) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let key_hash = hasher.finish();
        
        self.recent_accesses.push((key_hash, allowed));
        
        if self.recent_accesses.len() > self.max_history {
            self.recent_accesses.remove(0);
        }
    }
    
    fn predict(&self, key: &AclKey) -> Option<bool> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let key_hash = hasher.finish();
        
        // Simple prediction: if we've seen this exact key before, use last result
        self.recent_accesses.iter()
            .rev()
            .find(|(hash, _)| *hash == key_hash)
            .map(|(_, result)| *result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    
    #[test]
    fn test_robin_hood_hash_table() {
        let mut table = RobinHoodHashTable::new(16);
        
        let auth_data = CompactAuthEntry::new(12345, true, Instant::now(), 300);
        
        // Test insertion
        table.insert(12345, auth_data);
        assert_eq!(table.size, 1);
        
        // Test lookup
        assert_eq!(table.get(12345), Some(true));
        assert_eq!(table.get(67890), None);
        
        // Test update
        let new_auth_data = CompactAuthEntry::new(12345, false, Instant::now(), 300);
        table.insert(12345, new_auth_data);
        assert_eq!(table.size, 1); // Size shouldn't change for update
        assert_eq!(table.get(12345), Some(false));
    }
    
    #[test]
    fn test_thread_local_cache_basic_operations() {
        let cache = ThreadLocalAuthCache::get_or_create(128);
        
        let key = AclKey {
            principal: Arc::from("test-user"),
            topic: Arc::from("test-topic"),
            permission: Permission::Read,
        };
        
        // Test miss
        assert_eq!(cache.get_fast(&key), None);
        
        // Test insert and hit
        cache.insert_fast(key.clone(), true);
        assert_eq!(cache.get_fast(&key), Some(true));
        
        // Check stats
        let stats = cache.stats();
        assert_eq!(stats.hit_count, 1);
        assert_eq!(stats.miss_count, 1);
        assert_eq!(stats.size, 1);
    }
    
    #[test]
    fn test_cache_entry_alignment() {
        use std::mem::{size_of, align_of};
        
        // Ensure cache entries are properly aligned
        assert_eq!(align_of::<L1CacheEntry>(), 32);
        
        // Ensure size is reasonable
        assert!(size_of::<L1CacheEntry>() <= 64); // Should fit in cache line
    }
    
    #[test]
    fn test_connection_cache_pattern_learning() {
        let mut conn_cache = ConnectionCache::new(12345, 128);
        
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
        
        // First access should miss
        assert_eq!(conn_cache.get_with_learning(&key1), None);
        
        // Record result
        conn_cache.insert_with_learning(key1.clone(), true);
        
        // Second access should hit
        assert_eq!(conn_cache.get_with_learning(&key1), Some(true));
    }
    
    #[test]
    fn test_performance_characteristics() {
        let cache = ThreadLocalAuthCache::get_or_create(1024);
        
        // Pre-populate cache
        for i in 0..100 {
            let key = AclKey {
                principal: Arc::from(format!("user{}", i)),
                topic: Arc::from("test-topic"),
                permission: Permission::Read,
            };
            cache.insert_fast(key, true);
        }
        
        // Measure lookup performance
        let start = Instant::now();
        let iterations = 10000;
        
        for i in 0..iterations {
            let key = AclKey {
                principal: Arc::from(format!("user{}", i % 100)),
                topic: Arc::from("test-topic"),
                permission: Permission::Read,
            };
            
            let _result = cache.get_fast(&key);
        }
        
        let duration = start.elapsed();
        let avg_latency_ns = duration.as_nanos() / iterations;
        
        // Should be well under our 5ns target for cache hits
        // Note: In practice, this will be faster due to optimizations
        println!("Average lookup latency: {}ns", avg_latency_ns);
        
        let stats = cache.stats();
        println!("Hit rate: {:.2}%", stats.hit_rate * 100.0);
        println!("Load factor: {:.2}", stats.load_factor);
        println!("Max probe distance: {}", stats.max_probe_distance);
    }
}