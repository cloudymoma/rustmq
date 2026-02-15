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

pub mod acl;
pub mod auth;
pub mod facades;
pub mod metrics;
pub mod tls;
pub mod ultra_fast;

#[cfg(test)]
pub mod tests;

// Re-export key types for convenient access
pub use acl::{
    AclEntry, AclManager, AclOperation, AclRule, Effect, PermissionSet, ResourcePattern,
};
pub use auth::{
    AclKey, AuthContext, AuthenticationManager, AuthorizationManager, AuthorizedRequest,
    Permission, Principal,
};
pub use facades::{
    AuthenticationFacade, AuthorizationFacade, CertificateFacade, SecurityHealthCheck,
    SecurityHealthCheckResult,
};
pub use metrics::SecurityMetrics;
pub use tls::{
    CaGenerationParams, CaSettings, CachingClientCertVerifier, CertificateAuditEntry,
    CertificateInfo, CertificateManager, CertificateRequest, CertificateRole, CertificateStatus,
    CertificateTemplate, EnhancedCertificateManagementConfig, ExtendedKeyUsage, KeyType, KeyUsage,
    RevocationList, RevocationReason, RevokedCertificate, TlsConfig, ValidationResult,
};
pub use ultra_fast::{
    PerformanceTargets, UltraFastAuthResult, UltraFastAuthSystem, UltraFastConfig,
};

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
///
/// This is now a thin coordinator that delegates to focused facades for
/// different security concerns. The facades provide single-responsibility
/// interfaces while maintaining backward compatibility through direct manager access.
pub struct SecurityManager {
    // Core managers (kept for backward compatibility)
    authentication: Arc<AuthenticationManager>,
    authorization: Arc<AuthorizationManager>,
    certificate_manager: Arc<CertificateManager>,
    metrics: Arc<SecurityMetrics>,
    config: SecurityConfig,

    // Facades for organized access
    authentication_facade: AuthenticationFacade,
    authorization_facade: AuthorizationFacade,
    certificate_facade: CertificateFacade,
    health_check: SecurityHealthCheck,
}

impl SecurityManager {
    /// Create a new security manager with the given configuration
    pub async fn new(config: SecurityConfig) -> Result<Self, RustMqError> {
        let metrics = Arc::new(SecurityMetrics::new()?);

        let certificate_manager =
            Arc::new(CertificateManager::new(config.certificate_management.clone()).await?);

        let authentication = Arc::new(
            AuthenticationManager::new(
                certificate_manager.clone(),
                config.tls.clone(),
                metrics.clone(),
            )
            .await?,
        );

        let authorization = Arc::new(
            AuthorizationManager::new(
                config.acl.clone(),
                metrics.clone(),
                None, // No ACL manager in test mode - fail-secure
            )
            .await?,
        );

        // Create facades
        let authentication_facade =
            AuthenticationFacade::new(authentication.clone(), metrics.clone());

        let authorization_facade = AuthorizationFacade::new(authorization.clone(), metrics.clone());

        let certificate_facade =
            CertificateFacade::new(certificate_manager.clone(), metrics.clone());

        let health_check = SecurityHealthCheck::new(config.tls.clone(), metrics.clone());

        Ok(Self {
            authentication,
            authorization,
            certificate_manager,
            metrics,
            config,
            authentication_facade,
            authorization_facade,
            certificate_facade,
            health_check,
        })
    }

