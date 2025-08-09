use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub broker: BrokerConfig,
    pub network: NetworkConfig,
    pub wal: WalConfig,
    pub cache: CacheConfig,
    pub object_storage: ObjectStorageConfig,
    pub controller: ControllerConfig,
    pub replication: ReplicationConfig,
    pub etl: EtlConfig,
    pub scaling: ScalingConfig,
    pub operations: OperationsConfig,
    pub rate_limiting: RateLimitConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub id: String,
    pub rack_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub quic_listen: String,
    pub rpc_listen: String,
    pub max_connections: usize,
    pub connection_timeout_ms: u64,
    pub quic_config: QuicConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    pub max_concurrent_uni_streams: u32,
    pub max_concurrent_bidi_streams: u32,
    pub max_idle_timeout_ms: u64,
    pub max_stream_data: u64,
    pub max_connection_data: u64,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            max_concurrent_uni_streams: 1000,
            max_concurrent_bidi_streams: 1000,
            max_idle_timeout_ms: 30_000, // 30 seconds
            max_stream_data: 1_024_000, // 1MB per stream
            max_connection_data: 10_240_000, // 10MB per connection
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalConfig {
    pub path: PathBuf,
    pub capacity_bytes: u64,
    pub fsync_on_write: bool,
    pub segment_size_bytes: u64,
    pub buffer_size: usize,
    /// Time interval in milliseconds after which WAL segments are uploaded regardless of size
    pub upload_interval_ms: u64,
    /// Flush interval in milliseconds when fsync_on_write is false
    pub flush_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub write_cache_size_bytes: u64,
    pub read_cache_size_bytes: u64,
    pub eviction_policy: EvictionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionPolicy {
    Lru,
    Lfu,
    Random,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStorageConfig {
    pub storage_type: StorageType,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub multipart_threshold: u64,
    pub max_concurrent_uploads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageType {
    S3,
    Gcs,
    Azure,
    Local { path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConfig {
    pub endpoints: Vec<String>,
    pub election_timeout_ms: u64,
    pub heartbeat_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub min_in_sync_replicas: usize,
    pub ack_timeout_ms: u64,
    pub max_replication_lag: u64,
    /// Heartbeat timeout in milliseconds - how long to wait before considering a follower unresponsive
    pub heartbeat_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlConfig {
    pub enabled: bool,
    pub memory_limit_bytes: usize,
    pub execution_timeout_ms: u64,
    pub max_concurrent_executions: usize,
    /// Priority-based pipeline configurations
    pub pipelines: Vec<EtlPipelineConfig>,
    /// Instance pool configuration for WASM modules
    pub instance_pool: EtlInstancePoolConfig,
}

/// Priority-based ETL pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlPipelineConfig {
    /// Unique pipeline identifier
    pub pipeline_id: String,
    /// Human-readable pipeline name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Enable/disable this pipeline
    pub enabled: bool,
    /// Ordered stages with priority levels
    pub stages: Vec<EtlStage>,
    /// Global timeout for entire pipeline execution (ms)
    pub global_timeout_ms: u64,
    /// Maximum retry attempts for failed modules
    pub max_retries: u32,
    /// Error handling strategy for pipeline failures
    pub error_handling: ErrorHandlingStrategy,
}

/// ETL stage representing a priority level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlStage {
    /// Priority level (lower numbers execute first: 0 → 1 → 2 → ...)
    pub priority: u32,
    /// Modules to execute at this priority level
    pub modules: Vec<EtlModuleInstance>,
    /// Allow concurrent execution of modules at same priority
    pub parallel_execution: bool,
    /// Stage-specific timeout override (ms)
    pub stage_timeout_ms: Option<u64>,
    /// Continue pipeline execution even if a module fails
    pub continue_on_error: bool,
}

/// ETL module instance configuration with filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlModuleInstance {
    /// Module identifier
    pub module_id: String,
    /// Instance-specific configuration
    pub instance_config: ModuleInstanceConfig,
    /// Topic filters to determine when this module should execute
    pub topic_filters: Vec<TopicFilter>,
    /// Conditional rules for message-based filtering
    pub conditional_rules: Vec<ConditionalRule>,
}

/// Topic filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicFilter {
    /// Type of filter to apply
    pub filter_type: FilterType,
    /// Pattern string (format depends on filter type)
    pub pattern: String,
    /// Case-sensitive matching
    pub case_sensitive: bool,
    /// Invert the match result
    pub negate: bool,
}

/// Supported filter types with performance characteristics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterType {
    /// Direct string comparison (O(1) hash lookup)
    Exact,
    /// Glob patterns with * and ? (compiled patterns)
    Wildcard,
    /// Full regex support (cached compiled regex)
    Regex,
    /// String prefix matching (linear scan)
    Prefix,
    /// String suffix matching (linear scan)
    Suffix,
    /// Substring matching (contains)
    Contains,
}

/// Conditional rule for message filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalRule {
    /// Type of condition to evaluate
    pub condition_type: ConditionType,
    /// JSONPath-style field selector
    pub field_path: String,
    /// Comparison operator
    pub operator: ComparisonOperator,
    /// Value to compare against
    pub value: serde_json::Value,
    /// Invert the condition result
    pub negate: bool,
}

