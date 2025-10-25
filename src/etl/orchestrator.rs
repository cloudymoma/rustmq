use crate::config::{
    EtlPipelineConfig, EtlStage, EtlModuleInstance, ErrorHandlingStrategy,
    ModuleInstanceConfig, EtlInstancePoolConfig
};
use crate::etl::filter::{TopicFilterEngine, ConditionalRuleEngine, FilterResult};
use crate::etl::instance_pool::{WasmInstancePool, InstanceCheckout, WasmContext};
use crate::{Result, error::RustMqError, types::Record};
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use std::time::{Duration, Instant};
use std::cmp::Ordering;
use futures::future::join_all;
use tracing::{info, warn, error, debug};
use bytes::Bytes;
use dashmap::DashMap;

/// Priority-based ETL pipeline orchestrator with optimized execution
pub struct EtlPipelineOrchestrator {
    /// Configured pipelines indexed by pipeline ID (lock-free with DashMap)
    pipelines: Arc<DashMap<String, Arc<PipelineInstance>>>,
    /// WASM instance pool for reusable execution contexts
    instance_pool: Arc<WasmInstancePool>,
    /// Global execution semaphore for resource management
    execution_semaphore: Arc<Semaphore>,
    /// Orchestrator statistics
    stats: Arc<OrchestratorStats>,
}

/// Runtime pipeline instance with compiled filters and engines
struct PipelineInstance {
    config: EtlPipelineConfig,
    /// Compiled stages sorted by priority
    stages: Vec<CompiledStage>,
    /// Overall pipeline statistics
    stats: PipelineStats,
}

/// Compiled stage with optimized execution components
struct CompiledStage {
    priority: u32,
    modules: Vec<CompiledModule>,
    parallel_execution: bool,
    stage_timeout_ms: Option<u64>,
    continue_on_error: bool,
}

/// Compiled module with pre-built filter engines
struct CompiledModule {
    module_id: String,
    instance_config: ModuleInstanceConfig,
    topic_filter: TopicFilterEngine,
    conditional_rules: ConditionalRuleEngine,
    /// Module execution statistics
    stats: ModuleExecutionStats,
}

/// Pipeline execution context for a single record
pub struct PipelineExecutionContext {
    pub pipeline_id: String,
    pub record: Record,
    pub topic: String,
    pub execution_id: String,
    pub start_time: Instant,
}

/// Result of pipeline execution
#[derive(Debug, Clone)]
pub struct PipelineExecutionResult {
    pub success: bool,
    pub original_record: Record,
    pub transformed_record: Record,
    pub execution_metadata: ExecutionMetadata,
}

/// Execution metadata with detailed timing and transformation info
#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    pub pipeline_id: String,
    pub execution_id: String,
    pub stages_executed: Vec<StageExecutionInfo>,
    pub total_execution_time: Duration,
    pub transformations_applied: usize,
    pub errors_encountered: usize,
    pub modules_skipped: usize,
}

/// Information about stage execution
#[derive(Debug, Clone)]
pub struct StageExecutionInfo {
    pub priority: u32,
    pub modules_executed: Vec<ModuleExecutionInfo>,
    pub stage_execution_time: Duration,
    pub parallel_execution: bool,
}

/// Information about module execution
#[derive(Debug, Clone)]
pub struct ModuleExecutionInfo {
    pub module_id: String,
    pub execution_time: Duration,
    pub success: bool,
    pub error_message: Option<String>,
    pub bytes_processed: usize,
    pub transformation_applied: bool,
    /// Transformed output (only populated for parallel execution)
    pub output: Option<Vec<u8>>,
}

/// Priority queue item for stage execution ordering
#[derive(Debug, Clone)]
struct PriorityStage {
    priority: u32,
    stage_index: usize,
}

impl Eq for PriorityStage {}

impl PartialEq for PriorityStage {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Ord for PriorityStage {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (lower priority numbers first)
        other.priority.cmp(&self.priority)
    }
}

impl PartialOrd for PriorityStage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Orchestrator performance statistics
#[derive(Debug, Default)]
pub struct OrchestratorStats {
    pub total_executions: std::sync::atomic::AtomicU64,
    pub successful_executions: std::sync::atomic::AtomicU64,
    pub failed_executions: std::sync::atomic::AtomicU64,
    pub total_execution_time: std::sync::atomic::AtomicU64,
    pub active_pipelines: std::sync::atomic::AtomicUsize,
    pub average_latency_ms: std::sync::atomic::AtomicU64,
}

