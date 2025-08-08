//! Principal Management and Extraction
//!
//! This module provides utilities for extracting and managing principals (identities)
//! from client certificates, with support for string interning and caching for
//! memory efficiency.

use crate::error::{Result, RustMqError};
use std::sync::Arc;
use std::collections::HashMap;
use dashmap::DashMap;
use x509_parser::prelude::*;
use rustls::Certificate;

/// Principal type - interned string for memory efficiency
pub type Principal = Arc<str>;

/// Principal extractor with caching for performance
pub struct PrincipalExtractor {
    /// Cache for certificate DER data (fingerprint -> DER data)
    certificate_der_cache: Arc<DashMap<Vec<u8>, Arc<Vec<u8>>>>,
    
    /// Cache for extracted principals (certificate fingerprint -> principal)
    principal_cache: Arc<DashMap<String, Principal>>,
    
    /// String interning pool for memory efficiency
    string_pool: Arc<DashMap<String, Principal>>,
}

impl PrincipalExtractor {
    /// Create a new principal extractor
    pub fn new() -> Self {
        Self {
            certificate_der_cache: Arc::new(DashMap::new()),
            principal_cache: Arc::new(DashMap::new()),
            string_pool: Arc::new(DashMap::new()),
        }
    }
    
    /// Extract principal from client certificate with caching
    pub fn extract_from_certificate(
        &self,
        cert: &Certificate,
        fingerprint: &str,
    ) -> Result<Principal> {
        // Check principal cache first
        if let Some(principal) = self.principal_cache.get(fingerprint) {
            return Ok(principal.clone());
        }
        
        // Parse certificate and extract CN
        let parsed_cert = self.parse_certificate(&cert.0)?;
        let principal_str = self.extract_common_name(&parsed_cert)?;
        
        // Intern the string for memory efficiency
        let principal = self.intern_string(&principal_str);
        
        // Cache the result
        self.principal_cache.insert(fingerprint.to_string(), principal.clone());
        
        Ok(principal)
    }
    
