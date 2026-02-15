// Test modules for OpenRaft controller components

pub mod simple_integration_tests;

#[cfg(test)]
mod working_tests {
    use super::*;
    use crate::controller::*;
    use std::time::Duration;

    /// Test basic working OpenRaft integration functionality
    #[tokio::test]
    async fn test_working_openraft_integration() {
        let config = WorkingRaftConfig::default();
        let manager = WorkingOpenRaftManager::new(config).await.unwrap();

        // Test start/stop
        manager.start().await.unwrap();
        assert!(manager.is_leader().await); // Single node should be leader

        manager.stop().await.unwrap();
    }

    /// Test simplified OpenRaft implementation
    #[tokio::test]
    async fn test_simplified_openraft() {
        let helper = WorkingRaftIntegrationHelper::new(1, vec![]);

        // Become leader for testing
        helper.become_leader().await;
        assert!(helper.is_leader().await);

        // Test topic operations
        let config = crate::controller::service::TopicConfig {
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        };

        let result = helper
            .create_topic("integration-test-topic".to_string(), 3, 2, config)
            .await
            .unwrap();

        assert!(result.success);

        // Verify topic exists
        let metadata = helper.get_cluster_metadata().await;
        assert_eq!(metadata.topics.len(), 1);
        assert!(metadata.topics.contains_key("integration-test-topic"));
    }

    /// Test working integration helper
    #[tokio::test]
    async fn test_working_integration_helper() {
        let helper = WorkingRaftIntegrationHelper::new(1, vec![]);

        // Become leader for testing
        helper.become_leader().await;
        assert!(helper.is_leader().await);

        // Test topic operations
        let config = crate::controller::service::TopicConfig {
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        };

        let result = helper
            .create_topic("working-integration-topic".to_string(), 3, 2, config)
            .await
            .unwrap();

        assert!(result.success);

        // Verify topic exists
        let metadata = helper.get_cluster_metadata().await;
        assert_eq!(metadata.topics.len(), 1);
        assert!(metadata.topics.contains_key("working-integration-topic"));
    }
}
