use serde::{Deserialize, Serialize};

/// Comprehensive security configuration for RustMQ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// TLS/mTLS configuration
    pub tls: TlsConfig,
    /// ACL configuration
    pub acl: AclConfig,
    /// Certificate management configuration
    pub certificate_management: CertificateManagementConfig,
    /// Audit logging configuration
    pub audit: AuditConfig,
}

/// TLS configuration for secure connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS/mTLS
    pub enabled: bool,
    /// Path to CA certificate file
    pub ca_cert_path: String,
    /// Path to CA certificate chain (optional)
    pub ca_cert_chain_path: Option<String>,
    /// Path to server certificate file
    pub server_cert_path: String,
    /// Path to server private key file
    pub server_key_path: String,
    /// Require client certificates
    pub client_cert_required: bool,
    /// Certificate verification mode
    pub cert_verify_mode: CertVerifyMode,
    /// Path to Certificate Revocation List (optional)
    pub crl_path: Option<String>,
    /// OCSP responder URL (optional)
    pub ocsp_url: Option<String>,
    /// Minimum TLS version
    pub min_tls_version: TlsVersion,
    /// Certificate refresh interval in hours
    pub cert_refresh_interval_hours: u64,
    /// Allowed cipher suites
    pub cipher_suites: Vec<String>,
}

/// Certificate verification modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CertVerifyMode {
    /// Verify certificate chain and hostname
    Full,
    /// Verify only certificate chain
    ChainOnly,
    /// No verification (for testing only)
    None,
}

/// TLS version enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TlsVersion {
    #[serde(rename = "1.2")]
    V12,
    #[serde(rename = "1.3")]
    V13,
}

/// ACL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclConfig {
    /// Enable ACL authorization
    pub enabled: bool,
    /// Cache size in megabytes
    pub cache_size_mb: usize,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Number of L2 cache shards (None = auto-calculate from CPU cores)
    pub l2_shard_count: Option<usize>,
    /// Multiplier for auto-calculating shard count from CPU cores (default: 2.0)
    pub l2_shard_multiplier: f64,
    /// Bloom filter size for negative caching
    pub bloom_filter_size: usize,
    /// Batch fetch size for controller requests
    pub batch_fetch_size: usize,
    /// Enable audit logging for authorization events
    pub enable_audit_logging: bool,
    /// Enable negative caching with bloom filter
    pub negative_cache_enabled: bool,
}

impl AclConfig {
    /// Calculate optimal L2 cache shard count based on CPU cores
    ///
    /// Returns a power-of-2 value between 8 and 512 shards.
    ///
    /// - If `l2_shard_count` is Some, returns that value (manual override)
    /// - If None, calculates from CPU cores: `next_power_of_2(logical_cores * multiplier)`
    /// - Clamps result to [8, 512] range
    pub fn effective_l2_shard_count(&self) -> usize {
        const MIN_SHARDS: usize = 8;
        const MAX_SHARDS: usize = 512;

        // Manual override takes precedence
        if let Some(manual_count) = self.l2_shard_count {
            return manual_count;
        }

        // Auto-calculate from CPU cores
        let logical_cores = num_cpus::get();
        let target = (logical_cores as f64 * self.l2_shard_multiplier) as usize;
        let power_of_2 = target.next_power_of_two();

        power_of_2.clamp(MIN_SHARDS, MAX_SHARDS)
    }
}

/// Certificate management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateManagementConfig {
    /// Path to CA certificate file
    pub ca_cert_path: String,
    /// Path to CA private key file
    pub ca_key_path: String,
    /// Certificate validity period in days
    pub cert_validity_days: u32,
    /// Auto-renew certificates before expiry (days)
    pub auto_renew_before_expiry_days: u32,
    /// Enable CRL checking
    pub crl_check_enabled: bool,
    /// Enable OCSP checking
    pub ocsp_check_enabled: bool,
}

/// Audit logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit logging
    pub enabled: bool,
    /// Log authentication events
    pub log_authentication_events: bool,
    /// Log authorization events (can be verbose)
    pub log_authorization_events: bool,
    /// Log certificate lifecycle events
    pub log_certificate_events: bool,
    /// Log failed authentication/authorization attempts
    pub log_failed_attempts: bool,
    /// Maximum log file size in megabytes
    pub max_log_size_mb: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            tls: TlsConfig::default(),
            acl: AclConfig::default(),
            certificate_management: CertificateManagementConfig::default(),
            audit: AuditConfig::default(),
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for development
            ca_cert_path: "/etc/rustmq/ca.pem".to_string(),
            ca_cert_chain_path: None,
            server_cert_path: "/etc/rustmq/server.pem".to_string(),
            server_key_path: "/etc/rustmq/server.key".to_string(),
            client_cert_required: true,
            cert_verify_mode: CertVerifyMode::Full,
            crl_path: None,
            ocsp_url: None,
            min_tls_version: TlsVersion::V13,
            cert_refresh_interval_hours: 1,
            cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_AES_128_GCM_SHA256".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
            ],
        }
    }
}

