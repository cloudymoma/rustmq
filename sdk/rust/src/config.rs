use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Client configuration for RustMQ connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Broker endpoints
    pub brokers: Vec<String>,
    
    /// Client ID for identification
    pub client_id: Option<String>,
    
    /// Connection timeout
    pub connect_timeout: Duration,
    
    /// Request timeout
    pub request_timeout: Duration,
    
    /// Enable TLS (deprecated: use tls_config.mode instead)
    pub enable_tls: bool,
    
    /// TLS configuration
    pub tls_config: Option<TlsConfig>,
    
    /// Connection pool size
    pub max_connections: usize,
    
    /// Keep-alive interval
    pub keep_alive_interval: Duration,
    
    /// Retry configuration
    pub retry_config: RetryConfig,
    
    /// Compression settings
    pub compression: CompressionConfig,
    
    /// Authentication settings
    pub auth: Option<AuthConfig>,
}

/// TLS configuration for secure RustMQ connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// TLS mode (disabled, server_auth, mutual_auth)
    pub mode: TlsMode,
    
    /// Certificate authority certificate path or PEM data
    pub ca_cert: Option<String>,
    
    /// Client certificate path or PEM data (for mTLS)
    pub client_cert: Option<String>,
    
    /// Client private key path or PEM data (for mTLS)
    pub client_key: Option<String>,
    
    /// Server name for SNI and certificate verification
    pub server_name: Option<String>,
    
    /// Skip certificate verification (insecure, dev only)
    pub insecure_skip_verify: bool,
    
    /// Supported TLS versions
    pub supported_versions: Vec<String>,
    
    /// Certificate validation settings
    pub validation: CertificateValidationConfig,
    
    /// ALPN protocols to negotiate
    pub alpn_protocols: Vec<String>,
}

/// TLS mode for client connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TlsMode {
    /// TLS disabled (insecure)
    Disabled,
    /// Server authentication only
    ServerAuth,
    /// Mutual TLS authentication
    MutualAuth,
}

/// Certificate validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateValidationConfig {
    /// Verify server certificate chain
    pub verify_chain: bool,
    
    /// Verify certificate expiration
    pub verify_expiration: bool,
    
    /// Check certificate revocation (CRL/OCSP)
    pub check_revocation: bool,
    
    /// Allow self-signed certificates
    pub allow_self_signed: bool,
    
    /// Maximum certificate chain depth
    pub max_chain_depth: usize,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_retries: usize,
    
    /// Base retry delay
    pub base_delay: Duration,
    
    /// Maximum retry delay
    pub max_delay: Duration,
    
    /// Retry multiplier for exponential backoff
    pub multiplier: f64,
    
    /// Jitter for retry timing
    pub jitter: bool,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Enable compression
    pub enabled: bool,
    
    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,
    
    /// Compression level (1-9)
    pub level: u8,
    
    /// Minimum size for compression
    pub min_size: usize,
}

/// Compression algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    None,
    Gzip,
    Lz4,
    Zstd,
}

/// Authentication configuration for RustMQ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication method
    pub method: AuthMethod,
    
    /// Username for SASL authentication
    pub username: Option<String>,
    
    /// Password for SASL authentication
    pub password: Option<String>,
    
    /// Token for token-based authentication
    pub token: Option<String>,
    
    /// Principal for mTLS authentication (extracted from certificate)
    pub principal: Option<String>,
    
    /// Additional authentication properties
    pub properties: std::collections::HashMap<String, String>,
    
    /// Security configuration for mTLS
    pub security: Option<SecurityConfig>,
}

/// Authentication methods supported by RustMQ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthMethod {
    /// No authentication
    None,
    /// SASL PLAIN authentication
    SaslPlain,
    /// SASL SCRAM-SHA-256 authentication
    SaslScram256,
    /// SASL SCRAM-SHA-512 authentication
    SaslScram512,
    /// JWT token-based authentication
    Token,
    /// Mutual TLS certificate authentication
    Mtls,
}

/// Security configuration for authentication and authorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable security features
    pub enabled: bool,
    
    /// Principal extraction configuration
    pub principal_extraction: PrincipalExtractionConfig,
    
    /// ACL configuration
    pub acl: AclClientConfig,
    
    /// Certificate management
    pub certificate_management: CertificateClientConfig,
}

/// Principal extraction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrincipalExtractionConfig {
    /// Extract principal from certificate Common Name
    pub use_common_name: bool,
    
    /// Extract principal from certificate Subject Alternative Name
    pub use_subject_alt_name: bool,
    
    /// Custom principal extraction patterns
    pub custom_patterns: Vec<String>,
    
    /// Principal normalization (lowercase, trim, etc.)
    pub normalize: bool,
}

/// ACL client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclClientConfig {
    /// Enable ACL checks
    pub enabled: bool,
    
    /// Client-side cache size (number of ACL entries)
    pub cache_size: usize,
    
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    
    /// Request batching for ACL checks
    pub batch_requests: bool,
    
    /// Fail open on ACL errors (allow access when ACL service unavailable)
    pub fail_open: bool,
}

