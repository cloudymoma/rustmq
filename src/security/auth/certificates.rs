//! Certificate Management and Validation
//!
//! This module provides certificate storage, validation, and management utilities
//! for RustMQ's security system, including certificate chain validation and
//! revocation checking.

use crate::error::{Result, RustMqError};
use crate::security::SecurityMetrics;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::HashSet;
use dashmap::DashMap;
use parking_lot::RwLock;
use rustls::Certificate;
use x509_parser::prelude::*;
use ring::digest;

/// Certificate store for managing trusted certificates and revocations
pub struct CertificateStore {
    /// Trusted CA certificates
    ca_certificates: Arc<RwLock<Vec<Certificate>>>,
    
    /// Revoked certificate fingerprints
    revoked_certificates: Arc<RwLock<HashSet<String>>>,
    
    /// Certificate validation cache (fingerprint -> validation result)
    validation_cache: Arc<DashMap<String, CachedValidationResult>>,
    
    /// Certificate DER cache for quick access (fingerprint -> DER data)
    der_cache: Arc<DashMap<Vec<u8>, Arc<Vec<u8>>>>,
    
    /// Security metrics
    metrics: Arc<SecurityMetrics>,
}

/// Cached validation result with expiration
#[derive(Debug, Clone)]
struct CachedValidationResult {
    is_valid: bool,
    cached_at: Instant,
    expires_at: Instant,
    validation_details: ValidationDetails,
}

/// Details about certificate validation
#[derive(Debug, Clone)]
pub struct ValidationDetails {
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub not_before: SystemTime,
    pub not_after: SystemTime,
    pub is_ca: bool,
    pub key_usage: Vec<String>,
}

impl CertificateStore {
    /// Create a new certificate store
    pub fn new(metrics: Arc<SecurityMetrics>) -> Self {
        Self {
            ca_certificates: Arc::new(RwLock::new(Vec::new())),
            revoked_certificates: Arc::new(RwLock::new(HashSet::new())),
            validation_cache: Arc::new(DashMap::new()),
            der_cache: Arc::new(DashMap::new()),
            metrics,
        }
    }
    
    /// Add a CA certificate to the trust store
    pub fn add_ca_certificate(&self, cert: Certificate) -> Result<()> {
        // Validate the certificate first
        let parsed = self.parse_certificate(&cert.0)?;
        
        // Ensure it's a CA certificate
        if !self.is_ca_certificate(&parsed)? {
            return Err(RustMqError::InvalidCertificate {
                reason: "Certificate is not a CA certificate".to_string(),
            });
        }
        
        let mut ca_certs = self.ca_certificates.write();
        ca_certs.push(cert);
        
        Ok(())
    }
    