/// Condition evaluation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionType {
    /// Check message header value
    HeaderValue,
    /// Check field in JSON payload using JSONPath
    PayloadField,
    /// Check total message size in bytes
    MessageSize,
    /// Check message age based on timestamp
    MessageAge,
}

/// Comparison operators for conditional rules
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ComparisonOperator {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Contains,
    StartsWith,
    EndsWith,
    /// Regex pattern matching
    Matches,
}

/// Error handling strategies for pipeline failures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorHandlingStrategy {
    /// Stop entire pipeline execution on first error
    StopPipeline,
    /// Skip failed module and continue with next
    SkipModule,
    /// Retry with exponential backoff
    RetryWithBackoff,
    /// Route failed message to dead letter topic
    SendToDeadLetter,
}

/// Module instance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInstanceConfig {
    /// Memory limit for module instance
    pub memory_limit_bytes: usize,
    /// Execution timeout for single invocation
    pub execution_timeout_ms: u64,
    /// Maximum concurrent instances of this module
    pub max_concurrent_instances: u32,
    /// Enable result caching
    pub enable_caching: bool,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Module-specific custom configuration
    pub custom_config: serde_json::Value,
}

/// WASM instance pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlInstancePoolConfig {
    /// Maximum number of instances per module in pool
    pub max_pool_size: usize,
    /// Number of instances to pre-warm on startup
    pub warmup_instances: usize,
    /// Instance creation rate limit (instances per second)
    pub creation_rate_limit: f64,
    /// Instance idle timeout before eviction (seconds)
    pub idle_timeout_seconds: u64,
    /// Enable LRU eviction when pool is full
    pub enable_lru_eviction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    /// Maximum number of brokers that can be added simultaneously
    pub max_concurrent_additions: usize,
    /// Maximum number of brokers that can be decommissioned simultaneously
    pub max_concurrent_decommissions: usize,
    /// Timeout for partition rebalancing during scaling operations (ms)
    pub rebalance_timeout_ms: u64,
    /// Gradual traffic migration rate (0.0 to 1.0 per minute)
    pub traffic_migration_rate: f64,
    /// Health check timeout for new brokers (ms)
    pub health_check_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsConfig {
    /// Enable runtime configuration updates
    pub allow_runtime_config_updates: bool,
    /// Rolling upgrade velocity (brokers per minute)
    pub upgrade_velocity: usize,
    /// Graceful shutdown timeout (ms)
    pub graceful_shutdown_timeout_ms: u64,
    /// Kubernetes deployment configuration
    pub kubernetes: KubernetesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfig {
    /// Use StatefulSets for deployment
    pub use_stateful_sets: bool,
    /// Persistent volume claim template
    pub pvc_storage_class: String,
    /// Volume size for WAL storage
    pub wal_volume_size: String,
    /// Pod affinity rules for volume attachment
    pub enable_pod_affinity: bool,
}

/// Comprehensive rate limiting configuration using Token Bucket algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Global rate limiting toggle - enable/disable all rate limiting
    pub enabled: bool,
    /// Global rate limits that apply across all clients and endpoints
    pub global: GlobalRateLimits,
    /// Per-IP rate limits with burst allowance
    pub per_ip: PerIpRateLimits,
    /// Endpoint-specific rate limits categorized by operation type
    pub endpoints: EndpointRateLimits,
    /// Cleanup configuration for expired rate limiters
    pub cleanup: RateLimitCleanupConfig,
}

