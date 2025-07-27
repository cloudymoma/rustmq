use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// Represents a message from RustMQ that will be inserted into BigQuery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigQueryMessage {
    /// The original message data
    pub data: serde_json::Value,
    
    /// Message metadata
    pub metadata: MessageMetadata,
    
    /// Transformed data ready for BigQuery insertion
    pub transformed_data: Option<serde_json::Value>,
    
    /// Insert ID for deduplication (optional)
    pub insert_id: Option<String>,
}

/// Metadata about the RustMQ message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Topic the message came from
    pub topic: String,
    
    /// Partition ID
    pub partition_id: u32,
    
    /// Message offset within the partition
    pub offset: u64,
    
    /// Message timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Message key (if any)
    pub key: Option<String>,
    
    /// Message headers
    pub headers: HashMap<String, String>,
    
    /// Producer information
    pub producer_id: Option<String>,
    
    /// Sequence number within producer
    pub sequence_number: Option<u64>,
}

/// Represents a batch of messages to be inserted into BigQuery
#[derive(Debug, Clone)]
pub struct BigQueryBatch {
    /// Messages in the batch
    pub messages: Vec<BigQueryMessage>,
    
    /// Batch metadata
    pub metadata: BatchMetadata,
}

/// Metadata about a batch
#[derive(Debug, Clone)]
pub struct BatchMetadata {
    /// Unique batch ID
    pub batch_id: String,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Total size in bytes
    pub size_bytes: usize,
    
    /// Number of messages
    pub message_count: usize,
    
    /// Retry attempt (0 for first attempt)
    pub retry_attempt: usize,
}

/// Result of a BigQuery insert operation
#[derive(Debug, Clone)]
pub struct InsertResult {
    /// Whether the insert was successful
    pub success: bool,
    
    /// Number of rows successfully inserted
    pub rows_inserted: usize,
    
    /// Any errors that occurred
    pub errors: Vec<InsertError>,
    
    /// BigQuery job ID (for storage write API)
    pub job_id: Option<String>,
    
    /// Execution statistics
    pub stats: InsertStats,
}

/// Statistics about an insert operation
#[derive(Debug, Clone)]
pub struct InsertStats {
    /// Total time taken for the operation
    pub duration_ms: u64,
    
    /// Time spent on data transformation
    pub transformation_time_ms: u64,
    
    /// Time spent on BigQuery API call
    pub api_time_ms: u64,
    
    /// Size of data sent to BigQuery
    pub bytes_sent: usize,
    
    /// Number of retries performed
    pub retry_count: usize,
}

/// Represents an error during BigQuery insertion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertError {
    /// Error message
    pub message: String,
    
    /// Error code (if available)
    pub code: Option<String>,
    
    /// Row index that caused the error
    pub row_index: Option<usize>,
    
    /// Field that caused the error
    pub field: Option<String>,
    
    /// Whether this error is retryable
    pub retryable: bool,
}

