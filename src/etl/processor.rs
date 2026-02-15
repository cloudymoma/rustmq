use crate::etl::orchestrator::{EtlPipelineOrchestrator, PipelineExecutionResult};
use crate::{Result, config::EtlConfig, error::RustMqError, types::*};
use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};

/// ETL processor that manages WebAssembly modules for real-time data processing
///
/// This processor provides both legacy single-module execution and new priority-based
/// pipeline execution, maintaining backward compatibility while offering advanced features.
pub struct EtlProcessor {
    #[allow(dead_code)]
    config: EtlConfig,
    /// Legacy module storage for backward compatibility
    modules: Arc<RwLock<HashMap<String, EtlModule>>>,
    execution_semaphore: Arc<Semaphore>,
    metrics: Arc<Mutex<EtlMetrics>>,
    /// New priority-based pipeline orchestrator
    orchestrator: Option<Arc<EtlPipelineOrchestrator>>,
}

/// Represents a loaded ETL module
#[derive(Clone)]
pub struct EtlModule {
    pub id: String,
    pub name: String,
    pub version: String,
    pub config: ModuleConfig,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used: Arc<Mutex<Instant>>,
    pub execution_count: Arc<std::sync::atomic::AtomicU64>,
}

/// Configuration for an ETL module
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleConfig {
    /// Maximum memory usage in bytes
    pub memory_limit_bytes: usize,
    /// Execution timeout in milliseconds
    pub timeout_ms: u64,
    /// Input data format expected by the module
    pub input_format: DataFormat,
    /// Output data format produced by the module
    pub output_format: DataFormat,
    /// Whether to cache module instances
    pub enable_caching: bool,
}

/// Data format specifications
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DataFormat {
    Json,
    Avro,
    Protobuf,
    RawBytes,
}

/// ETL processing request
#[derive(Debug, Clone)]
pub struct EtlRequest {
    pub module_id: String,
    pub input_data: Vec<u8>,
    pub context: ProcessingContext,
}

/// Processing context passed to ETL modules
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessingContext {
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub timestamp: i64,
    pub headers: HashMap<String, String>,
}

/// ETL processing response
#[derive(Debug, Clone)]
pub struct EtlResponse {
    pub success: bool,
    pub output_data: Option<Vec<u8>>,
    pub error_message: Option<String>,
    pub execution_time_ms: u64,
    pub memory_used_bytes: usize,
}

/// ETL processing pipeline trait
#[async_trait]
pub trait EtlPipeline: Send + Sync {
    /// Process a record through the ETL pipeline
    async fn process_record(&self, record: Record, modules: Vec<String>)
    -> Result<ProcessedRecord>;

    /// Load a new ETL module
    async fn load_module(&self, module_data: Vec<u8>, config: ModuleConfig) -> Result<String>;

    /// Unload an ETL module
    async fn unload_module(&self, module_id: &str) -> Result<()>;

    /// List loaded modules
    async fn list_modules(&self) -> Result<Vec<ModuleInfo>>;

    /// Get module execution statistics
    async fn get_module_stats(&self, module_id: &str) -> Result<ModuleStats>;
}

/// Processed record with ETL transformations applied
#[derive(Debug, Clone)]
pub struct ProcessedRecord {
    pub original: Record,
    pub transformed: Record,
    pub processing_metadata: ProcessingMetadata,
}

/// Processing metadata
#[derive(Debug, Clone)]
pub struct ProcessingMetadata {
    pub modules_applied: Vec<String>,
    pub total_execution_time_ms: u64,
    pub transformations_count: usize,
    pub error_count: usize,
}

/// Module information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub config: ModuleConfig,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub execution_count: u64,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}

/// Module execution statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleStats {
    pub module_id: String,
    pub total_executions: u64,
    pub total_execution_time_ms: u64,
    pub average_execution_time_ms: f64,
    pub error_count: u64,
    pub memory_usage_stats: MemoryUsageStats,
}

/// Memory usage statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryUsageStats {
    pub peak_usage_bytes: usize,
    pub average_usage_bytes: usize,
    pub total_allocations: u64,
}

