//! Integration test for client broker discovery
use rustmq::broker::Broker;
use rustmq::config::{
    BrokerConfig, CacheConfig, NetworkConfig, ObjectStorageConfig, ReplicationConfig, StorageType,
    WalConfig,
};
use rustmq::Config;
use rustmq_client::*;
use std::sync::atomic::{AtomicU16, Ordering};
use tempfile::TempDir;
use tokio::time::Duration;

static PORT_COUNTER: AtomicU16 = AtomicU16::new(16000);

fn get_unique_ports() -> (u16, u16) {
    let base = PORT_COUNTER.fetch_add(10, Ordering::SeqCst);
    (base, base + 1)
}

async fn create_test_broker_config(temp_dir: &TempDir, quic_port: u16, rpc_port: u16) -> Config {
    let wal_path = temp_dir.path().join("wal");
    std::fs::create_dir_all(&wal_path).unwrap();

    let object_storage_path = temp_dir.path().join("storage");
    std::fs::create_dir_all(&object_storage_path).unwrap();

    Config {
        broker: BrokerConfig {
            id: format!("test-broker-{}", quic_port),
            rack_id: "test-rack-1".to_string(),
        },
        network: NetworkConfig {
            quic_listen: format!("127.0.0.1:{}", quic_port),
            rpc_listen: format!("127.0.0.1:{}", rpc_port),
            max_connections: 1000,
            connection_timeout_ms: 30000,
            quic_config: rustmq::config::QuicConfig::default(),
        },
        wal: WalConfig {
            path: wal_path,
            capacity_bytes: 10 * 1024 * 1024,
            fsync_on_write: true,
            segment_size_bytes: 1024 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        },
        object_storage: ObjectStorageConfig {
            storage_type: StorageType::Local {
                path: object_storage_path,
            },
            bucket: "test-bucket".to_string(),
            region: "us-central1".to_string(),
            endpoint: "https://storage.googleapis.com".to_string(),
            access_key: None,
            secret_key: None,
            service_account_path: None,
            multipart_threshold: 100 * 1024 * 1024,
            max_concurrent_uploads: 4,
        },
        cache: CacheConfig {
            write_cache_size_bytes: 1024 * 1024,
            read_cache_size_bytes: 1024 * 1024,
            eviction_policy: rustmq::config::EvictionPolicy::Lru,
        },
        replication: ReplicationConfig {
            min_in_sync_replicas: 1,
            ack_timeout_ms: 5000,
            max_replication_lag: 1000,
            heartbeat_timeout_ms: 30000,
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn test_client_discovers_broker() {
    rustmq_client::init_crypto_provider();

    let temp_dir = TempDir::new().unwrap();
    let (quic_port, rpc_port) = get_unique_ports();

    let config = create_test_broker_config(&temp_dir, quic_port, rpc_port).await;

    // Start broker
    let mut broker = Broker::from_config(config)
        .await
        .expect("Failed to create broker");
    broker.start().await.expect("Failed to start broker");

    // Give QUIC server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Connect client (disable background discovery loop in test config to manually trigger and assert)
    let client_config = ClientConfig {
        brokers: vec![format!("127.0.0.1:{}", quic_port)],
        client_id: Some("discovery-test-client".to_string()),
        discovery_interval: None, // Disable background loop
        ..Default::default()
    };

    let client = RustMqClient::new(client_config)
        .await
        .expect("Failed to create client");

    assert!(client.is_connected().await);

    let connection = client.get_connection().await.expect("Failed to get connection");

    // Manually trigger discovery
    let brokers = connection.discover_brokers().await.expect("Failed to discover brokers");

    assert_eq!(brokers.len(), 1);
    assert_eq!(brokers[0].host, "127.0.0.1");
    assert_eq!(brokers[0].port_quic, quic_port);
    assert_eq!(brokers[0].port_rpc, rpc_port);

    // Close client and broker
    client.close().await.expect("Failed to close client");
    broker.stop().await.expect("Failed to stop broker");
}
