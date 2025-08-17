//! Miri memory safety tests for cache components
//! 
//! These tests focus on detecting memory safety issues in:
//! - Moka cache operations
//! - LRU cache operations  
//! - Cache eviction policies
//! - Concurrent cache access

#[cfg(miri)]
mod miri_cache_tests {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::runtime::Runtime;
    
    // Test both cache implementations
    #[cfg(feature = "moka-cache")]
    use rustmq::storage::cache::MokaCache;
    
    use rustmq::storage::cache::{Cache, LRUCache};

    #[test]
    fn test_lru_cache_memory_safety() {
        let cache = LRUCache::new(100); // 100 item capacity
        
        // Test basic operations
        cache.put("key1".to_string(), b"value1".to_vec());
        let value = cache.get("key1").unwrap();
        assert_eq!(value, b"value1");
        
        // Test overwrite
        cache.put("key1".to_string(), b"new_value1".to_vec());
        let value = cache.get("key1").unwrap();
        assert_eq!(value, b"new_value1");
        
        // Test eviction by filling cache
        for i in 0..150 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            cache.put(key, value);
        }
        
        // Verify cache size is maintained
        assert!(cache.len() <= 100);
        
        // Test that oldest items were evicted
        assert!(cache.get("key_0").is_none());
        assert!(cache.get("key_149").is_some());
    }

    #[test]
    fn test_lru_cache_concurrent_access() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let cache = Arc::new(LRUCache::new(50));
            
            // Test concurrent puts
            let mut put_handles = Vec::new();
            for i in 0..20 {
                let cache_clone = Arc::clone(&cache);
                let handle = tokio::spawn(async move {
                    for j in 0..5 {
                        let key = format!("concurrent_key_{}_{}", i, j);
                        let value = format!("concurrent_value_{}_{}", i, j).into_bytes();
                        cache_clone.put(key, value);
                        tokio::task::yield_now().await; // Allow other tasks to run
                    }
                });
                put_handles.push(handle);
            }
            
            futures::future::join_all(put_handles).await;
            
            // Test concurrent gets
            let mut get_handles = Vec::new();
            for i in 0..10 {
                let cache_clone = Arc::clone(&cache);
                let handle = tokio::spawn(async move {
                    for j in 0..5 {
                        let key = format!("concurrent_key_{}_{}", i, j);
                        if let Some(value) = cache_clone.get(&key) {
                            let expected = format!("concurrent_value_{}_{}", i, j).into_bytes();
                            assert_eq!(value, expected);
                        }
                        tokio::task::yield_now().await;
                    }
                });
                get_handles.push(handle);
            }
            
            futures::future::join_all(get_handles).await;
        });
    }

    #[cfg(feature = "moka-cache")]
    #[test]
    fn test_moka_cache_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let cache = MokaCache::new(100, Duration::from_secs(3600)).await; // 1 hour TTL
            
            // Test basic operations
            cache.put("moka_key1".to_string(), b"moka_value1".to_vec()).await;
            let value = cache.get("moka_key1").await.unwrap();
            assert_eq!(value, b"moka_value1");
            
            // Test overwrite
            cache.put("moka_key1".to_string(), b"new_moka_value1".to_vec()).await;
            let value = cache.get("moka_key1").await.unwrap();
            assert_eq!(value, b"new_moka_value1");
            
            // Test bulk operations
            for i in 0..50 {
                let key = format!("moka_bulk_key_{}", i);
                let value = format!("moka_bulk_value_{}", i).into_bytes();
                cache.put(key, value).await;
            }
            
            // Test retrieval
            for i in 0..50 {
                let key = format!("moka_bulk_key_{}", i);
                let value = cache.get(&key).await.unwrap();
                let expected = format!("moka_bulk_value_{}", i).into_bytes();
                assert_eq!(value, expected);
            }
        });
    }

    #[cfg(feature = "moka-cache")]
    #[test]
    fn test_moka_cache_concurrent_access() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let cache = Arc::new(MokaCache::new(100, Duration::from_secs(3600)).await);
            
            // Test concurrent puts and gets
            let mut handles = Vec::new();
            
            for i in 0..10 {
                let cache_clone = Arc::clone(&cache);
                let handle = tokio::spawn(async move {
                    // Put some data
                    for j in 0..10 {
                        let key = format!("moka_concurrent_{}_{}", i, j);
                        let value = format!("moka_concurrent_value_{}_{}", i, j).into_bytes();
                        cache_clone.put(key, value).await;
                    }
                    
                    // Get the data back
                    for j in 0..10 {
                        let key = format!("moka_concurrent_{}_{}", i, j);
                        if let Some(value) = cache_clone.get(&key).await {
                            let expected = format!("moka_concurrent_value_{}_{}", i, j).into_bytes();
                            assert_eq!(value, expected);
                        }
                    }
                    
                    i
                });
                handles.push(handle);
            }
            
            let results: Vec<_> = futures::future::join_all(handles)
                .await
                .into_iter()
                .map(|r| r.unwrap())
                .collect();
            
            assert_eq!(results.len(), 10);
        });
    }

    #[test]
    fn test_cache_edge_cases() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Test zero capacity cache
            let zero_cache = LRUCache::new(0);
            zero_cache.put("key".to_string(), b"value".to_vec());
            assert!(zero_cache.get("key").is_none());
            
            // Test single capacity cache
            let single_cache = LRUCache::new(1);
            single_cache.put("key1".to_string(), b"value1".to_vec());
            assert_eq!(single_cache.get("key1").unwrap(), b"value1");
            
            single_cache.put("key2".to_string(), b"value2".to_vec());
            assert!(single_cache.get("key1").is_none()); // Evicted
            assert_eq!(single_cache.get("key2").unwrap(), b"value2");
            
            // Test empty key and value
            single_cache.put("".to_string(), b"".to_vec());
            assert_eq!(single_cache.get("").unwrap(), b"");
            
            // Test very large values
            let large_value = vec![0x42u8; 65536]; // 64KB
            single_cache.put("large".to_string(), large_value.clone());
            assert_eq!(single_cache.get("large").unwrap(), large_value);
        });
    }

    #[test]
    fn test_cache_eviction_policies() {
        let cache = LRUCache::new(3);
        
        // Fill cache to capacity
        cache.put("a".to_string(), b"value_a".to_vec());
        cache.put("b".to_string(), b"value_b".to_vec());
        cache.put("c".to_string(), b"value_c".to_vec());
        
        assert_eq!(cache.len(), 3);
        
        // Access 'a' to make it recently used
        let _ = cache.get("a");
        
        // Add new item, should evict 'b' (least recently used)
        cache.put("d".to_string(), b"value_d".to_vec());
        
        assert!(cache.get("a").is_some()); // Still present
        assert!(cache.get("b").is_none());  // Evicted
        assert!(cache.get("c").is_some()); // Still present
        assert!(cache.get("d").is_some()); // Newly added
        
        // Test update doesn't change capacity
        cache.put("a".to_string(), b"new_value_a".to_vec());
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("a").unwrap(), b"new_value_a");
    }

    #[test]
    fn test_concurrent_eviction() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let cache = Arc::new(LRUCache::new(10)); // Small cache to force evictions
            
            // Multiple tasks adding items concurrently
            let mut handles = Vec::new();
            
            for i in 0..5 {
                let cache_clone = Arc::clone(&cache);
                let handle = tokio::spawn(async move {
                    for j in 0..20 {
                        let key = format!("evict_test_{}_{}", i, j);
                        let value = format!("evict_value_{}_{}", i, j).into_bytes();
                        cache_clone.put(key, value);
                        
                        // Occasionally check some items are still there
                        if j % 5 == 0 {
                            let check_key = format!("evict_test_{}_{}", i, j);
                            if let Some(val) = cache_clone.get(&check_key) {
                                let expected = format!("evict_value_{}_{}", i, j).into_bytes();
                                assert_eq!(val, expected);
                            }
                        }
                        
                        tokio::task::yield_now().await;
                    }
                });
                handles.push(handle);
            }
            
            futures::future::join_all(handles).await;
            
            // Cache should not exceed capacity
            assert!(cache.len() <= 10);
        });
    }

    #[test]
    fn test_cache_memory_patterns() {
        // Test various memory usage patterns that could reveal issues
        
        // Pattern 1: Rapid allocation/deallocation
        let cache = LRUCache::new(5);
        for i in 0..1000 {
            let key = format!("rapid_{}", i % 10);
            let value = vec![i as u8; 100]; // 100 bytes per value
            cache.put(key, value);
        }
        
        // Pattern 2: Large values
        let large_cache = LRUCache::new(2);
        let large_value1 = vec![0xAAu8; 10000]; // 10KB
        let large_value2 = vec![0xBBu8; 10000]; // 10KB
        large_cache.put("large1".to_string(), large_value1.clone());
        large_cache.put("large2".to_string(), large_value2.clone());
        
        assert_eq!(large_cache.get("large1").unwrap(), large_value1);
        assert_eq!(large_cache.get("large2").unwrap(), large_value2);
        
        // Pattern 3: Varied key lengths
        let varied_cache = LRUCache::new(10);
        for i in 0..20 {
            let key = "k".repeat(i + 1); // Keys from "k" to "kkkk..."
            let value = format!("value_{}", i).into_bytes();
            varied_cache.put(key, value);
        }
        
        // Verify some entries
        let long_key = "k".repeat(20);
        if let Some(value) = varied_cache.get(&long_key) {
            assert_eq!(value, b"value_19");
        }
    }
}