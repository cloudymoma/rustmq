pub mod client;
pub mod config;
pub mod error;
pub mod service;
pub mod types;

pub use client::*;
pub use config::*;
pub use error::*;
pub use service::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    /// Test helper to create a mock BigQuery message
    fn create_test_message() -> types::BigQueryMessage {
        let metadata = types::MessageMetadata {
            topic: "test-topic".to_string(),
            partition_id: 0,
            offset: 123,
            timestamp: Utc::now(),
            key: Some("test-key".to_string()),
            headers: {
                let mut headers = HashMap::new();
                headers.insert("source".to_string(), "test".to_string());
                headers
            },
            producer_id: Some("test-producer".to_string()),
            sequence_number: Some(1),
        };

        types::BigQueryMessage::new(
            serde_json::json!({
                "id": 123,
                "message": "test message",
                "timestamp": "2023-01-01T00:00:00Z"
            }),
            metadata,
        )
    }

    /// Test helper to create a test configuration
    fn create_test_config() -> config::BigQuerySubscriberConfig {
        config::BigQuerySubscriberConfig {
            project_id: "test-project".to_string(),
            dataset: "test_dataset".to_string(),
            table: "test_table".to_string(),
            write_method: config::WriteMethod::StreamingInserts {
                skip_invalid_rows: false,
                ignore_unknown_values: false,
                template_suffix: None,
            },
            subscription: config::SubscriptionConfig {
                broker_endpoints: vec!["localhost:9092".to_string()],
                topic: "test-topic".to_string(),
                consumer_group: "test-group".to_string(),
                partitions: None,
                start_offset: config::StartOffset::Latest,
                max_messages_per_fetch: 100,
                fetch_timeout_ms: 5000,
                session_timeout_ms: 30000,
            },
            auth: config::AuthConfig {
                method: config::AuthMethod::ApplicationDefault,
                service_account_key_file: None,
                scopes: vec![
                    "https://www.googleapis.com/auth/bigquery".to_string(),
                    "https://www.googleapis.com/auth/bigquery.insertdata".to_string(),
                ],
            },
            batching: config::BatchingConfig {
                max_rows_per_batch: 1000,
                max_batch_size_bytes: 1024 * 1024,
                max_batch_latency_ms: 1000,
                max_concurrent_batches: 10,
            },
            schema: config::SchemaConfig {
                mapping: config::SchemaMappingStrategy::Direct,
                column_mappings: HashMap::new(),
                default_values: HashMap::new(),
                auto_create_table: false,
                table_schema: None,
            },
            error_handling: config::ErrorHandlingConfig {
                max_retries: 3,
                retry_backoff: config::RetryBackoffStrategy::Exponential {
                    base_ms: 1000,
                    max_ms: 30000,
                },
                dead_letter_action: config::DeadLetterAction::Log,
                dead_letter_config: None,
            },
            monitoring: config::MonitoringConfig {
                enable_metrics: true,
                metrics_prefix: "test".to_string(),
                enable_health_check: true,
                health_check_addr: "127.0.0.1:8080".to_string(),
                log_successful_inserts: false,
                log_level: "info".to_string(),
            },
        }
    }

    #[test]
    fn test_bigquery_message_creation() {
        let message = create_test_message();
        
        assert_eq!(message.metadata.topic, "test-topic");
        assert_eq!(message.metadata.partition_id, 0);
        assert_eq!(message.metadata.offset, 123);
        assert!(message.insert_id.is_none()); // Should be None initially
        
        // Check that data is properly stored
        assert_eq!(message.data["id"], 123);
        assert_eq!(message.data["message"], "test message");
    }

    #[test]
    fn test_bigquery_message_insert_id_generation() {
        let mut message = create_test_message();
        
        // Initially no insert ID
        assert!(message.insert_id.is_none());
        
        // Generate insert ID
        message.generate_insert_id();
        
        // Should now have an insert ID
        assert!(message.insert_id.is_some());
        let insert_id = message.insert_id.as_ref().unwrap();
        
        // Insert ID should contain expected components
        assert!(insert_id.contains("test-topic"));
        assert!(insert_id.contains("0")); // partition
        assert!(insert_id.contains("123")); // offset
    }

    #[test]
    fn test_bigquery_batch_creation() {
        let batch_id = "test-batch-123".to_string();
        let batch = types::BigQueryBatch::new(batch_id.clone());
        
        assert_eq!(batch.metadata.batch_id, batch_id);
        assert_eq!(batch.messages.len(), 0);
        assert_eq!(batch.metadata.size_bytes, 0);
        assert_eq!(batch.metadata.message_count, 0);
        assert_eq!(batch.metadata.retry_attempt, 0);
    }

    #[test]
    fn test_bigquery_batch_add_message() {
        let mut batch = types::BigQueryBatch::new("test-batch".to_string());
        let message = create_test_message();
        
        // Initially empty
        assert_eq!(batch.messages.len(), 0);
        assert_eq!(batch.metadata.message_count, 0);
        
        // Add message
        batch.add_message(message);
        
        // Should now have one message
        assert_eq!(batch.messages.len(), 1);
        assert_eq!(batch.metadata.message_count, 1);
        assert!(batch.metadata.size_bytes > 0); // Should estimate size
    }

    #[test]
    fn test_bigquery_batch_is_full() {
        let mut batch = types::BigQueryBatch::new("test-batch".to_string());
        
        // Not full when empty
        assert!(!batch.is_full(10, 1024));
        
        // Add messages until full by count
        for i in 0..5 {
            let mut message = create_test_message();
            message.metadata.offset = i;
            batch.add_message(message);
        }
        
        // Should be full when max_rows is 3
        assert!(batch.is_full(3, 10_000_000));
        
        // Should not be full when max_rows is 10
        assert!(!batch.is_full(10, 10_000_000));
    }

    #[test]
    fn test_insert_result_success() {
        let stats = types::InsertStats {
            duration_ms: 100,
            transformation_time_ms: 10,
            api_time_ms: 80,
            bytes_sent: 1024,
            retry_count: 0,
        };
        
        let result = types::InsertResult::success(5, stats);
        
        assert!(result.success);
        assert_eq!(result.rows_inserted, 5);
        assert!(result.errors.is_empty());
        assert_eq!(result.stats.duration_ms, 100);
    }

    #[test]
    fn test_insert_result_failure() {
        let stats = types::InsertStats {
            duration_ms: 200,
            transformation_time_ms: 20,
            api_time_ms: 150,
            bytes_sent: 0,
            retry_count: 2,
        };
        
        let errors = vec![
            types::InsertError {
                message: "Test error".to_string(),
                code: Some("TEST_ERROR".to_string()),
                row_index: Some(0),
                field: None,
                retryable: true,
            }
        ];
        
        let result = types::InsertResult::failure(errors.clone(), stats);
        
        assert!(!result.success);
        assert_eq!(result.rows_inserted, 0);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].message, "Test error");
        assert_eq!(result.stats.retry_count, 2);
    }

    #[test]
    fn test_config_validation() {
        let config = create_test_config();
        
        // Should have valid required fields
        assert!(!config.project_id.is_empty());
        assert!(!config.dataset.is_empty());
        assert!(!config.table.is_empty());
        assert!(!config.subscription.topic.is_empty());
        assert!(!config.subscription.broker_endpoints.is_empty());
        
        // Should have reasonable defaults
        assert!(config.batching.max_rows_per_batch > 0);
        assert!(config.batching.max_batch_size_bytes > 0);
        assert!(config.error_handling.max_retries > 0);
    }

    #[test]
    fn test_health_status_serialization() {
        let health_status = types::HealthStatus {
            status: types::HealthState::Healthy,
            last_successful_insert: Some(Utc::now()),
            last_error: None,
            backlog_size: 0,
            messages_per_minute: 100,
            successful_inserts_per_minute: 95,
            failed_inserts_per_minute: 5,
            error_rate: 5.0,
            bigquery_connection: types::ConnectionStatus::Connected,
            rustmq_connection: types::ConnectionStatus::Connected,
        };
        
        // Should serialize and deserialize correctly
        let serialized = serde_json::to_string(&health_status).expect("Failed to serialize");
        let _deserialized: types::HealthStatus = serde_json::from_str(&serialized)
            .expect("Failed to deserialize");
    }

    #[test]
    fn test_metrics_serialization() {
        let metrics = types::SubscriberMetrics {
            messages_received: 1000,
            messages_processed: 950,
            messages_failed: 50,
            bytes_processed: 1024 * 1024,
            api_calls: 100,
            successful_api_calls: 95,
            failed_api_calls: 5,
            batches_created: 20,
            batches_sent: 18,
            avg_batch_size: 50.0,
            avg_processing_latency_ms: 25.5,
            avg_api_latency_ms: 100.2,
            current_backlog_size: 10,
            peak_backlog_size: 100,
        };
        
        // Should serialize and deserialize correctly
        let serialized = serde_json::to_string(&metrics).expect("Failed to serialize");
        let _deserialized: types::SubscriberMetrics = serde_json::from_str(&serialized)
            .expect("Failed to deserialize");
    }

    #[test]
    fn test_retry_backoff_strategies() {
        // Test exponential backoff
        let exponential = config::RetryBackoffStrategy::Exponential {
            base_ms: 1000,
            max_ms: 30000,
        };
        
        // Should serialize correctly
        let _serialized = serde_json::to_string(&exponential).expect("Failed to serialize");
        
        // Test linear backoff
        let linear = config::RetryBackoffStrategy::Linear {
            increment_ms: 500,
        };
        
        let _serialized = serde_json::to_string(&linear).expect("Failed to serialize");
        
        // Test fixed backoff
        let fixed = config::RetryBackoffStrategy::Fixed {
            delay_ms: 2000,
        };
        
        let _serialized = serde_json::to_string(&fixed).expect("Failed to serialize");
    }

    #[test]
    fn test_start_offset_types() {
        // Test Latest
        let latest = config::StartOffset::Latest;
        let _serialized = serde_json::to_string(&latest).expect("Failed to serialize");
        
        // Test Earliest
        let earliest = config::StartOffset::Earliest;
        let _serialized = serde_json::to_string(&earliest).expect("Failed to serialize");
        
        // Test Specific
        let specific = config::StartOffset::Specific(12345);
        let _serialized = serde_json::to_string(&specific).expect("Failed to serialize");
    }

    #[test]
    fn test_write_methods() {
        // Test StreamingInserts
        let streaming = config::WriteMethod::StreamingInserts {
            skip_invalid_rows: true,
            ignore_unknown_values: false,
            template_suffix: Some("test".to_string()),
        };
        let _serialized = serde_json::to_string(&streaming).expect("Failed to serialize");
        
        // Test StorageWrite
        let storage = config::WriteMethod::StorageWrite {
            stream_type: config::StorageWriteStreamType::Default,
            auto_commit: true,
        };
        let _serialized = serde_json::to_string(&storage).expect("Failed to serialize");
    }

    #[test]
    fn test_schema_mapping_strategies() {
        // Test Direct mapping
        let direct = config::SchemaMappingStrategy::Direct;
        let _serialized = serde_json::to_string(&direct).expect("Failed to serialize");
        
        // Test Custom mapping
        let custom = config::SchemaMappingStrategy::Custom;
        let _serialized = serde_json::to_string(&custom).expect("Failed to serialize");
        
        // Test Nested mapping
        let nested = config::SchemaMappingStrategy::Nested {
            root_field: "data".to_string(),
        };
        let _serialized = serde_json::to_string(&nested).expect("Failed to serialize");
    }

    #[test]
    fn test_dead_letter_actions() {
        // Test Drop
        let drop = config::DeadLetterAction::Drop;
        let _serialized = serde_json::to_string(&drop).expect("Failed to serialize");
        
        // Test Log
        let log = config::DeadLetterAction::Log;
        let _serialized = serde_json::to_string(&log).expect("Failed to serialize");
        
        // Test DeadLetterQueue
        let dlq = config::DeadLetterAction::DeadLetterQueue;
        let _serialized = serde_json::to_string(&dlq).expect("Failed to serialize");
        
        // Test File
        let file = config::DeadLetterAction::File {
            path: "/tmp/dead_letters.json".to_string(),
        };
        let _serialized = serde_json::to_string(&file).expect("Failed to serialize");
    }
}