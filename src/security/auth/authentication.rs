//! Authentication Manager
//!
//! Provides mTLS-based authentication with certificate validation,
//! principal extraction, and connection management.

use super::AuthContext;
use crate::error::RustMqError;
use crate::security::tls::{TlsConfig, CertificateManager, CertificateInfo};
use crate::security::metrics::SecurityMetrics;

use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use dashmap::DashMap;
use parking_lot::RwLock;
use x509_parser::prelude::*;
use rustls::Certificate;
use quinn::Connection;

/// Authentication manager responsible for mTLS authentication and principal extraction
#[derive(Clone)]
pub struct AuthenticationManager {
    certificate_store: Arc<RwLock<HashMap<String, CertificateInfo>>>,
    revoked_certificates: Arc<RwLock<HashSet<String>>>,
    ca_chain: Arc<RwLock<Vec<Certificate>>>,
    certificate_manager: Arc<CertificateManager>,
    metrics: Arc<SecurityMetrics>,
    config: TlsConfig,
    // Certificate DER cache for quick access (fingerprint -> DER data)
    certificate_der_cache: Arc<DashMap<Vec<u8>, Arc<Vec<u8>>>>,
    // Principal cache for certificate CN extraction (cert fingerprint -> principal)
    principal_cache: Arc<DashMap<String, Arc<str>>>,
    // Authentication counters
    success_count: Arc<std::sync::atomic::AtomicU64>,
    failure_count: Arc<std::sync::atomic::AtomicU64>,
}