    /// Create a new security manager with custom storage path (for testing)
    pub async fn new_with_storage_path(
        config: SecurityConfig,
        storage_path: &std::path::Path,
    ) -> Result<Self, RustMqError> {
        use crate::security::tls::EnhancedCertificateManagementConfig;

        let metrics = Arc::new(SecurityMetrics::new()?);

        // Create enhanced certificate config with custom storage path
        let enhanced_cert_config = EnhancedCertificateManagementConfig {
            basic: config.certificate_management.clone(),
            storage_path: storage_path.to_string_lossy().to_string(),
            ..Default::default()
        };

        let certificate_manager =
            Arc::new(CertificateManager::new_with_enhanced_config(enhanced_cert_config).await?);

        let authentication = Arc::new(
            AuthenticationManager::new(
                certificate_manager.clone(),
                config.tls.clone(),
                metrics.clone(),
            )
            .await?,
        );

        let authorization = Arc::new(
            AuthorizationManager::new(
                config.acl.clone(),
                metrics.clone(),
                None, // No ACL manager in test mode - fail-secure
            )
            .await?,
        );

        // Create facades
        let authentication_facade =
            AuthenticationFacade::new(authentication.clone(), metrics.clone());

        let authorization_facade = AuthorizationFacade::new(authorization.clone(), metrics.clone());

        let certificate_facade =
            CertificateFacade::new(certificate_manager.clone(), metrics.clone());

        let health_check = SecurityHealthCheck::new(config.tls.clone(), metrics.clone());

        Ok(Self {
            authentication,
            authorization,
            certificate_manager,
            metrics,
            config,
            authentication_facade,
            authorization_facade,
            certificate_facade,
            health_check,
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

    // ==================== Facade Accessors ====================

    /// Get the authentication facade for focused authentication operations
    pub fn authentication_facade(&self) -> &AuthenticationFacade {
        &self.authentication_facade
    }

    /// Get the authorization facade for focused authorization operations
    pub fn authorization_facade(&self) -> &AuthorizationFacade {
        &self.authorization_facade
    }

    /// Get the certificate facade for focused certificate operations
    pub fn certificate_facade(&self) -> &CertificateFacade {
        &self.certificate_facade
    }

    /// Get the health check facade for security health monitoring
    pub fn health_check(&self) -> &SecurityHealthCheck {
        &self.health_check
    }

    // ==================== Configuration ====================

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
                tls_config.clone(),
                self.metrics.clone(),
            )
            .await?,
        );

        // Recreate authentication facade
        self.authentication_facade =
            AuthenticationFacade::new(self.authentication.clone(), self.metrics.clone());

        // Recreate health check with new TLS config
        self.health_check = SecurityHealthCheck::new(tls_config, self.metrics.clone());

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
                None, // No ACL manager in test mode - fail-secure
            )
            .await?,
        );

        // Recreate authorization facade
        self.authorization_facade =
            AuthorizationFacade::new(self.authorization.clone(), self.metrics.clone());

        Ok(())
    }

    // ==================== Health Check Methods (Delegated to Facades) ====================
    //
    // These methods are kept for backward compatibility but now delegate to the
    // focused facades. New code should use the facade accessors directly.

    /// Get certificate manager metrics for health checks
    ///
    /// Delegates to CertificateFacade. For new code, use:
    /// `security_manager.certificate_facade().get_metrics()`
    pub async fn get_certificate_metrics(&self) -> Result<CertificateHealthMetrics, RustMqError> {
        self.certificate_facade.get_metrics().await
    }

    /// Get authorization metrics for health checks
    ///
    /// Delegates to AuthorizationFacade. For new code, use:
    /// `security_manager.authorization_facade().get_metrics()`
    pub async fn get_authorization_metrics(
        &self,
    ) -> Result<AuthorizationHealthMetrics, RustMqError> {
        self.authorization_facade.get_metrics().await
    }

    /// Get authentication metrics for health checks
    ///
    /// Delegates to AuthenticationFacade. For new code, use:
    /// `security_manager.authentication_facade().get_metrics()`
    pub async fn get_authentication_metrics(
        &self,
    ) -> Result<AuthenticationHealthMetrics, RustMqError> {
        self.authentication_facade.get_metrics().await
    }

    /// Test authorization decision for health checks
    ///
    /// Delegates to AuthorizationFacade. For new code, use:
    /// `security_manager.authorization_facade().test_decision()`
    pub async fn test_authorization_decision(
        &self,
        principal: &str,
        resource: &str,
        operation: &str,
    ) -> Result<bool, RustMqError> {
        self.authorization_facade
            .test_decision(principal, resource, operation)
            .await
    }

    /// Get TLS configuration status for health checks
    ///
    /// Delegates to SecurityHealthCheck. For new code, use:
    /// `security_manager.health_check().get_tls_status()`
    pub async fn get_tls_configuration_status(&self) -> Result<TlsHealthStatus, RustMqError> {
        self.health_check.get_tls_status().await
    }

    /// Test security storage health
    ///
    /// Delegates to SecurityHealthCheck. For new code, use:
    /// `security_manager.health_check().get_storage_health()`
    pub async fn test_security_storage_health(
        &self,
    ) -> Result<SecurityStorageMetrics, RustMqError> {
        self.health_check.get_storage_health().await
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

// Health check metrics structures

/// Certificate manager health metrics
#[derive(Debug, Clone)]
pub struct CertificateHealthMetrics {
    pub total_certificates: u64,
    pub certificates_expiring_soon: u64,
    pub validation_failure_rate: f64,
}

/// Authorization manager health metrics
#[derive(Debug, Clone)]
pub struct AuthorizationHealthMetrics {
    pub cache_hit_rate: f64,
    pub average_latency_ns: u64,
    pub total_rules: u64,
    pub is_synchronized: bool,
}

/// Authentication manager health metrics
#[derive(Debug, Clone)]
pub struct AuthenticationHealthMetrics {
    pub success_rate: f64,
    pub average_auth_time_ms: u64,
    pub certificate_validation_enabled: bool,
    pub failed_attempts_last_hour: u64,
}

/// TLS configuration health status
#[derive(Debug, Clone)]
pub struct TlsHealthStatus {
    pub has_secure_ciphers: bool,
    pub allows_weak_protocols: bool,
    pub requires_client_certificates: bool,
    pub certificate_rotation_enabled: bool,
}

/// Security storage health metrics
#[derive(Debug, Clone)]
pub struct SecurityStorageMetrics {
    pub avg_read_latency_ms: u64,
    pub avg_write_latency_ms: u64,
    pub storage_utilization_percent: f64,
    pub backup_current: bool,
    pub replication_healthy: bool,
}
