use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub broker: BrokerConfig,
    pub network: NetworkConfig,
    pub wal: WalConfig,
    pub cache: CacheConfig,
    pub object_storage: ObjectStorageConfig,
    pub controller: ControllerConfig,
    pub replication: ReplicationConfig,
    pub etl: EtlConfig,
    pub scaling: ScalingConfig,
    pub operations: OperationsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub id: String,
    pub rack_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub quic_listen: String,
    pub rpc_listen: String,
    pub max_connections: usize,
    pub connection_timeout_ms: u64,
    pub quic_config: QuicConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    pub max_concurrent_uni_streams: u32,
    pub max_concurrent_bidi_streams: u32,
    pub max_idle_timeout_ms: u64,
    pub max_stream_data: u64,
    pub max_connection_data: u64,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            max_concurrent_uni_streams: 1000,
            max_concurrent_bidi_streams: 1000,
            max_idle_timeout_ms: 30_000, // 30 seconds
            max_stream_data: 1_024_000, // 1MB per stream
            max_connection_data: 10_240_000, // 10MB per connection
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalConfig {
    pub path: PathBuf,
    pub capacity_bytes: u64,
    pub fsync_on_write: bool,
    pub segment_size_bytes: u64,
    pub buffer_size: usize,
    /// Time interval in milliseconds after which WAL segments are uploaded regardless of size
    pub upload_interval_ms: u64,
    /// Flush interval in milliseconds when fsync_on_write is false
    pub flush_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub write_cache_size_bytes: u64,
    pub read_cache_size_bytes: u64,
    pub eviction_policy: EvictionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionPolicy {
    Lru,
    Lfu,
    Random,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStorageConfig {
    pub storage_type: StorageType,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub multipart_threshold: u64,
    pub max_concurrent_uploads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageType {
    S3,
    Gcs,
    Azure,
    Local { path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConfig {
    pub endpoints: Vec<String>,
    pub election_timeout_ms: u64,
    pub heartbeat_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub min_in_sync_replicas: usize,
    pub ack_timeout_ms: u64,
    pub max_replication_lag: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlConfig {
    pub enabled: bool,
    pub memory_limit_bytes: usize,
    pub execution_timeout_ms: u64,
    pub max_concurrent_executions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    /// Maximum number of brokers that can be added simultaneously
    pub max_concurrent_additions: usize,
    /// Maximum number of brokers that can be decommissioned simultaneously
    pub max_concurrent_decommissions: usize,
    /// Timeout for partition rebalancing during scaling operations (ms)
    pub rebalance_timeout_ms: u64,
    /// Gradual traffic migration rate (0.0 to 1.0 per minute)
    pub traffic_migration_rate: f64,
    /// Health check timeout for new brokers (ms)
    pub health_check_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsConfig {
    /// Enable runtime configuration updates
    pub allow_runtime_config_updates: bool,
    /// Rolling upgrade velocity (brokers per minute)
    pub upgrade_velocity: usize,
    /// Graceful shutdown timeout (ms)
    pub graceful_shutdown_timeout_ms: u64,
    /// Kubernetes deployment configuration
    pub kubernetes: KubernetesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfig {
    /// Use StatefulSets for deployment
    pub use_stateful_sets: bool,
    /// Persistent volume claim template
    pub pvc_storage_class: String,
    /// Volume size for WAL storage
    pub wal_volume_size: String,
    /// Pod affinity rules for volume attachment
    pub enable_pod_affinity: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: BrokerConfig {
                id: "broker-001".to_string(),
                rack_id: "default".to_string(),
            },
            network: NetworkConfig {
                quic_listen: "0.0.0.0:9092".to_string(),
                rpc_listen: "0.0.0.0:9093".to_string(),
                max_connections: 10000,
                connection_timeout_ms: 30000,
                quic_config: QuicConfig::default(),
            },
            wal: WalConfig {
                path: PathBuf::from("/tmp/rustmq/wal"),
                capacity_bytes: 10 * 1024 * 1024 * 1024, // 10GB
                fsync_on_write: true,
                segment_size_bytes: 128 * 1024 * 1024, // 128MB
                buffer_size: 64 * 1024, // 64KB
                upload_interval_ms: 10 * 60 * 1000, // 10 minutes
                flush_interval_ms: 1000, // 1 second
            },
            cache: CacheConfig {
                write_cache_size_bytes: 1024 * 1024 * 1024, // 1GB
                read_cache_size_bytes: 2 * 1024 * 1024 * 1024, // 2GB
                eviction_policy: EvictionPolicy::Lru,
            },
            object_storage: ObjectStorageConfig {
                storage_type: StorageType::Local {
                    path: PathBuf::from("/tmp/rustmq/storage"),
                },
                bucket: "rustmq-data".to_string(),
                region: "us-central1".to_string(),
                endpoint: "https://storage.googleapis.com".to_string(),
                access_key: None,
                secret_key: None,
                multipart_threshold: 100 * 1024 * 1024, // 100MB
                max_concurrent_uploads: 10,
            },
            controller: ControllerConfig {
                endpoints: vec!["controller-1:9094".to_string()],
                election_timeout_ms: 5000,
                heartbeat_interval_ms: 1000,
            },
            replication: ReplicationConfig {
                min_in_sync_replicas: 2,
                ack_timeout_ms: 5000,
                max_replication_lag: 1000,
            },
            etl: EtlConfig {
                enabled: false,
                memory_limit_bytes: 64 * 1024 * 1024, // 64MB
                execution_timeout_ms: 5000,
                max_concurrent_executions: 100,
            },
            scaling: ScalingConfig {
                max_concurrent_additions: 3,
                max_concurrent_decommissions: 1, // Safety constraint: one at a time
                rebalance_timeout_ms: 300_000, // 5 minutes
                traffic_migration_rate: 0.1, // 10% per minute
                health_check_timeout_ms: 30_000, // 30 seconds
            },
            operations: OperationsConfig {
                allow_runtime_config_updates: true,
                upgrade_velocity: 1, // 1 broker per minute
                graceful_shutdown_timeout_ms: 60_000, // 1 minute
                kubernetes: KubernetesConfig {
                    use_stateful_sets: true,
                    pvc_storage_class: "fast-ssd".to_string(),
                    wal_volume_size: "50Gi".to_string(),
                    enable_pod_affinity: true,
                },
            },
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| crate::error::RustMqError::Config(e.to_string()))?;
        Ok(config)
    }

    pub fn validate(&self) -> crate::Result<()> {
        if self.broker.id.is_empty() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "broker.id cannot be empty".to_string(),
            ));
        }

        if self.wal.capacity_bytes == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "wal.capacity_bytes must be greater than 0".to_string(),
            ));
        }

        if self.replication.min_in_sync_replicas == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "replication.min_in_sync_replicas must be greater than 0".to_string(),
            ));
        }

        if self.scaling.max_concurrent_decommissions == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "scaling.max_concurrent_decommissions must be greater than 0".to_string(),
            ));
        }

        if self.scaling.max_concurrent_decommissions > 10 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "scaling.max_concurrent_decommissions should not exceed 10 for safety".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation_max_concurrent_decommissions() {
        let mut config = Config::default();
        
        // Test zero value (should fail)
        config.scaling.max_concurrent_decommissions = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be greater than 0"));

        // Test exceeding maximum (should fail)
        config.scaling.max_concurrent_decommissions = 15;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("should not exceed 10"));

        // Test valid value (should pass)
        config.scaling.max_concurrent_decommissions = 2;
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.scaling.max_concurrent_decommissions, 1);
    }
}