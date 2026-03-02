use serde::{Deserialize, Serialize};

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
    /// Priority level (lower numbers execute first: 0 -> 1 -> 2 -> ...)
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
