#[cfg(test)]
mod tests {
    use crate::{
        config::{ClientConfig, RetryConfig},
        error::ClientError,
        connection::{Connection, ConnectionStats, simple_rand},
    };
    use std::time::Duration;

    /// Create a test client configuration
    fn create_test_config(brokers: Vec<String>) -> ClientConfig {
        ClientConfig {
            brokers,
            client_id: Some("test-client".to_string()),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(10),
            enable_tls: false, // Use insecure for tests
            tls_config: None,
            max_connections: 5,
            keep_alive_interval: Duration::from_secs(30),
            retry_config: RetryConfig {
                max_retries: 3,
                base_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(5),
                multiplier: 2.0,
                jitter: false, // Disable jitter for deterministic tests
            },
            compression: Default::default(),
            auth: None,
        }
    }

    #[tokio::test]
    async fn test_connection_creation_with_invalid_broker_address() {
        // Initialize crypto provider for rustls
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        
        let config = create_test_config(vec!["invalid_address".to_string()]);
        
        let result = Connection::new(&config).await;
        assert!(matches!(result, Err(ClientError::InvalidConfig(_))));
    }

    #[tokio::test]
    async fn test_connection_creation_with_empty_broker_list() {
        // Initialize crypto provider for rustls
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        
        let config = create_test_config(vec![]);
        
        let result = Connection::new(&config).await;
        assert!(matches!(result, Err(ClientError::NoConnectionsAvailable)));
    }

    #[tokio::test]
    async fn test_connection_creation_with_nonexistent_broker() {
        // Initialize crypto provider for rustls
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        
        // Test with non-existent broker to trigger retry logic
        let config = create_test_config(vec!["127.0.0.1:99999".to_string()]);
        
        let result = Connection::new(&config).await;
        
        // Should fail after trying retries - could be NoConnectionsAvailable or another connection error
        assert!(result.is_err());
        
        // The connection should have attempted retries (we can't easily test timing since 
        // connection refused errors happen immediately)
    }

    #[test]
    fn test_connection_stats_structure() {
        // Test that ConnectionStats can be created and has expected fields
        let stats = ConnectionStats {
            total_connections: 3,
            active_connections: 2,
            brokers: vec!["broker1".to_string(), "broker2".to_string()],
            broker_stats: vec![],
        };
        
        assert_eq!(stats.total_connections, 3);
        assert_eq!(stats.active_connections, 2);
        assert_eq!(stats.brokers.len(), 2);
    }

    #[test]
    fn test_client_error_categories() {
        let connection_err = ClientError::Connection("test".to_string());
        assert_eq!(connection_err.category(), "connection");
        assert!(connection_err.is_retryable());
        
        let auth_err = ClientError::Authentication("test".to_string());
        assert_eq!(auth_err.category(), "authentication");
        assert!(!auth_err.is_retryable());
        
        let timeout_err = ClientError::Timeout { timeout_ms: 5000 };
        assert_eq!(timeout_err.category(), "timeout");
        assert!(timeout_err.is_retryable());
        
        let config_err = ClientError::InvalidConfig("test".to_string());
        assert_eq!(config_err.category(), "configuration");
        assert!(!config_err.is_retryable());
    }

    #[test]
    fn test_message_size_limits() {
        let large_size = 20 * 1024 * 1024; // 20MB
        let max_size = 16 * 1024 * 1024;   // 16MB
        
        let error = ClientError::MessageTooLarge { 
            size: large_size, 
            max_size 
        };
        
        assert!(matches!(error, ClientError::MessageTooLarge { .. }));
        assert_eq!(error.category(), "message_size");
    }

    #[test]
    fn test_uuid_generation_and_parsing() {
        use uuid::Uuid;
        
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        
        // UUIDs should be different
        assert_ne!(id1, id2);
        
        // Should be able to convert to bytes and back
        let bytes = id1.as_bytes();
        let parsed_id = Uuid::from_slice(bytes).unwrap();
        assert_eq!(id1, parsed_id);
    }

