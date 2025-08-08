//! Authentication and Authorization Module
//!
//! This module provides the core authentication and authorization functionality
//! for RustMQ, including mTLS authentication, principal extraction, and
//! multi-level ACL caching.

pub mod authentication;
pub mod authorization;
pub mod cache;
pub mod certificates;
pub mod principal;

pub use authentication::AuthenticationManager;
pub use authorization::AuthorizationManager;
pub use cache::{AclCache, AclCacheEntry, CacheLevel};
pub use certificates::CertificateStore;
pub use principal::PrincipalExtractor;

use std::sync::Arc;
use std::time::Instant;

/// Authentication context containing validated identity information
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub principal: Arc<str>,
    pub certificate_fingerprint: Option<String>,
    pub authenticated_at: Instant,
    pub groups: Vec<String>,
}

impl AuthContext {
    pub fn new(principal: Arc<str>) -> Self {
        Self {
            principal,
            certificate_fingerprint: None,
            authenticated_at: Instant::now(),
            groups: Vec::new(),
        }
    }
    
    pub fn with_certificate(mut self, fingerprint: String) -> Self {
        self.certificate_fingerprint = Some(fingerprint);
        self
    }
    
    pub fn with_groups(mut self, groups: Vec<String>) -> Self {
        self.groups = groups;
        self
    }
}

/// Authorized request wrapper containing both the request and auth context
#[derive(Debug)]
pub struct AuthorizedRequest<T> {
    pub principal: Arc<str>,
    pub request: T,
    pub permissions: PermissionSet,
    pub auth_context: AuthContext,
}

impl<T> AuthorizedRequest<T> {
    pub fn new(
        principal: Arc<str>,
        request: T,
        permissions: PermissionSet,
        auth_context: AuthContext,
    ) -> Self {
        Self {
            principal,
            request,
            permissions,
            auth_context,
        }
    }
}

/// Principal type for identity management
pub type Principal = Arc<str>;

/// Permission enumeration for fine-grained access control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Permission {
    Read = 0b001,
    Write = 0b010,
    Admin = 0b100,
}

impl Permission {
    pub fn from_bits(bits: u8) -> Vec<Permission> {
        let mut permissions = Vec::new();
        if bits & Permission::Read as u8 != 0 {
            permissions.push(Permission::Read);
        }
        if bits & Permission::Write as u8 != 0 {
            permissions.push(Permission::Write);
        }
        if bits & Permission::Admin as u8 != 0 {
            permissions.push(Permission::Admin);
        }
        permissions
    }
    
    pub fn to_bits(permissions: &[Permission]) -> u8 {
        permissions.iter().fold(0u8, |acc, &perm| acc | perm as u8)
    }
}

/// Set of permissions for efficient permission checking
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PermissionSet {
    bits: u8,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self { bits: 0 }
    }
    
    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.bits |= permission as u8;
        self
    }
    
    pub fn add_permission(&mut self, permission: Permission) {
        self.bits |= permission as u8;
    }
    
    pub fn remove_permission(&mut self, permission: Permission) {
        self.bits &= !(permission as u8);
    }
    
    pub fn contains(&self, permission: Permission) -> bool {
        (self.bits & permission as u8) != 0
    }
    
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }
    
    pub fn has_all(&self, permissions: &[Permission]) -> bool {
        let required_bits = Permission::to_bits(permissions);
        (self.bits & required_bits) == required_bits
    }
    
    pub fn has_any(&self, permissions: &[Permission]) -> bool {
        let check_bits = Permission::to_bits(permissions);
        (self.bits & check_bits) != 0
    }
}

impl From<Vec<Permission>> for PermissionSet {
    fn from(permissions: Vec<Permission>) -> Self {
        Self {
            bits: Permission::to_bits(&permissions),
        }
    }
}

impl From<&[Permission]> for PermissionSet {
    fn from(permissions: &[Permission]) -> Self {
        Self {
            bits: Permission::to_bits(permissions),
        }
    }
}

/// ACL key for cache lookups - optimized for performance
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclKey {
    pub principal: Arc<str>,
    pub topic: Arc<str>,
    pub permission: Permission,
}

impl AclKey {
    pub fn new(principal: Arc<str>, topic: Arc<str>, permission: Permission) -> Self {
        Self {
            principal,
            topic,
            permission,
        }
    }
}