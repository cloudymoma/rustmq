use crate::{Result, storage::traits::Cache, config::CacheConfig};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

#[derive(Debug, Clone)]
struct CacheEntry {
    data: Bytes,
    last_accessed: Instant,
    access_count: u64,
}

pub struct LruCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    order: Arc<RwLock<VecDeque<String>>>,
    max_size_bytes: u64,
    current_size: Arc<parking_lot::Mutex<u64>>,
}

impl LruCache {
    pub fn new(max_size_bytes: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            order: Arc::new(RwLock::new(VecDeque::new())),
            max_size_bytes,
            current_size: Arc::new(parking_lot::Mutex::new(0)),
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

#[async_trait]
impl Cache for LruCache {
    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        let mut entries = self.entries.write();
        let mut order = self.order.write();

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
        let entry_size = value.len() as u64;
        self.evict_if_needed(entry_size)?;

        let mut entries = self.entries.write();
        let mut order = self.order.write();
        let mut current_size = self.current_size.lock();

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
        let mut entries = self.entries.write();
        let mut current_size = self.current_size.lock();

        if let Some(entry) = entries.remove(key) {
            *current_size -= entry.data.len() as u64;
        }

        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        let mut entries = self.entries.write();
        let mut current_size = self.current_size.lock();

        entries.clear();
        *current_size = 0;

        Ok(())
    }

    async fn size(&self) -> Result<usize> {
        let entries = self.entries.read();
        Ok(entries.len())
    }
}

pub struct CacheManager {
    write_cache: Arc<dyn Cache>,
    read_cache: Arc<dyn Cache>,
}

impl CacheManager {
    pub fn new(config: &CacheConfig) -> Self {
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
}