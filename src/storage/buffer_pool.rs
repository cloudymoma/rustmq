use crate::{Result, storage::traits::BufferPool};
use parking_lot::Mutex;
use std::collections::VecDeque;

/// A size-classed pool that recycles `Vec<u8>` buffers to reduce allocation churn
/// on hot I/O paths.
///
/// NOTE ON ALIGNMENT: despite the name, this pool does **not** provide hardware
/// page alignment. A `Vec<u8>` cannot safely carry a custom base alignment (e.g.
/// 4 KiB), because it must be freed with the same `Layout` the global allocator
/// used for `u8` (align 1) — reconstructing one over a custom-aligned allocation
/// is undefined behavior. The pool's value is buffer *reuse*, not alignment.
///
/// The only code path that genuinely requires O_DIRECT alignment is the
/// segmented WAL writer, which uses its own page-aligned `AlignedBuf`
/// (`wal::aligned_buf`) for block-aligned `pwrite`s and must not rely on this
/// pool. This pool's job is size-classed buffer *reuse*, not alignment.
pub struct AlignedBufferPool {
    pools: Vec<Mutex<VecDeque<Vec<u8>>>>,
    /// Capacity rounding granularity for pooled buffers (improves reuse across
    /// similarly-sized requests). Not a hardware-alignment guarantee — see the
    /// type-level note above.
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

    /// Allocate a fresh buffer of exactly `size` bytes, with capacity rounded up to
    /// the pool's granularity so it slots cleanly into a size class on reuse.
    ///
    /// This does NOT page-align the allocation (see the type-level note). The old
    /// implementation tried to via `drain(0..offset)`, which was a no-op: `drain`
    /// shifts the retained elements back to the `Vec`'s original (unaligned) base
    /// pointer, so the returned data was never aligned.
    fn allocate_aligned(&self, size: usize) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(self.aligned_size(size));
        buffer.resize(size, 0);
        buffer
    }
}

impl BufferPool for AlignedBufferPool {
    fn get_aligned_buffer(&self, size: usize) -> Result<Vec<u8>> {
        let class_index = self.size_class_index(size);
        let mut pool = self.pools[class_index].lock();

        if let Some(mut buffer) = pool.pop_front() {
            if buffer.len() >= size {
                buffer.truncate(size);
                return Ok(buffer);
            }
        }

        // If no suitable buffer is found, try the next size class up
        if class_index + 1 < self.pools.len() {
            let mut next_pool = self.pools[class_index + 1].lock();
            if let Some(mut buffer) = next_pool.pop_front() {
                if buffer.len() >= size {
                    buffer.truncate(size);
                    return Ok(buffer);
                }
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
    fn test_get_buffer_returns_exact_requested_size() {
        // Callers (e.g. LocalObjectStorage reads) rely on the returned buffer being
        // exactly the requested length, not the rounded capacity.
        let pool = AlignedBufferPool::new(4096, 10);
        let buffer = pool.get_aligned_buffer(1024).unwrap();
        assert_eq!(buffer.len(), 1024);
        // Buffer must be usable as backing storage without further growth.
        assert!(buffer.capacity() >= 1024);
    }

    #[test]
    fn test_buffer_pool_reuses_allocation() {
        // The whole point of the pool is to avoid re-allocating on hot I/O paths:
        // a returned buffer must come back out on the next same-size request.
        let pool = AlignedBufferPool::new(512, 10);
        let buffer1 = pool.get_aligned_buffer(1024).unwrap();
        let ptr1 = buffer1.as_ptr();

        pool.return_buffer(buffer1);

        let buffer2 = pool.get_aligned_buffer(1024).unwrap();
        let ptr2 = buffer2.as_ptr();

        assert_eq!(
            ptr1, ptr2,
            "pool should hand back the same recycled allocation"
        );
        assert_eq!(buffer2.len(), 1024);
    }

    #[test]
    fn test_recycled_buffer_is_independently_writable() {
        // Recycled buffers must be safe to write to without aliasing live buffers.
        let pool = AlignedBufferPool::new(4096, 10);
        let mut a = pool.get_aligned_buffer(2048).unwrap();
        a.iter_mut().for_each(|b| *b = 0xAB);
        pool.return_buffer(a);

        let mut b = pool.get_aligned_buffer(2048).unwrap();
        // Reused storage may retain old bytes; callers overwrite before use.
        b.iter_mut().for_each(|x| *x = 0xCD);
        assert!(b.iter().all(|&x| x == 0xCD));
        assert_eq!(b.len(), 2048);
    }
}