/// ETL processor metrics
#[derive(Debug, Default, Clone)]
pub struct EtlMetrics {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub total_execution_time_ms: u64,
    pub peak_memory_usage_bytes: usize,
    pub modules_loaded: usize,
}

impl EtlProcessor {
    /// Create a new ETL processor with optional priority-based pipeline support
    pub fn new(config: EtlConfig) -> Result<Self> {
        let execution_semaphore = Arc::new(Semaphore::new(config.max_concurrent_executions));

        // Initialize orchestrator if pipelines are configured
        let orchestrator = if !config.pipelines.is_empty() {
            let orchestrator = EtlPipelineOrchestrator::new(
                config.pipelines.clone(),
                config.instance_pool.clone(),
                config.max_concurrent_executions,
            )?;
            Some(Arc::new(orchestrator))
        } else {
            None
        };

        Ok(Self {
            config,
            modules: Arc::new(RwLock::new(HashMap::new())),
            execution_semaphore,
            metrics: Arc::new(Mutex::new(EtlMetrics::default())),
            orchestrator,
        })
    }

    /// Create a new ETL processor with explicit orchestrator (for testing)
    pub fn with_orchestrator(
        config: EtlConfig,
        orchestrator: EtlPipelineOrchestrator,
    ) -> Result<Self> {
        let execution_semaphore = Arc::new(Semaphore::new(config.max_concurrent_executions));

        Ok(Self {
            config,
            modules: Arc::new(RwLock::new(HashMap::new())),
            execution_semaphore,
            metrics: Arc::new(Mutex::new(EtlMetrics::default())),
            orchestrator: Some(Arc::new(orchestrator)),
        })
    }

    /// Execute an ETL module with the given request
    #[cfg(feature = "wasm")]
    pub async fn execute_module(&self, request: EtlRequest) -> Result<EtlResponse> {
        let start_time = Instant::now();

        // Acquire execution permit
        let _permit = self.execution_semaphore.acquire().await.map_err(|_| {
            RustMqError::EtlProcessingFailed("Failed to acquire execution permit".to_string())
        })?;

        // Get the module
        let module = {
            let modules = self.modules.read().await;
            modules
                .get(&request.module_id)
                .ok_or_else(|| RustMqError::EtlModuleNotFound(request.module_id.clone()))?
                .clone()
        };

        // Update last used timestamp
        *module.last_used.lock() = Instant::now();

        // Simplified WASM execution for now - just pass through data
        tokio::time::sleep(Duration::from_millis(10)).await; // Simulate processing

        // Update execution count
        module
            .execution_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Update metrics
        {
            let mut metrics = self.metrics.lock();
            metrics.total_executions += 1;
            metrics.successful_executions += 1;
            metrics.total_execution_time_ms += start_time.elapsed().as_millis() as u64;
        }

        Ok(EtlResponse {
            success: true,
            output_data: Some(request.input_data), // Pass through for now
            error_message: None,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            memory_used_bytes: 1024, // Mock memory usage
        })
    }

    #[cfg(not(feature = "wasm"))]
    pub async fn execute_module(&self, request: EtlRequest) -> Result<EtlResponse> {
        // Mock implementation when WASM feature is disabled
        let start_time = Instant::now();

        // Simulate processing time
        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(EtlResponse {
            success: true,
            output_data: Some(request.input_data), // Pass through data unchanged
            error_message: None,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            memory_used_bytes: 1024, // Mock memory usage
        })
    }

    /// Get current ETL processing metrics
    pub fn get_metrics(&self) -> EtlMetrics {
        (*self.metrics.lock()).clone()
    }

    /// Process record with explicit topic for pipeline filtering
    pub async fn process_record_with_topic(
        &self,
        record: Record,
        topic: &str,
    ) -> Result<ProcessedRecord> {
        if let Some(orchestrator) = &self.orchestrator {
            self.process_record_with_pipelines(record, topic).await
        } else {
            // Legacy behavior - process with empty module list
            self.process_record_legacy(record, vec![]).await
        }
    }

    /// Get orchestrator statistics if available
    pub fn get_orchestrator_stats(&self) -> Option<crate::etl::orchestrator::OrchestratorStats> {
        self.orchestrator.as_ref().map(|o| o.get_stats())
    }

