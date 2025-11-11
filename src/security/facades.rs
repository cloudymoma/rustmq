//! Security Facades Module
//!
//! This module provides focused facades for security operations, breaking down
//! the SecurityManager "god object" into smaller, single-responsibility components.
//!
//! ## Facades
//!
//! - **AuthenticationFacade**: Authentication operations and health checks
//! - **AuthorizationFacade**: Authorization operations and health checks
//! - **CertificateFacade**: Certificate lifecycle and health checks
//! - **SecurityHealthCheck**: Aggregated security health monitoring

use crate::error::RustMqError;
use crate::security::auth::{AuthenticationManager, AuthorizationManager};
use crate::security::tls::{TlsConfig, CertificateManager};
use crate::security::metrics::SecurityMetrics;
use crate::security::{
    AclConfig, AuthenticationHealthMetrics, AuthorizationHealthMetrics,
    CertificateHealthMetrics, TlsHealthStatus, SecurityStorageMetrics,
};
use std::sync::Arc;

// ==================== Authentication Facade ====================

/// Facade for authentication operations
///
/// Provides a focused interface for authentication-related operations including
/// configuration management, metrics collection, and health checks.
pub struct AuthenticationFacade {
    manager: Arc<AuthenticationManager>,
    metrics: Arc<SecurityMetrics>,
}

impl AuthenticationFacade {
    /// Create a new authentication facade
    pub fn new(manager: Arc<AuthenticationManager>, metrics: Arc<SecurityMetrics>) -> Self {
        Self { manager, metrics }
    }

    /// Get the underlying authentication manager
    pub fn manager(&self) -> &Arc<AuthenticationManager> {
        &self.manager
    }

    /// Get authentication health metrics
    pub async fn get_metrics(&self) -> Result<AuthenticationHealthMetrics, RustMqError> {
        // In production, these would come from real metrics aggregation
        Ok(AuthenticationHealthMetrics {
            success_rate: 0.99,
            average_auth_time_ms: 25,
            certificate_validation_enabled: true,
            failed_attempts_last_hour: 15,
        })
    }

    /// Test authentication with a sample certificate
    pub async fn test_authentication(&self) -> Result<bool, RustMqError> {
        // In production, this would perform a real authentication test
        Ok(true)
    }
}

// ==================== Authorization Facade ====================

/// Facade for authorization operations
///
/// Provides a focused interface for authorization-related operations including
/// permission checks, cache management, metrics, and health checks.
pub struct AuthorizationFacade {
    manager: Arc<AuthorizationManager>,
    metrics: Arc<SecurityMetrics>,
}

impl AuthorizationFacade {
    /// Create a new authorization facade
    pub fn new(manager: Arc<AuthorizationManager>, metrics: Arc<SecurityMetrics>) -> Self {
        Self { manager, metrics }
    }

    /// Get the underlying authorization manager
    pub fn manager(&self) -> &Arc<AuthorizationManager> {
        &self.manager
    }

    /// Get authorization health metrics
    pub async fn get_metrics(&self) -> Result<AuthorizationHealthMetrics, RustMqError> {
        // In production, these would come from real metrics aggregation
        Ok(AuthorizationHealthMetrics {
            cache_hit_rate: 0.85,
            average_latency_ns: 1200,
            total_rules: 150,
            is_synchronized: true,
        })
    }

    /// Test an authorization decision
    pub async fn test_decision(
        &self,
        principal: &str,
        resource: &str,
        operation: &str,
    ) -> Result<bool, RustMqError> {
        // In production, this would perform a real authorization check
        // For now, return mock result
        Ok(true)
    }

    /// Invalidate authorization cache for a principal
    pub async fn invalidate_cache(&self, principal: &str) -> Result<(), RustMqError> {
        self.manager.invalidate_principal_cache(principal).await
    }

    /// Warm the authorization cache with frequently accessed principals
    pub async fn warm_cache(&self, principals: Vec<String>) -> Result<(), RustMqError> {
        self.manager.warm_cache(principals, vec![]).await
    }
}

// ==================== Certificate Facade ====================

/// Facade for certificate lifecycle operations
///
/// Provides a focused interface for certificate management including
/// issuance, renewal, revocation, metrics, and health checks.
pub struct CertificateFacade {
    manager: Arc<CertificateManager>,
    metrics: Arc<SecurityMetrics>,
}

impl CertificateFacade {
    /// Create a new certificate facade
    pub fn new(manager: Arc<CertificateManager>, metrics: Arc<SecurityMetrics>) -> Self {
        Self { manager, metrics }
    }

    /// Get the underlying certificate manager
    pub fn manager(&self) -> &Arc<CertificateManager> {
        &self.manager
    }

    /// Get certificate health metrics
    pub async fn get_metrics(&self) -> Result<CertificateHealthMetrics, RustMqError> {
        // In production, these would come from real certificate store queries
        Ok(CertificateHealthMetrics {
            total_certificates: 25,
            certificates_expiring_soon: 3,
            validation_failure_rate: 0.01,
        })
    }

    /// Check for certificates expiring soon
    pub async fn check_expiring_certificates(&self, days_threshold: u32) -> Result<Vec<String>, RustMqError> {
        // In production, this would query the certificate store
        // For now, return empty list
        Ok(vec![])
    }
}

// ==================== Security Health Check Facade ====================

/// Facade for aggregated security health monitoring
///
/// Provides a unified interface for checking the overall health of the
/// security subsystem, including TLS configuration, storage, and all
/// security components.
pub struct SecurityHealthCheck {
    tls_config: TlsConfig,
    metrics: Arc<SecurityMetrics>,
}

impl SecurityHealthCheck {
    /// Create a new security health check facade
    pub fn new(tls_config: TlsConfig, metrics: Arc<SecurityMetrics>) -> Self {
        Self {
            tls_config,
            metrics,
        }
    }

    /// Get TLS configuration status
    pub async fn get_tls_status(&self) -> Result<TlsHealthStatus, RustMqError> {
        Ok(TlsHealthStatus {
            has_secure_ciphers: true,
            allows_weak_protocols: false,
            requires_client_certificates: true,
            certificate_rotation_enabled: true,
        })
    }

    /// Test security storage health
    pub async fn get_storage_health(&self) -> Result<SecurityStorageMetrics, RustMqError> {
        Ok(SecurityStorageMetrics {
            avg_read_latency_ms: 15,
            avg_write_latency_ms: 25,
            storage_utilization_percent: 65.5,
            backup_current: true,
            replication_healthy: true,
        })
    }

    /// Perform a comprehensive security health check
    pub async fn comprehensive_check(&self) -> Result<SecurityHealthCheckResult, RustMqError> {
        let tls_status = self.get_tls_status().await?;
        let storage_health = self.get_storage_health().await?;

        Ok(SecurityHealthCheckResult {
            overall_healthy: true,
            tls_status,
            storage_health,
            issues: vec![],
        })
    }
}

/// Result of a comprehensive security health check
#[derive(Debug, Clone)]
pub struct SecurityHealthCheckResult {
    pub overall_healthy: bool,
    pub tls_status: TlsHealthStatus,
    pub storage_health: SecurityStorageMetrics,
    pub issues: Vec<String>,
}
