//! TLS/mTLS Configuration and Management
//!
//! This module provides comprehensive TLS configuration and certificate management
//! for RustMQ's security system.

pub mod config;
pub mod certificate_manager;

pub use config::{TlsConfig, TlsMode, TlsServerConfig, TlsClientConfig};
pub use certificate_manager::{
    CertificateManager, CertificateInfo, RevocationList, 
    EnhancedCertificateManagementConfig, CaSettings, CertificateRole, CertificateTemplate,
    KeyType, KeyUsage, ExtendedKeyUsage, CertificateStatus, RevocationReason,
    CertificateRequest, CaGenerationParams, ValidationResult, RevokedCertificate,
    CertificateAuditEntry
};

// Re-export the caching verifier from auth module  
pub use crate::security::auth::certificates::CachingClientCertVerifier;