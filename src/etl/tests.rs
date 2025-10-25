//! Comprehensive test suite for the priority-based ETL pipeline system
//!
//! This module contains integration tests that verify the correct operation
//! of the entire ETL system including orchestrator, filters, instance pools,
//! and backward compatibility.

use super::*;
use crate::config::*;
use crate::types::{Record, Header};
use processor::{EtlProcessor, MockEtlProcessor};
use orchestrator::EtlPipelineOrchestrator;
use filter::{TopicFilterEngine, ConditionalRuleEngine};
use instance_pool::WasmInstancePool;
use std::time::Duration;
use serde_json::json;
use bytes::Bytes;

/// Test basic pipeline orchestrator creation and configuration
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_orchestrator_basic_functionality() {
    let pipeline_config = create_test_pipeline_config();
    let instance_pool_config = create_test_instance_pool_config();

    let orchestrator = EtlPipelineOrchestrator::new(
        vec![pipeline_config],
        instance_pool_config,
        50,
    ).unwrap();

    let stats = orchestrator.get_stats();
    assert_eq!(stats.active_pipelines.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(stats.total_executions.load(std::sync::atomic::Ordering::Relaxed), 0);
}

/// Test priority-based execution ordering
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_priority_execution_ordering() {
    let mut pipeline_config = create_test_pipeline_config();
    
    // Create stages with different priorities
    pipeline_config.stages = vec![
        create_test_stage(2, "low-priority-module", false),
        create_test_stage(0, "high-priority-module", false),
        create_test_stage(1, "medium-priority-module", false),
    ];

    let instance_pool_config = create_test_instance_pool_config();
    let orchestrator = EtlPipelineOrchestrator::new(
        vec![pipeline_config],
        instance_pool_config,
        50,
    ).unwrap();

    let record = create_test_record();
    let results = orchestrator.execute_pipelines(record, "test.topic").await.unwrap();
    
    assert_eq!(results.len(), 1);
    assert!(results[0].success);
    
    // Verify stages executed in priority order (0, 1, 2)
    let execution_metadata = &results[0].execution_metadata;
    assert_eq!(execution_metadata.stages_executed.len(), 3);
    assert_eq!(execution_metadata.stages_executed[0].priority, 0);
    assert_eq!(execution_metadata.stages_executed[1].priority, 1);
    assert_eq!(execution_metadata.stages_executed[2].priority, 2);
}

/// Test parallel execution within same priority level
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_parallel_execution() {
    let mut pipeline_config = create_test_pipeline_config();
    
    // Create stage with multiple modules and parallel execution enabled
    let mut stage = create_test_stage(0, "module-1", true);
    stage.modules.push(create_test_module("module-2"));
    stage.modules.push(create_test_module("module-3"));
    pipeline_config.stages = vec![stage];

    let instance_pool_config = create_test_instance_pool_config();
    let orchestrator = EtlPipelineOrchestrator::new(
        vec![pipeline_config],
        instance_pool_config,
        50,
    ).unwrap();

    let record = create_test_record();
    let results = orchestrator.execute_pipelines(record, "test.topic").await.unwrap();
    
    assert_eq!(results.len(), 1);
    assert!(results[0].success);
    
    // Verify all modules in the stage executed
    let stage_info = &results[0].execution_metadata.stages_executed[0];
    assert_eq!(stage_info.modules_executed.len(), 3);
    assert!(stage_info.parallel_execution);
}

/// Test comprehensive topic filtering
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_comprehensive_topic_filtering() {
    // Test exact match filter
    let exact_filters = vec![
        TopicFilter {
            filter_type: FilterType::Exact,
            pattern: "events.user.signup".to_string(),
            case_sensitive: true,
            negate: false,
        }
    ];
    let engine = TopicFilterEngine::new(exact_filters).unwrap();
    assert!(engine.matches_topic("events.user.signup").matches);
    assert!(!engine.matches_topic("events.user.login").matches);

    // Test wildcard filter
    let wildcard_filters = vec![
        TopicFilter {
            filter_type: FilterType::Wildcard,
            pattern: "events.*".to_string(),
            case_sensitive: true,
            negate: false,
        }
    ];
    let engine = TopicFilterEngine::new(wildcard_filters).unwrap();
    assert!(engine.matches_topic("events.user.signup").matches);
    assert!(engine.matches_topic("events.order.created").matches);
    assert!(!engine.matches_topic("logs.application").matches);

    // Test regex filter
    let regex_filters = vec![
        TopicFilter {
            filter_type: FilterType::Regex,
            pattern: r"^sensor-\d+$".to_string(),
            case_sensitive: true,
            negate: false,
        }
    ];
    let engine = TopicFilterEngine::new(regex_filters).unwrap();
    assert!(engine.matches_topic("sensor-123").matches);
    assert!(engine.matches_topic("sensor-999").matches);
    assert!(!engine.matches_topic("sensor-abc").matches);

    // Test prefix filter
    let prefix_filters = vec![
        TopicFilter {
            filter_type: FilterType::Prefix,
            pattern: "logs.".to_string(),
            case_sensitive: true,
            negate: false,
        }
    ];
    let engine = TopicFilterEngine::new(prefix_filters).unwrap();
    assert!(engine.matches_topic("logs.application").matches);
    assert!(engine.matches_topic("logs.system.error").matches);
    assert!(!engine.matches_topic("events.user.signup").matches);

    // Test suffix filter
    let suffix_filters = vec![
        TopicFilter {
            filter_type: FilterType::Suffix,
            pattern: ".critical".to_string(),
            case_sensitive: true,
            negate: false,
        }
    ];
    let engine = TopicFilterEngine::new(suffix_filters).unwrap();
    assert!(engine.matches_topic("events.alert.critical").matches);
    assert!(engine.matches_topic("logs.error.critical").matches);
    assert!(!engine.matches_topic("events.user.signup").matches);

    // Test contains filter
    let contains_filters = vec![
        TopicFilter {
            filter_type: FilterType::Contains,
            pattern: "error".to_string(),
            case_sensitive: true,
            negate: false,
        }
    ];
    let engine = TopicFilterEngine::new(contains_filters).unwrap();
    assert!(engine.matches_topic("logs.error.critical").matches);
    assert!(engine.matches_topic("events.error.happened").matches);
    assert!(!engine.matches_topic("events.user.signup").matches);
}

/// Test conditional rule evaluation
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_conditional_rule_evaluation() {
    // Test header value condition
    let header_rules = vec![
        ConditionalRule {
            condition_type: ConditionType::HeaderValue,
            field_path: "content-type".to_string(),
            operator: ComparisonOperator::Equals,
            value: json!("application/json"),
            negate: false,
        }
    ];
    let engine = ConditionalRuleEngine::new(header_rules).unwrap();
    
    let record_with_header = Record::new(
        Some(b"test".to_vec()),
        b"{}".to_vec(),
        vec![
            Header::new(
                "content-type".to_string(),
                b"application/json".to_vec(),
            )
        ],
        1234567890,
    );
    assert!(engine.evaluate_record(&record_with_header, "test.topic").unwrap());

    let record_without_header = Record::new(
        Some(b"test".to_vec()),
        b"{}".to_vec(),
        vec![],
        1234567890,
    );
    assert!(!engine.evaluate_record(&record_without_header, "test.topic").unwrap());

    // Test payload field condition
    let payload_rules = vec![
        ConditionalRule {
            condition_type: ConditionType::PayloadField,
            field_path: "user.id".to_string(),
            operator: ComparisonOperator::Equals,
            value: json!("user123"),
            negate: false,
        }
    ];
    let engine = ConditionalRuleEngine::new(payload_rules).unwrap();
    
    let payload = r#"{"user": {"id": "user123", "name": "John"}}"#;
    let record_with_payload = Record::new(
        Some(b"test".to_vec()),
        payload.as_bytes().to_vec(),
        vec![],
        1234567890,
    );
    assert!(engine.evaluate_record(&record_with_payload, "test.topic").unwrap());

    // Test message size condition
    let size_rules = vec![
        ConditionalRule {
            condition_type: ConditionType::MessageSize,
            field_path: "".to_string(),
            operator: ComparisonOperator::GreaterThan,
            value: json!(10),
            negate: false,
        }
    ];
    let engine = ConditionalRuleEngine::new(size_rules).unwrap();
    
    let large_record = Record::new(
        Some(b"test".to_vec()),
        b"this is a large message payload".to_vec(),
        vec![],
        1234567890,
    );
    assert!(engine.evaluate_record(&large_record, "test.topic").unwrap());

    let small_record = Record::new(
        Some(b"test".to_vec()),
        b"small".to_vec(),
        vec![],
        1234567890,
    );
    assert!(!engine.evaluate_record(&small_record, "test.topic").unwrap());
}

/// Test WASM instance pool functionality
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_wasm_instance_pool() {
    let config = EtlInstancePoolConfig {
        max_pool_size: 5,
        warmup_instances: 2,
        creation_rate_limit: 10.0,
        idle_timeout_seconds: 300,
        enable_lru_eviction: true,
    };

    let pool = WasmInstancePool::new(config).unwrap();
    
    let module_config = ModuleInstanceConfig {
        memory_limit_bytes: 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_instances: 10,
        enable_caching: true,
        cache_ttl_seconds: 300,
        custom_config: json!({}),
    };

    // Test instance checkout and return
    let checkout = pool.checkout_instance("test-module", &module_config).await.unwrap();
    assert_eq!(checkout.instance.module_id, "test-module");
    
    let stats = pool.get_global_stats();
    assert_eq!(stats.total_instances_created.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(stats.total_pool_misses.load(std::sync::atomic::Ordering::Relaxed), 1);

    // Return instance
    checkout.return_handle.return_instance(checkout.instance).await.unwrap();

    // Checkout again (should hit pool)
    let checkout2 = pool.checkout_instance("test-module", &module_config).await.unwrap();
    let stats = pool.get_global_stats();
    assert_eq!(stats.total_pool_hits.load(std::sync::atomic::Ordering::Relaxed), 1);

    // Test pre-warming
    pool.prewarm_module("prewarm-module", &module_config).await.unwrap();
    let stats = pool.get_global_stats();
    assert!(stats.total_instances_created.load(std::sync::atomic::Ordering::Relaxed) > 1);
}

/// Test error handling strategies
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_error_handling_strategies() {
    // Test SkipModule strategy
    let mut pipeline_config = create_test_pipeline_config();
    pipeline_config.error_handling = ErrorHandlingStrategy::SkipModule;
    
    // Create a stage with a non-existent module (will fail)
    pipeline_config.stages = vec![
        create_test_stage(0, "non-existent-module", false),
        create_test_stage(1, "valid-module", false),
    ];

    let instance_pool_config = create_test_instance_pool_config();
    let orchestrator = EtlPipelineOrchestrator::new(
        vec![pipeline_config],
        instance_pool_config,
        50,
    ).unwrap();

    let record = create_test_record();
    let results = orchestrator.execute_pipelines(record, "test.topic").await.unwrap();
    
    assert_eq!(results.len(), 1);
    // Pipeline should continue despite failed module
    assert!(results[0].execution_metadata.errors_encountered > 0);
    assert_eq!(results[0].execution_metadata.stages_executed.len(), 2);
}

/// Test backward compatibility with existing ETL processor
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_backward_compatibility() {
    let config = EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 100,
        pipelines: vec![], // No pipelines - legacy mode
        instance_pool: create_test_instance_pool_config(),
    };

    let processor = EtlProcessor::new(config).unwrap();
    assert!(!processor.has_pipelines());

    // Test legacy module loading
    let module_config = crate::etl::processor::ModuleConfig {
        memory_limit_bytes: 1024 * 1024,
        timeout_ms: 5000,
        input_format: crate::etl::processor::DataFormat::Json,
        output_format: crate::etl::processor::DataFormat::Json,
        enable_caching: true,
    };

    let module_id = processor.load_module(vec![1, 2, 3, 4], module_config).await.unwrap();
    assert!(!module_id.is_empty());

    // Test legacy record processing
    let record = create_test_record();
    let result = processor.process_record(record, vec![module_id.clone()]).await.unwrap();
    assert_eq!(result.processing_metadata.modules_applied.len(), 1);
    assert_eq!(result.processing_metadata.modules_applied[0], module_id);
}

/// Test integration between processor and orchestrator
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_processor_orchestrator_integration() {
    let pipeline_config = create_test_pipeline_config();
    let config = EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 100,
        pipelines: vec![pipeline_config],
        instance_pool: create_test_instance_pool_config(),
    };

    let processor = EtlProcessor::new(config).unwrap();
    assert!(processor.has_pipelines());

    // Test pipeline-based record processing
    let record = create_test_record();
    let result = processor.process_record_with_topic(record, "test.topic").await.unwrap();
    
    // Should have transformation applied
    assert!(result.processing_metadata.transformations_count > 0);
    assert!(!result.processing_metadata.modules_applied.is_empty());

    // Test orchestrator stats
    let stats = processor.get_orchestrator_stats().unwrap();
    assert!(stats.total_executions.load(std::sync::atomic::Ordering::Relaxed) > 0);
}

