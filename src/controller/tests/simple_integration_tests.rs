// Simple integration tests for working OpenRaft implementation
// Tests basic functionality without complex multi-node scenarios

use std::time::Duration;
use tokio::time::timeout;
use tracing_test::traced_test;

use crate::controller::{
    WorkingOpenRaftManager, WorkingRaftConfig, WorkingRaftIntegrationHelper,
    WorkingNodeId, WorkingAppData, TopicConfig as ServiceTopicConfig,
};
use crate::types::{BrokerInfo};

#[tokio::test]
#[traced_test]
async fn test_working_single_node_cluster() {
    let config = WorkingRaftConfig::default();
    let manager = WorkingOpenRaftManager::new(config).await.unwrap();
    
    // Start the manager
    manager.start().await.unwrap();
    
    // Should be leader in single-node setup
    assert!(manager.is_leader().await);
    
    // Test basic operations
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 0);
    assert_eq!(metadata.brokers.len(), 0);
    
    // Stop the manager
    manager.stop().await.unwrap();
}

#[tokio::test]
#[traced_test]
async fn test_working_topic_operations() {
    let config = WorkingRaftConfig::default();
    let manager = WorkingOpenRaftManager::new(config).await.unwrap();
    
    manager.start().await.unwrap();
    
    // Create a topic
    let topic_config = ServiceTopicConfig {
        retention_ms: Some(86400000),
        segment_bytes: Some(1073741824),
        compression_type: Some("lz4".to_string()),
    };
    
    let result = manager.create_topic(
        "working-test-topic".to_string(),
        3,
        1, // Single node replication
        topic_config,
    ).await.unwrap();
    
    assert!(result.success);
    assert!(result.error_message.is_none());
    
    // Verify topic exists
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 1);
    assert!(metadata.topics.contains_key("working-test-topic"));
    
    // Test topic deletion
    let result = manager.delete_topic("working-test-topic".to_string()).await.unwrap();
    assert!(result.success);
    
    // Verify topic is gone
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 0);
    
    manager.stop().await.unwrap();
}

#[tokio::test]
#[traced_test]
async fn test_working_broker_management() {
    let config = WorkingRaftConfig::default();
    let manager = WorkingOpenRaftManager::new(config).await.unwrap();
    
    manager.start().await.unwrap();
    
    let broker = BrokerInfo {
        id: "working-broker-1".to_string(),
        host: "localhost".to_string(),
        port_quic: 9092,
        port_rpc: 9093,
        rack_id: "rack-1".to_string(),
    };
    
    // Add broker
    let result = manager.add_broker(broker.clone()).await.unwrap();
    assert!(result.success);
    
    // Verify broker exists
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.brokers.len(), 1);
    assert!(metadata.brokers.contains_key("working-broker-1"));
    
    // Remove broker
    let result = manager.remove_broker("working-broker-1".to_string()).await.unwrap();
    assert!(result.success);
    
    // Verify broker is gone
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.brokers.len(), 0);
    
    manager.stop().await.unwrap();
}

#[tokio::test]
#[traced_test]
async fn test_working_integration_helper() {
    let helper = WorkingRaftIntegrationHelper::new(1, vec![]);
    
    // Become leader for testing
    helper.become_leader().await;
    assert!(helper.is_leader().await);
    
    // Test current term
    let term = helper.get_current_term().await;
    assert_eq!(term, 1); // Should be 1 after becoming leader
    
    // Test topic creation
    let config = ServiceTopicConfig {
        retention_ms: Some(86400000),
        segment_bytes: Some(1073741824),
        compression_type: Some("lz4".to_string()),
    };
    
    let result = helper.create_topic(
        "helper-test-topic".to_string(),
        3,
        2,
        config,
    ).await.unwrap();
    
    assert!(result.success);
    assert!(result.error_message.is_none());
    
    // Verify topic exists
    let metadata = helper.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 1);
    assert!(metadata.topics.contains_key("helper-test-topic"));
    
    // Test topic deletion
    let result = helper.delete_topic("helper-test-topic".to_string()).await.unwrap();
    assert!(result.success);
    
    // Verify topic is gone
    let metadata = helper.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 0);
}

#[tokio::test]
#[traced_test]
async fn test_working_leadership_requirements() {
    let helper = WorkingRaftIntegrationHelper::new(1, vec![]);
    
    // Should not be leader initially
    assert!(!helper.is_leader().await);
    
    // Operations should fail when not leader
    let config = ServiceTopicConfig {
        retention_ms: Some(86400000),
        segment_bytes: Some(1073741824),
        compression_type: Some("lz4".to_string()),
    };
    
    let result = helper.create_topic(
        "no-leader-topic".to_string(),
        3,
        2,
        config,
    ).await.unwrap();
    
    assert!(!result.success);
    assert!(result.error_message.is_some());
    assert_eq!(result.error_message.unwrap(), "Not the leader");
    
    // After becoming leader, operations should succeed
    helper.become_leader().await;
    assert!(helper.is_leader().await);
    
    let config = ServiceTopicConfig {
        retention_ms: Some(86400000),
        segment_bytes: Some(1073741824),
        compression_type: Some("lz4".to_string()),
    };
    
    let result = helper.create_topic(
        "leader-topic".to_string(),
        3,
        2,
        config,
    ).await.unwrap();
    
    assert!(result.success);
}