impl Default for AclConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_size_mb: 50,
            cache_ttl_seconds: 300,
            l2_shard_count: None,     // Auto-calculate from CPU cores
            l2_shard_multiplier: 2.0, // Balanced production default
            bloom_filter_size: 1_000_000,
            batch_fetch_size: 100,
            enable_audit_logging: true,
            negative_cache_enabled: true,
        }
    }
}

impl Default for CertificateManagementConfig {
    fn default() -> Self {
        Self {
            ca_cert_path: "/etc/rustmq/ca.pem".to_string(),
            ca_key_path: "/etc/rustmq/ca.key".to_string(),
            cert_validity_days: 365,
            auto_renew_before_expiry_days: 30,
            crl_check_enabled: true,
            ocsp_check_enabled: false, // Can be slow, disabled by default
        }
    }
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

impl SecurityConfig {
    /// Validate the security configuration
    pub fn validate(&self) -> crate::Result<()> {
        self.tls.validate()?;
        self.acl.validate()?;
        self.certificate_management.validate()?;
        self.audit.validate()?;
        Ok(())
    }
}

impl TlsConfig {
    pub fn validate(&self) -> crate::Result<()> {
        if self.enabled {
            if self.ca_cert_path.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.tls.ca_cert_path cannot be empty when TLS is enabled".to_string(),
                ));
            }

            if self.server_cert_path.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.tls.server_cert_path cannot be empty when TLS is enabled".to_string(),
                ));
            }

            if self.server_key_path.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.tls.server_key_path cannot be empty when TLS is enabled".to_string(),
                ));
            }

            if self.cert_refresh_interval_hours == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.tls.cert_refresh_interval_hours must be greater than 0".to_string(),
                ));
            }

            if self.cipher_suites.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.tls.cipher_suites cannot be empty when TLS is enabled".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl AclConfig {
    pub fn validate(&self) -> crate::Result<()> {
        if self.enabled {
            if self.cache_size_mb == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.acl.cache_size_mb must be greater than 0".to_string(),
                ));
            }

            if self.cache_ttl_seconds == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.acl.cache_ttl_seconds must be greater than 0".to_string(),
                ));
            }

            // Validate multiplier range
            if self.l2_shard_multiplier < 0.5 || self.l2_shard_multiplier > 8.0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.acl.l2_shard_multiplier must be between 0.5 and 8.0".to_string(),
                ));
            }

            // Validate manual override (if specified)
            if let Some(count) = self.l2_shard_count {
                if count == 0 || (count & (count - 1)) != 0 {
                    return Err(crate::error::RustMqError::InvalidConfig(
                        "security.acl.l2_shard_count must be a power of 2".to_string(),
                    ));
                }

                if count < 8 || count > 512 {
                    return Err(crate::error::RustMqError::InvalidConfig(
                        "security.acl.l2_shard_count must be between 8 and 512".to_string(),
                    ));
                }
            }

            if self.bloom_filter_size < 1000 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.acl.bloom_filter_size should be at least 1000".to_string(),
                ));
            }

            if self.batch_fetch_size == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.acl.batch_fetch_size must be greater than 0".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl CertificateManagementConfig {
    pub fn validate(&self) -> crate::Result<()> {
        if self.ca_cert_path.is_empty() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "security.certificate_management.ca_cert_path cannot be empty".to_string(),
            ));
        }

        if self.ca_key_path.is_empty() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "security.certificate_management.ca_key_path cannot be empty".to_string(),
            ));
        }

        if self.cert_validity_days == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "security.certificate_management.cert_validity_days must be greater than 0"
                    .to_string(),
            ));
        }

        if self.auto_renew_before_expiry_days >= self.cert_validity_days {
            return Err(crate::error::RustMqError::InvalidConfig(
                "security.certificate_management.auto_renew_before_expiry_days must be less than cert_validity_days".to_string(),
            ));
        }

        Ok(())
    }
}

impl AuditConfig {
    pub fn validate(&self) -> crate::Result<()> {
        if self.enabled && self.max_log_size_mb == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "security.audit.max_log_size_mb must be greater than 0 when audit is enabled"
                    .to_string(),
            ));
        }

        Ok(())
    }
}