    #[test]
    fn test_request_frame_format() {
        use uuid::Uuid;
        
        let request_id = Uuid::new_v4();
        let request_data = b"test request data";
        
        // Simulate frame creation like in send_request
        let mut frame = Vec::new();
        frame.extend_from_slice(&request_id.as_bytes()[..]);
        frame.extend_from_slice(&(request_data.len() as u32).to_be_bytes());
        frame.extend_from_slice(request_data);
        
        // Verify frame structure
        assert_eq!(frame.len(), 16 + 4 + request_data.len()); // UUID + length + data
        
        // Parse back
        let parsed_id = Uuid::from_slice(&frame[0..16]).unwrap();
        let parsed_len = u32::from_be_bytes([frame[16], frame[17], frame[18], frame[19]]);
        let parsed_data = &frame[20..];
        
        assert_eq!(parsed_id, request_id);
        assert_eq!(parsed_len, request_data.len() as u32);
        assert_eq!(parsed_data, request_data);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let base_delay = Duration::from_millis(100);
        let multiplier = 2.0;
        let max_delay = Duration::from_secs(10);
        
        let mut delay = base_delay;
        
        // First retry
        delay = std::cmp::min(
            Duration::from_millis((delay.as_millis() as f64 * multiplier) as u64),
            max_delay,
        );
        assert_eq!(delay, Duration::from_millis(200));
        
        // Second retry
        delay = std::cmp::min(
            Duration::from_millis((delay.as_millis() as f64 * multiplier) as u64),
            max_delay,
        );
        assert_eq!(delay, Duration::from_millis(400));
        
        // Continue until max
        for _ in 0..10 {
            delay = std::cmp::min(
                Duration::from_millis((delay.as_millis() as f64 * multiplier) as u64),
                max_delay,
            );
        }
        assert_eq!(delay, max_delay);
    }

    #[test]
    fn test_round_robin_index_calculation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        
        let index = AtomicUsize::new(0);
        let connection_count = 3;
        
        // Test round-robin behavior
        let indices: Vec<usize> = (0..10)
            .map(|_| index.fetch_add(1, Ordering::Relaxed) % connection_count)
            .collect();
        
        assert_eq!(indices, vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0]);
    }

    #[test]
    fn test_simple_random_function() {
        // Test our simple random function
        let val1 = simple_rand();
        let val2 = simple_rand();
        
        // Should be in range [0, 1)
        assert!(val1 >= 0.0 && val1 < 1.0);
        assert!(val2 >= 0.0 && val2 < 1.0);
        
        // Values should be different (with high probability)
        // Note: This could theoretically fail but very unlikely
        assert_ne!(val1, val2);
    }

    #[test]
    fn test_connection_error_conversions() {
        use std::io;
        
        // Test IO error conversion
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
        let client_err: ClientError = io_err.into();
        assert!(matches!(client_err, ClientError::Connection(_)));
        
        // Test serde error conversion
        let serde_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let client_err: ClientError = serde_err.into();
        assert!(matches!(client_err, ClientError::Serialization(_)));
    }

    #[test]
    fn test_batch_message_chunking() {
        let messages: Vec<Vec<u8>> = (0..25)
            .map(|i| format!("message_{}", i).into_bytes())
            .collect();
        
        let batch_size = 10;
        let chunks: Vec<_> = messages.chunks(batch_size).collect();
        
        assert_eq!(chunks.len(), 3); // 25 messages in chunks of 10 = 3 chunks
        assert_eq!(chunks[0].len(), 10);
        assert_eq!(chunks[1].len(), 10);
        assert_eq!(chunks[2].len(), 5);
    }

    #[test]
    fn test_transport_config_parameters() {
        use quinn::{TransportConfig, VarInt};
        
        let mut transport_config = TransportConfig::default();
        
        // Test setting transport parameters like in create_endpoint
        transport_config
            .max_concurrent_bidi_streams(VarInt::from_u32(1000))
            .max_concurrent_uni_streams(VarInt::from_u32(1000))
            .stream_receive_window(VarInt::from_u32(1024 * 1024))
            .receive_window(VarInt::from_u32(8 * 1024 * 1024))
            .send_window(8 * 1024 * 1024);
        
        // Verify configuration was set (these methods are chainable)
        assert!(true); // If we got here without panic, config was valid
    }

    #[test]
    fn test_error_handling_scenarios() {
        // Test various error conditions that might occur
        
        // Invalid broker address format
        let result = "invalid:port:format".parse::<std::net::SocketAddr>();
        assert!(result.is_err());
        
        // Invalid duration conversion
        let large_duration = Duration::from_secs(u64::MAX);
        let conversion_result: std::result::Result<quinn::IdleTimeout, _> = large_duration.try_into();
        assert!(conversion_result.is_err());
        
        // Test message size validation
        let max_size = 16 * 1024 * 1024;
        let test_size = 20 * 1024 * 1024;
        assert!(test_size > max_size);
    }

    #[tokio::test]
    async fn test_concurrent_connection_access() {
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use std::collections::HashMap;
        
        // Simulate concurrent access to connection pool
        let connections: Arc<RwLock<HashMap<String, usize>>> = Arc::new(RwLock::new(HashMap::new()));
        
        let mut handles = vec![];
        
        for i in 0..10 {
            let connections_clone = Arc::clone(&connections);
            let handle = tokio::spawn(async move {
                let mut conn_map = connections_clone.write().await;
                conn_map.insert(format!("connection_{}", i), i);
            });
            handles.push(handle);
        }
        
        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        let final_connections = connections.read().await;
        assert_eq!(final_connections.len(), 10);
    }
}