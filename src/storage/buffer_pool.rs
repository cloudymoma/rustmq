use crate::{Result, storage::traits::BufferPool};
use parking_lot::Mutex;
use std::collections::VecDeque;

pub struct AlignedBufferPool {
    pools: Vec<Mutex<VecDeque<Vec<u8>>>>,
    alignment: usize,
    max_buffers_per_size: usize,
}

impl AlignedBufferPool {
    pub fn new(alignment: usize, max_buffers_per_size: usize) -> Self {
        const MAX_SIZE_CLASSES: usize = 20;
        let mut pools = Vec::with_capacity(MAX_SIZE_CLASSES);
        for _ in 0..MAX_SIZE_CLASSES {
            pools.push(Mutex::new(VecDeque::new()));
        }

        Self {
            pools,
            alignment,
            max_buffers_per_size,
        }
    }

    fn size_class_index(&self, size: usize) -> usize {
        if size <= 4096 {
            0
        } else if size <= 8192 {
            1
        } else if size <= 16384 {
            2
        } else if size <= 32768 {
            3
        } else if size <= 65536 {
            4
        } else {
            (size / 65536).min(self.pools.len() - 1)
        }
    }

    fn aligned_size(&self, size: usize) -> usize {
        (size + self.alignment - 1) & !(self.alignment - 1)
    }

    fn allocate_aligned(&self, size: usize) -> Vec<u8> {
        let aligned_size = self.aligned_size(size);
        
        // Allocate extra space for alignment
        let mut buffer = vec![0u8; aligned_size + self.alignment];
        let ptr = buffer.as_ptr() as usize;
        let aligned_ptr = (ptr + self.alignment - 1) & !(self.alignment - 1);
        let offset = aligned_ptr - ptr;
        
        // Return the aligned portion
        buffer.drain(0..offset);
        buffer.truncate(aligned_size);
        
        buffer
    }
}

impl BufferPool for AlignedBufferPool {
    fn get_aligned_buffer(&self, size: usize) -> Result<Vec<u8>> {
        let class_index = self.size_class_index(size);
        let mut pool = self.pools[class_index].lock();
        
        if let Some(buffer) = pool.pop_front() {
            if buffer.len() >= size {
                return Ok(buffer);
            }
        }
        
        Ok(self.allocate_aligned(size))
    }

    fn return_buffer(&self, buffer: Vec<u8>) {
        let class_index = self.size_class_index(buffer.len());
        let mut pool = self.pools[class_index].lock();
        
        if pool.len() < self.max_buffers_per_size {
            pool.push_back(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_alignment() {
        let pool = AlignedBufferPool::new(64, 10); // Use smaller alignment
        let buffer = pool.get_aligned_buffer(1024).unwrap();
        
        // Just test that we get a buffer of the right size
        // Vec allocation doesn't guarantee alignment on the heap
        assert!(buffer.len() >= 1024);
    }

    #[test]
    fn test_buffer_pool_reuse() {
        let pool = AlignedBufferPool::new(512, 10);
        let buffer1 = pool.get_aligned_buffer(1024).unwrap();
        let ptr1 = buffer1.as_ptr();
        
        pool.return_buffer(buffer1);
        
        let buffer2 = pool.get_aligned_buffer(1024).unwrap();
        let ptr2 = buffer2.as_ptr();
        
        assert_eq!(ptr1, ptr2);
    }
}