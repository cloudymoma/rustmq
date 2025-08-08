//! Security module for RustMQ Rust SDK
//!
//! This module provides client-side security functionality including:
//! - mTLS authentication with certificate validation
//! - Principal extraction from client certificates
//! - ACL-aware request handling with caching
//! - Certificate management and auto-renewal

use crate::{
    config::{
        TlsConfig, TlsMode, AuthConfig, AuthMethod, SecurityConfig,
        PrincipalExtractionConfig, AclClientConfig,
    },
    error::{ClientError, Result},
};
use rustls::{RootCertStore, ClientConfig as RustlsClientConfig};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::{
    collections::HashMap,
    io::BufReader,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::RwLock as AsyncRwLock;
use x509_parser::prelude::*;
use dashmap::DashMap;
use serde::{Serialize, Deserialize};

/// Principal type for identity management
pub type Principal = Arc<str>;

/// Security context for authenticated requests
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub principal: Principal,
    pub certificate_info: Option<CertificateInfo>,
    pub permissions: PermissionSet,
    pub auth_time: Instant,
}

/// Certificate information extracted from client certificate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub not_before: u64,
    pub not_after: u64,
    pub fingerprint: String,
    pub attributes: HashMap<String, String>,
}

/// Permission set for ACL operations
#[derive(Debug, Clone, Default)]
pub struct PermissionSet {
    pub read_topics: Vec<String>,
    pub write_topics: Vec<String>,
    pub admin_operations: Vec<String>,
}

impl PermissionSet {
    /// Check if the permission set allows reading from a topic
    pub fn can_read_topic(&self, topic: &str) -> bool {
        self.read_topics.iter().any(|pattern| matches_pattern(topic, pattern))
    }

    /// Check if the permission set allows writing to a topic
    pub fn can_write_topic(&self, topic: &str) -> bool {
        self.write_topics.iter().any(|pattern| matches_pattern(topic, pattern))
    }

    /// Check if the permission set allows admin operations
    pub fn can_admin(&self, operation: &str) -> bool {
        self.admin_operations.iter().any(|pattern| matches_pattern(operation, pattern))
    }
}

/// Security manager for SDK client-side security operations
pub struct SecurityManager {
    config: SecurityConfig,
    principal_extractor: PrincipalExtractor,
    certificate_validator: CertificateValidator,
    acl_cache: AclCache,
    metrics: SecurityMetrics,
}

impl SecurityManager {
    /// Create a new security manager
    pub fn new(config: SecurityConfig) -> Result<Self> {
        let principal_extractor = PrincipalExtractor::new(config.principal_extraction.clone());
        let certificate_validator = CertificateValidator::new()?;
        let acl_cache = AclCache::new(config.acl.clone());
        let metrics = SecurityMetrics::default();

        Ok(Self {
            config,
            principal_extractor,
            certificate_validator,
            acl_cache,
            metrics,
        })
    }

    /// Authenticate a request and return security context
    pub async fn authenticate(
        &self,
        tls_config: &TlsConfig,
        auth_config: &AuthConfig,
    ) -> Result<SecurityContext> {
        match auth_config.method {
            AuthMethod::Mtls => self.authenticate_mtls(tls_config).await,
            AuthMethod::Token => self.authenticate_token(auth_config).await,
            AuthMethod::SaslPlain => self.authenticate_sasl(auth_config).await,
            AuthMethod::None => Ok(SecurityContext {
                principal: Arc::from("anonymous"),
                certificate_info: None,
                permissions: PermissionSet::default(),
                auth_time: Instant::now(),
            }),
            _ => Err(ClientError::UnsupportedAuthMethod {
                method: format!("{:?}", auth_config.method),
            }),
        }
    }

