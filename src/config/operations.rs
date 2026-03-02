use serde::{Deserialize, Serialize};

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
