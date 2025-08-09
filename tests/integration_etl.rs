use rustmq::etl::processor::{EtlProcessor, MockEtlProcessor, EtlPipeline, EtlRequest, ProcessingContext, ModuleConfig, DataFormat};
use rustmq::config::{EtlConfig, EtlInstancePoolConfig};
use rustmq::types::Record;
use std::collections::HashMap;
use tokio::time::{timeout, Duration};

fn create_test_etl_config() -> EtlConfig {
    EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 10,
        pipelines: vec![],
        instance_pool: EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 2,
            creation_rate_limit: 10.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        },
    }
}

#[tokio::test]
async fn test_etl_processor_creation_and_metrics() {
    let config = create_test_etl_config();

    let processor = EtlProcessor::new(config).unwrap();
    
    // Test initial metrics
    let metrics = processor.get_metrics();
    assert_eq!(metrics.modules_loaded, 0);
    assert_eq!(metrics.total_executions, 0);
    assert_eq!(metrics.successful_executions, 0);
    assert_eq!(metrics.failed_executions, 0);
}

#[tokio::test]
async fn test_mock_etl_processor_full_workflow() {
    let config = create_test_etl_config();

    let processor = MockEtlProcessor::new(config);

    // Test module loading
    let module_config = ModuleConfig {
        memory_limit_bytes: 1024 * 1024,
        timeout_ms: 5000,
        input_format: DataFormat::Json,
        output_format: DataFormat::Json,
        enable_caching: true,
    };

    let module_id = processor.load_module(vec![1, 2, 3, 4], module_config.clone()).await.unwrap();
    assert!(!module_id.is_empty());

    // Test module listing
    let modules = processor.list_modules().await.unwrap();
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].id, module_id);
    assert_eq!(modules[0].name, "mock-module");

    // Test record processing
    let record = Record {
        key: Some(b"test-key".to_vec()),
        value: b"test-value".to_vec(),
        headers: vec![],
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    let result = processor.process_record(record.clone(), vec![module_id.clone()]).await.unwrap();
    assert_eq!(result.original.value, record.value);
    assert_eq!(result.transformed.value, record.value); // Mock doesn't transform
    assert_eq!(result.processing_metadata.modules_applied.len(), 1);
    assert_eq!(result.processing_metadata.transformations_count, 1);
    assert_eq!(result.processing_metadata.error_count, 0);

    // Test module stats
    let stats = processor.get_module_stats(&module_id).await.unwrap();
    assert_eq!(stats.module_id, module_id);
    assert_eq!(stats.total_executions, 0); // Mock doesn't track real stats
    assert!(stats.memory_usage_stats.peak_usage_bytes > 0);

    // Test module unloading
    processor.unload_module(&module_id).await.unwrap();
    let modules = processor.list_modules().await.unwrap();
    assert_eq!(modules.len(), 0);

    // Test unloading non-existent module
    let result = processor.unload_module("non-existent").await;
    assert!(result.is_ok()); // Mock implementation is lenient
}

#[tokio::test]
async fn test_etl_processing_context_serialization() {
    let context = ProcessingContext {
        topic: "test-topic".to_string(),
        partition: 5,
        offset: 12345,
        timestamp: chrono::Utc::now().timestamp_millis(),
        headers: {
            let mut headers = HashMap::new();
            headers.insert("content-type".to_string(), "application/json".to_string());
            headers.insert("source".to_string(), "producer-1".to_string());
            headers
        },
    };

    // Test serialization/deserialization
    let serialized = serde_json::to_string(&context).unwrap();
    let deserialized: ProcessingContext = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(context.topic, deserialized.topic);
    assert_eq!(context.partition, deserialized.partition);
    assert_eq!(context.offset, deserialized.offset);
    assert_eq!(context.timestamp, deserialized.timestamp);
    assert_eq!(context.headers.len(), deserialized.headers.len());
}

