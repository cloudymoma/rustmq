//! Miri memory safety tests for storage components
//! 
//! These tests focus on detecting memory safety issues in:
//! - WAL (Write-Ahead Log) operations
//! - Buffer pool management
//! - Object storage interactions
//! - Concurrent access patterns

#[cfg(miri)]
mod miri_storage_tests {
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::runtime::Runtime;
    
    use rustmq::storage::{wal::DirectIOWal, buffer_pool::AlignedBufferPool};
    use rustmq::storage::object_storage::{ObjectStorage, LocalObjectStorage};

    #[test]
    fn test_wal_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let wal_path = temp_dir.path().join("test_wal");
            
            // Test WAL creation and basic operations
            let wal = DirectIOWal::new(&wal_path).await.unwrap();
            
            // Test single write
            let data = b"test_message_1";
            let offset = wal.append(data).await.unwrap();
            
            // Test read back
            let read_data = wal.read_at(offset, data.len()).await.unwrap();
            assert_eq!(data, &read_data[..]);
            
            // Test multiple sequential writes
            for i in 0..10 {
                let msg = format!("message_{}", i);
                wal.append(msg.as_bytes()).await.unwrap();
            }
            
            // Test WAL sync
            wal.sync().await.unwrap();
        });
    }

    #[test]
    fn test_wal_concurrent_access() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let wal_path = temp_dir.path().join("concurrent_wal");
            
            let wal = Arc::new(DirectIOWal::new(&wal_path).await.unwrap());
            
            // Test concurrent writes (Miri will detect data races)
            let mut handles = Vec::new();
            
            for i in 0..5 {
                let wal_clone = Arc::clone(&wal);
                let handle = tokio::spawn(async move {
                    let data = format!("concurrent_message_{}", i);
                    wal_clone.append(data.as_bytes()).await.unwrap()
                });
                handles.push(handle);
            }
            
            // Wait for all writes to complete
            let offsets: Vec<_> = futures::future::join_all(handles)
                .await
                .into_iter()
                .map(|r| r.unwrap())
                .collect();
            
            // Verify all writes succeeded
            assert_eq!(offsets.len(), 5);
            
            // Test concurrent reads
            let mut read_handles = Vec::new();
            for (i, offset) in offsets.iter().enumerate() {
                let wal_clone = Arc::clone(&wal);
                let offset = *offset;
                let handle = tokio::spawn(async move {
                    let expected = format!("concurrent_message_{}", i);
                    let data = wal_clone.read_at(offset, expected.len()).await.unwrap();
                    assert_eq!(expected.as_bytes(), &data[..]);
                });
                read_handles.push(handle);
            }
            
            futures::future::join_all(read_handles).await;
        });
    }

    #[test]
    fn test_buffer_pool_memory_safety() {
        // Test buffer pool allocation and deallocation
        let pool = AlignedBufferPool::new(4096, 10); // 4KB buffers, 10 max
        
        // Test single allocation
        let buffer = pool.get_buffer();
        assert_eq!(buffer.len(), 4096);
        assert_eq!(buffer.as_ptr() as usize % 4096, 0); // Check alignment
        
        // Test multiple allocations
        let mut buffers = Vec::new();
        for _ in 0..5 {
            buffers.push(pool.get_buffer());
        }
        
        // Test that all buffers are properly aligned
        for buffer in &buffers {
            assert_eq!(buffer.as_ptr() as usize % 4096, 0);
        }
        
        // Drop buffers (return to pool)
        drop(buffers);
        
        // Test reallocation after return
        let new_buffer = pool.get_buffer();
        assert_eq!(new_buffer.len(), 4096);
    }

    #[test]
    fn test_buffer_pool_concurrent_access() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = Arc::new(AlignedBufferPool::new(1024, 20));
            
            // Test concurrent buffer allocation
            let mut handles = Vec::new();
            
            for i in 0..10 {
                let pool_clone = Arc::clone(&pool);
                let handle = tokio::spawn(async move {
                    let buffer = pool_clone.get_buffer();
                    
                    // Write some data to ensure we're not sharing memory
                    let pattern = (i as u8).wrapping_mul(17); // Unique pattern per task
                    for byte in buffer.iter_mut() {
                        *byte = pattern;
                    }
                    
                    // Simulate some work
                    tokio::task::yield_now().await;
                    
                    // Verify data integrity
                    for byte in buffer.iter() {
                        assert_eq!(*byte, pattern);
                    }
                    
                    buffer
                });
                handles.push(handle);
            }
            
            let buffers: Vec<_> = futures::future::join_all(handles)
                .await
                .into_iter()
                .map(|r| r.unwrap())
                .collect();
            
            // Verify all buffers have different contents
            for (i, buffer) in buffers.iter().enumerate() {
                let expected_pattern = (i as u8).wrapping_mul(17);
                for byte in buffer.iter() {
                    assert_eq!(*byte, expected_pattern);
                }
            }
        });
    }

    #[test]
    fn test_object_storage_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let storage = LocalObjectStorage::new(temp_dir.path()).await.unwrap();
            
            // Test basic put/get operations
            let key = "test/object/key";
            let data = b"test object data for memory safety verification";
            
            storage.put(key, data).await.unwrap();
            let retrieved = storage.get(key).await.unwrap();
            
            assert_eq!(data, &retrieved[..]);
            
            // Test list operation
            let keys = storage.list("test/").await.unwrap();
            assert!(keys.contains(&key.to_string()));
            
            // Test delete operation
            storage.delete(key).await.unwrap();
            
            // Verify deletion
            let result = storage.get(key).await;
            assert!(result.is_err()); // Should fail to get deleted object
        });
    }

    #[test]
    fn test_object_storage_concurrent_operations() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let storage = Arc::new(LocalObjectStorage::new(temp_dir.path()).await.unwrap());
            
            // Test concurrent puts
            let mut put_handles = Vec::new();
            for i in 0..5 {
                let storage_clone = Arc::clone(&storage);
                let handle = tokio::spawn(async move {
                    let key = format!("concurrent/object/{}", i);
                    let data = format!("concurrent data {}", i);
                    storage_clone.put(&key, data.as_bytes()).await.unwrap();
                    key
                });
                put_handles.push(handle);
            }
            
            let keys: Vec<_> = futures::future::join_all(put_handles)
                .await
                .into_iter()
                .map(|r| r.unwrap())
                .collect();
            
            // Test concurrent gets
            let mut get_handles = Vec::new();
            for (i, key) in keys.iter().enumerate() {
                let storage_clone = Arc::clone(&storage);
                let key = key.clone();
                let handle = tokio::spawn(async move {
                    let data = storage_clone.get(&key).await.unwrap();
                    let expected = format!("concurrent data {}", i);
                    assert_eq!(expected.as_bytes(), &data[..]);
                });
                get_handles.push(handle);
            }
            
            futures::future::join_all(get_handles).await;
        });
    }

    #[test]
    fn test_storage_edge_cases() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            
            // Test empty data
            let wal = DirectIOWal::new(&temp_dir.path().join("empty_wal")).await.unwrap();
            let offset = wal.append(&[]).await.unwrap();
            let data = wal.read_at(offset, 0).await.unwrap();
            assert_eq!(data.len(), 0);
            
            // Test large data (potential for buffer overruns)
            let large_data = vec![0x42u8; 65536]; // 64KB
            let offset = wal.append(&large_data).await.unwrap();
            let read_data = wal.read_at(offset, large_data.len()).await.unwrap();
            assert_eq!(large_data, read_data);
            
            // Test zero-sized buffer pool
            let zero_pool = AlignedBufferPool::new(0, 1);
            let buffer = zero_pool.get_buffer();
            assert_eq!(buffer.len(), 0);
        });
    }
}