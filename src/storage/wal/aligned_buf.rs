//! Page-aligned heap buffer for O_DIRECT I/O.
//!
//! O_DIRECT requires the memory buffer base address (as well as the file offset and
//! length) to be a multiple of the logical block size. A `Vec<u8>` cannot carry a
//! custom base alignment safely — it must be freed with the same `Layout` the global
//! allocator used for `u8` (align 1) — so we manage the allocation directly here and
//! free it with the matching `Layout` in `Drop`.

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ptr::NonNull;

/// Block size used for O_DIRECT alignment. 4096 is a safe superset of the common
/// 512-byte logical block size on essentially all storage.
pub const BLOCK_SIZE: usize = 4096;

/// A heap buffer whose base address and capacity are multiples of [`BLOCK_SIZE`].
pub struct AlignedBuf {
    ptr: NonNull<u8>,
    cap: usize,
}

// Safety: AlignedBuf owns a unique heap allocation; sending/sharing it is as safe as
// a Box<[u8]>.
unsafe impl Send for AlignedBuf {}
unsafe impl Sync for AlignedBuf {}

impl AlignedBuf {
    /// Allocate a zeroed, block-aligned buffer with capacity rounded up to a multiple
    /// of [`BLOCK_SIZE`] (minimum one block).
    pub fn new(min_capacity: usize) -> Self {
        let cap = min_capacity.max(BLOCK_SIZE).next_multiple_of(BLOCK_SIZE);
        let layout = Layout::from_size_align(cap, BLOCK_SIZE).expect("valid aligned layout");
        // Safety: cap > 0 and layout is valid.
        let raw = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(raw).unwrap_or_else(|| std::alloc::handle_alloc_error(layout));
        Self { ptr, cap }
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn as_slice(&self) -> &[u8] {
        // Safety: ptr is valid for cap bytes, initialized to zero.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.cap) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // Safety: ptr is valid for cap bytes and uniquely borrowed.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.cap) }
    }

    /// True if the base address is block-aligned (always true by construction; used in tests).
    pub fn is_block_aligned(&self) -> bool {
        (self.ptr.as_ptr() as usize) % BLOCK_SIZE == 0
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.cap, BLOCK_SIZE).expect("valid aligned layout");
        // Safety: ptr was allocated with this exact layout in `new`.
        unsafe { dealloc(self.ptr.as_ptr(), layout) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_rounds_up_to_block_multiple() {
        assert_eq!(AlignedBuf::new(1).capacity(), BLOCK_SIZE);
        assert_eq!(AlignedBuf::new(BLOCK_SIZE).capacity(), BLOCK_SIZE);
        assert_eq!(AlignedBuf::new(BLOCK_SIZE + 1).capacity(), 2 * BLOCK_SIZE);
        assert_eq!(AlignedBuf::new(0).capacity(), BLOCK_SIZE);
    }

    #[test]
    fn base_address_is_block_aligned() {
        for cap in [1usize, 4096, 4097, 100_000] {
            let buf = AlignedBuf::new(cap);
            assert!(buf.is_block_aligned(), "cap {cap} not aligned");
        }
    }

    #[test]
    fn starts_zeroed_and_is_writable() {
        let mut buf = AlignedBuf::new(BLOCK_SIZE);
        assert!(buf.as_slice().iter().all(|&b| b == 0));
        buf.as_mut_slice()[..3].copy_from_slice(&[1, 2, 3]);
        assert_eq!(&buf.as_slice()[..3], &[1, 2, 3]);
        assert_eq!(buf.as_slice()[3], 0);
    }
}
