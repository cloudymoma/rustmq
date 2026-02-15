use bytes::Bytes;
use rustmq::config::{EtlConfig, ReplicationConfig, ScalingConfig, WalConfig};
use rustmq::controller::service::{ControllerService, CreateTopicRequest, TopicConfig};
use rustmq::etl::processor::{DataFormat, EtlPipeline, MockEtlProcessor, ModuleConfig};
use rustmq::network::grpc_server::{BrokerReplicationRpc, BrokerReplicationServiceImpl};
use rustmq::replication::ReplicationManager;
use rustmq::replication::manager::MockReplicationRpcClient;
use rustmq::replication::traits::ReplicationManager as ReplicationManagerTrait;
use rustmq::storage::{AlignedBufferPool, DirectIOWal};
use rustmq::types::*;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn test_full_system_integration() {
    // Setup temporary directory for WAL
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    // Setup storage layer
    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());

    // Setup controller service
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let controller = ControllerService::new("controller-1".to_string(), vec![], scaling_config);

    // Become leader
    controller.start_election().await.unwrap();

    // Register brokers
    let broker1 = BrokerInfo {
        id: "broker-1".to_string(),
        host: "localhost".to_string(),
        port_quic: 9092,
        port_rpc: 9093,
        rack_id: "rack-1".to_string(),
    };

    let broker2 = BrokerInfo {
        id: "broker-2".to_string(),
        host: "localhost".to_string(),
        port_quic: 9192,
        port_rpc: 9193,
        rack_id: "rack-1".to_string(),
    };

    controller.register_broker(broker1).await.unwrap();
    controller.register_broker(broker2).await.unwrap();

    // Create a topic
    let create_request = CreateTopicRequest {
        name: "integration-test-topic".to_string(),
        partitions: 2,
        replication_factor: 2,
        config: Some(TopicConfig {
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        }),
    };

    let response = controller.create_topic(create_request).await.unwrap();
    assert!(response.success);

    // Setup replication manager
    let replication_config = ReplicationConfig {
        min_in_sync_replicas: 2,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let topic_partition = TopicPartition {
        topic: "integration-test-topic".to_string(),
        partition: 0,
    };

    let replica_set = vec!["broker-1".to_string(), "broker-2".to_string()];
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let replication_manager = ReplicationManager::new(
        1,
        topic_partition.clone(),
        "broker-1".to_string(),
        1,
        replica_set,
        replication_config,
        wal.clone(),
        rpc_client,
    );

    // Setup ETL processor
    let etl_config = EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 10,
        pipelines: vec![],
        instance_pool: rustmq::config::EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 2,
            creation_rate_limit: 10.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        },
    };

    let etl_processor = MockEtlProcessor::new(etl_config);

    // Load ETL module
    let module_config = ModuleConfig {
        memory_limit_bytes: 1024 * 1024,
        timeout_ms: 5000,
        input_format: DataFormat::Json,
        output_format: DataFormat::Json,
        enable_caching: true,
    };

    let module_id = etl_processor
        .load_module(b"data-transformation-module".to_vec(), module_config)
        .await
        .unwrap();

    // End-to-end message flow test
    let original_record = Record::new(
        Some(b"integration-test-key".to_vec()),
        b"integration-test-value".to_vec(),
        vec![],
        chrono::Utc::now().timestamp_millis(),
    );

    // 1. Process through ETL pipeline
    let processed_record = etl_processor
        .process_record(original_record.clone(), vec![module_id])
        .await
        .unwrap();

    assert_eq!(
        processed_record.processing_metadata.modules_applied.len(),
        1
    );
    assert_eq!(processed_record.processing_metadata.error_count, 0);

    // 2. Replicate the processed record
    let wal_record = WalRecord {
        topic_partition: topic_partition.clone(),
        offset: 0,
        record: processed_record.transformed,
        crc32: 0,
    };

    let replication_result = replication_manager
        .replicate_record(&wal_record)
        .await
        .unwrap();
    assert_eq!(replication_result.offset, 0);
    assert!(matches!(
        replication_result.durability,
        DurabilityLevel::Durable
    ));

    // 3. Verify cluster metadata consistency
    let metadata = controller.get_cluster_metadata().await.unwrap();
    assert_eq!(metadata.topics.len(), 1);
    assert_eq!(metadata.brokers.len(), 2);
    assert_eq!(metadata.partition_assignments.len(), 2); // 2 partitions
    assert!(metadata.leader.is_some());

    // 4. Test network layer (gRPC service)
    let grpc_service = BrokerReplicationServiceImpl::new("broker-1".to_string());

    // Register the partition handler for the topic partition we created
    use rustmq::replication::FollowerReplicationHandler;
    let follower_handler = Arc::new(FollowerReplicationHandler::new(
        topic_partition.clone(),
        1, // leader epoch
        Some("broker-1".to_string()),
        wal.clone(),
        "broker-1".to_string(),
    ));
    grpc_service.register_follower_handler(topic_partition.clone(), follower_handler);

    let replicate_request = ReplicateDataRequest {
        leader_epoch: 1,
        topic_partition,
        records: vec![],
        leader_id: "broker-1".to_string(),
    };

    let grpc_response = grpc_service
        .replicate_data(replicate_request)
        .await
        .unwrap();
    assert!(grpc_response.success);

    // 5. Test heartbeat mechanism
    let heartbeat_request = HeartbeatRequest {
        leader_epoch: 1,
        leader_id: "broker-1".to_string(),
        topic_partition: TopicPartition {
            topic: "integration-test-topic".to_string(),
            partition: 0,
        },
        high_watermark: 1,
    };

    let heartbeat_response = grpc_service
        .send_heartbeat(heartbeat_request)
        .await
        .unwrap();
    assert!(heartbeat_response.success);
    assert!(heartbeat_response.follower_state.is_some());

    println!("✅ Full system integration test completed successfully");
}

