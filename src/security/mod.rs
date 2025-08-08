//! RustMQ Security Module
//!
//! This module provides comprehensive authentication and authorization functionality
//! for RustMQ, including mTLS authentication, multi-level ACL caching, and certificate
//! lifecycle management.
//!
//! ## Key Components
//!
//! - **Authentication**: mTLS-based identity verification with certificate validation
//! - **Authorization**: Multi-level ACL cache system achieving sub-100ns lookup performance
//! - **Certificate Management**: Complete certificate lifecycle with CA operations
//! - **TLS Integration**: Enhanced QUIC server with mTLS support
//!
//! ## Performance Characteristics
//!
//! - L1 Cache (connection-local): ~10ns authorization latency
//! - L2 Cache (broker-wide): ~50ns authorization latency
//! - L3 Cache (bloom filter): ~20ns negative lookup rejection
//! - String interning: 60-80% memory reduction for principals/topics
//! - Batch fetching: 10-100x reduction in Controller RPC load

pub mod auth;
pub mod acl;
pub mod tls;
pub mod metrics;

#[cfg(test)]
pub mod tests;

// Re-export key types for convenient access
pub use auth::{
    AuthenticationManager, AuthorizationManager, AuthContext, AuthorizedRequest,
    Principal, Permission, AclKey,
};
pub use acl::{
    AclManager, AclRule, AclEntry, AclOperation, PermissionSet,
    ResourcePattern, Effect,
};
pub use tls::{
    TlsConfig, CertificateManager, CertificateInfo, RevocationList,
    CachingClientCertVerifier, EnhancedCertificateManagementConfig, 
    CaSettings, CertificateRole, CertificateTemplate, KeyType, KeyUsage, 
    ExtendedKeyUsage, CertificateStatus, RevocationReason, CertificateRequest,
    CaGenerationParams, ValidationResult, RevokedCertificate, CertificateAuditEntry,
};
pub use metrics::SecurityMetrics;

use crate::error::RustMqError;
use std::sync::Arc;

/// Security configuration for RustMQ
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub tls: TlsConfig,
    pub acl: AclConfig,
    pub certificate_management: CertificateManagementConfig,
    pub audit: AuditConfig,
}

/// ACL configuration parameters
#[derive(Debug, Clone)]
pub struct AclConfig {
    pub enabled: bool,
    pub cache_size_mb: usize,
    pub cache_ttl_seconds: u64,
    pub l2_shard_count: usize,
    pub bloom_filter_size: usize,
    pub batch_fetch_size: usize,
    pub enable_audit_logging: bool,
    pub negative_cache_enabled: bool,
}

impl Default for AclConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_size_mb: 50,
            cache_ttl_seconds: 300,
            l2_shard_count: 32,
            bloom_filter_size: 1_000_000,
            batch_fetch_size: 100,
            enable_audit_logging: true,
            negative_cache_enabled: true,
        }
    }
}

/// Certificate management configuration
#[derive(Debug, Clone)]
pub struct CertificateManagementConfig {
    pub ca_cert_path: String,
    pub ca_key_path: String,
    pub cert_validity_days: u32,
    pub auto_renew_before_expiry_days: u32,
    pub crl_check_enabled: bool,
    pub ocsp_check_enabled: bool,
}

impl Default for CertificateManagementConfig {
    fn default() -> Self {
        Self {
            ca_cert_path: "/etc/rustmq/ca.pem".to_string(),
            ca_key_path: "/etc/rustmq/ca.key".to_string(),
            cert_validity_days: 365,
            auto_renew_before_expiry_days: 30,
            crl_check_enabled: true,
            ocsp_check_enabled: false,
        }
    }
}

/// Audit logging configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_authentication_events: bool,
    pub log_authorization_events: bool,
    pub log_certificate_events: bool,
    pub log_failed_attempts: bool,
    pub max_log_size_mb: usize,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_authentication_events: true,
            log_authorization_events: false, // Too verbose for production
            log_certificate_events: true,
            log_failed_attempts: true,
            max_log_size_mb: 100,
        }
    }
}

/// Main security manager that coordinates all security components
pub struct SecurityManager {
    authentication: Arc<AuthenticationManager>,
    authorization: Arc<AuthorizationManager>,
    certificate_manager: Arc<CertificateManager>,
    metrics: Arc<SecurityMetrics>,
    config: SecurityConfig,
}