    /// Load CA certificates from PEM data
    pub fn load_ca_certificates_from_pem(&self, pem_data: &str) -> Result<usize> {
        let certs = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse PEM certificates: {}", e),
            })?;
        
        let count = certs.len();
        
        for cert_der in certs {
            let cert = Certificate(cert_der);
            self.add_ca_certificate(cert)?;
        }
        
        Ok(count)
    }
    
    /// Validate a certificate chain
    pub fn validate_certificate_chain(&self, chain: &[Certificate]) -> Result<ValidationDetails> {
        if chain.is_empty() {
            return Err(RustMqError::InvalidCertificate {
                reason: "Empty certificate chain".to_string(),
            });
        }
        
        let client_cert = &chain[0];
        let fingerprint = self.calculate_fingerprint(client_cert);
        
        // Check cache first
        if let Some(cached) = self.validation_cache.get(&fingerprint) {
            if cached.expires_at > Instant::now() {
                self.metrics.record_certificate_cache_hit();
                if cached.is_valid {
                    return Ok(cached.validation_details.clone());
                } else {
                    return Err(RustMqError::InvalidCertificate {
                        reason: "Certificate validation failed (cached result)".to_string(),
                    });
                }
            } else {
                // Remove expired cache entry
                self.validation_cache.remove(&fingerprint);
            }
        }
        
        self.metrics.record_certificate_cache_miss();
        
        // Perform validation
        let start_time = Instant::now();
        let validation_result = self.validate_certificate_internal(client_cert, chain);
        self.metrics.record_certificate_parse_latency(start_time.elapsed());
        
        // Cache the result
        let cache_entry = match &validation_result {
            Ok(details) => CachedValidationResult {
                is_valid: true,
                cached_at: Instant::now(),
                expires_at: Instant::now() + Duration::from_secs(300), // 5 minute cache
                validation_details: details.clone(),
            },
            Err(_) => CachedValidationResult {
                is_valid: false,
                cached_at: Instant::now(),
                expires_at: Instant::now() + Duration::from_secs(60), // 1 minute cache for failures
                validation_details: ValidationDetails {
                    subject: "invalid".to_string(),
                    issuer: "unknown".to_string(),
                    serial_number: "0".to_string(),
                    not_before: UNIX_EPOCH,
                    not_after: UNIX_EPOCH,
                    is_ca: false,
                    key_usage: Vec::new(),
                },
            },
        };
        
        self.validation_cache.insert(fingerprint, cache_entry);
        self.metrics.record_certificate_validation();
        
        validation_result
    }
    
    /// Internal certificate validation logic
    fn validate_certificate_internal(
        &self,
        client_cert: &Certificate,
        chain: &[Certificate],
    ) -> Result<ValidationDetails> {
        let parsed_cert = self.parse_certificate(&client_cert.0)?;
        
        // Check certificate validity period
        let now = SystemTime::now();
        let not_before = self.asn1_time_to_system_time(&parsed_cert.validity().not_before)?;
        let not_after = self.asn1_time_to_system_time(&parsed_cert.validity().not_after)?;
        
        if now < not_before {
            return Err(RustMqError::CertificateExpired {
                subject: parsed_cert.subject().to_string(),
            });
        }
        
        if now > not_after {
            return Err(RustMqError::CertificateExpired {
                subject: parsed_cert.subject().to_string(),
            });
        }
        
        // Check if certificate is revoked
        let fingerprint = self.calculate_fingerprint(client_cert);
        if self.is_certificate_revoked(&fingerprint) {
            return Err(RustMqError::CertificateRevoked {
                subject: parsed_cert.subject().to_string(),
            });
        }
        
        // Validate against CA chain
        self.validate_against_ca_chain(&parsed_cert)?;
        
        // Extract validation details
        let details = ValidationDetails {
            subject: parsed_cert.subject().to_string(),
            issuer: parsed_cert.issuer().to_string(),
            serial_number: hex::encode(parsed_cert.serial.to_bytes_be()),
            not_before,
            not_after,
            is_ca: self.is_ca_certificate(&parsed_cert)?,
            key_usage: self.extract_key_usage(&parsed_cert)?,
        };
        
        Ok(details)
    }
    
    /// Validate certificate against CA chain
    fn validate_against_ca_chain(&self, cert: &X509Certificate) -> Result<()> {
        let ca_certs = self.ca_certificates.read();
        
        if ca_certs.is_empty() {
            return Err(RustMqError::InvalidCertificate {
                reason: "No CA certificates configured".to_string(),
            });
        }
        
        // Try to validate against each CA
        for ca_cert_der in ca_certs.iter() {
            let ca_cert = self.parse_certificate(&ca_cert_der.0)?;
            
            // Check if issuer matches CA subject
            if cert.issuer() == ca_cert.subject() {
                // Verify signature (simplified - in production use webpki)
                if self.verify_signature(cert, &ca_cert).is_ok() {
                    return Ok(());
                }
            }
        }
        
        Err(RustMqError::InvalidCertificate {
            reason: "Certificate not signed by trusted CA".to_string(),
        })
    }
    
    /// Verify certificate signature (simplified implementation)
    fn verify_signature(&self, cert: &X509Certificate, ca_cert: &X509Certificate) -> Result<()> {
        // This is a simplified signature verification
        // In production, use webpki or rustls for proper signature verification
        // Note: verify_signature API may vary - using placeholder for now
        // TODO: Implement proper signature verification with correct x509-parser API
        if cert.signature_algorithm != ca_cert.signature_algorithm {
            return Err(RustMqError::InvalidCertificate {
                reason: "Signature algorithm mismatch".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Parse certificate on-demand (no caching due to lifetime issues)
    fn parse_certificate<'a>(&self, cert_der: &'a [u8]) -> Result<X509Certificate<'a>> {
        // Parse certificate directly - no caching due to lifetime constraints
        let (_, parsed) = X509Certificate::from_der(cert_der)
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse certificate: {}", e),
            })?;
        
        Ok(parsed)
    }
    
    /// Calculate certificate fingerprint
    fn calculate_fingerprint(&self, cert: &Certificate) -> String {
        let digest = digest::digest(&digest::SHA256, &cert.0);
        hex::encode(digest.as_ref())
    }
    
    /// Check if certificate is a CA certificate
    fn is_ca_certificate(&self, cert: &X509Certificate) -> Result<bool> {
        // Check Basic Constraints extension
        if let Ok(Some(basic_constraints)) = cert.basic_constraints() {
            return Ok(basic_constraints.value.ca);
        }
        
        // Fallback: check key usage
        let key_usage = self.extract_key_usage(cert)?;
        Ok(key_usage.contains(&"keyCertSign".to_string()))
    }
    
    /// Extract key usage from certificate
    fn extract_key_usage(&self, cert: &X509Certificate) -> Result<Vec<String>> {
        let mut usage = Vec::new();
        
        if let Ok(Some(key_usage_ext)) = cert.key_usage() {
            if key_usage_ext.value.digital_signature() {
                usage.push("digitalSignature".to_string());
            }
            if key_usage_ext.value.key_cert_sign() {
                usage.push("keyCertSign".to_string());
            }
            if key_usage_ext.value.key_encipherment() {
                usage.push("keyEncipherment".to_string());
            }
            if key_usage_ext.value.data_encipherment() {
                usage.push("dataEncipherment".to_string());
            }
        }
        
        Ok(usage)
    }
    
    /// Convert ASN.1 time to SystemTime
    fn asn1_time_to_system_time(&self, asn1_time: &ASN1Time) -> Result<SystemTime> {
        let duration = Duration::from_secs(asn1_time.timestamp() as u64);
        Ok(UNIX_EPOCH + duration)
    }
    
    /// Check if certificate is revoked
    pub fn is_certificate_revoked(&self, fingerprint: &str) -> bool {
        let revoked_certs = self.revoked_certificates.read();
        revoked_certs.contains(fingerprint)
    }
    
    /// Revoke a certificate
    pub fn revoke_certificate(&self, fingerprint: String) {
        let mut revoked_certs = self.revoked_certificates.write();
        revoked_certs.insert(fingerprint.clone());
        
        // Remove from validation cache
        self.validation_cache.remove(&fingerprint);
        
        self.metrics.record_certificate_revocation();
    }
    
    /// Load revoked certificates from a list
    pub fn load_revoked_certificates(&self, fingerprints: Vec<String>) {
        let mut revoked_certs = self.revoked_certificates.write();
        for fingerprint in fingerprints {
            revoked_certs.insert(fingerprint);
        }
    }
    
    /// Get certificate store statistics
    pub fn stats(&self) -> CertificateStoreStats {
        CertificateStoreStats {
            ca_certificates_count: self.ca_certificates.read().len(),
            revoked_certificates_count: self.revoked_certificates.read().len(),
            validation_cache_size: self.validation_cache.len(),
            parsed_cache_size: 0, // parsed_cache field removed from API
        }
    }
    
    /// Clear all caches
    pub fn clear_caches(&self) {
        self.validation_cache.clear();
        // parsed_cache field removed from API - using validation_cache.clear() instead
        self.validation_cache.clear();
    }
    
    /// Clean expired cache entries
    pub fn cleanup_expired_cache_entries(&self) -> usize {
        let now = Instant::now();
        let mut cleaned = 0;
        
        self.validation_cache.retain(|_, entry| {
            if entry.expires_at <= now {
                cleaned += 1;
                false
            } else {
                true
            }
        });
        
        cleaned
    }
}