    /// Authenticate using mTLS
    async fn authenticate_mtls(&self, tls_config: &TlsConfig) -> Result<SecurityContext> {
        if tls_config.mode != TlsMode::MutualAuth {
            return Err(ClientError::InvalidConfig(
                "mTLS authentication requires TlsMode::MutualAuth".to_string(),
            ));
        }

        let client_cert_pem = tls_config.client_cert.as_ref().ok_or_else(|| {
            ClientError::InvalidConfig("Client certificate required for mTLS".to_string())
        })?;

        // Load and validate client certificate
        let cert_der = self.load_certificate_from_pem(client_cert_pem)?;
        let cert_info = self.extract_certificate_info(&cert_der)?;
        let principal = self.principal_extractor.extract_principal(&cert_info)?;

        // Validate certificate
        self.certificate_validator.validate_certificate(&cert_der, tls_config)?;

        // Get permissions from ACL cache
        let permissions = self.acl_cache.get_permissions(&principal).await?;

        self.metrics.record_authentication("mtls", true);

        Ok(SecurityContext {
            principal,
            certificate_info: Some(cert_info),
            permissions,
            auth_time: Instant::now(),
        })
    }

    /// Authenticate using token
    async fn authenticate_token(&self, auth_config: &AuthConfig) -> Result<SecurityContext> {
        let token = auth_config.token.as_ref().ok_or_else(|| {
            ClientError::InvalidConfig("Token required for token authentication".to_string())
        })?;

        // TODO: Implement JWT token validation
        // For now, extract principal from token (placeholder implementation)
        let principal = Arc::from(format!("token-user-{}", &token[..8]));
        let permissions = self.acl_cache.get_permissions(&principal).await?;

        self.metrics.record_authentication("token", true);

        Ok(SecurityContext {
            principal,
            certificate_info: None,
            permissions,
            auth_time: Instant::now(),
        })
    }

    /// Authenticate using SASL
    async fn authenticate_sasl(&self, auth_config: &AuthConfig) -> Result<SecurityContext> {
        let username = auth_config.username.as_ref().ok_or_else(|| {
            ClientError::InvalidConfig("Username required for SASL authentication".to_string())
        })?;

        // TODO: Implement SASL authentication logic
        // For now, just use the username as principal
        let principal = Arc::from(username.clone());
        let permissions = self.acl_cache.get_permissions(&principal).await?;

        self.metrics.record_authentication("sasl", true);

        Ok(SecurityContext {
            principal,
            certificate_info: None,
            permissions,
            auth_time: Instant::now(),
        })
    }

    /// Load certificate from PEM format
    fn load_certificate_from_pem(&self, pem_data: &str) -> Result<Vec<u8>> {
        let mut reader = BufReader::new(pem_data.as_bytes());
        let certs: std::result::Result<Vec<_>, _> = certs(&mut reader).collect();
        let certs = certs.map_err(|e| {
            ClientError::InvalidCertificate {
                reason: format!("Failed to parse certificate PEM: {}", e),
            }
        })?;

        if certs.is_empty() {
            return Err(ClientError::InvalidCertificate {
                reason: "No certificates found in PEM data".to_string(),
            });
        }

        Ok(certs[0].to_vec())
    }

    /// Extract certificate information
    fn extract_certificate_info(&self, cert_der: &[u8]) -> Result<CertificateInfo> {
        let (_, cert) = X509Certificate::from_der(cert_der).map_err(|e| {
            ClientError::InvalidCertificate {
                reason: format!("Failed to parse certificate: {}", e),
            }
        })?;

        let subject = cert.subject().to_string();
        let issuer = cert.issuer().to_string();
        let serial_number = cert.serial.to_str_radix(16);

        let not_before = cert.validity().not_before.timestamp() as u64;
        let not_after = cert.validity().not_after.timestamp() as u64;

        // Calculate certificate fingerprint (SHA256)
        let fingerprint = hex::encode(ring::digest::digest(&ring::digest::SHA256, cert_der));

        let mut attributes = HashMap::new();
        
        // Extract subject attributes
        for rdn in cert.subject().iter() {
            for attr in rdn.iter() {
                if let Ok(value) = attr.as_str() {
                    let attr_type = attr.attr_type();
                    let oid_name = if attr_type == &x509_parser::oid_registry::OID_X509_COMMON_NAME {
                        "CN"
                    } else if attr_type == &x509_parser::oid_registry::OID_X509_ORGANIZATION_NAME {
                        "O"
                    } else if attr_type == &x509_parser::oid_registry::OID_X509_ORGANIZATIONAL_UNIT {
                        "OU"
                    } else if attr_type == &x509_parser::oid_registry::OID_X509_COUNTRY_NAME {
                        "C"
                    } else if attr_type == &x509_parser::oid_registry::OID_X509_LOCALITY_NAME {
                        "L"
                    } else if attr_type == &x509_parser::oid_registry::OID_X509_STATE_OR_PROVINCE_NAME {
                        "ST"
                    } else {
                        continue;
                    };
                    
                    attributes.insert(oid_name.to_string(), value.to_string());
                }
            }
        }

        Ok(CertificateInfo {
            subject,
            issuer,
            serial_number,
            not_before,
            not_after,
            fingerprint,
            attributes,
        })
    }