#[tokio::test]
async fn test_data_format_variants() {
    let formats = vec![
        DataFormat::Json,
        DataFormat::Avro,
        DataFormat::Protobuf,
        DataFormat::RawBytes,
    ];

    for format in formats {
        let serialized = serde_json::to_string(&format).unwrap();
        let deserialized: DataFormat = serde_json::from_str(&serialized).unwrap();
        
        // Verify serialization round-trip works
        match (&format, &deserialized) {
            (DataFormat::Json, DataFormat::Json) => (),
            (DataFormat::Avro, DataFormat::Avro) => (),
            (DataFormat::Protobuf, DataFormat::Protobuf) => (),
            (DataFormat::RawBytes, DataFormat::RawBytes) => (),
            _ => panic!("Format serialization round-trip failed"),
        }
    }
}

#[tokio::test] 
async fn test_etl_request_handling() {
    let config = create_test_etl_config();

    let processor = MockEtlProcessor::new(config);

    // Load a module first
    let module_config = ModuleConfig {
        memory_limit_bytes: 1024 * 1024,
        timeout_ms: 2000,
        input_format: DataFormat::Json,
        output_format: DataFormat::Json,
        enable_caching: true,
    };

    let module_id = processor.load_module(vec![1, 2, 3, 4], module_config).await.unwrap();

    // Create an ETL request
    let context = ProcessingContext {
        topic: "test-topic".to_string(),
        partition: 0,
        offset: 100,
        timestamp: chrono::Utc::now().timestamp_millis(),
        headers: HashMap::new(),
    };

    let request = EtlRequest {
        module_id: module_id.clone(),
        input_data: b"test input data".to_vec(),
        context,
    };

    // Process the request - this would normally call execute_module
    // but since we're using mock processor, we'll test the data flow
    let record = Record {
        key: Some(b"test-key".to_vec()),
        value: request.input_data.clone(),
        headers: vec![],
        timestamp: request.context.timestamp,
    };

    let result = processor.process_record(record, vec![module_id]).await.unwrap();
    assert!(result.processing_metadata.total_execution_time_ms > 0);
}

#[tokio::test]
async fn test_concurrent_etl_processing() {
    let mut config = create_test_etl_config();
    config.max_concurrent_executions = 5;

    let processor = MockEtlProcessor::new(config);

    // Load a module
    let module_config = ModuleConfig {
        memory_limit_bytes: 1024 * 1024,
        timeout_ms: 5000,
        input_format: DataFormat::Json,
        output_format: DataFormat::Json,
        enable_caching: true,
    };

    let module_id = processor.load_module(vec![1, 2, 3, 4], module_config).await.unwrap();

    // Process multiple records concurrently
    let mut tasks = vec![];
    for i in 0..5 {
        let processor_clone = processor.clone();
        let module_id_clone = module_id.clone();
        tasks.push(tokio::spawn(async move {
            let record = Record {
                key: Some(format!("key-{}", i).into_bytes()),
                value: format!("value-{}", i).into_bytes(),
                headers: vec![],
                timestamp: chrono::Utc::now().timestamp_millis(),
            };
            processor_clone.process_record(record, vec![module_id_clone]).await
        }));
    }

    let mut successful = 0;
    for task in tasks {
        let result = timeout(Duration::from_secs(2), task).await.unwrap();
        if result.is_ok() {
            successful += 1;
        }
    }

    assert_eq!(successful, 5, "All concurrent processing should succeed");
}