#[tokio::test]
#[traced_test]
async fn test_working_multiple_operations() {
    let config = WorkingRaftConfig::default();
    let manager = WorkingOpenRaftManager::new(config).await.unwrap();
    
    manager.start().await.unwrap();
    
    // Create multiple topics
    for i in 1..=5 {
        let topic_config = ServiceTopicConfig {
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        };
        
        let result = manager.create_topic(
            format!("topic-{}", i),
            3,
            1,
            topic_config,
        ).await.unwrap();
        
        assert!(result.success);
    }
    
    // Verify all topics exist
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 5);
    
    for i in 1..=5 {
        assert!(metadata.topics.contains_key(&format!("topic-{}", i)));
    }
    
    // Delete all topics
    for i in 1..=5 {
        let result = manager.delete_topic(format!("topic-{}", i)).await.unwrap();
        assert!(result.success);
    }
    
    // Verify all topics are gone
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 0);
    
    manager.stop().await.unwrap();
}

#[tokio::test]
#[traced_test]
async fn test_working_concurrent_operations() {
    let config = WorkingRaftConfig::default();
    let manager = WorkingOpenRaftManager::new(config).await.unwrap();
    
    manager.start().await.unwrap();
    
    // Create multiple brokers concurrently
    let handles: Vec<_> = (1..=10)
        .map(|i| {
            let manager = &manager;
            async move {
                let broker = BrokerInfo {
                    id: format!("concurrent-broker-{}", i),
                    host: "localhost".to_string(),
                    port_quic: 9090 + i,
                    port_rpc: 9100 + i,
                    rack_id: format!("rack-{}", i),
                };
                manager.add_broker(broker).await
            }
        })
        .collect();
    
    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.success);
    }
    
    // Verify all brokers were added
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.brokers.len(), 10);
    
    for i in 1..=10 {
        assert!(metadata.brokers.contains_key(&format!("concurrent-broker-{}", i)));
    }
    
    manager.stop().await.unwrap();
}

#[tokio::test]
#[traced_test]
async fn test_working_error_cases() {
    let config = WorkingRaftConfig::default();
    let manager = WorkingOpenRaftManager::new(config).await.unwrap();
    
    manager.start().await.unwrap();
    
    // Try to create a topic
    let topic_config = ServiceTopicConfig {
        retention_ms: Some(86400000),
        segment_bytes: Some(1073741824),
        compression_type: Some("lz4".to_string()),
    };
    
    let result = manager.create_topic(
        "error-test-topic".to_string(),
        3,
        1,
        topic_config.clone(),
    ).await.unwrap();
    assert!(result.success);
    
    // Try to create the same topic again (should fail)
    let result = manager.create_topic(
        "error-test-topic".to_string(),
        3,
        1,
        topic_config,
    ).await.unwrap();
    assert!(!result.success);
    assert!(result.error_message.is_some());
    assert!(result.error_message.unwrap().contains("already exists"));
    
    // Try to delete a non-existent topic
    let result = manager.delete_topic("non-existent-topic".to_string()).await.unwrap();
    assert!(!result.success);
    assert!(result.error_message.is_some());
    assert!(result.error_message.unwrap().contains("not found"));
    
    // Try to remove a non-existent broker
    let result = manager.remove_broker("non-existent-broker".to_string()).await.unwrap();
    assert!(!result.success);
    assert!(result.error_message.is_some());
    assert!(result.error_message.unwrap().contains("not found"));
    
    manager.stop().await.unwrap();
}

#[tokio::test]
#[traced_test]
async fn test_working_state_persistence() {
    let config = WorkingRaftConfig::default();
    let manager = WorkingOpenRaftManager::new(config).await.unwrap();
    
    manager.start().await.unwrap();
    
    // Create some state
    let topic_config = ServiceTopicConfig {
        retention_ms: Some(86400000),
        segment_bytes: Some(1073741824),
        compression_type: Some("lz4".to_string()),
    };
    
    manager.create_topic(
        "persistent-topic".to_string(),
        3,
        1,
        topic_config,
    ).await.unwrap();
    
    let broker = BrokerInfo {
        id: "persistent-broker".to_string(),
        host: "localhost".to_string(),
        port_quic: 9092,
        port_rpc: 9093,
        rack_id: "rack-1".to_string(),
    };
    manager.add_broker(broker).await.unwrap();
    
    // Verify state exists
    let metadata = manager.get_cluster_metadata().await;
    assert_eq!(metadata.topics.len(), 1);
    assert_eq!(metadata.brokers.len(), 1);
    assert!(metadata.last_applied_log > 0);
    
    manager.stop().await.unwrap();
}