/// Global rate limits that apply across the entire admin API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRateLimits {
    /// Maximum requests per second across all clients
    pub requests_per_second: u32,
    /// Maximum burst capacity for global rate limiting
    pub burst_capacity: u32,
    /// Window size in seconds for rate limit calculations
    pub window_size_secs: u64,
}

/// Per-IP rate limiting configuration with burst allowance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerIpRateLimits {
    /// Enable per-IP rate limiting
    pub enabled: bool,
    /// Maximum requests per second per IP address
    pub requests_per_second: u32,
    /// Burst capacity - allows short bursts above the sustained rate
    pub burst_capacity: u32,
    /// Window size in seconds for per-IP rate calculations
    pub window_size_secs: u64,
    /// Maximum number of IP addresses to track simultaneously
    pub max_tracked_ips: usize,
    /// Time to keep IP rate limiters after last access (seconds)
    pub ip_expiry_secs: u64,
}

/// Endpoint-specific rate limits categorized by operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRateLimits {
    /// Rate limits for health check endpoints (frequent monitoring)
    pub health: EndpointCategoryLimits,
    /// Rate limits for read operations (listing, describing resources)
    pub read_operations: EndpointCategoryLimits,
    /// Rate limits for write operations (creating, deleting resources)
    pub write_operations: EndpointCategoryLimits,
    /// Rate limits for cluster management operations (most expensive)
    pub cluster_operations: EndpointCategoryLimits,
}

/// Rate limiting configuration for a specific category of endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointCategoryLimits {
    /// Enable rate limiting for this endpoint category
    pub enabled: bool,
    /// Maximum requests per second for this category
    pub requests_per_second: u32,
    /// Burst capacity for this category
    pub burst_capacity: u32,
    /// Window size in seconds for this category
    pub window_size_secs: u64,
    /// List of endpoint patterns that belong to this category
    pub endpoint_patterns: Vec<String>,
}

/// Configuration for cleaning up expired rate limiters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitCleanupConfig {
    /// Enable automatic cleanup of expired rate limiters
    pub enabled: bool,
    /// Interval between cleanup runs (seconds)
    pub cleanup_interval_secs: u64,
    /// Maximum age of unused rate limiters before cleanup (seconds)
    pub max_age_secs: u64,
    /// Maximum number of rate limiters to clean per run
    pub max_cleanup_per_run: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            global: GlobalRateLimits::default(),
            per_ip: PerIpRateLimits::default(),
            endpoints: EndpointRateLimits::default(),
            cleanup: RateLimitCleanupConfig::default(),
        }
    }
}

impl Default for GlobalRateLimits {
    fn default() -> Self {
        Self {
            requests_per_second: 1000, // 1000 RPS globally
            burst_capacity: 2000,       // Allow burst up to 2000 requests
            window_size_secs: 60,       // 1-minute window for rate calculations
        }
    }
}

impl Default for PerIpRateLimits {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_second: 50,    // 50 RPS per IP
            burst_capacity: 100,        // Allow burst up to 100 requests per IP
            window_size_secs: 60,       // 1-minute window
            max_tracked_ips: 10000,     // Track up to 10,000 IP addresses
            ip_expiry_secs: 3600,       // Remove IP limiters after 1 hour of inactivity
        }
    }
}

impl Default for EndpointRateLimits {
    fn default() -> Self {
        Self {
            health: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 100,   // Health checks are frequent
                burst_capacity: 200,
                window_size_secs: 60,
                endpoint_patterns: vec![
                    "/health".to_string(),
                    "/api/v1/health".to_string(),
                ],
            },
            read_operations: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 30,    // Moderate limits for read operations
                burst_capacity: 60,
                window_size_secs: 60,
                endpoint_patterns: vec![
                    "/api/v1/cluster".to_string(),
                    "/api/v1/topics".to_string(),
                    "/api/v1/topics/*".to_string(), // GET operations
                    "/api/v1/brokers".to_string(),
                ],
            },
            write_operations: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 10,    // Lower limits for write operations
                burst_capacity: 20,
                window_size_secs: 60,
                endpoint_patterns: vec![
                    "POST:/api/v1/topics".to_string(),
                    "DELETE:/api/v1/topics/*".to_string(),
                ],
            },
            cluster_operations: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 5,     // Very restrictive for cluster operations
                burst_capacity: 10,
                window_size_secs: 60,
                endpoint_patterns: vec![
                    "/api/v1/cluster/rebalance".to_string(),
                    "/api/v1/cluster/scale".to_string(),
                    "/api/v1/brokers/*/decommission".to_string(),
                ],
            },
        }
    }
}