/// Health status of the BigQuery subscriber
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall status
    pub status: HealthState,
    
    /// Last successful insert timestamp
    pub last_successful_insert: Option<DateTime<Utc>>,
    
    /// Last error timestamp
    pub last_error: Option<DateTime<Utc>>,
    
    /// Current backlog size
    pub backlog_size: usize,
    
    /// Number of messages processed in the last minute
    pub messages_per_minute: u64,
    
    /// Number of successful inserts in the last minute
    pub successful_inserts_per_minute: u64,
    
    /// Number of failed inserts in the last minute
    pub failed_inserts_per_minute: u64,
    
    /// Current error rate (percentage)
    pub error_rate: f64,
    
    /// BigQuery connection status
    pub bigquery_connection: ConnectionStatus,
    
    /// RustMQ connection status
    pub rustmq_connection: ConnectionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthState {
    #[serde(rename = "healthy")]
    Healthy,
    #[serde(rename = "degraded")]
    Degraded,
    #[serde(rename = "unhealthy")]
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionStatus {
    #[serde(rename = "connected")]
    Connected,
    #[serde(rename = "disconnected")]
    Disconnected,
    #[serde(rename = "error")]
    Error { message: String },
}

/// Metrics collected by the BigQuery subscriber
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscriberMetrics {
    /// Total messages received
    pub messages_received: u64,
    
    /// Total messages processed successfully
    pub messages_processed: u64,
    
    /// Total messages failed
    pub messages_failed: u64,
    
    /// Total bytes processed
    pub bytes_processed: u64,
    
    /// Total BigQuery API calls
    pub api_calls: u64,
    
    /// Total successful API calls
    pub successful_api_calls: u64,
    
    /// Total failed API calls
    pub failed_api_calls: u64,
    
    /// Total batches created
    pub batches_created: u64,
    
    /// Total batches sent
    pub batches_sent: u64,
    
    /// Average batch size
    pub avg_batch_size: f64,
    
    /// Average processing latency (milliseconds)
    pub avg_processing_latency_ms: f64,
    
    /// Average API latency (milliseconds)
    pub avg_api_latency_ms: f64,
    
    /// Current backlog size
    pub current_backlog_size: usize,
    
    /// Peak backlog size
    pub peak_backlog_size: usize,
}

/// Configuration validation result
#[derive(Debug)]
pub struct ValidationResult {
    /// Whether the configuration is valid
    pub valid: bool,
    
    /// List of validation errors
    pub errors: Vec<String>,
    
    /// List of validation warnings
    pub warnings: Vec<String>,
}

impl BigQueryMessage {
    /// Create a new BigQuery message from raw data
    pub fn new(
        data: serde_json::Value,
        metadata: MessageMetadata,
    ) -> Self {
        Self {
            data,
            metadata,
            transformed_data: None,
            insert_id: None,
        }
    }
    
    /// Generate a unique insert ID for deduplication
    pub fn generate_insert_id(&mut self) {
        let id = format!(
            "{}_{}_{}_{}", 
            self.metadata.topic,
            self.metadata.partition_id,
            self.metadata.offset,
            self.metadata.timestamp.timestamp_micros()
        );
        self.insert_id = Some(id);
    }
}

impl BigQueryBatch {
    /// Create a new empty batch
    pub fn new(batch_id: String) -> Self {
        Self {
            messages: Vec::new(),
            metadata: BatchMetadata {
                batch_id,
                created_at: Utc::now(),
                size_bytes: 0,
                message_count: 0,
                retry_attempt: 0,
            },
        }
    }
    
    /// Add a message to the batch
    pub fn add_message(&mut self, message: BigQueryMessage) {
        // Estimate message size (rough approximation)
        let message_size = serde_json::to_string(&message.data)
            .map(|s| s.len())
            .unwrap_or(0);
        
        self.metadata.size_bytes += message_size;
        self.metadata.message_count += 1;
        self.messages.push(message);
    }
    
    /// Check if the batch is full based on configuration
    pub fn is_full(&self, max_rows: usize, max_bytes: usize) -> bool {
        self.messages.len() >= max_rows || self.metadata.size_bytes >= max_bytes
    }
    
    /// Check if the batch has aged beyond the maximum latency
    pub fn is_aged(&self, max_latency_ms: u64) -> bool {
        let age_ms = Utc::now()
            .signed_duration_since(self.metadata.created_at)
            .num_milliseconds() as u64;
        age_ms >= max_latency_ms
    }
}

impl InsertResult {
    /// Create a successful insert result
    pub fn success(rows_inserted: usize, stats: InsertStats) -> Self {
        Self {
            success: true,
            rows_inserted,
            errors: Vec::new(),
            job_id: None,
            stats,
        }
    }
    
    /// Create a failed insert result
    pub fn failure(errors: Vec<InsertError>, stats: InsertStats) -> Self {
        Self {
            success: false,
            rows_inserted: 0,
            errors,
            job_id: None,
            stats,
        }
    }
}