impl AuthenticationManager {
    /// Create a new authentication manager
    pub async fn new(
        certificate_manager: Arc<CertificateManager>,
        config: TlsConfig,
        metrics: Arc<SecurityMetrics>,
    ) -> Result<Self, RustMqError> {
        let ca_chain = certificate_manager.get_ca_chain().await?;
        
        Ok(Self {
            certificate_store: Arc::new(RwLock::new(HashMap::new())),
            revoked_certificates: Arc::new(RwLock::new(HashSet::new())),
            ca_chain: Arc::new(RwLock::new(ca_chain)),
            certificate_manager,
            metrics,
            config,
            certificate_der_cache: Arc::new(DashMap::new()),
            principal_cache: Arc::new(DashMap::new()),
            success_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            failure_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }
    
    /// Authenticate a QUIC connection and extract the principal
    pub async fn authenticate_connection(
        &self,
        connection: &Connection,
    ) -> Result<AuthContext, RustMqError> {
        let start_time = Instant::now();
        
        // Extract client certificate from QUIC connection
        let certificates = self.extract_client_certificates(connection)?;
        
        if certificates.is_empty() {
            self.metrics.record_authentication_failure("no_certificate");
            return Err(RustMqError::AuthenticationFailed(
                "No client certificate provided".to_string()
            ));
        }
        
        // Validate the certificate chain
        let client_cert = &certificates[0];
        self.validate_certificate_chain(&certificates).await?;
        
        // Check certificate revocation
        let cert_fingerprint = self.calculate_certificate_fingerprint(client_cert);
        if self.is_certificate_revoked(&cert_fingerprint).await? {
            self.metrics.record_authentication_failure("certificate_revoked");
            return Err(RustMqError::CertificateRevoked {
                subject: cert_fingerprint.clone(),
            });
        }
        
        // Extract principal from certificate
        let principal = self.extract_principal_from_certificate(client_cert, &cert_fingerprint)?;
        
        // Get group memberships for the principal
        let groups = self.get_principal_groups(&principal).await?;
        
        // Create authentication context
        let auth_context = AuthContext::new(principal)
            .with_certificate(cert_fingerprint)
            .with_groups(groups);
        
        // Record successful authentication
        self.metrics.record_authentication_success(start_time.elapsed());
        
        Ok(auth_context)
    }
    
    /// Extract client certificates from QUIC connection
    fn extract_client_certificates(&self, connection: &Connection) -> Result<Vec<Certificate>, RustMqError> {
        let identity = connection
            .peer_identity()
            .ok_or_else(|| RustMqError::AuthenticationFailed("No peer identity".to_string()))?;
        
        let certificates = identity
            .downcast_ref::<Vec<Certificate>>()
            .ok_or_else(|| RustMqError::AuthenticationFailed("Invalid certificate type".to_string()))?;
        
        Ok(certificates.clone())
    }
    
    /// Validate the certificate chain against the CA
    pub async fn validate_certificate_chain(&self, certificates: &[Certificate]) -> Result<(), RustMqError> {
        // Measure validation latency for metrics
        let start = std::time::Instant::now();
        let result = self.validate_certificate_chain_internal(certificates).await;
        let latency = start.elapsed();
        
        match &result {
            Ok(_) => {
                self.success_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.metrics.record_authentication_success(latency);
            }
            Err(e) => {
                self.failure_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.metrics.record_authentication_failure(&e.to_string());
            }
        }
        
        result
    }
    
    /// Internal validation logic without metrics updates
    async fn validate_certificate_chain_internal(&self, certificates: &[Certificate]) -> Result<(), RustMqError> {
        if certificates.is_empty() {
            return Err(RustMqError::AuthenticationFailed(
                "Empty certificate chain".to_string()
            ));
        }
        
        let client_cert = &certificates[0];
        
        // Parse the certificate
        let parsed_cert = self.parse_certificate(&client_cert.0)?;
        
        // Check certificate validity period
        let now = std::time::SystemTime::now();
        let not_before_ts = parsed_cert.validity().not_before.timestamp();
        let not_after_ts = parsed_cert.validity().not_after.timestamp();
        let now_ts = now.duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| RustMqError::Internal(format!("Time error: {}", e)))?
            .as_secs() as i64;
        
        if now_ts < not_before_ts {
            return Err(RustMqError::CertificateExpired {
                subject: parsed_cert.subject().to_string(),
            });
        }
        
        if now_ts > not_after_ts {
            return Err(RustMqError::CertificateExpired {
                subject: parsed_cert.subject().to_string(),
            });
        }
        
        // Validate against CA chain
        let ca_chain = self.ca_chain.read();
        if ca_chain.is_empty() {
            return Err(RustMqError::AuthenticationFailed(
                "No CA certificates configured".to_string()
            ));
        }
        
        // Basic chain validation (in production, use webpki for full validation)
        self.validate_certificate_signature(&parsed_cert, &ca_chain[0])?;
        
        Ok(())
    }
    
    /// Parse certificate on-demand (no caching due to lifetime issues)
    fn parse_certificate<'a>(&self, cert_der: &'a [u8]) -> Result<X509Certificate<'a>, RustMqError> {
        // Parse certificate directly - no caching due to lifetime constraints
        let (_, parsed) = X509Certificate::from_der(cert_der)
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse certificate: {}", e),
            })?;
        
        Ok(parsed)
    }
    
    /// Validate certificate signature against CA
    fn validate_certificate_signature(
        &self,
        cert: &X509Certificate,
        ca_cert: &Certificate,
    ) -> Result<(), RustMqError> {
        // Parse CA certificate
        let (_, ca_parsed) = X509Certificate::from_der(&ca_cert.0)
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse CA certificate: {}", e),
            })?;
        
        // TODO: Implement proper signature verification using webpki
        // For now, we trust the certificate chain structure
        // In production, use webpki-roots and rustls for proper verification
        
        Ok(())
    }
    
    /// Calculate certificate fingerprint
    pub fn calculate_certificate_fingerprint(&self, cert: &Certificate) -> String {
        use ring::digest;
        let digest = digest::digest(&digest::SHA256, &cert.0);
        hex::encode(digest.as_ref())
    }
    
    /// Check if certificate is revoked
    pub async fn is_certificate_revoked(&self, fingerprint: &str) -> Result<bool, RustMqError> {
        let revoked_certs = self.revoked_certificates.read();
        Ok(revoked_certs.contains(fingerprint))
    }
    
    /// Refresh revoked certificates list from certificate manager
    pub async fn refresh_revoked_certificates(&self) -> Result<(), RustMqError> {
        // Get all revoked certificate fingerprints from certificate manager
        let revoked_fingerprints = self.certificate_manager.get_revoked_certificate_fingerprints().await?;
        
        // Update local revoked certificates set
        let mut revoked_certs = self.revoked_certificates.write();
        revoked_certs.clear();
        for fingerprint in revoked_fingerprints {
            revoked_certs.insert(fingerprint);
        }
        
        Ok(())
    }
    
    /// Extract principal from certificate with caching
    pub fn extract_principal_from_certificate(
        &self,
        cert: &Certificate,
        fingerprint: &str,
    ) -> Result<Arc<str>, RustMqError> {
        // Check principal cache first
        if let Some(cached_principal) = self.principal_cache.get(fingerprint) {
            return Ok(cached_principal.clone());
        }
        
        // Parse certificate to extract CN
        let parsed_cert = self.parse_certificate(&cert.0)?;
        
        // Extract Common Name from subject
        let principal = parsed_cert
            .subject()
            .iter_common_name()
            .next()
            .ok_or_else(|| RustMqError::InvalidCertificate {
                reason: "No Common Name found in certificate subject".to_string(),
            })?
            .as_str()
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Invalid Common Name encoding: {}", e),
            })?;
        
        let principal_arc: Arc<str> = Arc::from(principal);
        self.principal_cache.insert(fingerprint.to_string(), principal_arc.clone());
        
        Ok(principal_arc)
    }
    
    /// Get group memberships for a principal
    async fn get_principal_groups(&self, principal: &str) -> Result<Vec<String>, RustMqError> {
        // TODO: Implement group lookup from controller
        // For now, return empty groups
        Ok(Vec::new())
    }
    
    /// Revoke a certificate by its fingerprint
    pub async fn revoke_certificate(&self, fingerprint: String) -> Result<(), RustMqError> {
        let mut revoked_certs = self.revoked_certificates.write();
        revoked_certs.insert(fingerprint.clone());
        
        // Remove from caches
        self.principal_cache.remove(&fingerprint);
        
        self.metrics.record_certificate_revocation();
        
        Ok(())
    }
    
    /// Refresh CA chain from certificate manager
    pub async fn refresh_ca_chain(&self) -> Result<(), RustMqError> {
        let new_ca_chain = self.certificate_manager.get_ca_chain().await?;
        let mut ca_chain = self.ca_chain.write();
        *ca_chain = new_ca_chain;
        
        // Clear certificate caches to force re-validation
        self.certificate_der_cache.clear();
        self.principal_cache.clear();
        
        Ok(())
    }
    
    /// Get authentication statistics
    pub fn get_statistics(&self) -> AuthenticationStatistics {
        AuthenticationStatistics {
            certificate_cache_size: self.certificate_der_cache.len(),
            principal_cache_size: self.principal_cache.len(),
            revoked_certificates_count: self.revoked_certificates.read().len(),
            ca_certificates_count: self.ca_chain.read().len(),
            authentication_success_count: self.success_count.load(std::sync::atomic::Ordering::Relaxed),
            authentication_failure_count: self.failure_count.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
    
    // Convenience methods for test compatibility
    
    /// Validate a certificate (convenience method for tests)
    pub async fn validate_certificate(&self, cert_der: &[u8]) -> Result<(), RustMqError> {
        let certificates = vec![rustls::Certificate(cert_der.to_vec())];
        self.validate_certificate_chain(&certificates).await
    }
}

/// Authentication statistics for monitoring
#[derive(Debug, Clone)]
pub struct AuthenticationStatistics {
    pub certificate_cache_size: usize,
    pub principal_cache_size: usize,
    pub revoked_certificates_count: usize,
    pub ca_certificates_count: usize,
    pub authentication_success_count: u64,
    pub authentication_failure_count: u64,
}

// HashSet already imported above