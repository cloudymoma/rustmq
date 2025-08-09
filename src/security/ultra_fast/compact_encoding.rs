//! Compact Permission Encoding
//!
//! This module provides memory-efficient encoding of authorization data
//! to maximize cache performance and minimize memory usage.
//!
//! ## Key Features
//!
//! - Bit-packed permission representation (64-bit total)
//! - Cache-line aligned structures
//! - Fast expiration checking
//! - Compact topic pattern encoding

use crate::security::auth::Permission;
use std::time::{Instant, Duration};

/// Compact authorization entry optimized for cache performance
#[repr(C, align(8))] // 8-byte aligned for optimal packing
#[derive(Copy, Clone, Debug)]
pub struct CompactAuthEntry {
    /// Hash of the authorization key (principal + topic + permission)
    pub key_hash: u64,
    
    /// Packed authorization data
    /// Bits 0-31: Permission bits (32 permissions max)
    /// Bits 32-62: Expiry timestamp (relative to epoch, seconds)
    /// Bit 63: Allowed flag (1 = allowed, 0 = denied)
    pub data: u64,
}

impl CompactAuthEntry {
    /// Empty/invalid entry marker
    pub const EMPTY: Self = Self {
        key_hash: 0,
        data: 0,
    };
    
    /// Create a new compact auth entry
    pub fn new(key_hash: u64, allowed: bool, created_at: Instant, ttl_seconds: u64) -> Self {
        let base_timestamp = Self::get_base_timestamp();
        let relative_timestamp = if created_at >= base_timestamp {
            created_at.duration_since(base_timestamp).as_secs()
        } else {
            // Handle past timestamps by using negative offset from current time
            let duration_before_base = base_timestamp.duration_since(created_at).as_secs();
            // This represents seconds before the base, we'll store as 0 but adjust expiry
            0
        };
        
        let expiry_timestamp = if created_at >= base_timestamp {
            relative_timestamp + ttl_seconds
        } else {
            // For past timestamps, calculate when they would expire relative to base
            let duration_before_base = base_timestamp.duration_since(created_at).as_secs();
            if duration_before_base >= ttl_seconds {
                // Entry was created more than TTL seconds ago, it's already expired
                0
            } else {
                // Entry expires (ttl_seconds - duration_before_base) after base
                ttl_seconds - duration_before_base
            }
        };
        
        // Pack data: [allowed(1)] [expiry(31)] [permissions(32)]
        let mut data = 0u64;
        
        // Set allowed flag (bit 63)
        if allowed {
            data |= 1u64 << 63;
        }
        
        // Set expiry timestamp (bits 32-62, 31 bits = ~68 years from base)
        data |= (expiry_timestamp & 0x7FFF_FFFF) << 32;
        
        // Default permission bits (bits 0-31) - can be extended for fine-grained permissions
        data |= 1u64; // Basic permission bit
        
        Self {
            key_hash,
            data,
        }
    }
    
    /// Check if entry is allowed
    #[inline(always)]
    pub fn allowed(&self) -> bool {
        (self.data >> 63) != 0
    }
    
    /// Check if entry has expired
    #[inline(always)]
    pub fn is_expired(&self) -> bool {
        let expiry_timestamp = (self.data >> 32) & 0x7FFF_FFFF;
        
        // If expiry timestamp is 0, it means the entry was already expired when created
        if expiry_timestamp == 0 {
            return true;
        }
        
        let now = Instant::now();
        let base_timestamp = Self::get_base_timestamp();
        let current_relative = if now >= base_timestamp {
            now.duration_since(base_timestamp).as_secs()
        } else {
            0
        };
        
        current_relative > expiry_timestamp
    }
    
    /// Get permission bits
    #[inline(always)]
    pub fn permission_bits(&self) -> u32 {
        (self.data & 0xFFFF_FFFF) as u32
    }
    
    /// Check if entry has specific permission
    #[inline(always)]
    pub fn has_permission(&self, permission: Permission) -> bool {
        let permission_bit = encode_permission(permission);
        (self.permission_bits() & permission_bit) != 0
    }
    
    /// Get base timestamp for relative time calculation
    fn get_base_timestamp() -> Instant {
        // Use a fixed base timestamp to make relative times work
        // In practice, this could be when the system started
        static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *BASE.get_or_init(|| Instant::now())
    }
    
    /// Check if this is an empty/invalid entry
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.key_hash == 0 && self.data == 0
    }
}

impl Default for CompactAuthEntry {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl PartialEq for CompactAuthEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key_hash == other.key_hash
    }
}