    /// Create RustLS client configuration with security settings
    pub fn create_rustls_config(&self, tls_config: &TlsConfig) -> Result<RustlsClientConfig> {
        if !tls_config.is_enabled() {
            return Err(ClientError::InvalidConfig(
                "TLS must be enabled to create RustLS config".to_string(),
            ));
        }

        // Validate TLS configuration
        tls_config.validate().map_err(|e| {
            ClientError::InvalidConfig(format!("Invalid TLS configuration: {}", e))
        })?;

        let mut root_store = RootCertStore::empty();

        // Load CA certificates
        if let Some(ca_cert_pem) = &tls_config.ca_cert {
            self.load_ca_certificates(&mut root_store, ca_cert_pem)?;
        } else if !tls_config.insecure_skip_verify {
            // Load system root certificates
            root_store.extend(
                webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
            );
        }

        let config_builder = RustlsClientConfig::builder()
            .with_root_certificates(root_store);

        // Configure client authentication if mTLS is enabled
        let rustls_config = if tls_config.requires_client_cert() {
            let client_cert_chain = self.load_client_certificate_chain(tls_config)?;
            let client_key = self.load_client_private_key(tls_config)?;
            
            config_builder
                .with_client_auth_cert(client_cert_chain, client_key)
                .map_err(|e| ClientError::Tls(format!("Failed to configure client certificate: {}", e)))?
        } else {
            config_builder
                .with_no_client_auth()
        };

        Ok(rustls_config)
    }

    /// Load CA certificates into root store
    fn load_ca_certificates(&self, root_store: &mut RootCertStore, ca_cert_pem: &str) -> Result<()> {
        let mut reader = BufReader::new(ca_cert_pem.as_bytes());
        let certs: std::result::Result<Vec<_>, _> = certs(&mut reader).collect();
        let certs = certs.map_err(|e| {
            ClientError::InvalidCertificate {
                reason: format!("Failed to parse CA certificate PEM: {}", e),
            }
        })?;

        for cert in certs {
            root_store.add(CertificateDer::from(cert)).map_err(|e| {
                ClientError::InvalidCertificate {
                    reason: format!("Failed to add CA certificate: {}", e),
                }
            })?;
        }

        Ok(())
    }

    /// Load client certificate chain
    fn load_client_certificate_chain(&self, tls_config: &TlsConfig) -> Result<Vec<CertificateDer<'static>>> {
        let client_cert_pem = tls_config.client_cert.as_ref().ok_or_else(|| {
            ClientError::InvalidConfig("Client certificate required".to_string())
        })?;

        let mut reader = BufReader::new(client_cert_pem.as_bytes());
        let certs: std::result::Result<Vec<_>, _> = certs(&mut reader).collect();
        let certs = certs.map_err(|e| {
            ClientError::InvalidCertificate {
                reason: format!("Failed to parse client certificate PEM: {}", e),
            }
        })?;