    /// Add a new pipeline at runtime
    pub async fn add_pipeline(&self, config: crate::config::EtlPipelineConfig) -> Result<()> {
        if let Some(orchestrator) = &self.orchestrator {
            orchestrator.add_pipeline(config).await
        } else {
            Err(RustMqError::EtlProcessingFailed(
                "No orchestrator available for pipeline management".to_string(),
            ))
        }
    }

    /// Remove a pipeline at runtime
    pub async fn remove_pipeline(&self, pipeline_id: &str) -> Result<()> {
        if let Some(orchestrator) = &self.orchestrator {
            orchestrator.remove_pipeline(pipeline_id).await
        } else {
            Err(RustMqError::EtlProcessingFailed(
                "No orchestrator available for pipeline management".to_string(),
            ))
        }
    }

    /// Pre-warm WASM instances for all pipeline modules
    pub async fn prewarm_instances(&self) -> Result<()> {
        if let Some(orchestrator) = &self.orchestrator {
            orchestrator.prewarm_instances().await
        } else {
            // No-op for processors without orchestrator
            Ok(())
        }
    }

    /// Check if processor has priority-based pipelines enabled
    pub fn has_pipelines(&self) -> bool {
        self.orchestrator.is_some()
    }

    /// Process record using legacy single-module approach
    async fn process_record_legacy(
        &self,
        mut record: Record,
        module_ids: Vec<String>,
    ) -> Result<ProcessedRecord> {
        let start_time = Instant::now();
        let mut processing_metadata = ProcessingMetadata {
            modules_applied: Vec::new(),
            total_execution_time_ms: 0,
            transformations_count: 0,
            error_count: 0,
        };

        let original_record = record.clone();

        // Apply each module in sequence
        for module_id in module_ids {
            let context = ProcessingContext {
                topic: "unknown".to_string(), // Would be filled in by caller
                partition: 0,
                offset: 0,
                timestamp: record.timestamp,
                headers: record
                    .headers
                    .iter()
                    .map(|h| (h.key.clone(), String::from_utf8_lossy(&h.value).to_string()))
                    .collect(),
            };

            let request = EtlRequest {
                module_id: module_id.clone(),
                input_data: record.value.to_vec(), // Convert Bytes to Vec<u8> for ETL processing
                context,
            };

            match self.execute_module(request).await {
                Ok(response) => {
                    if response.success {
                        if let Some(output_data) = response.output_data {
                            record.value = Bytes::from(output_data); // Convert Vec<u8> back to Bytes
                            processing_metadata.transformations_count += 1;
                        }
                        processing_metadata.modules_applied.push(module_id);
                    } else {
                        processing_metadata.error_count += 1;
                        tracing::warn!(
                            "ETL module {} failed: {:?}",
                            module_id,
                            response.error_message
                        );
                    }
                    processing_metadata.total_execution_time_ms += response.execution_time_ms;
                }
                Err(e) => {
                    processing_metadata.error_count += 1;
                    tracing::error!("ETL module {} execution error: {}", module_id, e);
                }
            }
        }

        processing_metadata.total_execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(ProcessedRecord {
            original: original_record,
            transformed: record,
            processing_metadata,
        })
    }

    /// Process record using new priority-based pipeline orchestrator
    async fn process_record_with_pipelines(
        &self,
        record: Record,
        topic: &str,
    ) -> Result<ProcessedRecord> {
        if let Some(orchestrator) = &self.orchestrator {
            let results = orchestrator
                .execute_pipelines(record.clone(), topic)
                .await?;

            // Combine results from all pipelines that executed
            if let Some(final_result) = self
                .combine_pipeline_results(record.clone(), results)
                .await?
            {
                return Ok(final_result);
            }
        }

        // Fallback to no transformation if no pipelines executed
        Ok(ProcessedRecord {
            original: record.clone(),
            transformed: record,
            processing_metadata: ProcessingMetadata {
                modules_applied: vec![],
                total_execution_time_ms: 0,
                transformations_count: 0,
                error_count: 0,
            },
        })
    }

