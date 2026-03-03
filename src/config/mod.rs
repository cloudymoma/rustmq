pub mod broker;
pub mod controller;
pub mod etl;
pub mod network;
pub mod operations;
pub mod rate_limiting;
pub mod security;
pub mod storage;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Re-export all types for backward compatibility
pub use broker::*;
pub use controller::*;
pub use etl::*;
pub use network::*;
pub use operations::*;
pub use rate_limiting::*;
pub use security::*;
pub use storage::*;

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
    pub rate_limiting: RateLimitConfig,
    pub security: SecurityConfig,
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
                buffer_size: 64 * 1024,                // 64KB
                upload_interval_ms: 10 * 60 * 1000,    // 10 minutes
                flush_interval_ms: 1000,               // 1 second
            },
            cache: CacheConfig {
                write_cache_size_bytes: 1024 * 1024 * 1024,    // 1GB
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
                bind_addr: "127.0.0.1".to_string(),
                rpc_port: 9095,
                admin_port: 9642,
                raft: RaftConfig {
                    max_payload_entries: 300,
                    enable_tick: true,
                    enable_heartbeat: true,
                    heartbeat_timeout_ms: 2000, // Must be greater than controller.heartbeat_interval_ms (1000)
                    install_snapshot_timeout_ms: 300000, // 5 minutes
                    max_replication_lag: 1000,
                    enable_elect: true,
                    snapshot_policy: SnapshotPolicy {
                        log_entries_since_last: 5000,
                        enable_periodic: true,
                        periodic_interval_secs: 3600, // 1 hour
                    },
                    max_uncommitted_entries: 1000,
                    network_timeout_ms: 10000,
                },
            },
            replication: ReplicationConfig {
                min_in_sync_replicas: 2,
                ack_timeout_ms: 5000,
                max_replication_lag: 1000,
                heartbeat_timeout_ms: 30000, // 30 seconds
            },
            etl: EtlConfig {
                enabled: false,
                memory_limit_bytes: 64 * 1024 * 1024, // 64MB
                execution_timeout_ms: 5000,
                max_concurrent_executions: 100,
                pipelines: vec![], // No pipelines by default
                instance_pool: EtlInstancePoolConfig {
                    max_pool_size: 50,
                    warmup_instances: 5,
                    creation_rate_limit: 10.0, // 10 instances per second
                    idle_timeout_seconds: 300, // 5 minutes
                    enable_lru_eviction: true,
                },
            },
            scaling: ScalingConfig {
                max_concurrent_additions: 3,
                max_concurrent_decommissions: 1, // Safety constraint: one at a time
                rebalance_timeout_ms: 300_000,   // 5 minutes
                traffic_migration_rate: 0.1,     // 10% per minute
                health_check_timeout_ms: 30_000, // 30 seconds
            },
            operations: OperationsConfig {
                allow_runtime_config_updates: true,
                upgrade_velocity: 1,                  // 1 broker per minute
                graceful_shutdown_timeout_ms: 60_000, // 1 minute
                kubernetes: KubernetesConfig {
                    use_stateful_sets: true,
                    pvc_storage_class: "fast-ssd".to_string(),
                    wal_volume_size: "50Gi".to_string(),
                    enable_pod_affinity: true,
                },
            },
            rate_limiting: RateLimitConfig::default(),
            security: SecurityConfig::default(),
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

    /// Comprehensive validation of configuration with logical consistency checks
    pub fn validate(&self) -> crate::Result<()> {
        // Basic field validation
        self.validate_basic_fields()?;

        // Cross-component logical consistency checks
        self.validate_replication_consistency()?;
        self.validate_storage_consistency()?;
        self.validate_network_consistency()?;
        self.validate_controller_consistency()?;
        self.validate_scaling_consistency()?;
        self.validate_etl_consistency()?;
        self.validate_timeout_consistency()?;
        self.validate_performance_consistency()?;

        // Component-specific validation
        self.rate_limiting.validate()?;
        self.security.validate()?;

        Ok(())
    }

    /// Validate basic required fields
    fn validate_basic_fields(&self) -> crate::Result<()> {
        if self.broker.id.is_empty() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "broker.id cannot be empty".to_string(),
            ));
        }

        if self.broker.rack_id.is_empty() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "broker.rack_id cannot be empty".to_string(),
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

        // Create WAL directory if it doesn't exist
        if !self.wal.path.exists() {
            if let Err(e) = std::fs::create_dir_all(&self.wal.path) {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "Cannot create wal.path {}: {}",
                    self.wal.path.display(),
                    e
                )));
            }
        }

        Ok(())
    }

    /// Validate replication configuration consistency
    fn validate_replication_consistency(&self) -> crate::Result<()> {
        if self.replication.heartbeat_timeout_ms == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "replication.heartbeat_timeout_ms must be greater than 0".to_string(),
            ));
        }

        if self.controller.endpoints.len() > 1
            && self.replication.min_in_sync_replicas > self.controller.endpoints.len()
        {
            return Err(crate::error::RustMqError::InvalidConfig(format!(
                "replication.min_in_sync_replicas ({}) cannot exceed number of controller endpoints ({})",
                self.replication.min_in_sync_replicas,
                self.controller.endpoints.len()
            )));
        }

        if self.replication.heartbeat_timeout_ms <= self.controller.heartbeat_interval_ms {
            return Err(crate::error::RustMqError::InvalidConfig(
                "replication.heartbeat_timeout_ms must be greater than controller.heartbeat_interval_ms".to_string(),
            ));
        }

        if self.replication.ack_timeout_ms > self.replication.heartbeat_timeout_ms * 3 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "replication.ack_timeout_ms should not exceed 3x heartbeat_timeout_ms to avoid unnecessary delays".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate storage configuration consistency
    fn validate_storage_consistency(&self) -> crate::Result<()> {
        if self.wal.segment_size_bytes >= self.wal.capacity_bytes {
            return Err(crate::error::RustMqError::InvalidConfig(
                "wal.segment_size_bytes must be smaller than wal.capacity_bytes".to_string(),
            ));
        }

        if self.wal.buffer_size as u64 > self.wal.segment_size_bytes / 2 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "wal.buffer_size should not exceed half of wal.segment_size_bytes".to_string(),
            ));
        }

        if self.wal.upload_interval_ms < self.wal.flush_interval_ms * 2 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "wal.upload_interval_ms should be at least 2x flush_interval_ms".to_string(),
            ));
        }

        if self.cache.write_cache_size_bytes == 0 || self.cache.read_cache_size_bytes == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "cache sizes must be greater than 0".to_string(),
            ));
        }

        if self.object_storage.multipart_threshold < 5 * 1024 * 1024 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "object_storage.multipart_threshold should be at least 5MB".to_string(),
            ));
        }

        if let crate::config::StorageType::Local { path } = &self.object_storage.storage_type {
            if !path.exists() {
                if let Err(e) = std::fs::create_dir_all(path) {
                    return Err(crate::error::RustMqError::InvalidConfig(format!(
                        "Cannot create object_storage.storage_type.Local.path {}: {}",
                        path.display(),
                        e
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate network configuration consistency
    fn validate_network_consistency(&self) -> crate::Result<()> {
        let quic_addr = self
            .network
            .quic_listen
            .parse::<std::net::SocketAddr>()
            .map_err(|_| {
                crate::error::RustMqError::InvalidConfig(format!(
                    "network.quic_listen is not a valid socket address: {}",
                    self.network.quic_listen
                ))
            })?;

        let rpc_addr = self
            .network
            .rpc_listen
            .parse::<std::net::SocketAddr>()
            .map_err(|_| {
                crate::error::RustMqError::InvalidConfig(format!(
                    "network.rpc_listen is not a valid socket address: {}",
                    self.network.rpc_listen
                ))
            })?;

        if quic_addr.port() == rpc_addr.port() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "network.quic_listen and network.rpc_listen cannot use the same port".to_string(),
            ));
        }

        if self.network.max_connections == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "network.max_connections must be greater than 0".to_string(),
            ));
        }

        if self.network.quic_config.max_idle_timeout_ms < 1000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "network.quic_config.max_idle_timeout_ms should be at least 1000ms".to_string(),
            ));
        }

        if self.network.quic_config.max_stream_data == 0
            || self.network.quic_config.max_connection_data == 0
        {
            return Err(crate::error::RustMqError::InvalidConfig(
                "QUIC stream and connection data limits must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate controller configuration consistency
    fn validate_controller_consistency(&self) -> crate::Result<()> {
        if self.controller.endpoints.is_empty() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "controller.endpoints cannot be empty".to_string(),
            ));
        }

        if self.controller.election_timeout_ms <= self.controller.heartbeat_interval_ms * 2 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "controller.election_timeout_ms should be at least 2x heartbeat_interval_ms"
                    .to_string(),
            ));
        }

        if self.controller.raft.heartbeat_timeout_ms <= self.controller.heartbeat_interval_ms {
            return Err(crate::error::RustMqError::InvalidConfig(
                "controller.raft.heartbeat_timeout_ms must be greater than controller.heartbeat_interval_ms".to_string(),
            ));
        }

        if self.controller.raft.max_payload_entries == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "controller.raft.max_payload_entries must be greater than 0".to_string(),
            ));
        }

        if self.controller.raft.snapshot_policy.log_entries_since_last == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "controller.raft.snapshot_policy.log_entries_since_last must be greater than 0"
                    .to_string(),
            ));
        }

        let _addr = self
            .controller
            .bind_addr
            .parse::<std::net::IpAddr>()
            .map_err(|_| {
                crate::error::RustMqError::InvalidConfig(format!(
                    "controller.bind_addr is not a valid IP address: {}",
                    self.controller.bind_addr
                ))
            })?;

        Ok(())
    }

    /// Validate scaling configuration consistency
    fn validate_scaling_consistency(&self) -> crate::Result<()> {
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

        if self.scaling.traffic_migration_rate <= 0.0 || self.scaling.traffic_migration_rate > 1.0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "scaling.traffic_migration_rate must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.scaling.health_check_timeout_ms < 1000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "scaling.health_check_timeout_ms should be at least 1000ms".to_string(),
            ));
        }

        if self.operations.upgrade_velocity == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "operations.upgrade_velocity must be greater than 0".to_string(),
            ));
        }

        if self.operations.upgrade_velocity > self.controller.endpoints.len() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "operations.upgrade_velocity cannot exceed number of controller endpoints"
                    .to_string(),
            ));
        }

        Ok(())
    }

    /// Validate ETL configuration consistency
    fn validate_etl_consistency(&self) -> crate::Result<()> {
        if !self.etl.enabled {
            return Ok(());
        }

        if self.etl.memory_limit_bytes == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "etl.memory_limit_bytes must be greater than 0 when ETL is enabled".to_string(),
            ));
        }

        if self.etl.execution_timeout_ms == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "etl.execution_timeout_ms must be greater than 0 when ETL is enabled".to_string(),
            ));
        }

        if self.etl.instance_pool.max_pool_size == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "etl.instance_pool.max_pool_size must be greater than 0 when ETL is enabled"
                    .to_string(),
            ));
        }

        if self.etl.instance_pool.warmup_instances > self.etl.instance_pool.max_pool_size {
            return Err(crate::error::RustMqError::InvalidConfig(
                "etl.instance_pool.warmup_instances cannot exceed max_pool_size".to_string(),
            ));
        }

        for (i, pipeline) in self.etl.pipelines.iter().enumerate() {
            if pipeline.pipeline_id.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "etl.pipelines[{}].pipeline_id cannot be empty",
                    i
                )));
            }

            if pipeline.enabled && pipeline.stages.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "etl.pipelines[{}].stages cannot be empty when pipeline is enabled",
                    i
                )));
            }

            let mut prev_priority = None;
            for stage in &pipeline.stages {
                if let Some(prev) = prev_priority {
                    if stage.priority <= prev {
                        return Err(crate::error::RustMqError::InvalidConfig(format!(
                            "etl.pipelines[{}] stages must have increasing priority values",
                            i
                        )));
                    }
                }
                prev_priority = Some(stage.priority);

                if stage.modules.is_empty() {
                    return Err(crate::error::RustMqError::InvalidConfig(format!(
                        "etl.pipelines[{}] stage priority {} has no modules",
                        i, stage.priority
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate timeout consistency across components
    fn validate_timeout_consistency(&self) -> crate::Result<()> {
        if self.network.connection_timeout_ms < 1000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "network.connection_timeout_ms should be at least 1000ms".to_string(),
            ));
        }

        if self.etl.enabled
            && self.etl.execution_timeout_ms > self.network.connection_timeout_ms * 2
        {
            return Err(crate::error::RustMqError::InvalidConfig(
                "etl.execution_timeout_ms should not exceed 2x network.connection_timeout_ms"
                    .to_string(),
            ));
        }

        if self.operations.graceful_shutdown_timeout_ms < 5000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "operations.graceful_shutdown_timeout_ms should be at least 5000ms".to_string(),
            ));
        }

        if self.scaling.rebalance_timeout_ms < 30000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "scaling.rebalance_timeout_ms should be at least 30000ms for safe rebalancing"
                    .to_string(),
            ));
        }

        Ok(())
    }

    /// Validate performance-related configuration consistency
    fn validate_performance_consistency(&self) -> crate::Result<()> {
        if !self.wal.fsync_on_write && self.wal.flush_interval_ms == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "wal.flush_interval_ms must be greater than 0 when wal.fsync_on_write is false"
                    .to_string(),
            ));
        }

        if self.object_storage.max_concurrent_uploads == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "object_storage.max_concurrent_uploads must be greater than 0".to_string(),
            ));
        }

        if self.object_storage.max_concurrent_uploads > 100 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "object_storage.max_concurrent_uploads should not exceed 100 to avoid overwhelming the storage backend".to_string(),
            ));
        }

        if self.etl.enabled && self.etl.max_concurrent_executions == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "etl.max_concurrent_executions must be greater than 0 when ETL is enabled"
                    .to_string(),
            ));
        }

        let total_cache_size = self.cache.write_cache_size_bytes + self.cache.read_cache_size_bytes;
        if total_cache_size > 100_000_000_000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "total cache size exceeds 100GB - this may cause memory pressure".to_string(),
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

        config.scaling.max_concurrent_decommissions = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be greater than 0"));

        config.scaling.max_concurrent_decommissions = 15;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("should not exceed 10"));

        config.scaling.max_concurrent_decommissions = 2;
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_validation_heartbeat_timeout() {
        let mut config = Config::default();

        config.replication.heartbeat_timeout_ms = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("heartbeat_timeout_ms must be greater than 0"));

        config.replication.heartbeat_timeout_ms = 30000;
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        let result = config.validate();
        assert!(result.is_ok());
        assert_eq!(config.scaling.max_concurrent_decommissions, 1);
        assert_eq!(config.replication.heartbeat_timeout_ms, 30000);
    }

    #[test]
    fn test_rate_limiting_default_config() {
        let rate_limit_config = RateLimitConfig::default();
        assert!(rate_limit_config.validate().is_ok());

        assert!(rate_limit_config.enabled);
        assert_eq!(rate_limit_config.global.requests_per_second, 1000);
        assert_eq!(rate_limit_config.global.burst_capacity, 2000);
        assert_eq!(rate_limit_config.per_ip.requests_per_second, 50);
        assert_eq!(rate_limit_config.per_ip.burst_capacity, 100);
        assert!(rate_limit_config.per_ip.enabled);

        let endpoints = &rate_limit_config.endpoints;
        assert!(endpoints.health.enabled);
        assert_eq!(endpoints.health.requests_per_second, 100);
        assert_eq!(endpoints.read_operations.requests_per_second, 30);
        assert_eq!(endpoints.write_operations.requests_per_second, 10);
        assert_eq!(endpoints.cluster_operations.requests_per_second, 5);

        assert!(rate_limit_config.cleanup.enabled);
        assert_eq!(rate_limit_config.cleanup.cleanup_interval_secs, 300);
    }

    #[test]
    fn test_rate_limiting_validation_failures() {
        let mut config = RateLimitConfig::default();

        config.global.requests_per_second = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("global.requests_per_second must be greater than 0"));

        config.global.requests_per_second = 100;

        config.global.burst_capacity = 50;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("global.burst_capacity must be >= requests_per_second"));

        config.global.burst_capacity = 200;

        config.per_ip.max_tracked_ips = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("per_ip.max_tracked_ips must be greater than 0"));

        config.per_ip.max_tracked_ips = 1000;

        config.endpoints.health.endpoint_patterns.clear();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("health.endpoint_patterns cannot be empty when enabled"));
    }

    #[test]
    fn test_rate_limiting_extreme_values() {
        let mut config = RateLimitConfig::default();

        config.global.requests_per_second = 2_000_000;
        config.global.burst_capacity = 2_000_000;
        let result = config.validate();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("exceeds reasonable limit") || error_msg.contains("1M RPS"));

        config.global.requests_per_second = 1000;
        config.per_ip.max_tracked_ips = 2_000_000;
        let result = config.validate();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("exceeds reasonable limit") || error_msg.contains("1M IPs"));

        config.per_ip.max_tracked_ips = 10000;
        config.cleanup.cleanup_interval_secs = 5;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("should be at least 10 seconds"));
    }

    #[test]
    fn test_rate_limiting_disabled_validation() {
        let mut config = RateLimitConfig::default();

        config.per_ip.enabled = false;
        config.per_ip.requests_per_second = 0;
        assert!(config.validate().is_ok());

        config.endpoints.health.enabled = false;
        config.endpoints.health.requests_per_second = 0;
        config.endpoints.health.endpoint_patterns.clear();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_endpoint_patterns_validation() {
        let mut config = RateLimitConfig::default();

        config.endpoints.health.endpoint_patterns = vec!["".to_string(), "  ".to_string()];
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("contains empty pattern"));

        config.endpoints.health.endpoint_patterns = vec![
            "/health".to_string(),
            "/api/v1/health".to_string(),
            "GET:/api/v1/status".to_string(),
        ];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_endpoint_category_limits_individual_validation() {
        let mut category = EndpointCategoryLimits {
            enabled: true,
            requests_per_second: 10,
            burst_capacity: 5,
            window_size_secs: 60,
            endpoint_patterns: vec!["/test".to_string()],
        };

        let result = category.validate_category("test_category");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("burst_capacity must be >= requests_per_second"));

        category.burst_capacity = 20;
        assert!(category.validate_category("test_category").is_ok());
    }

    #[test]
    fn test_toml_serialization_deserialization() {
        let config = RateLimitConfig::default();
        let toml_string = toml::to_string(&config).unwrap();
        let deserialized: RateLimitConfig = toml::from_str(&toml_string).unwrap();
        assert!(deserialized.validate().is_ok());
        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(
            config.global.requests_per_second,
            deserialized.global.requests_per_second
        );
    }

    #[test]
    fn test_full_config_with_rate_limiting() {
        let config = Config::default();
        let toml_string = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_string).unwrap();
        assert!(deserialized.validate().is_ok());
        assert!(deserialized.rate_limiting.enabled);
        assert_eq!(deserialized.rate_limiting.global.requests_per_second, 1000);
    }

    #[test]
    fn test_validation_basic_fields() {
        let mut config = Config::default();

        config.broker.id = "".to_string();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("broker.id cannot be empty"));

        config.broker.id = "broker-1".to_string();
        config.broker.rack_id = "".to_string();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("broker.rack_id cannot be empty"));
    }

    #[test]
    fn test_validation_replication_consistency() {
        let mut config = Config::default();

        config.controller.endpoints = vec![
            "controller-1:9094".to_string(),
            "controller-2:9094".to_string(),
        ];
        config.replication.min_in_sync_replicas = 5;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot exceed number of controller endpoints"));

        config.replication.min_in_sync_replicas = 2;
        config.replication.heartbeat_timeout_ms = 500;
        config.controller.heartbeat_interval_ms = 1000;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(
            "heartbeat_timeout_ms must be greater than controller.heartbeat_interval_ms"
        ));

        config.replication.heartbeat_timeout_ms = 2000;
        config.controller.heartbeat_interval_ms = 1000;
        config.replication.ack_timeout_ms = 7000;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("should not exceed 3x heartbeat_timeout_ms"));
    }

    #[test]
    fn test_validation_storage_consistency() {
        let mut config = Config::default();

        config.wal.segment_size_bytes = 10 * 1024 * 1024 * 1024;
        config.wal.capacity_bytes = 5 * 1024 * 1024 * 1024;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("segment_size_bytes must be smaller than wal.capacity_bytes"));

        config.wal.capacity_bytes = 20 * 1024 * 1024 * 1024;
        config.wal.segment_size_bytes = 128 * 1024 * 1024;
        config.wal.buffer_size = 100 * 1024 * 1024;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("buffer_size should not exceed half of"));

        config.wal.buffer_size = 64 * 1024;
        config.wal.flush_interval_ms = 5000;
        config.wal.upload_interval_ms = 8000;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("upload_interval_ms should be at least 2x flush_interval_ms"));

        config.wal.upload_interval_ms = 10000;
        config.object_storage.multipart_threshold = 1024 * 1024;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("multipart_threshold should be at least 5MB"));
    }

    #[test]
    fn test_validation_network_consistency() {
        let mut config = Config::default();

        config.network.quic_listen = "invalid-address".to_string();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a valid socket address"));

        config.network.quic_listen = "0.0.0.0:9092".to_string();
        config.network.rpc_listen = "0.0.0.0:9092".to_string();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot use the same port"));

        config.network.rpc_listen = "0.0.0.0:9093".to_string();
        config.network.max_connections = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_connections must be greater than 0"));

        config.network.max_connections = 1000;
        config.network.quic_config.max_idle_timeout_ms = 500;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_idle_timeout_ms should be at least 1000ms"));
    }

    #[test]
    fn test_validation_controller_consistency() {
        let mut config = Config::default();

        config.controller.endpoints = vec![];
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("controller.endpoints cannot be empty"));

        config.controller.endpoints = vec!["controller-1:9094".to_string()];
        config.controller.heartbeat_interval_ms = 2000;
        config.controller.election_timeout_ms = 3000;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("election_timeout_ms should be at least 2x heartbeat_interval_ms"));

        config.controller.election_timeout_ms = 5000;
        config.controller.raft.heartbeat_timeout_ms = 1500;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(
            "raft.heartbeat_timeout_ms must be greater than controller.heartbeat_interval_ms"
        ));

        config.controller.raft.heartbeat_timeout_ms = 3000;
        config.controller.bind_addr = "invalid-ip".to_string();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a valid IP address"));
    }

    #[test]
    fn test_validation_scaling_consistency() {
        let mut config = Config::default();

        config.scaling.traffic_migration_rate = 1.5;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("traffic_migration_rate must be between 0.0 and 1.0"));

        config.scaling.traffic_migration_rate = 0.1;
        config.controller.endpoints = vec!["controller-1:9094".to_string()];
        config.operations.upgrade_velocity = 5;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("upgrade_velocity cannot exceed number of controller endpoints"));

        config.operations.upgrade_velocity = 1;
        config.scaling.health_check_timeout_ms = 500;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("health_check_timeout_ms should be at least 1000ms"));
    }

    #[test]
    fn test_validation_etl_consistency() {
        let mut config = Config::default();
        config.etl.enabled = true;

        config.etl.instance_pool.max_pool_size = 10;
        config.etl.instance_pool.warmup_instances = 15;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("warmup_instances cannot exceed max_pool_size"));

        config.etl.instance_pool.warmup_instances = 5;
        config.etl.pipelines = vec![crate::config::EtlPipelineConfig {
            pipeline_id: "".to_string(),
            name: "test-pipeline".to_string(),
            description: None,
            enabled: true,
            stages: vec![],
            global_timeout_ms: 5000,
            max_retries: 3,
            error_handling: crate::config::ErrorHandlingStrategy::StopPipeline,
        }];

        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("pipeline_id cannot be empty"));
    }

    #[test]
    fn test_validation_timeout_consistency() {
        let mut config = Config::default();

        config.network.connection_timeout_ms = 500;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("connection_timeout_ms should be at least 1000ms"));

        config.network.connection_timeout_ms = 30000;
        config.operations.graceful_shutdown_timeout_ms = 3000;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("graceful_shutdown_timeout_ms should be at least 5000ms"));

        config.operations.graceful_shutdown_timeout_ms = 60000;
        config.scaling.rebalance_timeout_ms = 10000;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("rebalance_timeout_ms should be at least 30000ms"));
    }

    #[test]
    fn test_validation_performance_consistency() {
        let mut config = Config::default();

        config.wal.fsync_on_write = false;
        config.wal.flush_interval_ms = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(
            "flush_interval_ms must be greater than 0 when wal.fsync_on_write is false"
        ));

        config.wal.flush_interval_ms = 1000;
        config.object_storage.max_concurrent_uploads = 200;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_concurrent_uploads should not exceed 100"));

        config.object_storage.max_concurrent_uploads = 10;
        config.cache.write_cache_size_bytes = 60_000_000_000;
        config.cache.read_cache_size_bytes = 50_000_000_000;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("total cache size exceeds 100GB"));
    }

    #[test]
    fn test_validation_with_valid_complex_config() {
        let mut config = Config::default();

        config.controller.endpoints = vec![
            "controller-1:9094".to_string(),
            "controller-2:9094".to_string(),
            "controller-3:9094".to_string(),
        ];
        config.replication.min_in_sync_replicas = 2;
        config.controller.heartbeat_interval_ms = 1000;
        config.controller.election_timeout_ms = 5000;
        config.replication.heartbeat_timeout_ms = 2000;
        config.controller.raft.heartbeat_timeout_ms = 2500;
        config.replication.ack_timeout_ms = 5000;

        config.wal.capacity_bytes = 20 * 1024 * 1024 * 1024;
        config.wal.segment_size_bytes = 128 * 1024 * 1024;
        config.wal.buffer_size = 32 * 1024;
        config.wal.flush_interval_ms = 1000;
        config.wal.upload_interval_ms = 10000;
        config.object_storage.multipart_threshold = 100 * 1024 * 1024;

        config.network.quic_listen = "0.0.0.0:9092".to_string();
        config.network.rpc_listen = "0.0.0.0:9093".to_string();
        config.network.max_connections = 10000;
        config.network.connection_timeout_ms = 30000;

        config.scaling.traffic_migration_rate = 0.1;
        config.operations.upgrade_velocity = 2;
        config.scaling.health_check_timeout_ms = 30000;
        config.operations.graceful_shutdown_timeout_ms = 60000;
        config.scaling.rebalance_timeout_ms = 300000;

        let result = config.validate();
        assert!(
            result.is_ok(),
            "Complex valid configuration should pass validation: {:?}",
            result
        );
    }

    #[test]
    fn test_validation_etl_pipeline_priorities() {
        let mut config = Config::default();
        config.etl.enabled = true;

        config.etl.pipelines = vec![crate::config::EtlPipelineConfig {
            pipeline_id: "test-pipeline".to_string(),
            name: "Test Pipeline".to_string(),
            description: None,
            enabled: true,
            stages: vec![
                crate::config::EtlStage {
                    priority: 2,
                    modules: vec![crate::config::EtlModuleInstance {
                        module_id: "module-1".to_string(),
                        instance_config: crate::config::ModuleInstanceConfig {
                            memory_limit_bytes: 1024 * 1024,
                            execution_timeout_ms: 1000,
                            max_concurrent_instances: 1,
                            enable_caching: false,
                            cache_ttl_seconds: 0,
                            custom_config: serde_json::Value::Null,
                        },
                        topic_filters: vec![],
                        conditional_rules: vec![],
                    }],
                    parallel_execution: false,
                    stage_timeout_ms: None,
                    continue_on_error: false,
                },
                crate::config::EtlStage {
                    priority: 1,
                    modules: vec![crate::config::EtlModuleInstance {
                        module_id: "module-2".to_string(),
                        instance_config: crate::config::ModuleInstanceConfig {
                            memory_limit_bytes: 1024 * 1024,
                            execution_timeout_ms: 1000,
                            max_concurrent_instances: 1,
                            enable_caching: false,
                            cache_ttl_seconds: 0,
                            custom_config: serde_json::Value::Null,
                        },
                        topic_filters: vec![],
                        conditional_rules: vec![],
                    }],
                    parallel_execution: false,
                    stage_timeout_ms: None,
                    continue_on_error: false,
                },
            ],
            global_timeout_ms: 5000,
            max_retries: 3,
            error_handling: crate::config::ErrorHandlingStrategy::StopPipeline,
        }];

        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("stages must have increasing priority values"));
    }

    #[test]
    fn test_tls_config_rejects_placeholder_ocsp_url() {
        let mut tls_config = crate::config::security::TlsConfig::default();
        tls_config.enabled = true;
        tls_config.ocsp_url = Some("http://ocsp.example.com".to_string());

        let result = tls_config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("placeholder domain"));
    }

    #[test]
    fn test_tls_config_accepts_valid_ocsp_url() {
        let mut tls_config = crate::config::security::TlsConfig::default();
        tls_config.enabled = true;
        tls_config.ocsp_url = Some("https://ocsp.digicert.com".to_string());

        let result = tls_config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_config_accepts_no_ocsp_url() {
        let mut tls_config = crate::config::security::TlsConfig::default();
        tls_config.enabled = true;
        tls_config.ocsp_url = None;

        let result = tls_config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_object_storage_validation_s3_credentials_consistency() {
        let mut storage_config = ObjectStorageConfig {
            storage_type: StorageType::S3,
            bucket: "test-bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: "".to_string(),
            access_key: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
            secret_key: None, // Missing secret key
            multipart_threshold: 10 * 1024 * 1024,
            max_concurrent_uploads: 4,
        };

        let result = storage_config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("access_key and secret_key must both be set"));

        // Both set = valid
        storage_config.secret_key = Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string());
        let result = storage_config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_object_storage_validation_zero_values() {
        let mut storage_config = ObjectStorageConfig {
            storage_type: StorageType::Local {
                path: "/tmp/test".into(),
            },
            bucket: "test".to_string(),
            region: "us".to_string(),
            endpoint: "".to_string(),
            access_key: None,
            secret_key: None,
            multipart_threshold: 0, // Invalid
            max_concurrent_uploads: 4,
        };

        let result = storage_config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("multipart_threshold must be greater than 0"));

        storage_config.multipart_threshold = 10 * 1024 * 1024;
        storage_config.max_concurrent_uploads = 0; // Invalid
        let result = storage_config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_concurrent_uploads must be greater than 0"));
    }
}