impl Default for RateLimitCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cleanup_interval_secs: 300,    // Clean up every 5 minutes
            max_age_secs: 3600,            // Remove limiters after 1 hour of inactivity
            max_cleanup_per_run: 1000,     // Clean up to 1000 limiters per run
        }
    }
}

impl RateLimitConfig {
    /// Validate the rate limiting configuration
    pub fn validate(&self) -> crate::Result<()> {
        // Validate global limits
        self.global.validate()?;
        
        // Validate per-IP limits
        self.per_ip.validate()?;
        
        // Validate endpoint limits
        self.endpoints.validate()?;
        
        // Validate cleanup configuration
        self.cleanup.validate()?;
        
        Ok(())
    }
}

impl GlobalRateLimits {
    pub fn validate(&self) -> crate::Result<()> {
        if self.requests_per_second == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.requests_per_second must be greater than 0".to_string(),
            ));
        }
        
        if self.burst_capacity == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.burst_capacity must be greater than 0".to_string(),
            ));
        }
        
        if self.burst_capacity < self.requests_per_second {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.burst_capacity must be >= requests_per_second".to_string(),
            ));
        }
        
        if self.window_size_secs == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.window_size_secs must be greater than 0".to_string(),
            ));
        }
        
        // Reasonable upper bounds
        if self.requests_per_second > 1_000_000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.requests_per_second exceeds reasonable limit (1M RPS)".to_string(),
            ));
        }
        
        Ok(())
    }
}

impl PerIpRateLimits {
    pub fn validate(&self) -> crate::Result<()> {
        if self.enabled {
            if self.requests_per_second == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.requests_per_second must be greater than 0".to_string(),
                ));
            }
            
            if self.burst_capacity == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.burst_capacity must be greater than 0".to_string(),
                ));
            }
            
            if self.burst_capacity < self.requests_per_second {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.burst_capacity must be >= requests_per_second".to_string(),
                ));
            }
            
            if self.window_size_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.window_size_secs must be greater than 0".to_string(),
                ));
            }
            
            if self.max_tracked_ips == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.max_tracked_ips must be greater than 0".to_string(),
                ));
            }
            
            if self.ip_expiry_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.ip_expiry_secs must be greater than 0".to_string(),
                ));
            }
            
            // Sanity check: don't track too many IPs (memory usage concern)
            if self.max_tracked_ips > 1_000_000 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.max_tracked_ips exceeds reasonable limit (1M IPs)".to_string(),
                ));
            }
        }
        
        Ok(())
    }
}

impl EndpointRateLimits {
    pub fn validate(&self) -> crate::Result<()> {
        self.health.validate_category("health")?;
        self.read_operations.validate_category("read_operations")?;
        self.write_operations.validate_category("write_operations")?;
        self.cluster_operations.validate_category("cluster_operations")?;
        Ok(())
    }
}

impl EndpointCategoryLimits {
    pub fn validate_category(&self, category_name: &str) -> crate::Result<()> {
        if self.enabled {
            if self.requests_per_second == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    format!("rate_limiting.endpoints.{}.requests_per_second must be greater than 0", category_name),
                ));
            }
            
            if self.burst_capacity == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    format!("rate_limiting.endpoints.{}.burst_capacity must be greater than 0", category_name),
                ));
            }
            
            if self.burst_capacity < self.requests_per_second {
                return Err(crate::error::RustMqError::InvalidConfig(
                    format!("rate_limiting.endpoints.{}.burst_capacity must be >= requests_per_second", category_name),
                ));
            }
            
            if self.window_size_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    format!("rate_limiting.endpoints.{}.window_size_secs must be greater than 0", category_name),
                ));
            }
            
            if self.endpoint_patterns.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(
                    format!("rate_limiting.endpoints.{}.endpoint_patterns cannot be empty when enabled", category_name),
                ));
            }
            
            // Validate endpoint patterns are not empty strings
            for pattern in &self.endpoint_patterns {
                if pattern.trim().is_empty() {
                    return Err(crate::error::RustMqError::InvalidConfig(
                        format!("rate_limiting.endpoints.{}.endpoint_patterns contains empty pattern", category_name),
                    ));
                }
            }
        }
        
        Ok(())
    }
}