#[tokio::test]
async fn test_failure_recovery_scenarios() {
    // Setup basic components
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());

    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let controller = ControllerService::new(
        "controller-1".to_string(),
        vec![], // No peer controllers for simplified test scenario
        scaling_config,
    );

    // Test 1: Leader election after startup
    assert!(!controller.get_raft_info().is_leader);
    let won = controller.start_election().await.unwrap();
    assert!(won); // Should win in simplified scenario
    assert!(controller.get_raft_info().is_leader);

    // Test 2: Replication with insufficient replicas
    let replication_config = ReplicationConfig {
        min_in_sync_replicas: 3, // High requirement
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let topic_partition = TopicPartition {
        topic: "test-topic".to_string(),
        partition: 0,
    };

    let replica_set = vec!["broker-1".to_string()]; // Only one replica
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let replication_manager = ReplicationManager::new(
        1,
        topic_partition.clone(),
        "broker-1".to_string(),
        1,
        replica_set,
        replication_config,
        wal,
        rpc_client,
    );

    let record = WalRecord {
        topic_partition,
        offset: 0,
        record: Record::new(
            Some(b"test-key".to_vec()),
            b"test-value".to_vec(),
            vec![],
            chrono::Utc::now().timestamp_millis(),
        ),
        crc32: 0,
    };

    // Should still succeed locally but with appropriate durability level
    let result = replication_manager.replicate_record(&record).await.unwrap();
    assert_eq!(result.offset, 0);

    // Test 3: ETL processing with module errors
    let etl_config = EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 10,
        pipelines: vec![],
        instance_pool: rustmq::config::EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 2,
            creation_rate_limit: 10.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        },
    };

    let etl_processor = MockEtlProcessor::new(etl_config);

    let test_record = Record::new(
        Some(b"error-test".to_vec()),
        b"error-test-value".to_vec(),
        vec![],
        chrono::Utc::now().timestamp_millis(),
    );

    // Process with non-existent module - should handle gracefully
    let result = etl_processor
        .process_record(test_record, vec!["non-existent-module".to_string()])
        .await;

    // Mock processor handles this gracefully
    assert!(result.is_ok());

    println!("✅ Failure recovery scenarios test completed");
}

