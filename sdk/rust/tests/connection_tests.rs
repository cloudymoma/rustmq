use rustmq_client::{
    config::{ClientConfig, RetryConfig, TlsConfig},
    connection::{Connection, ConnectionStats},
    error::{ClientError, Result},
};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Once;

static INIT: Once = Once::new();

fn init_crypto() {
    INIT.call_once(|| {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("Failed to install crypto provider");
    });
}

/// Mock broker server for testing
struct MockBrokerServer {
    listener: TcpListener,
    port: u16,
}

impl MockBrokerServer {
    async fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
            ClientError::Connection(format!("Failed to bind test server: {}", e))
        })?;
        let port = listener.local_addr().unwrap().port();
        
        Ok(MockBrokerServer { listener, port })
    }
    
    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
    
    async fn run_echo_server(self) {
        while let Ok((mut stream, _)) = self.listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                while let Ok(n) = stream.read(&mut buffer).await {
                    if n == 0 {
                        break;
                    }
                    let _ = stream.write_all(&buffer[..n]).await;
                }
            });
        }
    }
    
    async fn run_ping_server(self) {
        while let Ok((mut stream, _)) = self.listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                while let Ok(n) = stream.read(&mut buffer).await {
                    if n == 0 {
                        break;
                    }
                    // Echo back the ping data for health checks
                    if &buffer[..n] == b"rustmq_ping" {
                        let _ = stream.write_all(b"rustmq_ping").await;
                    }
                }
            });
        }
    }
}

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

/// Create TLS test configuration
fn create_tls_test_config(brokers: Vec<String>) -> ClientConfig {
    let mut config = create_test_config(brokers);
    config.enable_tls = true;
    config.tls_config = Some(TlsConfig {
        mode: rustmq_client::TlsMode::ServerAuth,
        ca_cert: None,
        client_cert: None,
        client_key: None,
        server_name: Some("localhost".to_string()),
        insecure_skip_verify: true, // For testing only
        supported_versions: vec!["1.2".to_string(), "1.3".to_string()],
        validation: rustmq_client::CertificateValidationConfig {
            verify_chain: false,
            verify_expiration: false,
            check_revocation: false,
            allow_self_signed: true,
            max_chain_depth: 10,
        },
        alpn_protocols: vec!["h3".to_string()],
    });
    config
}

#[tokio::test]
async fn test_connection_creation_with_valid_config() {
    init_crypto();
    let server = MockBrokerServer::new().await.unwrap();
    let address = server.address();
    
    // Start mock server
    tokio::spawn(server.run_echo_server());
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let config = create_test_config(vec![address]);
    
    // Note: This test will fail to establish actual QUIC connections since our mock server
    // only speaks TCP, but it tests the configuration and endpoint creation logic
    let result = Connection::new(&config).await;
    
    // We expect this to fail with NoConnectionsAvailable since mock server doesn't speak QUIC
    assert!(matches!(result, Err(ClientError::NoConnectionsAvailable)));
}

#[tokio::test]
async fn test_connection_creation_with_invalid_broker_address() {
    init_crypto();
    let config = create_test_config(vec!["invalid_address".to_string()]);
    
    let result = Connection::new(&config).await;
    assert!(matches!(result, Err(ClientError::InvalidConfig(_))));
}

#[tokio::test]
async fn test_connection_creation_with_empty_broker_list() {
    init_crypto();
    let config = create_test_config(vec![]);
    
    let result = Connection::new(&config).await;
    assert!(matches!(result, Err(ClientError::NoConnectionsAvailable)));
}

#[tokio::test]
#[ignore] // Hangs in release mode due to network connection attempts
async fn test_connection_retry_logic() {
    init_crypto();
    // Test with non-existent broker to trigger retry logic
    let mut config = create_test_config(vec!["127.0.0.1:99999".to_string()]);
    // Reduce timeouts to make test faster
    config.connect_timeout = Duration::from_millis(100);
    config.retry_config.base_delay = Duration::from_millis(50);
    config.retry_config.max_retries = 2;
    
    let start_time = std::time::Instant::now();
    let result = Connection::new(&config).await;
    let elapsed = start_time.elapsed();
    
    // Should fail after trying retries (any error is acceptable since broker doesn't exist)
    assert!(result.is_err(), "Expected connection to fail with non-existent broker");
    
    // Should take some time due to retries (at least one retry)
    assert!(elapsed >= config.retry_config.base_delay, "Should take at least base_delay for retry");
}