impl RateLimitCleanupConfig {
    pub fn validate(&self) -> crate::Result<()> {
        if self.enabled {
            if self.cleanup_interval_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.cleanup_interval_secs must be greater than 0".to_string(),
                ));
            }
            
            if self.max_age_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.max_age_secs must be greater than 0".to_string(),
                ));
            }
            
            if self.max_cleanup_per_run == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.max_cleanup_per_run must be greater than 0".to_string(),
                ));
            }
            
            // Reasonable bounds
            if self.cleanup_interval_secs < 10 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.cleanup_interval_secs should be at least 10 seconds".to_string(),
                ));
            }
            
            if self.max_cleanup_per_run > 100_000 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.max_cleanup_per_run exceeds reasonable limit (100K)".to_string(),
                ));
            }
        }
        
        Ok(())
    }
}

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
    /// Number of L2 cache shards
    pub l2_shard_count: usize,
    /// Bloom filter size for negative caching
    pub bloom_filter_size: usize,
    /// Batch fetch size for controller requests
    pub batch_fetch_size: usize,
    /// Enable audit logging for authorization events
    pub enable_audit_logging: bool,
    /// Enable negative caching with bloom filter
    pub negative_cache_enabled: bool,
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
            l2_shard_count: 32,
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
            
            if self.l2_shard_count == 0 || (self.l2_shard_count & (self.l2_shard_count - 1)) != 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "security.acl.l2_shard_count must be a power of 2".to_string(),
                ));
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
                "security.certificate_management.cert_validity_days must be greater than 0".to_string(),
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
                "security.audit.max_log_size_mb must be greater than 0 when audit is enabled".to_string(),
            ));
        }
        
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: BrokerConfig {
                id: "broker-001".to_string(),
                rack_id: "default".to_string(),
            },
            network: NetworkConfig {
                quic_listen: "0.0.0.0:9092".to_string(),
                rpc_listen: "0.0.0.0:9093".to_string(),
                max_connections: 10000,
                connection_timeout_ms: 30000,
                quic_config: QuicConfig::default(),
            },
            wal: WalConfig {
                path: PathBuf::from("/tmp/rustmq/wal"),
                capacity_bytes: 10 * 1024 * 1024 * 1024, // 10GB
                fsync_on_write: true,
                segment_size_bytes: 128 * 1024 * 1024, // 128MB
                buffer_size: 64 * 1024, // 64KB
                upload_interval_ms: 10 * 60 * 1000, // 10 minutes
                flush_interval_ms: 1000, // 1 second
            },
            cache: CacheConfig {
                write_cache_size_bytes: 1024 * 1024 * 1024, // 1GB
                read_cache_size_bytes: 2 * 1024 * 1024 * 1024, // 2GB
                eviction_policy: EvictionPolicy::Lru,
            },
            object_storage: ObjectStorageConfig {
                storage_type: StorageType::Local {
                    path: PathBuf::from("/tmp/rustmq/storage"),
                },
                bucket: "rustmq-data".to_string(),
                region: "us-central1".to_string(),
                endpoint: "https://storage.googleapis.com".to_string(),
                access_key: None,
                secret_key: None,
                multipart_threshold: 100 * 1024 * 1024, // 100MB
                max_concurrent_uploads: 10,
            },
            controller: ControllerConfig {
                endpoints: vec!["controller-1:9094".to_string()],
                election_timeout_ms: 5000,
                heartbeat_interval_ms: 1000,
            },
            replication: ReplicationConfig {
                min_in_sync_replicas: 2,
                ack_timeout_ms: 5000,
                max_replication_lag: 1000,
                heartbeat_timeout_ms: 30000, // 30 seconds
            },
            etl: EtlConfig {
                enabled: false,
                memory_limit_bytes: 64 * 1024 * 1024, // 64MB
                execution_timeout_ms: 5000,
                max_concurrent_executions: 100,
                pipelines: vec![], // No pipelines by default
                instance_pool: EtlInstancePoolConfig {
                    max_pool_size: 50,
                    warmup_instances: 5,
                    creation_rate_limit: 10.0, // 10 instances per second
                    idle_timeout_seconds: 300, // 5 minutes
                    enable_lru_eviction: true,
                },
            },
            scaling: ScalingConfig {
                max_concurrent_additions: 3,
                max_concurrent_decommissions: 1, // Safety constraint: one at a time
                rebalance_timeout_ms: 300_000, // 5 minutes
                traffic_migration_rate: 0.1, // 10% per minute
                health_check_timeout_ms: 30_000, // 30 seconds
            },
            operations: OperationsConfig {
                allow_runtime_config_updates: true,
                upgrade_velocity: 1, // 1 broker per minute
                graceful_shutdown_timeout_ms: 60_000, // 1 minute
                kubernetes: KubernetesConfig {
                    use_stateful_sets: true,
                    pvc_storage_class: "fast-ssd".to_string(),
                    wal_volume_size: "50Gi".to_string(),
                    enable_pod_affinity: true,
                },
            },
            rate_limiting: RateLimitConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| crate::error::RustMqError::Config(e.to_string()))?;
        Ok(config)
    }

    pub fn validate(&self) -> crate::Result<()> {
        if self.broker.id.is_empty() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "broker.id cannot be empty".to_string(),
            ));
        }

        if self.wal.capacity_bytes == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "wal.capacity_bytes must be greater than 0".to_string(),
            ));
        }

        if self.replication.min_in_sync_replicas == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "replication.min_in_sync_replicas must be greater than 0".to_string(),
            ));
        }

        if self.scaling.max_concurrent_decommissions == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "scaling.max_concurrent_decommissions must be greater than 0".to_string(),
            ));
        }

        if self.scaling.max_concurrent_decommissions > 10 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "scaling.max_concurrent_decommissions should not exceed 10 for safety".to_string(),
            ));
        }

        if self.replication.heartbeat_timeout_ms == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "replication.heartbeat_timeout_ms must be greater than 0".to_string(),
            ));
        }

        // Validate rate limiting configuration
        self.rate_limiting.validate()?;

        // Validate security configuration
        self.security.validate()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation_max_concurrent_decommissions() {
        let mut config = Config::default();
        
        // Test zero value (should fail)
        config.scaling.max_concurrent_decommissions = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be greater than 0"));

        // Test exceeding maximum (should fail)
        config.scaling.max_concurrent_decommissions = 15;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("should not exceed 10"));

        // Test valid value (should pass)
        config.scaling.max_concurrent_decommissions = 2;
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_validation_heartbeat_timeout() {
        let mut config = Config::default();
        
        // Test zero value (should fail)
        config.replication.heartbeat_timeout_ms = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("heartbeat_timeout_ms must be greater than 0"));

        // Test valid value (should pass)
        config.replication.heartbeat_timeout_ms = 30000;
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.scaling.max_concurrent_decommissions, 1);
        assert_eq!(config.replication.heartbeat_timeout_ms, 30000);
    }

    #[test]
    fn test_rate_limiting_default_config() {
        let rate_limit_config = RateLimitConfig::default();
        assert!(rate_limit_config.validate().is_ok());
        
        // Test default values
        assert!(rate_limit_config.enabled);
        assert_eq!(rate_limit_config.global.requests_per_second, 1000);
        assert_eq!(rate_limit_config.global.burst_capacity, 2000);
        assert_eq!(rate_limit_config.per_ip.requests_per_second, 50);
        assert_eq!(rate_limit_config.per_ip.burst_capacity, 100);
        assert!(rate_limit_config.per_ip.enabled);
        
        // Test endpoint defaults
        let endpoints = &rate_limit_config.endpoints;
        assert!(endpoints.health.enabled);
        assert_eq!(endpoints.health.requests_per_second, 100);
        assert_eq!(endpoints.read_operations.requests_per_second, 30);
        assert_eq!(endpoints.write_operations.requests_per_second, 10);
        assert_eq!(endpoints.cluster_operations.requests_per_second, 5);
        
        // Test cleanup defaults
        assert!(rate_limit_config.cleanup.enabled);
        assert_eq!(rate_limit_config.cleanup.cleanup_interval_secs, 300);
    }

    #[test]
    fn test_rate_limiting_validation_failures() {
        let mut config = RateLimitConfig::default();
        
        // Test invalid global requests_per_second
        config.global.requests_per_second = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("global.requests_per_second must be greater than 0"));
        
        // Reset to valid value
        config.global.requests_per_second = 100;
        
        // Test invalid global burst_capacity
        config.global.burst_capacity = 50; // Less than requests_per_second
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("global.burst_capacity must be >= requests_per_second"));
        
        // Reset to valid value
        config.global.burst_capacity = 200;
        
        // Test invalid per-IP configuration
        config.per_ip.max_tracked_ips = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("per_ip.max_tracked_ips must be greater than 0"));
        
        // Reset to valid value
        config.per_ip.max_tracked_ips = 1000;
        
        // Test empty endpoint patterns
        config.endpoints.health.endpoint_patterns.clear();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("health.endpoint_patterns cannot be empty when enabled"));
    }

    #[test]
    fn test_rate_limiting_extreme_values() {
        let mut config = RateLimitConfig::default();
        
        // Test excessive global RPS
        config.global.requests_per_second = 2_000_000; // Above limit
        config.global.burst_capacity = 2_000_000; // Make sure burst is not less than RPS
        let result = config.validate();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("exceeds reasonable limit") || error_msg.contains("1M RPS"));
        
        // Test excessive max_tracked_ips
        config.global.requests_per_second = 1000; // Reset to valid
        config.per_ip.max_tracked_ips = 2_000_000; // Above limit
        let result = config.validate();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("exceeds reasonable limit") || error_msg.contains("1M IPs"));
        
        // Test very short cleanup interval
        config.per_ip.max_tracked_ips = 10000; // Reset to valid
        config.cleanup.cleanup_interval_secs = 5; // Below minimum
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("should be at least 10 seconds"));
    }

    #[test]
    fn test_rate_limiting_disabled_validation() {
        let mut config = RateLimitConfig::default();
        
        // Disable per-IP rate limiting
        config.per_ip.enabled = false;
        config.per_ip.requests_per_second = 0; // Invalid value, but should be ignored
        
        // Should pass validation because per_ip is disabled
        assert!(config.validate().is_ok());
        
        // Disable endpoint category
        config.endpoints.health.enabled = false;
        config.endpoints.health.requests_per_second = 0; // Invalid value, but should be ignored
        config.endpoints.health.endpoint_patterns.clear(); // Invalid, but should be ignored
        
        // Should still pass validation because health endpoints are disabled
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_endpoint_patterns_validation() {
        let mut config = RateLimitConfig::default();
        
        // Test empty pattern string
        config.endpoints.health.endpoint_patterns = vec!["".to_string(), "  ".to_string()];
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("contains empty pattern"));
        
        // Test valid patterns
        config.endpoints.health.endpoint_patterns = vec![
            "/health".to_string(),
            "/api/v1/health".to_string(),
            "GET:/api/v1/status".to_string(),
        ];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_endpoint_category_limits_individual_validation() {
        let mut category = EndpointCategoryLimits {
            enabled: true,
            requests_per_second: 10,
            burst_capacity: 5, // Invalid: less than RPS
            window_size_secs: 60,
            endpoint_patterns: vec!["/test".to_string()],
        };
        
        let result = category.validate_category("test_category");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("burst_capacity must be >= requests_per_second"));
        
        // Fix the burst capacity
        category.burst_capacity = 20;
        assert!(category.validate_category("test_category").is_ok());
    }

    #[test]
    fn test_toml_serialization_deserialization() {
        let config = RateLimitConfig::default();
        
        // Serialize to TOML
        let toml_string = toml::to_string(&config).unwrap();
        println!("Serialized TOML:\n{}", toml_string);
        
        // Deserialize from TOML
        let deserialized: RateLimitConfig = toml::from_str(&toml_string).unwrap();
        
        // Verify the deserialized config is valid
        assert!(deserialized.validate().is_ok());
        
        // Verify some key values match
        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(config.global.requests_per_second, deserialized.global.requests_per_second);
        assert_eq!(config.per_ip.max_tracked_ips, deserialized.per_ip.max_tracked_ips);
        assert_eq!(config.endpoints.health.requests_per_second, deserialized.endpoints.health.requests_per_second);
    }

    #[test]
    fn test_full_config_with_rate_limiting() {
        let config = Config::default();
        
        // Serialize the full config to TOML
        let toml_string = toml::to_string(&config).unwrap();
        
        // Deserialize it back
        let deserialized: Config = toml::from_str(&toml_string).unwrap();
        
        // Validate the full config including rate limiting
        assert!(deserialized.validate().is_ok());
        
        // Verify rate limiting is properly included
        assert!(deserialized.rate_limiting.enabled);
        assert_eq!(deserialized.rate_limiting.global.requests_per_second, 1000);
    }
}