impl SecurityManager {
    /// Create a new security manager with the given configuration
    pub async fn new(config: SecurityConfig) -> Result<Self, RustMqError> {
        let metrics = Arc::new(SecurityMetrics::new()?);
        
        let certificate_manager = Arc::new(
            CertificateManager::new(config.certificate_management.clone()).await?
        );
        
        let authentication = Arc::new(
            AuthenticationManager::new(
                certificate_manager.clone(),
                config.tls.clone(),
                metrics.clone(),
            ).await?
        );
        
        let authorization = Arc::new(
            AuthorizationManager::new(
                config.acl.clone(),
                metrics.clone(),
            ).await?
        );
        
        Ok(Self {
            authentication,
            authorization,
            certificate_manager,
            metrics,
            config,
        })
    }
    
    /// Create a new security manager with custom storage path (for testing)
    pub async fn new_with_storage_path(config: SecurityConfig, storage_path: &std::path::Path) -> Result<Self, RustMqError> {
        use crate::security::tls::EnhancedCertificateManagementConfig;
        
        let metrics = Arc::new(SecurityMetrics::new()?);
        
        // Create enhanced certificate config with custom storage path
        let enhanced_cert_config = EnhancedCertificateManagementConfig {
            basic: config.certificate_management.clone(),
            storage_path: storage_path.to_string_lossy().to_string(),
            ..Default::default()
        };
        
        let certificate_manager = Arc::new(
            CertificateManager::new_with_enhanced_config(enhanced_cert_config).await?
        );
        
        let authentication = Arc::new(
            AuthenticationManager::new(
                certificate_manager.clone(),
                config.tls.clone(),
                metrics.clone(),
            ).await?
        );
        
        let authorization = Arc::new(
            AuthorizationManager::new(
                config.acl.clone(),
                metrics.clone(),
            ).await?
        );
        
        Ok(Self {
            authentication,
            authorization,
            certificate_manager,
            metrics,
            config,
        })
    }
    
    /// Get the authentication manager
    pub fn authentication(&self) -> &Arc<AuthenticationManager> {
        &self.authentication
    }
    
    /// Get the authorization manager
    pub fn authorization(&self) -> &Arc<AuthorizationManager> {
        &self.authorization
    }
    
    /// Get the certificate manager
    pub fn certificate_manager(&self) -> &Arc<CertificateManager> {
        &self.certificate_manager
    }
    
    /// Get security metrics
    pub fn metrics(&self) -> &Arc<SecurityMetrics> {
        &self.metrics
    }
    
    /// Check if security is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.tls.enabled
    }
    
    /// Get the current security configuration
    pub fn get_current_config(&self) -> &SecurityConfig {
        &self.config
    }
    
    /// Update TLS configuration
    pub async fn update_tls_config(&mut self, tls_config: TlsConfig) -> Result<(), RustMqError> {
        self.config.tls = tls_config.clone();
        
        // Recreate authentication manager with new TLS config
        self.authentication = Arc::new(
            AuthenticationManager::new(
                self.certificate_manager.clone(),
                tls_config,
                self.metrics.clone(),
            ).await?
        );
        
        Ok(())
    }
    
    /// Update ACL configuration
    pub async fn update_acl_config(&mut self, acl_config: AclConfig) -> Result<(), RustMqError> {
        // Validate ACL configuration
        if acl_config.cache_size_mb == 0 {
            return Err(RustMqError::Config("Cache size cannot be zero".to_string()));
        }
        
        self.config.acl = acl_config.clone();
        
        // Recreate authorization manager with new ACL config
        self.authorization = Arc::new(
            AuthorizationManager::new(
                acl_config,
                self.metrics.clone(),
            ).await?
        );
        
        Ok(())
    }
}

/// Security context for authenticated and authorized requests
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub principal: Arc<str>,
    pub permissions: PermissionSet,
    pub certificate_info: Option<CertificateInfo>,
    pub auth_time: std::time::Instant,
}

impl SecurityContext {
    /// Create a new security context
    pub fn new(
        principal: Arc<str>,
        permissions: PermissionSet,
        certificate_info: Option<CertificateInfo>,
    ) -> Self {
        Self {
            principal,
            permissions,
            certificate_info,
            auth_time: std::time::Instant::now(),
        }
    }
    
    /// Check if the context has a specific permission
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions.contains(permission)
    }
    
    /// Get the age of this security context
    pub fn age(&self) -> std::time::Duration {
        self.auth_time.elapsed()
    }
}