#[tokio::test]
async fn test_concurrent_system_operations() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());

    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 3,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let controller = Arc::new(ControllerService::new(
        "controller-1".to_string(),
        vec![],
        scaling_config,
    ));

    controller.start_election().await.unwrap();

    // Test concurrent decommission slot operations
    let mut slot_tasks = vec![];
    for i in 0..3 {
        let controller_clone = controller.clone();
        slot_tasks.push(tokio::spawn(async move {
            controller_clone
                .acquire_decommission_slot(format!("broker-{}", i), format!("admin-{}", i))
                .await
        }));
    }

    let mut successful_acquisitions = 0;
    for task in slot_tasks {
        let result = timeout(Duration::from_secs(1), task).await.unwrap();
        if result.is_ok() {
            successful_acquisitions += 1;
        }
    }

    assert_eq!(successful_acquisitions, 3);

    // Test concurrent replication operations
    let replication_config = ReplicationConfig {
        min_in_sync_replicas: 1,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let topic_partition = TopicPartition {
        topic: "concurrent-test".to_string(),
        partition: 0,
    };

    let replica_set = vec!["broker-1".to_string()];
    let rpc_client = Arc::new(MockReplicationRpcClient);

    let replication_manager = Arc::new(ReplicationManager::new(
        1,
        topic_partition.clone(),
        "broker-1".to_string(),
        1,
        replica_set,
        replication_config,
        wal,
        rpc_client,
    ));

    let mut replication_tasks = vec![];
    for i in 0..5 {
        let manager = replication_manager.clone();
        let tp = topic_partition.clone();
        replication_tasks.push(tokio::spawn(async move {
            let record = WalRecord {
                topic_partition: tp,
                offset: i,
                record: Record::new(
                    Some(format!("concurrent-key-{}", i).into_bytes()),
                    format!("concurrent-value-{}", i).into_bytes(),
                    vec![],
                    chrono::Utc::now().timestamp_millis(),
                ),
                crc32: 0,
            };
            manager.replicate_record(&record).await
        }));
    }

    let mut successful_replications = 0;
    for task in replication_tasks {
        let result = timeout(Duration::from_secs(2), task).await.unwrap();
        if result.is_ok() {
            successful_replications += 1;
        }
    }

    assert_eq!(successful_replications, 5);

    // Test concurrent ETL processing
    let etl_config = EtlConfig {
        enabled: true,
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_executions: 10,
        pipelines: vec![],
        instance_pool: rustmq::config::EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 2,
            creation_rate_limit: 10.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        },
    };

    let etl_processor = Arc::new(MockEtlProcessor::new(etl_config));

    let module_config = ModuleConfig {
        memory_limit_bytes: 1024 * 1024,
        timeout_ms: 5000,
        input_format: DataFormat::Json,
        output_format: DataFormat::Json,
        enable_caching: true,
    };

    let module_id = etl_processor
        .load_module(b"concurrent-module".to_vec(), module_config)
        .await
        .unwrap();

    let mut etl_tasks = vec![];
    for i in 0..5 {
        let processor = etl_processor.clone();
        let module = module_id.clone();
        etl_tasks.push(tokio::spawn(async move {
            let record = Record::new(
                Some(format!("etl-key-{}", i).into_bytes()),
                format!("etl-value-{}", i).into_bytes(),
                vec![],
                chrono::Utc::now().timestamp_millis(),
            );
            processor.process_record(record, vec![module]).await
        }));
    }

    let mut successful_etl = 0;
    for task in etl_tasks {
        let result = timeout(Duration::from_secs(2), task).await.unwrap();
        if result.is_ok() {
            successful_etl += 1;
        }
    }

    assert_eq!(successful_etl, 5);

    println!("✅ Concurrent system operations test completed");
}