    /// Extract principal from certificate with additional attributes
    pub fn extract_with_attributes(
        &self,
        cert: &Certificate,
        fingerprint: &str,
    ) -> Result<PrincipalInfo> {
        let parsed_cert = self.parse_certificate(&cert.0)?;
        
        // Extract principal (CN)
        let principal_str = self.extract_common_name(&parsed_cert)?;
        let principal = self.intern_string(&principal_str);
        
        // Extract additional attributes
        let attributes = self.extract_certificate_attributes(&parsed_cert)?;
        
        let principal_info = PrincipalInfo {
            principal,
            attributes,
            certificate_fingerprint: fingerprint.to_string(),
        };
        
        Ok(principal_info)
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
    
    /// Extract Common Name from certificate subject
    fn extract_common_name(&self, cert: &X509Certificate) -> Result<String> {
        let subject = cert.subject();
        
        // Look for Common Name (CN) in subject
        for rdn in subject.iter() {
            for attr in rdn.iter() {
                if let Ok(cn) = attr.as_str() {
                    if attr.attr_type() == &x509_parser::oid_registry::OID_X509_COMMON_NAME {
                        return Ok(cn.to_string());
                    }
                }
            }
        }
        
        // Fallback: try to extract any string-like attribute
        for rdn in subject.iter() {
            for attr in rdn.iter() {
                if let Ok(value) = attr.as_str() {
                    if !value.trim().is_empty() {
                        return Ok(value.to_string());
                    }
                }
            }
        }
        
        Err(RustMqError::PrincipalExtraction(
            "No valid principal found in certificate subject".to_string()
        ))
    }
    
    /// Extract additional certificate attributes
    fn extract_certificate_attributes(&self, cert: &X509Certificate) -> Result<HashMap<String, String>> {
        let mut attributes = HashMap::new();
        
        // Extract subject attributes
        for rdn in cert.subject().iter() {
            for attr in rdn.iter() {
                if let Ok(value) = attr.as_str() {
                    // Use if-else comparisons instead of pattern matching for non-structural OID types
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
        
        // Extract validity period
        attributes.insert(
            "not_before".to_string(),
            cert.validity().not_before.to_string(),
        );
        attributes.insert(
            "not_after".to_string(),
            cert.validity().not_after.to_string(),
        );
        
        // Extract serial number
        let serial = cert.serial.to_str_radix(16);
        attributes.insert("serial_number".to_string(), serial);
        
        Ok(attributes)
    }
    
    /// Intern a string for memory efficiency
    fn intern_string(&self, s: &str) -> Principal {
        self.string_pool
            .entry(s.to_string())
            .or_insert_with(|| Arc::from(s))
            .clone()
    }
    
    /// Get statistics about the principal extractor
    pub fn stats(&self) -> PrincipalExtractorStats {
        PrincipalExtractorStats {
            certificate_cache_size: self.certificate_der_cache.len(),
            principal_cache_size: self.principal_cache.len(),
            string_pool_size: self.string_pool.len(),
        }
    }
    
    /// Clear all caches
    pub fn clear_caches(&self) {
        self.certificate_der_cache.clear();
        self.principal_cache.clear();
        // Don't clear string pool as it's used for interning
    }
}

impl Default for PrincipalExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Principal information with additional certificate attributes
#[derive(Debug, Clone)]
pub struct PrincipalInfo {
    /// The principal identity (usually CN from certificate)
    pub principal: Principal,
    
    /// Additional certificate attributes (O, OU, C, etc.)
    pub attributes: HashMap<String, String>,
    
    /// Certificate fingerprint for tracking
    pub certificate_fingerprint: String,
}

impl PrincipalInfo {
    /// Get organization from attributes
    pub fn organization(&self) -> Option<&str> {
        self.attributes.get("O").map(|s| s.as_str())
    }
    
    /// Get organizational unit from attributes
    pub fn organizational_unit(&self) -> Option<&str> {
        self.attributes.get("OU").map(|s| s.as_str())
    }
    
    /// Get country from attributes
    pub fn country(&self) -> Option<&str> {
        self.attributes.get("C").map(|s| s.as_str())
    }
    
    /// Get locality from attributes
    pub fn locality(&self) -> Option<&str> {
        self.attributes.get("L").map(|s| s.as_str())
    }
    
    /// Get state/province from attributes
    pub fn state_or_province(&self) -> Option<&str> {
        self.attributes.get("ST").map(|s| s.as_str())
    }
    
    /// Get certificate serial number
    pub fn serial_number(&self) -> Option<&str> {
        self.attributes.get("serial_number").map(|s| s.as_str())
    }
    
    /// Check if the principal matches a pattern
    pub fn matches_pattern(&self, pattern: &str) -> bool {
        // Simple pattern matching - could be extended with regex
        if pattern.contains('*') {
            // Wildcard matching
            let pattern_parts: Vec<&str> = pattern.split('*').collect();
            if pattern_parts.len() == 2 {
                let prefix = pattern_parts[0];
                let suffix = pattern_parts[1];
                return self.principal.starts_with(prefix) && self.principal.ends_with(suffix);
            }
        }
        
        // Exact match
        self.principal.as_ref() == pattern
    }
    
    /// Check if the principal belongs to an organization
    pub fn is_from_organization(&self, org: &str) -> bool {
        self.organization()
            .map(|o| o.eq_ignore_ascii_case(org))
            .unwrap_or(false)
    }
}

/// Statistics for the principal extractor
#[derive(Debug, Clone)]
pub struct PrincipalExtractorStats {
    pub certificate_cache_size: usize,
    pub principal_cache_size: usize,
    pub string_pool_size: usize,
}

/// Utility functions for principal management
pub mod utils {
    use super::*;
    
    /// Validate that a principal name is acceptable
    pub fn validate_principal_name(principal: &str) -> Result<()> {
        if principal.is_empty() {
            return Err(RustMqError::PrincipalExtraction(
                "Principal name cannot be empty".to_string()
            ));
        }
        
        if principal.len() > 255 {
            return Err(RustMqError::PrincipalExtraction(
                "Principal name too long (max 255 characters)".to_string()
            ));
        }
        
        // Check for invalid characters
        if principal.contains(['\0', '\n', '\r', '\t']) {
            return Err(RustMqError::PrincipalExtraction(
                "Principal name contains invalid characters".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Normalize a principal name for consistent comparisons
    pub fn normalize_principal_name(principal: &str) -> String {
        principal.trim().to_lowercase()
    }
    
    /// Check if a principal name matches a list of allowed patterns
    pub fn matches_allowed_patterns(principal: &str, patterns: &[String]) -> bool {
        if patterns.is_empty() {
            return true; // No restrictions
        }
        
        for pattern in patterns {
            if matches_single_pattern(principal, pattern) {
                return true;
            }
        }
        
        false
    }
    
    /// Check if a principal matches a single pattern
    fn matches_single_pattern(principal: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true; // Wildcard allows everything
        }
        
        if pattern.contains('*') {
            // Simple wildcard matching
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return principal.starts_with(prefix) && principal.ends_with(suffix);
            }
        }
        
        // Exact match
        principal == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::utils::*;
    
    #[test]
    fn test_principal_validation() {
        assert!(validate_principal_name("valid-user").is_ok());
        assert!(validate_principal_name("user@example.com").is_ok());
        
        assert!(validate_principal_name("").is_err());
        assert!(validate_principal_name(&"x".repeat(256)).is_err());
        assert!(validate_principal_name("user\nwith\nnewline").is_err());
    }
    
    #[test]
    fn test_principal_normalization() {
        assert_eq!(normalize_principal_name("  User@Example.COM  "), "user@example.com");
        assert_eq!(normalize_principal_name("ADMIN"), "admin");
    }
    
    #[test]
    fn test_pattern_matching() {
        assert!(matches_allowed_patterns("user1", &["user*".to_string()]));
        assert!(matches_allowed_patterns("admin", &["*".to_string()]));
        assert!(matches_allowed_patterns("test@example.com", &["*@example.com".to_string()]));
        
        assert!(!matches_allowed_patterns("user1", &["admin*".to_string()]));
        assert!(!matches_allowed_patterns("test@other.com", &["*@example.com".to_string()]));
    }
    
    #[test]
    fn test_principal_info_methods() {
        let mut attributes = HashMap::new();
        attributes.insert("O".to_string(), "Example Corp".to_string());
        attributes.insert("OU".to_string(), "Engineering".to_string());
        attributes.insert("C".to_string(), "US".to_string());
        
        let info = PrincipalInfo {
            principal: Arc::from("test-user"),
            attributes,
            certificate_fingerprint: "abc123".to_string(),
        };
        
        assert_eq!(info.organization(), Some("Example Corp"));
        assert_eq!(info.organizational_unit(), Some("Engineering"));
        assert_eq!(info.country(), Some("US"));
        assert!(info.is_from_organization("Example Corp"));
        assert!(info.matches_pattern("test-*"));
        assert!(!info.matches_pattern("admin-*"));
    }
    
    #[test]
    fn test_principal_extractor_stats() {
        let extractor = PrincipalExtractor::new();
        let stats = extractor.stats();
        
        assert_eq!(stats.certificate_cache_size, 0);
        assert_eq!(stats.principal_cache_size, 0);
        assert_eq!(stats.string_pool_size, 0);
    }
}