/// Test runtime pipeline management
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_runtime_pipeline_management() {
    let config = EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 100,
        pipelines: vec![],
        instance_pool: create_test_instance_pool_config(),
    };

    let processor = EtlProcessor::new(config).unwrap();
    
    // Initially no orchestrator because no pipelines
    assert!(!processor.has_pipelines());

    // Create processor with orchestrator
    let pipeline_config = create_test_pipeline_config();
    let config_with_pipeline = EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 100,
        pipelines: vec![pipeline_config.clone()],
        instance_pool: create_test_instance_pool_config(),
    };

    let processor_with_orchestrator = EtlProcessor::new(config_with_pipeline).unwrap();
    assert!(processor_with_orchestrator.has_pipelines());

    // Test adding pipeline at runtime
    let new_pipeline_config = create_test_pipeline_config_with_id("runtime-pipeline");
    processor_with_orchestrator.add_pipeline(new_pipeline_config).await.unwrap();

    // Test removing pipeline at runtime
    processor_with_orchestrator.remove_pipeline("runtime-pipeline").await.unwrap();
}

/// Test performance characteristics
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_performance_characteristics() {
    let pipeline_config = create_test_pipeline_config();
    let instance_pool_config = create_test_instance_pool_config();

    let orchestrator = EtlPipelineOrchestrator::new(
        vec![pipeline_config],
        instance_pool_config,
        50,
    ).unwrap();

    // Pre-warm instances for better performance
    orchestrator.prewarm_instances().await.unwrap();

    let record = create_test_record();
    let start_time = std::time::Instant::now();
    
    // Execute multiple times to test performance
    for _ in 0..10 {
        let results = orchestrator.execute_pipelines(record.clone(), "test.topic").await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }
    
    let total_time = start_time.elapsed();
    let avg_time_per_execution = total_time / 10;
    
    // Performance should be reasonable (less than 100ms per execution)
    assert!(avg_time_per_execution < Duration::from_millis(100));
    
    let stats = orchestrator.get_stats();
    assert_eq!(stats.total_executions.load(std::sync::atomic::Ordering::Relaxed), 10);
    assert_eq!(stats.successful_executions.load(std::sync::atomic::Ordering::Relaxed), 10);
}