    /// Combine results from multiple pipeline executions
    async fn combine_pipeline_results(
        &self,
        original_record: Record,
        results: Vec<PipelineExecutionResult>,
    ) -> Result<Option<ProcessedRecord>> {
        if results.is_empty() {
            return Ok(None);
        }

        // For now, use the result from the first successful pipeline
        // In a more sophisticated implementation, you might:
        // - Apply transformations in order
        // - Merge transformations from multiple pipelines
        // - Apply conflict resolution strategies

        let mut final_record = original_record.clone();
        let mut total_modules_applied = Vec::new();
        let mut total_execution_time_ms = 0;
        let mut total_transformations = 0;
        let mut total_errors = 0;

        for result in &results {
            if result.success {
                // Use the transformation from the first successful result
                if total_transformations == 0 {
                    final_record = result.transformed_record.clone();
                }

                // Aggregate metadata
                total_execution_time_ms +=
                    result.execution_metadata.total_execution_time.as_millis() as u64;
                total_transformations += result.execution_metadata.transformations_applied;
                total_errors += result.execution_metadata.errors_encountered;

                // Collect all applied modules
                for stage in &result.execution_metadata.stages_executed {
                    for module in &stage.modules_executed {
                        if module.success {
                            total_modules_applied.push(module.module_id.clone());
                        }
                    }
                }
            }
        }

        Ok(Some(ProcessedRecord {
            original: original_record,
            transformed: final_record,
            processing_metadata: ProcessingMetadata {
                modules_applied: total_modules_applied,
                total_execution_time_ms,
                transformations_count: total_transformations,
                error_count: total_errors,
            },
        }))
    }
}

#[async_trait]
impl EtlPipeline for EtlProcessor {
    async fn process_record(
        &self,
        record: Record,
        module_ids: Vec<String>,
    ) -> Result<ProcessedRecord> {
        // If orchestrator is available and module_ids is empty, use pipeline-based processing
        if let Some(orchestrator) = &self.orchestrator {
            if module_ids.is_empty() {
                return self.process_record_with_pipelines(record, "unknown").await;
            }
        }

        // Legacy single-module processing for backward compatibility
        self.process_record_legacy(record, module_ids).await
    }

    async fn load_module(&self, _module_data: Vec<u8>, config: ModuleConfig) -> Result<String> {
        let module_id = uuid::Uuid::new_v4().to_string();

        let etl_module = EtlModule {
            id: module_id.clone(),
            name: "user-module".to_string(), // Could be extracted from WASM metadata
            version: "1.0.0".to_string(),
            config,
            created_at: chrono::Utc::now(),
            last_used: Arc::new(Mutex::new(Instant::now())),
            execution_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };

        let mut modules = self.modules.write().await;
        modules.insert(module_id.clone(), etl_module);

        // Update metrics
        {
            let mut metrics = self.metrics.lock();
            metrics.modules_loaded = modules.len();
        }

        tracing::info!("Loaded ETL module: {}", module_id);
        Ok(module_id)
    }

    async fn unload_module(&self, module_id: &str) -> Result<()> {
        let mut modules = self.modules.write().await;
        if modules.remove(module_id).is_some() {
            // Update metrics
            {
                let mut metrics = self.metrics.lock();
                metrics.modules_loaded = modules.len();
            }
            tracing::info!("Unloaded ETL module: {}", module_id);
            Ok(())
        } else {
            Err(RustMqError::EtlModuleNotFound(module_id.to_string()))
        }
    }

    async fn list_modules(&self) -> Result<Vec<ModuleInfo>> {
        let modules = self.modules.read().await;
        Ok(modules
            .values()
            .map(|module| {
                ModuleInfo {
                    id: module.id.clone(),
                    name: module.name.clone(),
                    version: module.version.clone(),
                    config: module.config.clone(),
                    created_at: module.created_at,
                    execution_count: module
                        .execution_count
                        .load(std::sync::atomic::Ordering::SeqCst),
                    last_used: Some(chrono::Utc::now()), // Simplified
                }
            })
            .collect())
    }