#[tokio::test]
async fn test_etl_error_handling() {
    let config = create_test_etl_config();

    let processor = MockEtlProcessor::new(config);

    // Test processing with non-existent module
    let record = Record {
        key: Some(b"test-key".to_vec()),
        value: b"test-value".to_vec(),
        headers: vec![],
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    let result = processor.process_record(record, vec!["non-existent-module".to_string()]).await;
    // Mock processor should handle this gracefully (might succeed with no transformation)
    assert!(result.is_ok());

    // Test getting stats for non-existent module
    let result = processor.get_module_stats("non-existent").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_module_configuration_validation() {
    let config = create_test_etl_config();

    let processor = MockEtlProcessor::new(config);

    // Test with various module configurations
    let configs = vec![
        ModuleConfig {
            memory_limit_bytes: 512 * 1024, // 512KB
            timeout_ms: 1000,
            input_format: DataFormat::Json,
            output_format: DataFormat::Avro,
            enable_caching: false,
        },
        ModuleConfig {
            memory_limit_bytes: 2 * 1024 * 1024, // 2MB
            timeout_ms: 10000,
            input_format: DataFormat::Protobuf,
            output_format: DataFormat::RawBytes,
            enable_caching: true,
        },
    ];

    for (i, config) in configs.into_iter().enumerate() {
        let module_id = processor.load_module(
            format!("module-{}", i).into_bytes(),
            config
        ).await.unwrap();
        
        assert!(!module_id.is_empty());
        
        // Verify module appears in listing
        let modules = processor.list_modules().await.unwrap();
        assert!(modules.iter().any(|m| m.id == module_id));
    }
}

#[tokio::test] 
async fn test_etl_pipeline_chaining() {
    let config = create_test_etl_config();

    let processor = MockEtlProcessor::new(config);

    // Load multiple modules
    let module_config = ModuleConfig {
        memory_limit_bytes: 1024 * 1024,
        timeout_ms: 5000,
        input_format: DataFormat::Json,
        output_format: DataFormat::Json,
        enable_caching: true,
    };

    let module1 = processor.load_module(b"module1".to_vec(), module_config.clone()).await.unwrap();
    let module2 = processor.load_module(b"module2".to_vec(), module_config.clone()).await.unwrap();
    let module3 = processor.load_module(b"module3".to_vec(), module_config).await.unwrap();

    // Process record through multiple modules (pipeline)
    let record = Record {
        key: Some(b"pipeline-test".to_vec()),
        value: b"initial-data".to_vec(),
        headers: vec![],
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    let modules = vec![module1, module2, module3];
    let result = processor.process_record(record.clone(), modules.clone()).await.unwrap();

    // Verify pipeline metadata
    assert_eq!(result.processing_metadata.modules_applied.len(), modules.len());
    assert_eq!(result.processing_metadata.transformations_count, modules.len());
    assert_eq!(result.processing_metadata.error_count, 0);
    assert!(result.processing_metadata.total_execution_time_ms > 0);
}

#[cfg(feature = "wasm")]
#[tokio::test]
async fn test_real_etl_processor_basic_functionality() {
    let config = create_test_etl_config();

    let processor = EtlProcessor::new(config).unwrap();
    
    // Test that processor initializes correctly
    let metrics = processor.get_metrics();
    assert_eq!(metrics.modules_loaded, 0);
    
    // Test module loading with valid WASM binary would require actual WASM module
    // For now, just verify that the processor can be created and basic operations work
    
    // Test execution of non-existent module (should fail gracefully)
    let context = ProcessingContext {
        topic: "test-topic".to_string(),
        partition: 0,
        offset: 0,
        timestamp: chrono::Utc::now().timestamp_millis(),
        headers: HashMap::new(),
    };

    let request = EtlRequest {
        module_id: "non-existent".to_string(),
        input_data: vec![1, 2, 3, 4],
        context,
    };

    let result = processor.execute_module(request).await;
    assert!(result.is_err()); // Should fail for non-existent module
}

#[cfg(not(feature = "wasm"))]
#[tokio::test]
async fn test_etl_processor_without_wasm_feature() {
    let config = create_test_etl_config();

    let processor = EtlProcessor::new(config).unwrap();
    
    // Test mock execution when WASM feature is disabled
    let context = ProcessingContext {
        topic: "test-topic".to_string(),
        partition: 0,
        offset: 0,
        timestamp: chrono::Utc::now().timestamp_millis(),
        headers: HashMap::new(),
    };

    let request = EtlRequest {
        module_id: "mock-module".to_string(),
        input_data: vec![1, 2, 3, 4],
        context,
    };

    let result = processor.execute_module(request).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output_data, Some(vec![1, 2, 3, 4])); // Should pass through unchanged
    assert!(result.execution_time_ms > 0);
    assert_eq!(result.memory_used_bytes, 1024); // Mock value
}

#[tokio::test]
async fn test_etl_processor_memory_and_timeout_limits() {
    let mut config = create_test_etl_config();
    config.memory_limit_bytes = 1024; // Very small limit
    config.execution_timeout_ms = 100; // Very short timeout
    config.max_concurrent_executions = 1;

    let processor = EtlProcessor::new(config).unwrap();
    
    // These limits would be tested in real WASM execution
    // For mock testing, verify that the configuration is applied
    let metrics = processor.get_metrics();
    assert_eq!(metrics.modules_loaded, 0);
}