/// Certificate management client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateClientConfig {
    /// Enable automatic certificate renewal
    pub auto_renew: bool,
    
    /// Renew certificate this many days before expiry
    pub renew_before_expiry_days: u32,
    
    /// Certificate storage location
    pub cert_storage_path: Option<String>,
    
    /// Certificate request template
    pub cert_template: Option<CertificateTemplate>,
}

/// Certificate template for requesting new certificates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateTemplate {
    /// Subject common name
    pub common_name: String,
    
    /// Subject organization
    pub organization: Option<String>,
    
    /// Subject organizational unit
    pub organizational_unit: Option<String>,
    
    /// Subject country
    pub country: Option<String>,
    
    /// Subject locality
    pub locality: Option<String>,
    
    /// Subject state/province
    pub state_or_province: Option<String>,
    
    /// Subject alternative names
    pub subject_alt_names: Vec<String>,
    
    /// Key usage extensions
    pub key_usage: Vec<String>,
    
    /// Extended key usage extensions
    pub extended_key_usage: Vec<String>,
}

/// Producer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerConfig {
    /// Producer ID
    pub producer_id: Option<String>,
    
    /// Batch size for batching messages
    pub batch_size: usize,
    
    /// Batch timeout
    pub batch_timeout: Duration,
    
    /// Maximum message size
    pub max_message_size: usize,
    
    /// Acknowledgment level
    pub ack_level: AckLevel,
    
    /// Idempotent producer
    pub idempotent: bool,
    
    /// Compression for producer
    pub compression: CompressionConfig,
    
    /// Custom message properties
    pub default_properties: std::collections::HashMap<String, String>,
}

/// Consumer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerConfig {
    /// Consumer ID
    pub consumer_id: Option<String>,
    
    /// Consumer group
    pub consumer_group: String,
    
    /// Auto-commit interval
    pub auto_commit_interval: Duration,
    
    /// Enable auto-commit
    pub enable_auto_commit: bool,
    
    /// Fetch size
    pub fetch_size: usize,
    
    /// Fetch timeout
    pub fetch_timeout: Duration,
    
    /// Starting position
    pub start_position: StartPosition,
    
    /// Maximum retry attempts for message processing
    pub max_retry_attempts: usize,
    
    /// Dead letter queue topic
    pub dead_letter_queue: Option<String>,
}

/// Acknowledgment levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AckLevel {
    /// No acknowledgment required
    None,
    
    /// Acknowledgment from leader only
    Leader,
    
    /// Acknowledgment from all replicas
    All,
}

/// Starting position for consumers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StartPosition {
    /// Start from earliest available message
    Earliest,
    
    /// Start from latest message
    Latest,
    
    /// Start from specific offset
    Offset(u64),
    
    /// Start from specific timestamp
    Timestamp(u64),
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["localhost:9092".to_string()],
            client_id: None,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            enable_tls: false,
            tls_config: Some(TlsConfig::default()),
            max_connections: 10,
            keep_alive_interval: Duration::from_secs(30),
            retry_config: RetryConfig::default(),
            compression: CompressionConfig::default(),
            auth: None,
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithm: CompressionAlgorithm::None,
            level: 6,
            min_size: 1024,
        }
    }
}

impl Default for ProducerConfig {
    fn default() -> Self {
        Self {
            producer_id: None,
            batch_size: 100,
            batch_timeout: Duration::from_millis(10),
            max_message_size: 1024 * 1024, // 1MB
            ack_level: AckLevel::All,
            idempotent: false,
            compression: CompressionConfig::default(),
            default_properties: std::collections::HashMap::new(),
        }
    }
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_id: None,
            consumer_group: "default-group".to_string(),
            auto_commit_interval: Duration::from_secs(5),
            enable_auto_commit: true,
            fetch_size: 100,
            fetch_timeout: Duration::from_secs(1),
            start_position: StartPosition::Latest,
            max_retry_attempts: 3,
            dead_letter_queue: None,
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            mode: TlsMode::Disabled,
            ca_cert: None,
            client_cert: None,
            client_key: None,
            server_name: None,
            insecure_skip_verify: false,
            supported_versions: vec!["TLSv1.3".to_string(), "TLSv1.2".to_string()],
            validation: CertificateValidationConfig::default(),
            alpn_protocols: vec!["h3".to_string(), "hq-interop".to_string()],
        }
    }
}

impl Default for CertificateValidationConfig {
    fn default() -> Self {
        Self {
            verify_chain: true,
            verify_expiration: true,
            check_revocation: false, // Disabled by default for performance
            allow_self_signed: false,
            max_chain_depth: 10,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            principal_extraction: PrincipalExtractionConfig::default(),
            acl: AclClientConfig::default(),
            certificate_management: CertificateClientConfig::default(),
        }
    }
}