/// Test filter evaluation performance
#[tokio::test]
#[cfg(not(feature = "wasm"))]
async fn test_filter_performance() {
    // Create a large number of filters
    let mut filters = Vec::new();
    
    // Add exact matches (should be fastest)
    for i in 0..1000 {
        filters.push(TopicFilter {
            filter_type: FilterType::Exact,
            pattern: format!("topic-{}", i),
            case_sensitive: true,
            negate: false,
        });
    }
    
    // Add wildcard patterns
    for i in 0..100 {
        filters.push(TopicFilter {
            filter_type: FilterType::Wildcard,
            pattern: format!("pattern-{}.*", i),
            case_sensitive: true,
            negate: false,
        });
    }
    
    // Add regex patterns
    for i in 0..10 {
        filters.push(TopicFilter {
            filter_type: FilterType::Regex,
            pattern: format!(r"^regex-{}-\d+$", i),
            case_sensitive: true,
            negate: false,
        });
    }

    let engine = TopicFilterEngine::new(filters).unwrap();
    let stats = engine.get_stats();
    assert_eq!(stats.exact_patterns, 1000);
    assert_eq!(stats.wildcard_patterns, 100);
    assert_eq!(stats.regex_patterns, 10);

    // Test performance of exact match (should be very fast)
    let start_time = std::time::Instant::now();
    for i in 0..1000 {
        let result = engine.matches_topic(&format!("topic-{}", i));
        assert!(result.matches);
        // Each evaluation should be under 100 microseconds
        assert!(result.evaluation_time_ns < 100_000);
    }
    let total_time = start_time.elapsed();
    
    // Total time for 1000 exact matches should be under 10ms
    assert!(total_time < Duration::from_millis(10));
}