/// Pipeline-specific statistics
#[derive(Debug, Default, Clone)]
pub struct PipelineStats {
    pub executions: u64,
    pub successes: u64,
    pub failures: u64,
    pub total_execution_time: Duration,
    pub average_execution_time: Duration,
    pub last_execution: Option<Instant>,
}

/// Module execution statistics
#[derive(Debug, Default, Clone)]
pub struct ModuleExecutionStats {
    pub executions: u64,
    pub successes: u64,
    pub failures: u64,
    pub total_execution_time: Duration,
    pub bytes_processed: u64,
    pub transformations_applied: u64,
}

impl EtlPipelineOrchestrator {
    /// Create a new ETL pipeline orchestrator
    pub fn new(
        pipelines: Vec<EtlPipelineConfig>,
        instance_pool_config: EtlInstancePoolConfig,
        max_concurrent_executions: usize,
    ) -> Result<Self> {
        let instance_pool = Arc::new(WasmInstancePool::new(instance_pool_config)?);
        let execution_semaphore = Arc::new(Semaphore::new(max_concurrent_executions));
        let stats = Arc::new(OrchestratorStats::default());

        let compiled_pipelines = DashMap::new();

        for pipeline_config in pipelines {
            if pipeline_config.enabled {
                let pipeline_instance = Self::compile_pipeline(pipeline_config)?;
                let pipeline_id = pipeline_instance.config.pipeline_id.clone();
                compiled_pipelines.insert(pipeline_id, Arc::new(pipeline_instance));
            }
        }

        stats.active_pipelines.store(
            compiled_pipelines.len(),
            std::sync::atomic::Ordering::Relaxed
        );

        Ok(Self {
            pipelines: Arc::new(compiled_pipelines),
            instance_pool,
            execution_semaphore,
            stats,
        })
    }

    /// Compile a pipeline configuration into optimized runtime representation
    fn compile_pipeline(config: EtlPipelineConfig) -> Result<PipelineInstance> {
        let mut compiled_stages = Vec::new();

        for stage in &config.stages {
            let mut compiled_modules = Vec::new();

            for module in &stage.modules {
                let topic_filter = TopicFilterEngine::new(module.topic_filters.clone())?;
                let conditional_rules = ConditionalRuleEngine::new(module.conditional_rules.clone())?;

                compiled_modules.push(CompiledModule {
                    module_id: module.module_id.clone(),
                    instance_config: module.instance_config.clone(),
                    topic_filter,
                    conditional_rules,
                    stats: ModuleExecutionStats::default(),
                });
            }

            compiled_stages.push(CompiledStage {
                priority: stage.priority,
                modules: compiled_modules,
                parallel_execution: stage.parallel_execution,
                stage_timeout_ms: stage.stage_timeout_ms,
                continue_on_error: stage.continue_on_error,
            });
        }

        // Sort stages by priority for optimal execution order
        compiled_stages.sort_by_key(|stage| stage.priority);

        Ok(PipelineInstance {
            config,
            stages: compiled_stages,
            stats: PipelineStats::default(),
        })
    }