/// Compact permission set for bulk operations
#[repr(C, align(8))]
#[derive(Copy, Clone, Debug)]
pub struct CompactPermissionSet {
    /// Bit mask for permissions (32 different permissions supported)
    permission_mask: u32,
    
    /// Resource pattern hash for pattern matching
    resource_pattern_hash: u32,
}

impl CompactPermissionSet {
    /// Create empty permission set
    pub const EMPTY: Self = Self {
        permission_mask: 0,
        resource_pattern_hash: 0,
    };
    
    /// Create new permission set
    pub fn new(permissions: &[Permission], resource_pattern: &str) -> Self {
        let mut permission_mask = 0u32;
        
        for permission in permissions {
            permission_mask |= encode_permission(*permission);
        }
        
        use std::hash::{Hash, Hasher};
        let mut hasher = ahash::AHasher::default();
        resource_pattern.hash(&mut hasher);
        let resource_pattern_hash = hasher.finish() as u32;
        
        Self {
            permission_mask,
            resource_pattern_hash,
        }
    }
    
    /// Check if permission set contains specific permission
    #[inline(always)]
    pub fn contains(&self, permission: Permission) -> bool {
        let permission_bit = encode_permission(permission);
        (self.permission_mask & permission_bit) != 0
    }
    
    /// Check if permission set matches resource pattern
    #[inline(always)]
    pub fn matches_resource(&self, pattern_hash: u32) -> bool {
        self.resource_pattern_hash == pattern_hash
    }
    
    /// Get permission mask
    pub fn permission_mask(&self) -> u32 {
        self.permission_mask
    }
    
    /// Add permission to set
    pub fn add_permission(&mut self, permission: Permission) {
        self.permission_mask |= encode_permission(permission);
    }
    
    /// Remove permission from set
    pub fn remove_permission(&mut self, permission: Permission) {
        self.permission_mask &= !encode_permission(permission);
    }
    
    /// Check if permission set is empty
    pub fn is_empty(&self) -> bool {
        self.permission_mask == 0
    }
}

/// Encode permission enum as bit flag
#[inline(always)]
pub fn encode_permission(permission: Permission) -> u32 {
    match permission {
        Permission::Read => 1u32 << 0,
        Permission::Write => 1u32 << 1,
        Permission::Admin => 1u32 << 2,
        // Room for 29 more permission types
    }
}

/// Decode permission bit flag to enum
pub fn decode_permission(permission_bit: u32) -> Option<Permission> {
    match permission_bit {
        bit if bit & (1u32 << 0) != 0 => Some(Permission::Read),
        bit if bit & (1u32 << 1) != 0 => Some(Permission::Write),
        bit if bit & (1u32 << 2) != 0 => Some(Permission::Admin),
        _ => None,
    }
}

/// Batch permission encoding for SIMD operations
pub struct BatchPermissionEncoder;

impl BatchPermissionEncoder {
    /// Encode multiple permissions for vectorized processing
    pub fn encode_batch(permissions: &[Permission]) -> Vec<u32> {
        permissions.iter()
            .map(|&p| encode_permission(p))
            .collect()
    }
    
    /// Check multiple permissions against a mask (vectorizable)
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    pub unsafe fn check_permissions_simd(
        permission_masks: &[u32; 8],
        required_masks: &[u32; 8],
    ) -> [bool; 8] {
        use std::arch::x86_64::*;
        
        // Load 8 u32 values into AVX2 registers
        let masks = _mm256_loadu_si256(permission_masks.as_ptr() as *const __m256i);
        let required = _mm256_loadu_si256(required_masks.as_ptr() as *const __m256i);
        
        // Perform bitwise AND to check permissions
        let result = _mm256_and_si256(masks, required);
        
        // Compare with required to check if all required bits are set
        let comparison = _mm256_cmpeq_epi32(result, required);
        
        // Extract results
        let result_mask = _mm256_movemask_epi8(comparison);
        
        // Convert to boolean array
        let mut results = [false; 8];
        for i in 0..8 {
            // Each i32 comparison produces 4 bytes of 0xFF or 0x00
            results[i] = (result_mask & (0xF << (i * 4))) != 0;
        }
        
        results
    }
    
    /// Fallback scalar implementation
    pub fn check_permissions_scalar(
        permission_masks: &[u32],
        required_masks: &[u32],
    ) -> Vec<bool> {
        permission_masks.iter()
            .zip(required_masks.iter())
            .map(|(&mask, &required)| (mask & required) == required)
            .collect()
    }
}

/// Memory layout validator for ensuring optimal cache performance
pub struct MemoryLayoutValidator;

