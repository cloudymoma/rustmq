use rustmq::config::ScalingConfig;
use rustmq::controller::service::{
    ControllerService, CreateTopicRequest, DeleteTopicRequest, TopicConfig,
};
use rustmq::types::*;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn test_controller_decommission_slot_management() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 2,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let peers = vec!["controller-2".to_string(), "controller-3".to_string()];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Test slot acquisition
    let result1 = controller
        .acquire_decommission_slot("broker-1".to_string(), "admin-1".to_string())
        .await
        .unwrap();

    assert!(!result1.operation_id.is_empty());
    assert!(!result1.slot_token.is_empty());

    // Test second slot acquisition
    let result2 = controller
        .acquire_decommission_slot("broker-2".to_string(), "admin-1".to_string())
        .await
        .unwrap();

    assert!(!result2.operation_id.is_empty());

    // Test that third slot fails (limit reached)
    let result3 = controller
        .acquire_decommission_slot("broker-3".to_string(), "admin-1".to_string())
        .await;

    assert!(result3.is_err());

    // Test status retrieval
    let status = controller.get_decommission_status().await.unwrap();
    assert_eq!(status.len(), 2);

    // Test slot release
    controller
        .release_decommission_slot(&result1.operation_id)
        .await
        .unwrap();

    let status = controller.get_decommission_status().await.unwrap();
    assert_eq!(status.len(), 1);

    // Test that new slot can be acquired after release
    let result4 = controller
        .acquire_decommission_slot("broker-4".to_string(), "admin-1".to_string())
        .await
        .unwrap();

    assert!(!result4.operation_id.is_empty());
}

#[tokio::test]
async fn test_controller_duplicate_broker_prevention() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 2,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let peers = vec![];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Acquire slot for broker-1
    let _result1 = controller
        .acquire_decommission_slot("broker-1".to_string(), "admin-1".to_string())
        .await
        .unwrap();

    // Try to acquire another slot for the same broker
    let result2 = controller
        .acquire_decommission_slot("broker-1".to_string(), "admin-2".to_string())
        .await;

    assert!(result2.is_err());
}

#[tokio::test]
async fn test_controller_raft_leadership() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    // Single node cluster for simple testing
    let peers = vec![];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Initially not a leader
    let raft_info = controller.get_raft_info();
    assert!(!raft_info.is_leader);
    assert_eq!(raft_info.current_term, 0);

    // Start election
    let won = controller.start_election().await.unwrap();
    assert!(won);

    // Should now be leader
    let raft_info = controller.get_raft_info();
    assert!(raft_info.is_leader);
    assert_eq!(raft_info.current_term, 1);
    assert_eq!(raft_info.current_leader, Some("controller-1".to_string()));
}

#[tokio::test]
async fn test_controller_topic_management() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let peers = vec![];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Become leader first
    controller.start_election().await.unwrap();

    // Add some brokers
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

    // Test topic creation
    let create_request = CreateTopicRequest {
        name: "test-topic".to_string(),
        partitions: 3,
        replication_factor: 2,
        config: Some(TopicConfig {
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        }),
    };

    let response = controller.create_topic(create_request).await.unwrap();
    assert!(response.success);
    assert!(response.error_message.is_none());

    // Verify topic exists in metadata
    let metadata = controller.get_cluster_metadata().await.unwrap();
    assert_eq!(metadata.topics.len(), 1);
    assert_eq!(metadata.topics[0].name, "test-topic");
    assert_eq!(metadata.topics[0].partitions, 3);
    assert_eq!(metadata.topics[0].replication_factor, 2);
    assert_eq!(metadata.partition_assignments.len(), 3); // 3 partitions

    // Test topic deletion
    let delete_request = DeleteTopicRequest {
        name: "test-topic".to_string(),
    };

    let response = controller.delete_topic(delete_request).await.unwrap();
    assert!(response.success);

    // Verify topic is gone
    let metadata = controller.get_cluster_metadata().await.unwrap();
    assert_eq!(metadata.topics.len(), 0);
    assert_eq!(metadata.partition_assignments.len(), 0);
}

#[tokio::test]
async fn test_controller_non_leader_operations() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let peers = vec!["controller-2".to_string()];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Don't become leader - should reject leader-only operations
    let create_request = CreateTopicRequest {
        name: "test-topic".to_string(),
        partitions: 3,
        replication_factor: 2,
        config: None,
    };

    let response = controller.create_topic(create_request).await.unwrap();
    assert!(!response.success);
    assert!(response.error_message.unwrap().contains("Not the leader"));
}

#[tokio::test]
async fn test_controller_scaling_config_updates() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let peers = vec![];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Test valid config update
    let new_config = ScalingConfig {
        max_concurrent_additions: 5,
        max_concurrent_decommissions: 3,
        rebalance_timeout_ms: 600_000,
        traffic_migration_rate: 0.2,
        health_check_timeout_ms: 60_000,
    };

    let result = controller.update_scaling_config(new_config).await;
    assert!(result.is_ok());

    // Test invalid config (zero decommissions)
    let invalid_config = ScalingConfig {
        max_concurrent_additions: 5,
        max_concurrent_decommissions: 0, // Invalid
        rebalance_timeout_ms: 600_000,
        traffic_migration_rate: 0.2,
        health_check_timeout_ms: 60_000,
    };

    let result = controller.update_scaling_config(invalid_config).await;
    assert!(result.is_err());

    // Test invalid config (too many decommissions)
    let invalid_config2 = ScalingConfig {
        max_concurrent_additions: 5,
        max_concurrent_decommissions: 15, // Too high
        rebalance_timeout_ms: 600_000,
        traffic_migration_rate: 0.2,
        health_check_timeout_ms: 60_000,
    };

    let result = controller.update_scaling_config(invalid_config2).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_controller_concurrent_operations() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 5,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let peers = vec![];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Test concurrent slot acquisitions
    let mut tasks = vec![];
    for i in 0..3 {
        let controller_clone = controller.clone();
        tasks.push(tokio::spawn(async move {
            controller_clone
                .acquire_decommission_slot(format!("broker-{}", i), format!("admin-{}", i))
                .await
        }));
    }

    let mut successful = 0;
    for task in tasks {
        let result = timeout(Duration::from_secs(1), task).await.unwrap();
        if result.is_ok() {
            successful += 1;
        }
    }

    assert_eq!(successful, 3, "All concurrent acquisitions should succeed");

    // Verify status
    let status = controller.get_decommission_status().await.unwrap();
    assert_eq!(status.len(), 3);
}

#[tokio::test]
async fn test_controller_metadata_consistency() {
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };

    let peers = vec![];
    let controller = ControllerService::new("controller-1".to_string(), peers, scaling_config);

    // Become leader
    controller.start_election().await.unwrap();

    // Add brokers and verify metadata consistency
    let broker = BrokerInfo {
        id: "broker-1".to_string(),
        host: "localhost".to_string(),
        port_quic: 9092,
        port_rpc: 9093,
        rack_id: "rack-1".to_string(),
    };

    controller.register_broker(broker).await.unwrap();

    let metadata1 = controller.get_cluster_metadata().await.unwrap();
    assert_eq!(metadata1.brokers.len(), 1);
    assert_eq!(metadata1.leader, Some("controller-1".to_string()));
    assert_eq!(metadata1.term, 1);

    // Get metadata again - should be consistent
    let metadata2 = controller.get_cluster_metadata().await.unwrap();
    assert_eq!(metadata1.brokers.len(), metadata2.brokers.len());
    assert_eq!(metadata1.leader, metadata2.leader);
    assert_eq!(metadata1.term, metadata2.term);
}