        Ok(certs.into_iter().map(CertificateDer::from).collect())
    }

    /// Load client private key
    fn load_client_private_key(&self, tls_config: &TlsConfig) -> Result<PrivateKeyDer<'static>> {
        let client_key_pem = tls_config.client_key.as_ref().ok_or_else(|| {
            ClientError::InvalidConfig("Client private key required".to_string())
        })?;

        let mut reader = BufReader::new(client_key_pem.as_bytes());
        
        // Try PKCS#8 format first
        let keys: std::result::Result<Vec<_>, _> = pkcs8_private_keys(&mut reader).collect();
        if let Ok(mut keys) = keys {
            if !keys.is_empty() {
                return Ok(PrivateKeyDer::from(keys.remove(0)));
            }
        }

        // Try RSA format
        let mut reader = BufReader::new(client_key_pem.as_bytes());
        let keys: std::result::Result<Vec<_>, _> = rsa_private_keys(&mut reader).collect();
        if let Ok(mut keys) = keys {
            if !keys.is_empty() {
                return Ok(PrivateKeyDer::from(keys.remove(0)));
            }
        }

        Err(ClientError::InvalidCertificate {
            reason: "Failed to parse client private key".to_string(),
        })
    }

    /// Get security metrics
    pub fn metrics(&self) -> &SecurityMetrics {
        &self.metrics
    }
}

/// Principal extractor for extracting identity from certificates
pub struct PrincipalExtractor {
    config: PrincipalExtractionConfig,
    string_pool: DashMap<String, Principal>,
}

impl PrincipalExtractor {
    /// Create a new principal extractor
    pub fn new(config: PrincipalExtractionConfig) -> Self {
        Self {
            config,
            string_pool: DashMap::new(),
        }
    }

    /// Extract principal from certificate info
    pub fn extract_principal(&self, cert_info: &CertificateInfo) -> Result<Principal> {
        let principal_str = if self.config.use_common_name {
            cert_info.attributes.get("CN").cloned()
        } else {
            None
        }.or_else(|| {
            if self.config.use_subject_alt_name {
                // TODO: Extract from SAN
                None
            } else {
                None
            }
        }).unwrap_or_else(|| {
            // Fallback to subject
            cert_info.subject.clone()
        });

        let normalized = if self.config.normalize {
            principal_str.trim().to_lowercase()
        } else {
            principal_str
        };

        // Intern the string for memory efficiency
        let principal = self.string_pool
            .entry(normalized.clone())
            .or_insert_with(|| Arc::from(normalized.as_str()))
            .clone();

        Ok(principal)
    }
}

/// Certificate validator for validating certificate chains and properties
pub struct CertificateValidator;

impl CertificateValidator {
    /// Create a new certificate validator
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    /// Validate certificate according to TLS configuration
    pub fn validate_certificate(&self, _cert_der: &[u8], tls_config: &TlsConfig) -> Result<()> {
        if tls_config.insecure_skip_verify {
            return Ok(()); // Skip validation
        }

        // TODO: Implement comprehensive certificate validation
        // - Chain validation
        // - Expiration checking
        // - Revocation checking (CRL/OCSP)
        // - Custom validation rules

        Ok(())
    }
}

/// ACL cache for caching permission lookups
pub struct AclCache {
    config: AclClientConfig,
    cache: AsyncRwLock<HashMap<Principal, (PermissionSet, Instant)>>,
}

impl AclCache {
    /// Create a new ACL cache
    pub fn new(config: AclClientConfig) -> Self {
        Self {
            config,
            cache: AsyncRwLock::new(HashMap::new()),
        }
    }

    /// Get permissions for a principal (with caching)
    pub async fn get_permissions(&self, principal: &Principal) -> Result<PermissionSet> {
        if !self.config.enabled {
            return Ok(PermissionSet::default());
        }

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some((permissions, cached_at)) = cache.get(principal) {
                let cache_ttl = Duration::from_secs(self.config.cache_ttl_seconds);
                if cached_at.elapsed() < cache_ttl {
                    return Ok(permissions.clone());
                }
            }
        }

        // Cache miss - fetch from server
        let permissions = self.fetch_permissions_from_server(principal).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(principal.clone(), (permissions.clone(), Instant::now()));
            