impl MemoryLayoutValidator {
    /// Validate structure alignment and size
    pub fn validate_compact_entry() -> Result<(), String> {
        use std::mem::{size_of, align_of};
        
        // Ensure CompactAuthEntry is properly sized
        if size_of::<CompactAuthEntry>() != 16 {
            return Err(format!(
                "CompactAuthEntry size is {} bytes, expected 16",
                size_of::<CompactAuthEntry>()
            ));
        }
        
        // Ensure proper alignment
        if align_of::<CompactAuthEntry>() != 8 {
            return Err(format!(
                "CompactAuthEntry alignment is {}, expected 8",
                align_of::<CompactAuthEntry>()
            ));
        }
        
        Ok(())
    }
    
    /// Calculate cache lines used by structure
    pub fn cache_lines_used<T>() -> usize {
        use std::mem::size_of;
        
        let cache_line_size = 64; // Standard x86_64 cache line size
        let struct_size = size_of::<T>();
        
        (struct_size + cache_line_size - 1) / cache_line_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_compact_auth_entry_creation() {
        let now = Instant::now();
        let entry = CompactAuthEntry::new(12345, true, now, 300);
        
        assert_eq!(entry.key_hash, 12345);
        assert!(entry.allowed());
        assert!(!entry.is_expired());
        assert!(!entry.is_empty());
    }
    
    #[test]
    fn test_compact_auth_entry_expiration() {
        let past = Instant::now() - Duration::from_secs(400);
        let entry = CompactAuthEntry::new(12345, true, past, 300);
        
        // Entry should be expired (created 400s ago with 300s TTL)
        assert!(entry.is_expired());
    }
    
    #[test]
    fn test_permission_encoding() {
        assert_eq!(encode_permission(Permission::Read), 1u32 << 0);
        assert_eq!(encode_permission(Permission::Write), 1u32 << 1);
        assert_eq!(encode_permission(Permission::Admin), 1u32 << 2);
        
        assert_eq!(decode_permission(1u32 << 0), Some(Permission::Read));
        assert_eq!(decode_permission(1u32 << 1), Some(Permission::Write));
        assert_eq!(decode_permission(1u32 << 2), Some(Permission::Admin));
    }
    
    #[test]
    fn test_compact_permission_set() {
        let permissions = vec![Permission::Read, Permission::Write];
        let perm_set = CompactPermissionSet::new(&permissions, "test-pattern");
        
        assert!(perm_set.contains(Permission::Read));
        assert!(perm_set.contains(Permission::Write));
        assert!(!perm_set.contains(Permission::Admin));
        assert!(!perm_set.is_empty());
        
        let mut empty_set = CompactPermissionSet::EMPTY;
        assert!(empty_set.is_empty());
        
        empty_set.add_permission(Permission::Read);
        assert!(empty_set.contains(Permission::Read));
        assert!(!empty_set.is_empty());
    }
    
    #[test]
    fn test_batch_permission_encoding() {
        let permissions = vec![Permission::Read, Permission::Write, Permission::Admin];
        let encoded = BatchPermissionEncoder::encode_batch(&permissions);
        
        assert_eq!(encoded.len(), 3);
        assert_eq!(encoded[0], encode_permission(Permission::Read));
        assert_eq!(encoded[1], encode_permission(Permission::Write));
        assert_eq!(encoded[2], encode_permission(Permission::Admin));
    }
    
    #[test]
    fn test_memory_layout_validation() {
        assert!(MemoryLayoutValidator::validate_compact_entry().is_ok());
        
        let cache_lines = MemoryLayoutValidator::cache_lines_used::<CompactAuthEntry>();
        assert_eq!(cache_lines, 1); // Should fit in single cache line
    }
    
    #[test]
    fn test_scalar_permission_checking() {
        let permission_masks = vec![0b111, 0b101, 0b001]; // All, Read+Admin, Read only
        let required_masks = vec![0b001, 0b100, 0b111];   // Read, Admin, All
        
        let results = BatchPermissionEncoder::check_permissions_scalar(
            &permission_masks,
            &required_masks,
        );
        
        assert_eq!(results, vec![true, true, false]);
    }
    
    #[test]
    fn test_compact_entry_size() {
        use std::mem::size_of;
        
        // Ensure our compact representation is truly compact
        assert_eq!(size_of::<CompactAuthEntry>(), 16); // 2 * u64
        assert_eq!(size_of::<CompactPermissionSet>(), 8); // 2 * u32
        
        // Compare with a hypothetical unoptimized version
        struct UnoptimizedEntry {
            principal: String,
            topic: String,
            permission: Permission,
            allowed: bool,
            created_at: Instant,
            ttl: Duration,
        }
        
        // The unoptimized version would be much larger
        assert!(size_of::<CompactAuthEntry>() < size_of::<UnoptimizedEntry>());
    }
}