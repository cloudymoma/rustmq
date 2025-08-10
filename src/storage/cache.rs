use crate::{Result, storage::traits::Cache, config::CacheConfig};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[cfg(feature = "moka-cache")]
use moka::future::Cache as MokaCache;
#[cfg(feature = "moka-cache")]
use std::time::Duration;

#[derive(Debug, Clone)]
struct CacheEntry {
    data: Bytes,
    last_accessed: Instant,
    access_count: u64,
}

/// Individual shard of the cache to reduce lock contention
struct LruCacheShard {
    entries: RwLock<HashMap<String, CacheEntry>>,
    order: RwLock<VecDeque<String>>,
    current_size: parking_lot::Mutex<u64>,
    max_size_bytes: u64,
}

pub struct LruCache {
    shards: Vec<Arc<LruCacheShard>>,
    shard_count: usize,
}

impl LruCacheShard {
    fn new(max_size_bytes: u64) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            order: RwLock::new(VecDeque::new()),
            current_size: parking_lot::Mutex::new(0),
            max_size_bytes,
        }
    }

    fn evict_if_needed(&self, new_entry_size: u64) -> Result<()> {
        let mut current_size = self.current_size.lock();
        let mut entries = self.entries.write();
        let mut order = self.order.write();

        while *current_size + new_entry_size > self.max_size_bytes {
            if let Some(key_to_evict) = order.pop_front() {
                if let Some(evicted_entry) = entries.remove(&key_to_evict) {
                    *current_size -= evicted_entry.data.len() as u64;
                }
            } else {
                break;
            }
        }
        Ok(())
    }
}

impl LruCache {
    pub fn new(max_size_bytes: u64) -> Self {
        const SHARD_COUNT: usize = 16; // Power of 2 for efficient modulo
        let size_per_shard = max_size_bytes / SHARD_COUNT as u64;
        
        let shards = (0..SHARD_COUNT)
            .map(|_| Arc::new(LruCacheShard::new(size_per_shard)))
            .collect();
        
        Self {
            shards,
            shard_count: SHARD_COUNT,
        }
    }

    fn get_shard(&self, key: &str) -> &Arc<LruCacheShard> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let shard_index = (hash as usize) % self.shard_count;
        &self.shards[shard_index]
    }
}

#[async_trait]
impl Cache for LruCache {
    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        let shard = self.get_shard(key);
        let mut entries = shard.entries.write();
        let mut order = shard.order.write();

        if let Some(entry) = entries.get_mut(key) {
            entry.last_accessed = Instant::now();
            entry.access_count += 1;

            // Move the accessed key to the back of the queue
            if let Some(pos) = order.iter().position(|x| x == key) {
                order.remove(pos);
            }
            order.push_back(key.to_string());

            Ok(Some(entry.data.clone()))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, key: &str, value: Bytes) -> Result<()> {
        let shard = self.get_shard(key);
        let entry_size = value.len() as u64;
        shard.evict_if_needed(entry_size)?;

        let mut entries = shard.entries.write();
        let mut order = shard.order.write();
        let mut current_size = shard.current_size.lock();

        if let Some(old_entry) = entries.get(key) {
            *current_size -= old_entry.data.len() as u64;
        } else {
            order.push_back(key.to_string());
        }

        let entry = CacheEntry {
            data: value,
            last_accessed: Instant::now(),
            access_count: 1,
        };

        *current_size += entry_size;
        entries.insert(key.to_string(), entry);

        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let shard = self.get_shard(key);
        let mut entries = shard.entries.write();
        let mut current_size = shard.current_size.lock();

        if let Some(entry) = entries.remove(key) {
            *current_size -= entry.data.len() as u64;
        }

        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        for shard in &self.shards {
            let mut entries = shard.entries.write();
            let mut current_size = shard.current_size.lock();
            let mut order = shard.order.write();

            entries.clear();
            order.clear();
            *current_size = 0;
        }

        Ok(())
    }