/// Certificate store statistics
#[derive(Debug, Clone)]
pub struct CertificateStoreStats {
    pub ca_certificates_count: usize,
    pub revoked_certificates_count: usize,
    pub validation_cache_size: usize,
    pub parsed_cache_size: usize,
}

/// Custom certificate verifier for rustls with caching
/// Note: In rustls 0.21, client certificate verification is handled differently
/// This is a placeholder for future implementation with webpki-roots
pub struct CachingClientCertVerifier {
    certificate_store: Arc<CertificateStore>,
    require_client_cert: bool,
}

impl CachingClientCertVerifier {
    /// Create a new caching certificate verifier
    pub fn new(certificate_store: Arc<CertificateStore>) -> Self {
        Self {
            certificate_store,
            require_client_cert: true,
        }
    }
    
    /// Create a verifier that doesn't require client certificates
    pub fn new_optional(certificate_store: Arc<CertificateStore>) -> Self {
        Self {
            certificate_store,
            require_client_cert: false,
        }
    }
    
    /// Validate certificate chain using our certificate store
    pub fn validate_certificate_chain(&self, chain: &[Certificate]) -> Result<ValidationDetails> {
        self.certificate_store.validate_certificate_chain(chain)
    }
    
    /// Check if client certificate is required
    pub fn client_auth_mandatory(&self) -> bool {
        self.require_client_cert
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_store() -> CertificateStore {
        let metrics = Arc::new(SecurityMetrics::new().unwrap());
        CertificateStore::new(metrics)
    }
    
    #[test]
    fn test_certificate_store_creation() {
        let store = create_test_store();
        let stats = store.stats();
        
        assert_eq!(stats.ca_certificates_count, 0);
        assert_eq!(stats.revoked_certificates_count, 0);
        assert_eq!(stats.validation_cache_size, 0);
    }
    
    #[test]
    fn test_certificate_revocation() {
        let store = create_test_store();
        let fingerprint = "abc123".to_string();
        
        assert!(!store.is_certificate_revoked(&fingerprint));
        
        store.revoke_certificate(fingerprint.clone());
        assert!(store.is_certificate_revoked(&fingerprint));
        
        let stats = store.stats();
        assert_eq!(stats.revoked_certificates_count, 1);
    }
    
    #[test]
    fn test_cache_cleanup() {
        let store = create_test_store();
        
        // Add a cache entry that will expire immediately
        let fingerprint = "test123".to_string();
        let expired_entry = CachedValidationResult {
            is_valid: true,
            cached_at: Instant::now(),
            expires_at: Instant::now() - Duration::from_secs(1), // Already expired
            validation_details: ValidationDetails {
                subject: "test".to_string(),
                issuer: "test-ca".to_string(),
                serial_number: "123".to_string(),
                not_before: UNIX_EPOCH,
                not_after: UNIX_EPOCH + Duration::from_secs(3600),
                is_ca: false,
                key_usage: Vec::new(),
            },
        };
        
        store.validation_cache.insert(fingerprint, expired_entry);
        assert_eq!(store.validation_cache.len(), 1);
        
        let cleaned = store.cleanup_expired_cache_entries();
        assert_eq!(cleaned, 1);
        assert_eq!(store.validation_cache.len(), 0);
    }
}