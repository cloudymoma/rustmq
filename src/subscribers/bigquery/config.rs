use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigQuerySubscriberConfig {
    /// Google Cloud Project ID
    pub project_id: String,
    
    /// BigQuery dataset name
    pub dataset: String,
    
    /// BigQuery table name
    pub table: String,
    
    /// BigQuery write method
    pub write_method: WriteMethod,
    
    /// RustMQ subscription configuration
    pub subscription: SubscriptionConfig,
    
    /// Authentication configuration
    pub auth: AuthConfig,
    
    /// Batching configuration
    pub batching: BatchingConfig,
    
    /// Schema configuration
    pub schema: SchemaConfig,
    
    /// Error handling configuration
    pub error_handling: ErrorHandlingConfig,
    
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriteMethod {
    /// Use BigQuery Streaming Inserts API (default)
    #[serde(rename = "streaming_inserts")]
    StreamingInserts {
        /// Skip invalid rows
        skip_invalid_rows: bool,
        /// Ignore unknown values
        ignore_unknown_values: bool,
        /// Template suffix for table partitioning
        template_suffix: Option<String>,
    },
    /// Use BigQuery Storage Write API (faster, more efficient)
    #[serde(rename = "storage_write")]
    StorageWrite {
        /// Write stream type
        stream_type: StorageWriteStreamType,
        /// Enable auto-commit
        auto_commit: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageWriteStreamType {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "committed")]
    Committed,
    #[serde(rename = "pending")]
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    /// RustMQ broker endpoints
    pub broker_endpoints: Vec<String>,
    
    /// Topic to subscribe to
    pub topic: String,
    
    /// Consumer group ID
    pub consumer_group: String,
    
    /// Partition IDs to consume (None means all partitions)
    pub partitions: Option<Vec<u32>>,
    
    /// Start from offset (latest, earliest, or specific offset)
    pub start_offset: StartOffset,
    
    /// Maximum number of messages to fetch per request
    pub max_messages_per_fetch: usize,
    
    /// Fetch timeout in milliseconds
    pub fetch_timeout_ms: u64,
    
    /// Consumer session timeout in milliseconds
    pub session_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StartOffset {
    #[serde(rename = "latest")]
    Latest,
    #[serde(rename = "earliest")]
    Earliest,
    #[serde(rename = "specific")]
    Specific(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication method
    pub method: AuthMethod,
    
    /// Service account key file path (for service account auth)
    pub service_account_key_file: Option<String>,
    
    /// Scopes for authentication
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    #[serde(rename = "service_account")]
    ServiceAccount,
    #[serde(rename = "metadata_server")]
    MetadataServer,
    #[serde(rename = "application_default")]
    ApplicationDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchingConfig {
    /// Maximum number of rows per batch
    pub max_rows_per_batch: usize,
    
    /// Maximum batch size in bytes
    pub max_batch_size_bytes: usize,
    
    /// Maximum time to wait before sending a partial batch (milliseconds)
    pub max_batch_latency_ms: u64,
    
    /// Maximum number of concurrent batches being processed
    pub max_concurrent_batches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConfig {
    /// Schema mapping strategy
    pub mapping: SchemaMappingStrategy,
    
    /// Column mappings for custom mapping
    pub column_mappings: HashMap<String, String>,
    
    /// Default values for missing fields
    pub default_values: HashMap<String, serde_json::Value>,
    
    /// Whether to auto-create table if it doesn't exist
    pub auto_create_table: bool,
    
    /// Table schema for auto-creation
    pub table_schema: Option<Vec<BigQueryFieldSchema>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchemaMappingStrategy {
    /// Use message as-is (expects JSON format)
    #[serde(rename = "direct")]
    Direct,
    /// Map specific fields from message
    #[serde(rename = "custom")]
    Custom,
    /// Extract fields from nested JSON
    #[serde(rename = "nested")]
    Nested { root_field: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigQueryFieldSchema {
    pub name: String,
    pub field_type: String,
    pub mode: String, // REQUIRED, OPTIONAL, REPEATED
    pub description: Option<String>,
    pub fields: Option<Vec<BigQueryFieldSchema>>, // For nested/record fields
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandlingConfig {
    /// Maximum number of retries for failed inserts
    pub max_retries: usize,
    
    /// Retry backoff strategy
    pub retry_backoff: RetryBackoffStrategy,
    
    /// What to do with messages that fail after all retries
    pub dead_letter_action: DeadLetterAction,
    
    /// Dead letter queue configuration
    pub dead_letter_config: Option<DeadLetterConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryBackoffStrategy {
    #[serde(rename = "linear")]
    Linear { increment_ms: u64 },
    #[serde(rename = "exponential")]
    Exponential { base_ms: u64, max_ms: u64 },
    #[serde(rename = "fixed")]
    Fixed { delay_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeadLetterAction {
    #[serde(rename = "drop")]
    Drop,
    #[serde(rename = "log")]
    Log,
    #[serde(rename = "dead_letter_queue")]
    DeadLetterQueue,
    #[serde(rename = "file")]
    File { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterConfig {
    /// Dead letter topic
    pub topic: String,
    /// Dead letter producer configuration
    pub producer_config: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable metrics collection
    pub enable_metrics: bool,
    
    /// Metrics prefix
    pub metrics_prefix: String,
    
    /// Enable health check endpoint
    pub enable_health_check: bool,
    
    /// Health check bind address
    pub health_check_addr: String,
    
    /// Enable logging of successful inserts
    pub log_successful_inserts: bool,
    
    /// Log level for BigQuery operations
    pub log_level: String,
}

impl Default for BigQuerySubscriberConfig {
    fn default() -> Self {
        Self {
            project_id: "".to_string(),
            dataset: "".to_string(),
            table: "".to_string(),
            write_method: WriteMethod::StreamingInserts {
                skip_invalid_rows: false,
                ignore_unknown_values: false,
                template_suffix: None,
            },
            subscription: SubscriptionConfig::default(),
            auth: AuthConfig::default(),
            batching: BatchingConfig::default(),
            schema: SchemaConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
            monitoring: MonitoringConfig::default(),
        }
    }
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            broker_endpoints: vec!["localhost:9092".to_string()],
            topic: "".to_string(),
            consumer_group: "bigquery-subscriber".to_string(),
            partitions: None,
            start_offset: StartOffset::Latest,
            max_messages_per_fetch: 1000,
            fetch_timeout_ms: 5000,
            session_timeout_ms: 30000,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            method: AuthMethod::ApplicationDefault,
            service_account_key_file: None,
            scopes: vec![
                "https://www.googleapis.com/auth/bigquery".to_string(),
                "https://www.googleapis.com/auth/bigquery.insertdata".to_string(),
            ],
        }
    }
}

impl Default for BatchingConfig {
    fn default() -> Self {
        Self {
            max_rows_per_batch: 1000,
            max_batch_size_bytes: 1024 * 1024 * 10, // 10MB
            max_batch_latency_ms: 1000,
            max_concurrent_batches: 10,
        }
    }
}

impl Default for SchemaConfig {
    fn default() -> Self {
        Self {
            mapping: SchemaMappingStrategy::Direct,
            column_mappings: HashMap::new(),
            default_values: HashMap::new(),
            auto_create_table: false,
            table_schema: None,
        }
    }
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_backoff: RetryBackoffStrategy::Exponential {
                base_ms: 1000,
                max_ms: 30000,
            },
            dead_letter_action: DeadLetterAction::Log,
            dead_letter_config: None,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_prefix: "rustmq_bigquery_subscriber".to_string(),
            enable_health_check: true,
            health_check_addr: "0.0.0.0:8080".to_string(),
            log_successful_inserts: false,
            log_level: "info".to_string(),
        }
    }
}