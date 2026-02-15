//! TLS/mTLS Configuration and Management
//!
//! This module provides comprehensive TLS configuration and certificate management
//! for RustMQ's security system.

pub mod certificate_manager;
pub mod config;
pub mod key_encryption;

pub use certificate_manager::{
    CaGenerationParams, CaSettings, CertificateAuditEntry, CertificateInfo, CertificateManager,
    CertificateRequest, CertificateRole, CertificateStatus, CertificateTemplate,
    EnhancedCertificateManagementConfig, ExtendedKeyUsage, KeyType, KeyUsage, RevocationList,
    RevocationReason, RevokedCertificate, ValidationResult,
};
pub use config::{TlsClientConfig, TlsConfig, TlsMode, TlsServerConfig};

// Re-export the caching verifier from auth module
pub use crate::security::auth::certificates::CachingClientCertVerifier;