    async fn size(&self) -> Result<usize> {
        let mut total_size = 0;
        for shard in &self.shards {
            let entries = shard.entries.read();
            total_size += entries.len();
        }
        Ok(total_size)
    }
}

#[cfg(feature = "moka-cache")]
pub struct MokaCacheAdapter {
    inner: MokaCache<String, Bytes>,
}

#[cfg(feature = "moka-cache")]
impl MokaCacheAdapter {
    pub fn new(max_size_bytes: u64) -> Self {
        let cache = MokaCache::builder()
            .max_capacity(max_size_bytes / 1024) // Rough estimate: assume 1KB average entry size
            .weigher(|_key: &String, value: &Bytes| -> u32 {
                // Weight = key size + value size
                (std::mem::size_of::<String>() + _key.len() + value.len())
                    .try_into().unwrap_or(u32::MAX)
            })
            .time_to_live(Duration::from_secs(3600)) // 1 hour TTL
            .eviction_listener(|key, _value, cause| {
                tracing::debug!("Cache eviction: key={}, cause={:?}", key, cause);
            })
            .build();
        
        Self { inner: cache }
    }
}

#[cfg(feature = "moka-cache")]
#[async_trait]
impl Cache for MokaCacheAdapter {
    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        Ok(self.inner.get(key).await)
    }
    
    async fn put(&self, key: &str, value: Bytes) -> Result<()> {
        self.inner.insert(key.to_string(), value).await;
        Ok(())
    }
    
    async fn remove(&self, key: &str) -> Result<()> {
        self.inner.invalidate(key);
        Ok(())
    }
    
    async fn clear(&self) -> Result<()> {
        self.inner.invalidate_all();
        Ok(())
    }
    
    async fn size(&self) -> Result<usize> {
        Ok(self.inner.entry_count() as usize)
    }
}

pub struct CacheManager {
    write_cache: Arc<dyn Cache>,
    read_cache: Arc<dyn Cache>,
}

impl CacheManager {
    pub fn new(config: &CacheConfig) -> Self {
        // Determine cache implementation based on config and available features
        #[cfg(feature = "moka-cache")]
        let use_moka = matches!(config.eviction_policy, crate::config::EvictionPolicy::Moka | crate::config::EvictionPolicy::Lru);
        
        #[cfg(not(feature = "moka-cache"))]
        let use_moka = false;
        
        if use_moka {
            #[cfg(feature = "moka-cache")]
            {
                let write_cache = Arc::new(MokaCacheAdapter::new(config.write_cache_size_bytes));
                let read_cache = Arc::new(MokaCacheAdapter::new(config.read_cache_size_bytes));
                
                // Spawn maintenance tasks for both caches
                let write_cache_clone = write_cache.clone();
                let read_cache_clone = read_cache.clone();
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(60));
                    loop {
                        interval.tick().await;
                        write_cache_clone.inner.run_pending_tasks().await;
                        read_cache_clone.inner.run_pending_tasks().await;
                    }
                });
                