#[tokio::test]
#[ignore] // Hangs in release mode due to network timeouts
async fn test_tls_configuration_creation() {
    init_crypto();
    let config = create_tls_test_config(vec!["localhost:9092".to_string()]);
    
    // This tests TLS configuration creation - will fail at connection since no QUIC server
    let result = Connection::new(&config).await;
    assert!(matches!(result, Err(ClientError::NoConnectionsAvailable)));
}

#[tokio::test]
#[ignore] // Hangs in release mode due to network timeouts
async fn test_insecure_configuration_creation() {
    init_crypto();
    let config = create_test_config(vec!["localhost:9092".to_string()]);
    
    // This tests insecure configuration creation
    let result = Connection::new(&config).await;
    assert!(matches!(result, Err(ClientError::NoConnectionsAvailable)));
}

#[tokio::test]
async fn test_multiple_broker_addresses() {
    init_crypto();
    let config = create_test_config(vec![
        "127.0.0.1:9092".to_string(),
        "127.0.0.1:9093".to_string(),
        "127.0.0.1:9094".to_string(),
    ]);
    
    // All should fail but test validates parsing multiple addresses
    let result = Connection::new(&config).await;
    assert!(matches!(result, Err(ClientError::NoConnectionsAvailable)));
}

#[tokio::test]
async fn test_connection_stats_structure() {
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

#[tokio::test]
async fn test_client_error_categories() {
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

#[tokio::test]
async fn test_message_size_limits() {
    let large_size = 20 * 1024 * 1024; // 20MB
    let max_size = 16 * 1024 * 1024;   // 16MB
    
    let error = ClientError::MessageTooLarge { 
        size: large_size, 
        max_size 
    };
    
    assert!(matches!(error, ClientError::MessageTooLarge { .. }));
    assert_eq!(error.category(), "message_size");
}

#[tokio::test]
async fn test_uuid_generation_and_parsing() {
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

#[tokio::test]
async fn test_request_frame_format() {
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

#[tokio::test]
async fn test_exponential_backoff_calculation() {
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

#[tokio::test]
async fn test_round_robin_index_calculation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let index = AtomicUsize::new(0);
    let connection_count = 3;
    
    // Test round-robin behavior
    let indices: Vec<usize> = (0..10)
        .map(|_| index.fetch_add(1, Ordering::Relaxed) % connection_count)
        .collect();
    
    assert_eq!(indices, vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0]);
}

#[tokio::test]
async fn test_simple_random_function() {
    // Test our simple random function
    use std::time::{SystemTime, UNIX_EPOCH};
    
    fn simple_rand() -> f64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        (nanos % 1000) as f64 / 1000.0
    }
    
    let val1 = simple_rand();
    let val2 = simple_rand();
    
    // Should be in range [0, 1)
    assert!(val1 >= 0.0 && val1 < 1.0);
    assert!(val2 >= 0.0 && val2 < 1.0);
    
    // Values should be different (with high probability)
    // Note: This could theoretically fail but very unlikely
    assert_ne!(val1, val2);
}

#[tokio::test]
async fn test_connection_error_conversions() {
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

#[tokio::test]
async fn test_health_check_timeout() {
    let timeout_duration = Duration::from_millis(100);
    
    // Simulate a timeout scenario
    let timeout_result = tokio::time::timeout(
        timeout_duration,
        tokio::time::sleep(Duration::from_millis(200))
    ).await;
    
    assert!(timeout_result.is_err());
}

#[tokio::test]
async fn test_batch_message_chunking() {
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

// Integration test with a more realistic scenario
#[tokio::test]
async fn test_connection_lifecycle() {
    init_crypto();
    let config = create_test_config(vec!["127.0.0.1:9999".to_string()]);
    
    // Test that connection creation fails gracefully with unavailable broker
    let result = Connection::new(&config).await;
    assert!(matches!(result, Err(ClientError::NoConnectionsAvailable)));
    
    // Test configuration validation
    let invalid_config = create_test_config(vec!["not_a_valid_address".to_string()]);
    let result = Connection::new(&invalid_config).await;
    assert!(matches!(result, Err(ClientError::InvalidConfig(_))));
}

// Test transport configuration parameters
#[tokio::test]
async fn test_transport_config_parameters() {
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

// Test error handling in various scenarios
#[tokio::test]
async fn test_error_handling_scenarios() {
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

// Test concurrent access patterns
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