    /// Execute a record through all applicable pipelines
    pub async fn execute_pipelines(
        &self,
        record: Record,
        topic: &str,
    ) -> Result<Vec<PipelineExecutionResult>> {
        let execution_id = uuid::Uuid::new_v4().to_string();
        let start_time = Instant::now();

        // Acquire execution permit for resource management
        let _permit = self.execution_semaphore.acquire().await
            .map_err(|_| RustMqError::EtlProcessingFailed(
                "Failed to acquire execution permit".to_string()
            ))?;

        let mut results = Vec::new();

        // Lock-free iteration over pipelines with DashMap
        for entry in self.pipelines.iter() {
            let (pipeline_id, pipeline) = entry.pair();

            // Check if any stage/module in this pipeline applies to this topic
            if self.pipeline_applies_to_topic(pipeline, topic).await? {
                let context = PipelineExecutionContext {
                    pipeline_id: pipeline_id.clone(),
                    record: record.clone(),
                    topic: topic.to_string(),
                    execution_id: execution_id.clone(),
                    start_time,
                };

                match self.execute_single_pipeline(pipeline, context).await {
                    Ok(result) => {
                        results.push(result);
                    }
                    Err(e) => {
                        error!("Pipeline {} execution failed: {}", pipeline_id, e);
                        self.stats.failed_executions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        // Create error result
                        results.push(PipelineExecutionResult {
                            success: false,
                            original_record: record.clone(),
                            transformed_record: record.clone(),
                            execution_metadata: ExecutionMetadata {
                                pipeline_id: pipeline_id.clone(),
                                execution_id: execution_id.clone(),
                                stages_executed: vec![],
                                total_execution_time: start_time.elapsed(),
                                transformations_applied: 0,
                                errors_encountered: 1,
                                modules_skipped: 0,
                            },
                        });
                    }
                }
            }
        }

        // Update global statistics
        let total_time = start_time.elapsed();
        self.stats.total_executions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.stats.total_execution_time.fetch_add(
            total_time.as_millis() as u64, 
            std::sync::atomic::Ordering::Relaxed
        );

        Ok(results)
    }