// Helper functions for creating test configurations

fn create_test_pipeline_config() -> EtlPipelineConfig {
    create_test_pipeline_config_with_id("test-pipeline")
}

fn create_test_pipeline_config_with_id(pipeline_id: &str) -> EtlPipelineConfig {
    EtlPipelineConfig {
        pipeline_id: pipeline_id.to_string(),
        name: "Test Pipeline".to_string(),
        description: Some("Test pipeline for unit tests".to_string()),
        enabled: true,
        stages: vec![create_test_stage(0, "test-module", false)],
        global_timeout_ms: 10000,
        max_retries: 3,
        error_handling: ErrorHandlingStrategy::SkipModule,
    }
}

fn create_test_stage(priority: u32, module_id: &str, parallel_execution: bool) -> EtlStage {
    EtlStage {
        priority,
        modules: vec![create_test_module(module_id)],
        parallel_execution,
        stage_timeout_ms: None,
        continue_on_error: true,
    }
}

fn create_test_module(module_id: &str) -> EtlModuleInstance {
    EtlModuleInstance {
        module_id: module_id.to_string(),
        instance_config: ModuleInstanceConfig {
            memory_limit_bytes: 1024 * 1024,
            execution_timeout_ms: 5000,
            max_concurrent_instances: 10,
            enable_caching: true,
            cache_ttl_seconds: 300,
            custom_config: json!({}),
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
}

fn create_test_instance_pool_config() -> EtlInstancePoolConfig {
    EtlInstancePoolConfig {
        max_pool_size: 10,
        warmup_instances: 2,
        creation_rate_limit: 10.0,
        idle_timeout_seconds: 300,
        enable_lru_eviction: true,
    }
}

fn create_test_record() -> Record {
    Record::new(
        Some(b"test-key".to_vec()),
        b"test-value".to_vec(),
        vec![
            Header::new(
                "content-type".to_string(),
                b"application/json".to_vec(),
            )
        ],
        chrono::Utc::now().timestamp_millis(),
    )
}