                return Self {
                    write_cache,
                    read_cache,
                };
            }
        }
        
        // Fallback to LRU cache
        Self {
            write_cache: Arc::new(LruCache::new(config.write_cache_size_bytes)),
            read_cache: Arc::new(LruCache::new(config.read_cache_size_bytes)),
        }
    }

    pub async fn serve_read(&self, key: &str) -> Result<Option<Bytes>> {
        if let Some(data) = self.write_cache.get(key).await? {
            return Ok(Some(data));
        }

        if let Some(data) = self.read_cache.get(key).await? {
            return Ok(Some(data));
        }

        Ok(None)
    }

    pub async fn cache_write(&self, key: &str, data: Bytes) -> Result<()> {
        self.write_cache.put(key, data).await
    }

    pub async fn cache_read(&self, key: &str, data: Bytes) -> Result<()> {
        self.read_cache.put(key, data).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lru_cache_basic_operations() {
        let cache = LruCache::new(1024);

        cache.put("key1", Bytes::from("value1")).await.unwrap();
        cache.put("key2", Bytes::from("value2")).await.unwrap();

        assert_eq!(cache.get("key1").await.unwrap().unwrap(), Bytes::from("value1"));
        assert_eq!(cache.get("key2").await.unwrap().unwrap(), Bytes::from("value2"));
        assert!(cache.get("key3").await.unwrap().is_none());

        cache.remove("key1").await.unwrap();
        assert!(cache.get("key1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = LruCache::new(20); // Small cache

        cache.put("key1", Bytes::from("value1")).await.unwrap(); // 6 bytes
        cache.put("key2", Bytes::from("value2")).await.unwrap(); // 6 bytes
        cache.put("key3", Bytes::from("value3_long")).await.unwrap(); // 11 bytes

        // Should evict oldest entries to make room
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        cache.put("key4", Bytes::from("new")).await.unwrap();

        assert!(cache.get("key1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cache_manager() {
        let config = CacheConfig {
            write_cache_size_bytes: 1024,
            read_cache_size_bytes: 1024,
            eviction_policy: crate::config::EvictionPolicy::Lru,
        };

        let manager = CacheManager::new(&config);

        manager.cache_write("write_key", Bytes::from("write_value")).await.unwrap();
        manager.cache_read("read_key", Bytes::from("read_value")).await.unwrap();

        assert_eq!(
            manager.serve_read("write_key").await.unwrap().unwrap(),
            Bytes::from("write_value")
        );
        assert_eq!(
            manager.serve_read("read_key").await.unwrap().unwrap(),
            Bytes::from("read_value")
        );
    }

    #[tokio::test]
    async fn test_sharded_cache_distribution() {
        let cache = LruCache::new(1024);

        // Add entries that should be distributed across shards
        let keys = vec!["key1", "key2", "key3", "key4", "key5", "key6", "key7", "key8"];
        for key in &keys {
            cache.put(key, Bytes::from(format!("value_{}", key))).await.unwrap();
        }

        // Verify all entries can be retrieved
        for key in &keys {
            let value = cache.get(key).await.unwrap().unwrap();
            assert_eq!(value, Bytes::from(format!("value_{}", key)));
        }

        // Verify total size
        assert_eq!(cache.size().await.unwrap(), keys.len());

        // Test concurrent access (this would show lock contention in the old implementation)
        let cache = Arc::new(cache);
        let mut handles = vec![];

        for i in 0..10 {
            let cache_clone = cache.clone();
            let handle = tokio::spawn(async move {
                for j in 0..100 {
                    let key = format!("concurrent_key_{}_{}", i, j);
                    let value = format!("concurrent_value_{}_{}", i, j);
                    cache_clone.put(&key, Bytes::from(value)).await.unwrap();
                    
                    // Immediate read to test read-write concurrency
                    let retrieved = cache_clone.get(&key).await.unwrap().unwrap();
                    assert_eq!(retrieved, Bytes::from(format!("concurrent_value_{}_{}", i, j)));
                }
            });
            handles.push(handle);
        }

        // Wait for all concurrent operations to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify the cache is still functional
        let final_size = cache.size().await.unwrap();
        assert!(final_size >= keys.len()); // Should include original keys plus concurrent ones
    }

    #[tokio::test]
    async fn test_cache_shard_independence() {
        let cache = LruCache::new(160); // Small cache to trigger evictions, 10 bytes per shard

        // Fill one shard with data
        cache.put("shard1_key1", Bytes::from("shard1_val1")).await.unwrap();
        cache.put("shard1_key2", Bytes::from("shard1_val2")).await.unwrap();
        
        // Add to different shard (different hash)
        cache.put("different_key", Bytes::from("different_val")).await.unwrap();
        
        // Verify both shards have data
        assert!(cache.get("shard1_key1").await.unwrap().is_some());
        assert!(cache.get("different_key").await.unwrap().is_some());
        
        // Clear cache
        cache.clear().await.unwrap();
        assert_eq!(cache.size().await.unwrap(), 0);
        
        // Verify all shards are cleared
        assert!(cache.get("shard1_key1").await.unwrap().is_none());
        assert!(cache.get("different_key").await.unwrap().is_none());
    }
}