            // Clean up old entries if cache is full
            if cache.len() > self.config.cache_size {
                self.cleanup_cache(&mut cache).await;
            }
        }

        Ok(permissions)
    }

    /// Fetch permissions from server
    async fn fetch_permissions_from_server(&self, _principal: &Principal) -> Result<PermissionSet> {
        // TODO: Implement server communication for ACL lookup
        // This would typically make a gRPC call to the controller
        
        // For now, return default permissions
        Ok(PermissionSet::default())
    }

    /// Clean up expired cache entries
    async fn cleanup_cache(&self, cache: &mut HashMap<Principal, (PermissionSet, Instant)>) {
        let cache_ttl = Duration::from_secs(self.config.cache_ttl_seconds);
        let now = Instant::now();
        
        cache.retain(|_, (_, cached_at)| now.duration_since(*cached_at) < cache_ttl);
    }
}

/// Security metrics for monitoring
#[derive(Debug, Default)]
pub struct SecurityMetrics {
    authentication_attempts: RwLock<HashMap<String, u64>>,
    authentication_successes: RwLock<HashMap<String, u64>>,
    acl_cache_hits: RwLock<u64>,
    acl_cache_misses: RwLock<u64>,
}

impl SecurityMetrics {
    /// Record authentication attempt
    pub fn record_authentication(&self, method: &str, success: bool) {
        if let Ok(mut attempts) = self.authentication_attempts.write() {
            *attempts.entry(method.to_string()).or_insert(0) += 1;
        }

        if success {
            if let Ok(mut successes) = self.authentication_successes.write() {
                *successes.entry(method.to_string()).or_insert(0) += 1;
            }
        }
    }

    /// Record ACL cache hit
    pub fn record_acl_cache_hit(&self) {
        if let Ok(mut hits) = self.acl_cache_hits.write() {
            *hits += 1;
        }
    }

    /// Record ACL cache miss
    pub fn record_acl_cache_miss(&self) {
        if let Ok(mut misses) = self.acl_cache_misses.write() {
            *misses += 1;
        }
    }

    /// Get authentication statistics
    pub fn get_auth_stats(&self) -> HashMap<String, (u64, u64)> {
        let attempts = self.authentication_attempts.read().unwrap();
        let successes = self.authentication_successes.read().unwrap();
        
        let mut stats = HashMap::new();
        for (method, attempt_count) in attempts.iter() {
            let success_count = successes.get(method).copied().unwrap_or(0);
            stats.insert(method.clone(), (*attempt_count, success_count));
        }
        
        stats
    }

    /// Get ACL cache statistics
    pub fn get_acl_cache_stats(&self) -> (u64, u64) {
        let hits = *self.acl_cache_hits.read().unwrap();
        let misses = *self.acl_cache_misses.read().unwrap();
        (hits, misses)
    }
}

/// Helper function for pattern matching
pub fn matches_pattern(text: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return text.starts_with(prefix) && text.ends_with(suffix);
        }
    }
    
    text == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_set() {
        let mut perms = PermissionSet::default();
        perms.read_topics.push("topic.*".to_string());
        perms.write_topics.push("logs.*".to_string());
        perms.admin_operations.push("*".to_string());

        assert!(perms.can_read_topic("topic.test"));
        assert!(!perms.can_read_topic("other.test"));
        assert!(perms.can_write_topic("logs.info"));
        assert!(!perms.can_write_topic("topic.test"));
        assert!(perms.can_admin("cluster.health"));
    }

    #[test]
    fn test_pattern_matching() {
        assert!(matches_pattern("test", "*"));
        assert!(matches_pattern("topic.test", "topic.*"));
        assert!(matches_pattern("user.admin", "*.admin"));
        assert!(!matches_pattern("topic.test", "logs.*"));
    }

    #[test]
    fn test_tls_config_validation() {
        let mut config = TlsConfig::default();
        config.mode = TlsMode::MutualAuth;
        
        // Should fail without certificates
        assert!(config.validate().is_err());
        
        config.client_cert = Some("cert".to_string());
        config.client_key = Some("key".to_string());
        config.ca_cert = Some("ca".to_string());
        
        // Should pass with all required components
        assert!(config.validate().is_ok());
    }
}