    /// Check if a pipeline applies to the given topic
    async fn pipeline_applies_to_topic(&self, pipeline: &PipelineInstance, topic: &str) -> Result<bool> {
        for stage in &pipeline.stages {
            for module in &stage.modules {
                let filter_result = module.topic_filter.matches_topic(topic);
                if filter_result.matches {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Execute a single pipeline with priority-based stage ordering
    async fn execute_single_pipeline(
        &self,
        pipeline: &PipelineInstance,
        context: PipelineExecutionContext,
    ) -> Result<PipelineExecutionResult> {
        let mut execution_metadata = ExecutionMetadata {
            pipeline_id: context.pipeline_id.clone(),
            execution_id: context.execution_id.clone(),
            stages_executed: Vec::new(),
            total_execution_time: Duration::ZERO,
            transformations_applied: 0,
            errors_encountered: 0,
            modules_skipped: 0,
        };

        let mut current_record = context.record.clone();
        
        // Create priority queue for stage execution (min-heap by priority)
        let mut stage_queue: BinaryHeap<PriorityStage> = pipeline.stages
            .iter()
            .enumerate()
            .map(|(index, stage)| PriorityStage {
                priority: stage.priority,
                stage_index: index,
            })
            .collect();

        // Execute stages in priority order
        while let Some(priority_stage) = stage_queue.pop() {
            let stage = &pipeline.stages[priority_stage.stage_index];
            
            // Apply global timeout check
            if context.start_time.elapsed().as_millis() > pipeline.config.global_timeout_ms as u128 {
                warn!("Pipeline {} global timeout exceeded", context.pipeline_id);
                execution_metadata.errors_encountered += 1;
                break;
            }

            match self.execute_stage(stage, &mut current_record, &context.topic).await {
                Ok(stage_info) => {
                    execution_metadata.stages_executed.push(stage_info.clone());
                    execution_metadata.transformations_applied += stage_info.modules_executed
                        .iter()
                        .filter(|m| m.transformation_applied)
                        .count();
                    // Count errors from individual module failures
                    execution_metadata.errors_encountered += stage_info.modules_executed
                        .iter()
                        .filter(|m| !m.success)
                        .count();
                }
                Err(e) => {
                    error!("Stage {} execution failed: {}", stage.priority, e);
                    execution_metadata.errors_encountered += 1;
                    
                    match pipeline.config.error_handling {
                        ErrorHandlingStrategy::StopPipeline => {
                            error!("Stopping pipeline {} due to stage failure", context.pipeline_id);
                            break;
                        }
                        ErrorHandlingStrategy::SkipModule => {
                            warn!("Skipping failed stage {} in pipeline {}", stage.priority, context.pipeline_id);
                            continue;
                        }
                        ErrorHandlingStrategy::RetryWithBackoff => {
                            // Implement retry logic here
                            warn!("Retry not implemented yet, skipping stage {}", stage.priority);
                            continue;
                        }
                        ErrorHandlingStrategy::SendToDeadLetter => {
                            // Implement dead letter routing here
                            warn!("Dead letter routing not implemented yet, skipping stage {}", stage.priority);
                            continue;
                        }
                    }
                }
            }
        }

        execution_metadata.total_execution_time = context.start_time.elapsed();
        let success = execution_metadata.errors_encountered == 0;

        if success {
            self.stats.successful_executions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        Ok(PipelineExecutionResult {
            success,
            original_record: context.record,
            transformed_record: current_record,
            execution_metadata,
        })
    }

    /// Execute a single stage with optional parallel module execution
    async fn execute_stage(
        &self,
        stage: &CompiledStage,
        record: &mut Record,
        topic: &str,
    ) -> Result<StageExecutionInfo> {
        let stage_start = Instant::now();
        let mut modules_executed = Vec::new();

        // Filter modules that apply to this record
        let applicable_modules = self.filter_applicable_modules(&stage.modules, record, topic).await?;

        if applicable_modules.is_empty() {
            debug!("No applicable modules for stage priority {}", stage.priority);
            return Ok(StageExecutionInfo {
                priority: stage.priority,
                modules_executed: vec![],
                stage_execution_time: stage_start.elapsed(),
                parallel_execution: stage.parallel_execution,
            });
        }

        if stage.parallel_execution && applicable_modules.len() > 1 {
            // Execute modules in parallel
            modules_executed = self.execute_modules_parallel(&stage.modules, applicable_modules, record, topic).await?;
        } else {
            // Execute modules sequentially
            modules_executed = self.execute_modules_sequential(&stage.modules, applicable_modules, record, topic).await?;
        }

        Ok(StageExecutionInfo {
            priority: stage.priority,
            modules_executed,
            stage_execution_time: stage_start.elapsed(),
            parallel_execution: stage.parallel_execution,
        })
    }

    /// Filter modules that apply to the current record
    async fn filter_applicable_modules(
        &self,
        modules: &[CompiledModule],
        record: &Record,
        topic: &str,
    ) -> Result<Vec<usize>> {
        let mut applicable = Vec::new();

        for (index, module) in modules.iter().enumerate() {
            // Check topic filter
            let topic_matches = module.topic_filter.matches_topic(topic);
            if !topic_matches.matches {
                continue;
            }

            // Check conditional rules
            let conditions_match = module.conditional_rules.evaluate_record(record, topic)?;
            if conditions_match {
                applicable.push(index);
            }
        }

        Ok(applicable)
    }

    /// Execute modules sequentially
    async fn execute_modules_sequential(
        &self,
        modules: &[CompiledModule],
        applicable_indices: Vec<usize>,
        record: &mut Record,
        topic: &str,
    ) -> Result<Vec<ModuleExecutionInfo>> {
        let mut results = Vec::new();

        for &index in &applicable_indices {
            let module = &modules[index];
            match self.execute_single_module(module, record, topic).await {
                Ok(module_info) => {
                    results.push(module_info);
                }
                Err(e) => {
                    error!("Module {} execution failed: {}", module.module_id, e);
                    results.push(ModuleExecutionInfo {
                        module_id: module.module_id.clone(),
                        execution_time: Duration::ZERO,
                        success: false,
                        error_message: Some(e.to_string()),
                        bytes_processed: 0,
                        transformation_applied: false,
                        output: None,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Execute modules in parallel (each with a copy of the record)
    async fn execute_modules_parallel(
        &self,
        modules: &[CompiledModule],
        applicable_indices: Vec<usize>,
        record: &mut Record,
        topic: &str,
    ) -> Result<Vec<ModuleExecutionInfo>> {
        let tasks: Vec<_> = applicable_indices
            .into_iter()
            .map(|index| {
                let module = &modules[index];
                let record_copy = record.clone();
                let topic_copy = topic.to_string();
                let module_id = module.module_id.clone();
                let instance_config = module.instance_config.clone();
                let instance_pool = Arc::clone(&self.instance_pool);
                
                tokio::spawn(async move {
                    let module_start = Instant::now();

                    match instance_pool.checkout_instance(&module_id, &instance_config).await {
                        Ok(mut checkout) => {
                            // Execute with timeout
                            let timeout_duration = Duration::from_millis(instance_config.execution_timeout_ms as u64);
                            let execute_result = tokio::time::timeout(
                                timeout_duration,
                                checkout.instance.wasm_context.execute("transform", &record_copy.value)
                            ).await;

                            match execute_result {
                                Ok(Ok(output)) => {
                                    checkout.return_handle.return_instance(checkout.instance).await.ok();
                                    ModuleExecutionInfo {
                                        module_id,
                                        execution_time: module_start.elapsed(),
                                        success: true,
                                        error_message: None,
                                        bytes_processed: record_copy.value.len(),
                                        transformation_applied: true,
                                        output: Some(output),
                                    }
                                }
                                Ok(Err(e)) => {
                                    checkout.return_handle.return_instance(checkout.instance).await.ok();
                                    ModuleExecutionInfo {
                                        module_id,
                                        execution_time: module_start.elapsed(),
                                        success: false,
                                        error_message: Some(e.to_string()),
                                        bytes_processed: 0,
                                        transformation_applied: false,
                                        output: None,
                                    }
                                }
                                Err(_timeout) => {
                                    checkout.return_handle.return_instance(checkout.instance).await.ok();
                                    ModuleExecutionInfo {
                                        module_id: module_id.clone(),
                                        execution_time: module_start.elapsed(),
                                        success: false,
                                        error_message: Some(format!("Execution timed out after {}ms", instance_config.execution_timeout_ms)),
                                        bytes_processed: 0,
                                        transformation_applied: false,
                                        output: None,
                                    }
                                }
                            }
                        }
                        Err(e) => ModuleExecutionInfo {
                            module_id,
                            execution_time: module_start.elapsed(),
                            success: false,
                            error_message: Some(e.to_string()),
                            bytes_processed: 0,
                            transformation_applied: false,
                            output: None,
                        }
                    }
                })
            })
            .collect();

        let results = join_all(tasks).await;
        let mut module_results = Vec::new();

        for result in results {
            match result {
                Ok(module_info) => module_results.push(module_info),
                Err(e) => {
                    error!("Task execution failed: {}", e);
                    module_results.push(ModuleExecutionInfo {
                        module_id: "unknown".to_string(),
                        execution_time: Duration::ZERO,
                        success: false,
                        error_message: Some(e.to_string()),
                        bytes_processed: 0,
                        transformation_applied: false,
                        output: None,
                    });
                }
            }
        }

        // Apply transformations from successful parallel executions
        // Strategy: Use the first successful transformation
        // Future enhancement: Implement merge strategies for multiple transformations
        for module_result in &module_results {
            if module_result.success && module_result.transformation_applied {
                if let Some(ref output) = module_result.output {
                    record.value = Bytes::from(output.clone());
                    debug!("Applied transformation from module {}", module_result.module_id);
                    break;
                }
            }
        }

        Ok(module_results)
    }

    /// Execute a single module
    async fn execute_single_module(
        &self,
        module: &CompiledModule,
        record: &mut Record,
        _topic: &str,
    ) -> Result<ModuleExecutionInfo> {
        let module_start = Instant::now();
        
        // Checkout WASM instance from pool
        let mut checkout = self.instance_pool
            .checkout_instance(&module.module_id, &module.instance_config)
            .await?;

        // Execute the transformation with timeout
        let timeout_duration = Duration::from_millis(module.instance_config.execution_timeout_ms as u64);
        let execute_result = tokio::time::timeout(
            timeout_duration,
            checkout.instance.wasm_context.execute("transform", &record.value)
        ).await;

        match execute_result {
            Ok(Ok(output)) => {
                // Apply transformation to record
                record.value = Bytes::from(output); // Convert Vec<u8> back to Bytes

                // Return instance to pool
                checkout.return_handle.return_instance(checkout.instance).await?;

                Ok(ModuleExecutionInfo {
                    module_id: module.module_id.clone(),
                    execution_time: module_start.elapsed(),
                    success: true,
                    error_message: None,
                    bytes_processed: record.value.len(),
                    transformation_applied: true,
                    output: None, // Sequential execution applies transformation directly to record
                })
            }
            Ok(Err(e)) => {
                // Execution failed
                checkout.return_handle.return_instance(checkout.instance).await.ok();

                Err(RustMqError::EtlProcessingFailed(
                    format!("Module {} execution failed: {}", module.module_id, e)
                ))
            }
            Err(_timeout) => {
                // Timeout occurred
                checkout.return_handle.return_instance(checkout.instance).await.ok();

                Err(RustMqError::EtlProcessingFailed(
                    format!("Module {} execution timed out after {}ms", module.module_id, module.instance_config.execution_timeout_ms)
                ))
            }
        }
    }

    /// Add a new pipeline at runtime (lock-free)
    pub async fn add_pipeline(&self, config: EtlPipelineConfig) -> Result<()> {
        if !config.enabled {
            return Ok(());
        }

        let pipeline_instance = Self::compile_pipeline(config)?;
        let pipeline_id = pipeline_instance.config.pipeline_id.clone();

        // Lock-free insert with DashMap
        self.pipelines.insert(pipeline_id.clone(), Arc::new(pipeline_instance));

        self.stats.active_pipelines.store(
            self.pipelines.len(),
            std::sync::atomic::Ordering::Relaxed
        );

        info!("Added new pipeline: {}", pipeline_id);
        Ok(())
    }

    /// Remove a pipeline at runtime (lock-free)
    pub async fn remove_pipeline(&self, pipeline_id: &str) -> Result<()> {
        // Lock-free remove with DashMap
        if self.pipelines.remove(pipeline_id).is_some() {
            self.stats.active_pipelines.store(
                self.pipelines.len(),
                std::sync::atomic::Ordering::Relaxed
            );
            info!("Removed pipeline: {}", pipeline_id);
            Ok(())
        } else {
            Err(RustMqError::EtlModuleNotFound(
                format!("Pipeline not found: {}", pipeline_id)
            ))
        }
    }

    /// Get orchestrator performance statistics
    pub fn get_stats(&self) -> OrchestratorStats {
        OrchestratorStats {
            total_executions: std::sync::atomic::AtomicU64::new(
                self.stats.total_executions.load(std::sync::atomic::Ordering::Relaxed)
            ),
            successful_executions: std::sync::atomic::AtomicU64::new(
                self.stats.successful_executions.load(std::sync::atomic::Ordering::Relaxed)
            ),
            failed_executions: std::sync::atomic::AtomicU64::new(
                self.stats.failed_executions.load(std::sync::atomic::Ordering::Relaxed)
            ),
            total_execution_time: std::sync::atomic::AtomicU64::new(
                self.stats.total_execution_time.load(std::sync::atomic::Ordering::Relaxed)
            ),
            active_pipelines: std::sync::atomic::AtomicUsize::new(
                self.stats.active_pipelines.load(std::sync::atomic::Ordering::Relaxed)
            ),
            average_latency_ms: std::sync::atomic::AtomicU64::new(
                self.stats.average_latency_ms.load(std::sync::atomic::Ordering::Relaxed)
            ),
        }
    }

    /// Pre-warm WASM instances for all modules in all pipelines (lock-free)
    pub async fn prewarm_instances(&self) -> Result<()> {
        // Lock-free iteration over pipelines
        for entry in self.pipelines.iter() {
            let pipeline = entry.value();
            for stage in &pipeline.stages {
                for module in &stage.modules {
                    self.instance_pool
                        .prewarm_module(&module.module_id, &module.instance_config)
                        .await?;
                }
            }
        }

        info!("Pre-warmed WASM instances for all pipeline modules");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use crate::types::Header;

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let pipeline_config = EtlPipelineConfig {
            pipeline_id: "test-pipeline".to_string(),
            name: "Test Pipeline".to_string(),
            description: Some("Test pipeline for unit tests".to_string()),
            enabled: true,
            stages: vec![],
            global_timeout_ms: 5000,
            max_retries: 3,
            error_handling: ErrorHandlingStrategy::SkipModule,
        };

        let instance_pool_config = EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 2,
            creation_rate_limit: 5.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        };

        let orchestrator = EtlPipelineOrchestrator::new(
            vec![pipeline_config],
            instance_pool_config,
            50,
        ).unwrap();

        let stats = orchestrator.get_stats();
        assert_eq!(stats.active_pipelines.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_priority_stage_ordering() {
        let mut stages = vec![
            PriorityStage { priority: 3, stage_index: 0 },
            PriorityStage { priority: 1, stage_index: 1 },
            PriorityStage { priority: 2, stage_index: 2 },
        ];

        let mut heap: BinaryHeap<PriorityStage> = stages.into_iter().collect();
        
        // Should pop in priority order: 1, 2, 3
        assert_eq!(heap.pop().unwrap().priority, 1);
        assert_eq!(heap.pop().unwrap().priority, 2);
        assert_eq!(heap.pop().unwrap().priority, 3);
    }

    #[tokio::test]
    #[cfg(not(feature = "wasm"))]
    async fn test_pipeline_execution_with_stages() {
        let stage = EtlStage {
            priority: 0,
            modules: vec![
                EtlModuleInstance {
                    module_id: "test-module".to_string(),
                    instance_config: ModuleInstanceConfig {
                        memory_limit_bytes: 1024 * 1024,
                        execution_timeout_ms: 5000,
                        max_concurrent_instances: 10,
                        enable_caching: true,
                        cache_ttl_seconds: 300,
                        custom_config: serde_json::Value::Object(serde_json::Map::new()),
                    },
                    topic_filters: vec![
                        TopicFilter {
                            filter_type: FilterType::Exact,
                            pattern: "test.topic".to_string(),
                            case_sensitive: true,
                            negate: false,
                        }
                    ],
                    conditional_rules: vec![],
                }
            ],
            parallel_execution: false,
            stage_timeout_ms: None,
            continue_on_error: true,
        };

        let pipeline_config = EtlPipelineConfig {
            pipeline_id: "test-pipeline".to_string(),
            name: "Test Pipeline".to_string(),
            description: None,
            enabled: true,
            stages: vec![stage],
            global_timeout_ms: 10000,
            max_retries: 1,
            error_handling: ErrorHandlingStrategy::SkipModule,
        };

        let instance_pool_config = EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 1,
            creation_rate_limit: 5.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        };

        let orchestrator = EtlPipelineOrchestrator::new(
            vec![pipeline_config],
            instance_pool_config,
            50,
        ).unwrap();

        let record = Record::new(
            Some(b"test-key".to_vec()),
            b"test-value".to_vec(),
            vec![],
            chrono::Utc::now().timestamp_millis(),
        );

        let results = orchestrator.execute_pipelines(record, "test.topic").await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(results[0].execution_metadata.stages_executed.len(), 1);
    }

    #[tokio::test]
    async fn test_pipeline_topic_filtering() {
        let stage = EtlStage {
            priority: 0,
            modules: vec![
                EtlModuleInstance {
                    module_id: "test-module".to_string(),
                    instance_config: ModuleInstanceConfig {
                        memory_limit_bytes: 1024 * 1024,
                        execution_timeout_ms: 5000,
                        max_concurrent_instances: 10,
                        enable_caching: true,
                        cache_ttl_seconds: 300,
                        custom_config: serde_json::Value::Object(serde_json::Map::new()),
                    },
                    topic_filters: vec![
                        TopicFilter {
                            filter_type: FilterType::Wildcard,
                            pattern: "events.*".to_string(),
                            case_sensitive: true,
                            negate: false,
                        }
                    ],
                    conditional_rules: vec![],
                }
            ],
            parallel_execution: false,
            stage_timeout_ms: None,
            continue_on_error: true,
        };

        let pipeline_config = EtlPipelineConfig {
            pipeline_id: "events-pipeline".to_string(),
            name: "Events Pipeline".to_string(),
            description: None,
            enabled: true,
            stages: vec![stage],
            global_timeout_ms: 10000,
            max_retries: 1,
            error_handling: ErrorHandlingStrategy::SkipModule,
        };

        let instance_pool_config = EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 1,
            creation_rate_limit: 5.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        };

        let orchestrator = EtlPipelineOrchestrator::new(
            vec![pipeline_config],
            instance_pool_config,
            50,
        ).unwrap();

        let record = Record::new(
            Some(b"test-key".to_vec()),
            b"test-value".to_vec(),
            vec![],
            chrono::Utc::now().timestamp_millis(),
        );

        // Should match events.user
        let results = orchestrator.execute_pipelines(record.clone(), "events.user").await.unwrap();
        assert_eq!(results.len(), 1);

        // Should not match logs.error
        let results = orchestrator.execute_pipelines(record, "logs.error").await.unwrap();
        assert_eq!(results.len(), 0);
    }
}