    async fn get_module_stats(&self, module_id: &str) -> Result<ModuleStats> {
        let modules = self.modules.read().await;
        let module = modules
            .get(module_id)
            .ok_or_else(|| RustMqError::EtlModuleNotFound(module_id.to_string()))?;

        let execution_count = module
            .execution_count
            .load(std::sync::atomic::Ordering::SeqCst);

        Ok(ModuleStats {
            module_id: module_id.to_string(),
            total_executions: execution_count,
            total_execution_time_ms: 0, // Would need to track this
            average_execution_time_ms: 0.0,
            error_count: 0, // Would need to track this
            memory_usage_stats: MemoryUsageStats {
                peak_usage_bytes: 0,
                average_usage_bytes: 0,
                total_allocations: 0,
            },
        })
    }
}

/// Mock ETL processor for testing
#[derive(Clone)]
pub struct MockEtlProcessor {
    #[allow(dead_code)]
    config: EtlConfig,
    modules: Arc<RwLock<HashMap<String, String>>>, // Just store module IDs
    orchestrator: Option<Arc<EtlPipelineOrchestrator>>,
}

impl MockEtlProcessor {
    pub fn new(config: EtlConfig) -> Self {
        Self {
            config,
            modules: Arc::new(RwLock::new(HashMap::new())),
            orchestrator: None, // Mock processor doesn't use orchestrator
        }
    }
}

#[async_trait]
impl EtlPipeline for MockEtlProcessor {
    async fn process_record(
        &self,
        record: Record,
        modules: Vec<String>,
    ) -> Result<ProcessedRecord> {
        // Mock processing - simulate processing through all modules
        Ok(ProcessedRecord {
            original: record.clone(),
            transformed: record,
            processing_metadata: ProcessingMetadata {
                modules_applied: modules.clone(), // Use actual modules passed in
                total_execution_time_ms: modules.len() as u64, // Simulate time per module
                transformations_count: modules.len(), // Count of actual transformations
                error_count: 0,
            },
        })
    }

    async fn load_module(&self, _module_data: Vec<u8>, _config: ModuleConfig) -> Result<String> {
        let module_id = uuid::Uuid::new_v4().to_string();
        let mut modules = self.modules.write().await;
        modules.insert(module_id.clone(), "mock-module".to_string());
        Ok(module_id)
    }

    async fn unload_module(&self, module_id: &str) -> Result<()> {
        let mut modules = self.modules.write().await;
        modules.remove(module_id);
        Ok(())
    }

    async fn list_modules(&self) -> Result<Vec<ModuleInfo>> {
        let modules = self.modules.read().await;
        Ok(modules
            .keys()
            .map(|id| ModuleInfo {
                id: id.clone(),
                name: "mock-module".to_string(),
                version: "1.0.0".to_string(),
                config: ModuleConfig {
                    memory_limit_bytes: 1024 * 1024,
                    timeout_ms: 5000,
                    input_format: DataFormat::Json,
                    output_format: DataFormat::Json,
                    enable_caching: true,
                },
                created_at: chrono::Utc::now(),
                execution_count: 0,
                last_used: None,
            })
            .collect())
    }