impl Default for PrincipalExtractionConfig {
    fn default() -> Self {
        Self {
            use_common_name: true,
            use_subject_alt_name: false,
            custom_patterns: Vec::new(),
            normalize: true,
        }
    }
}

impl Default for AclClientConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_size: 1000,
            cache_ttl_seconds: 300,
            batch_requests: true,
            fail_open: false,
        }
    }
}

impl Default for CertificateClientConfig {
    fn default() -> Self {
        Self {
            auto_renew: false,
            renew_before_expiry_days: 30,
            cert_storage_path: None,
            cert_template: None,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            method: AuthMethod::None,
            username: None,
            password: None,
            token: None,
            principal: None,
            properties: std::collections::HashMap::new(),
            security: None,
        }
    }
}

impl AuthConfig {
    /// Create a new mTLS authentication configuration
    pub fn mtls_config(
        ca_cert: String,
        client_cert: String,
        client_key: String,
        server_name: Option<String>,
    ) -> Self {
        Self {
            method: AuthMethod::Mtls,
            username: None,
            password: None,
            token: None,
            principal: None,
            properties: std::collections::HashMap::new(),
            security: Some(SecurityConfig {
                enabled: true,
                principal_extraction: PrincipalExtractionConfig::default(),
                acl: AclClientConfig::default(),
                certificate_management: CertificateClientConfig::default(),
            }),
        }
    }
    
    /// Create a token-based authentication configuration
    pub fn token_config(token: String) -> Self {
        Self {
            method: AuthMethod::Token,
            username: None,
            password: None,
            token: Some(token),
            principal: None,
            properties: std::collections::HashMap::new(),
            security: Some(SecurityConfig::default()),
        }
    }
    
    /// Create SASL PLAIN authentication configuration
    pub fn sasl_plain_config(username: String, password: String) -> Self {
        Self {
            method: AuthMethod::SaslPlain,
            username: Some(username),
            password: Some(password),
            token: None,
            principal: None,
            properties: std::collections::HashMap::new(),
            security: None,
        }
    }
    
    /// Check if this authentication configuration requires TLS
    pub fn requires_tls(&self) -> bool {
        matches!(self.method, AuthMethod::Mtls | AuthMethod::SaslPlain | AuthMethod::Token)
    }
    
    /// Check if this authentication configuration supports ACL
    pub fn supports_acl(&self) -> bool {
        self.security.as_ref().map(|s| s.acl.enabled).unwrap_or(false)
    }
}

impl TlsConfig {
    /// Create a secure TLS configuration for production use
    pub fn secure_config(
        ca_cert: String,
        client_cert: Option<String>,
        client_key: Option<String>,
        server_name: String,
    ) -> Self {
        let mode = if client_cert.is_some() && client_key.is_some() {
            TlsMode::MutualAuth
        } else {
            TlsMode::ServerAuth
        };
        
        Self {
            mode,
            ca_cert: Some(ca_cert),
            client_cert,
            client_key,
            server_name: Some(server_name),
            insecure_skip_verify: false,
            supported_versions: vec!["TLSv1.3".to_string()], // Only TLS 1.3 for security
            validation: CertificateValidationConfig {
                verify_chain: true,
                verify_expiration: true,
                check_revocation: true,
                allow_self_signed: false,
                max_chain_depth: 5,
            },
            alpn_protocols: vec!["h3".to_string()],
        }
    }
    
    /// Create an insecure TLS configuration for development (not recommended)
    pub fn insecure_config() -> Self {
        Self {
            mode: TlsMode::ServerAuth,
            ca_cert: None,
            client_cert: None,
            client_key: None,
            server_name: None,
            insecure_skip_verify: true,
            supported_versions: vec!["TLSv1.3".to_string(), "TLSv1.2".to_string()],
            validation: CertificateValidationConfig {
                verify_chain: false,
                verify_expiration: false,
                check_revocation: false,
                allow_self_signed: true,
                max_chain_depth: 10,
            },
            alpn_protocols: vec!["h3".to_string()],
        }
    }
    
    /// Check if this configuration enables TLS
    pub fn is_enabled(&self) -> bool {
        self.mode != TlsMode::Disabled
    }
    
    /// Check if this configuration requires client certificates
    pub fn requires_client_cert(&self) -> bool {
        self.mode == TlsMode::MutualAuth
    }
    
    /// Validate the TLS configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == TlsMode::Disabled {
            return Ok(()); // No validation needed for disabled TLS
        }
        
        if self.mode == TlsMode::MutualAuth {
            if self.client_cert.is_none() {
                return Err("Client certificate required for mutual TLS".to_string());
            }
            if self.client_key.is_none() {
                return Err("Client private key required for mutual TLS".to_string());
            }
        }
        
        if !self.insecure_skip_verify && self.ca_cert.is_none() {
            return Err("CA certificate required when certificate verification is enabled".to_string());
        }
        
        if self.supported_versions.is_empty() {
            return Err("At least one TLS version must be supported".to_string());
        }
        
        Ok(())
    }
}