#[tokio::test]
async fn test_system_configuration_updates() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let controller = ControllerService::new("controller-1".to_string(), vec![], scaling_config);

    // Test runtime configuration updates
    let new_config = ScalingConfig {
        max_concurrent_additions: 5,
        max_concurrent_decommissions: 3,
        rebalance_timeout_ms: 600_000,
        traffic_migration_rate: 0.2,
        health_check_timeout_ms: 60_000,
    };

    let result = controller.update_scaling_config(new_config).await;
    assert!(result.is_ok());

    // Test that the configuration update is effective
    let slot1 = controller
        .acquire_decommission_slot("broker-1".to_string(), "admin-1".to_string())
        .await
        .unwrap();

    let slot2 = controller
        .acquire_decommission_slot("broker-2".to_string(), "admin-1".to_string())
        .await
        .unwrap();

    let slot3 = controller
        .acquire_decommission_slot("broker-3".to_string(), "admin-1".to_string())
        .await
        .unwrap();

    // Should now be able to acquire 3 slots (updated from 1)
    assert!(!slot1.operation_id.is_empty());
    assert!(!slot2.operation_id.is_empty());
    assert!(!slot3.operation_id.is_empty());

    println!("✅ System configuration updates test completed");
}

#[tokio::test]
async fn test_data_consistency_across_components() {
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig {
        path: temp_dir.path().to_path_buf(),
        capacity_bytes: 1024 * 1024,
        fsync_on_write: false,
        segment_size_bytes: 64 * 1024,
        buffer_size: 4096,
        upload_interval_ms: 60_000,
        flush_interval_ms: 1000,
    };

    let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
    let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());

    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let controller = ControllerService::new("controller-1".to_string(), vec![], scaling_config);

    controller.start_election().await.unwrap();

    // Create consistent topic across components
    let topic_name = "consistency-test-topic";
    let create_request = CreateTopicRequest {
        name: topic_name.to_string(),
        partitions: 1,
        replication_factor: 1,
        config: None,
    };

    // Add broker first
    let broker = BrokerInfo {
        id: "broker-1".to_string(),
        host: "localhost".to_string(),
        port_quic: 9092,
        port_rpc: 9093,
        rack_id: "rack-1".to_string(),
    };
    controller.register_broker(broker).await.unwrap();

    let response = controller.create_topic(create_request).await.unwrap();
    assert!(response.success);

    // Verify topic exists in metadata
    let metadata = controller.get_cluster_metadata().await.unwrap();
    let topic = metadata.topics.iter().find(|t| t.name == topic_name);
    assert!(topic.is_some());

    // Setup replication for the same topic
    let topic_partition = TopicPartition {
        topic: topic_name.to_string(),
        partition: 0,
    };

    let replication_config = ReplicationConfig {
        min_in_sync_replicas: 1,
        ack_timeout_ms: 5000,
        max_replication_lag: 1000,
        heartbeat_timeout_ms: 30000,
    };

    let rpc_client = Arc::new(MockReplicationRpcClient);
    let replication_manager = ReplicationManager::new(
        1,
        topic_partition.clone(),
        "broker-1".to_string(),
        1,
        vec!["broker-1".to_string()],
        replication_config,
        wal,
        rpc_client,
    );

    // Process data through the pipeline
    let test_data = Record::new(
        Some(b"consistency-key".to_vec()),
        b"consistency-value".to_vec(),
        vec![],
        chrono::Utc::now().timestamp_millis(),
    );

    let wal_record = WalRecord {
        topic_partition,
        offset: 0,
        record: test_data,
        crc32: 0,
    };

    let replication_result = replication_manager
        .replicate_record(&wal_record)
        .await
        .unwrap();
    assert_eq!(replication_result.offset, 0);

    // Verify high watermark is consistent - it should be set to the local offset after replication
    let hwm = replication_manager.get_high_watermark().await.unwrap();
    // For single-replica setup, high watermark should be set to the replicated record's offset
    assert_eq!(
        hwm, 0,
        "High watermark should match replicated record offset in single-replica setup"
    );

    // Verify metadata is still consistent after operations
    let final_metadata = controller.get_cluster_metadata().await.unwrap();
    assert_eq!(final_metadata.topics.len(), metadata.topics.len());
    assert_eq!(final_metadata.brokers.len(), metadata.brokers.len());
    assert_eq!(final_metadata.leader, metadata.leader);

    println!("✅ Data consistency across components test completed");
}