    async fn get_module_stats(&self, module_id: &str) -> Result<ModuleStats> {
        let modules = self.modules.read().await;
        if !modules.contains_key(module_id) {
            return Err(RustMqError::EtlModuleNotFound(module_id.to_string()));
        }

        Ok(ModuleStats {
            module_id: module_id.to_string(),
            total_executions: 0,
            total_execution_time_ms: 0,
            average_execution_time_ms: 0.0,
            error_count: 0,
            memory_usage_stats: MemoryUsageStats {
                peak_usage_bytes: 1024,
                average_usage_bytes: 512,
                total_allocations: 1,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EtlConfig;

    #[tokio::test]
    async fn test_mock_etl_processor() {
        let config = EtlConfig {
            enabled: true,
            memory_limit_bytes: 64 * 1024 * 1024,
            execution_timeout_ms: 5000,
            max_concurrent_executions: 10,
            pipelines: vec![],
            instance_pool: crate::config::EtlInstancePoolConfig {
                max_pool_size: 10,
                warmup_instances: 2,
                creation_rate_limit: 5.0,
                idle_timeout_seconds: 300,
                enable_lru_eviction: true,
            },
        };

        let processor = MockEtlProcessor::new(config);

        // Test module loading
        let module_config = ModuleConfig {
            memory_limit_bytes: 1024 * 1024,
            timeout_ms: 5000,
            input_format: DataFormat::Json,
            output_format: DataFormat::Json,
            enable_caching: true,
        };

        let module_id = processor
            .load_module(vec![1, 2, 3, 4], module_config)
            .await
            .unwrap();
        assert!(!module_id.is_empty());

        // Test module listing
        let modules = processor.list_modules().await.unwrap();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].id, module_id);

        // Test record processing
        let record = Record::new(
            Some(b"test-key".to_vec()),
            b"test-value".to_vec(),
            vec![],
            chrono::Utc::now().timestamp_millis(),
        );

        let result = processor
            .process_record(record.clone(), vec![module_id.clone()])
            .await
            .unwrap();
        assert_eq!(result.original.value, record.value);
        assert_eq!(result.transformed.value, record.value);
        assert_eq!(result.processing_metadata.modules_applied.len(), 1);

        // Test module stats
        let stats = processor.get_module_stats(&module_id).await.unwrap();
        assert_eq!(stats.module_id, module_id);

        // Test module unloading
        processor.unload_module(&module_id).await.unwrap();
        let modules = processor.list_modules().await.unwrap();
        assert_eq!(modules.len(), 0);
    }

    #[tokio::test]
    async fn test_etl_processor_creation() {
        let config = EtlConfig {
            enabled: true,
            memory_limit_bytes: 64 * 1024 * 1024,
            execution_timeout_ms: 5000,
            max_concurrent_executions: 10,
            pipelines: vec![],
            instance_pool: crate::config::EtlInstancePoolConfig {
                max_pool_size: 10,
                warmup_instances: 2,
                creation_rate_limit: 5.0,
                idle_timeout_seconds: 300,
                enable_lru_eviction: true,
            },
        };

        let processor = EtlProcessor::new(config);
        assert!(processor.is_ok());

        let processor = processor.unwrap();
        let metrics = processor.get_metrics();
        assert_eq!(metrics.modules_loaded, 0);
        assert_eq!(metrics.total_executions, 0);
    }

    #[tokio::test]
    async fn test_processing_context_serialization() {
        let context = ProcessingContext {
            topic: "test-topic".to_string(),
            partition: 5,
            offset: 12345,
            timestamp: chrono::Utc::now().timestamp_millis(),
            headers: {
                let mut headers = HashMap::new();
                headers.insert("content-type".to_string(), "application/json".to_string());
                headers
            },
        };

        let serialized = serde_json::to_string(&context).unwrap();
        let deserialized: ProcessingContext = serde_json::from_str(&serialized).unwrap();

        assert_eq!(context.topic, deserialized.topic);
        assert_eq!(context.partition, deserialized.partition);
        assert_eq!(context.offset, deserialized.offset);
    }

    #[tokio::test]
    async fn test_data_format_handling() {
        let formats = vec![
            DataFormat::Json,
            DataFormat::Avro,
            DataFormat::Protobuf,
            DataFormat::RawBytes,
        ];

        for format in formats {
            let serialized = serde_json::to_string(&format).unwrap();
            let _deserialized: DataFormat = serde_json::from_str(&serialized).unwrap();
        }
    }

    #[cfg(feature = "wasm")]
    #[tokio::test]
    async fn test_wasm_module_execution_timeout() {
        let config = EtlConfig {
            enabled: true,
            memory_limit_bytes: 64 * 1024 * 1024,
            execution_timeout_ms: 5000,
            max_concurrent_executions: 10,
            pipelines: vec![],
            instance_pool: crate::config::EtlInstancePoolConfig {
                max_pool_size: 10,
                warmup_instances: 2,
                creation_rate_limit: 5.0,
                idle_timeout_seconds: 300,
                enable_lru_eviction: true,
            },
        };

        let processor = EtlProcessor::new(config).unwrap();

        // Create a mock request that would timeout
        let request = EtlRequest {
            module_id: "non-existent".to_string(),
            input_data: vec![1, 2, 3, 4],
            context: ProcessingContext {
                topic: "test".to_string(),
                partition: 0,
                offset: 0,
                timestamp: chrono::Utc::now().timestamp_millis(),
                headers: HashMap::new(),
            },
        };

        // This should fail since module doesn't exist
        let result = processor.execute_module(request).await;
        assert!(result.is_err());
    }
}
