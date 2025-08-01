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
    
    /// Enable TLS
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

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Certificate authority certificate
    pub ca_cert: Option<String>,
    
    /// Client certificate
    pub client_cert: Option<String>,
    
    /// Client private key
    pub client_key: Option<String>,
    
    /// Server name for SNI
    pub server_name: Option<String>,
    
    /// Skip certificate verification (insecure)
    pub insecure_skip_verify: bool,
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

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication method
    pub method: AuthMethod,
    
    /// Username for SASL
    pub username: Option<String>,
    
    /// Password for SASL
    pub password: Option<String>,
    
    /// Token for token-based auth
    pub token: Option<String>,
    
    /// Additional authentication properties
    pub properties: std::collections::HashMap<String, String>,
}

/// Authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    None,
    SaslPlain,
    SaslScram256,
    SaslScram512,
    Token,
    Mtls,
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
            tls_config: None,
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