use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadScoreNormalization {
    pub max_network_bytes_sec: f64,
    pub max_partitions_per_broker: f64,
    pub max_message_rate: f64,
}

impl Default for LoadScoreNormalization {
    fn default() -> Self {
        Self {
            max_network_bytes_sec: 1_250_000_000.0,
            max_partitions_per_broker: 4000.0,
            max_message_rate: 500_000.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadScoreWeights {
    pub cpu: f64,
    pub memory: f64,
    pub disk: f64,
    pub network: f64,
    pub partition: f64,
    pub message_rate: f64,
}

impl Default for LoadScoreWeights {
    fn default() -> Self {
        Self {
            cpu: 1.0,
            memory: 1.0,
            disk: 0.8,
            network: 0.7,
            partition: 0.5,
            message_rate: 0.3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    pub max_concurrent_additions: usize,
    pub max_concurrent_decommissions: usize,
    pub rebalance_timeout_ms: u64,
    pub traffic_migration_rate: f64,
    pub health_check_timeout_ms: u64,

    #[serde(default)]
    pub auto_rebalance_enabled: bool,
    #[serde(default = "default_60")]
    pub auto_rebalance_interval_secs: u64,
    #[serde(default = "default_1_5")]
    pub imbalance_threshold_ratio: f64,
    #[serde(default = "default_5")]
    pub max_concurrent_moves_per_broker: usize,
    #[serde(default = "default_50mb")]
    pub rebalance_bandwidth_limit_bytes: u64,
    #[serde(default = "default_300")]
    pub rebalance_cooldown_secs: u64,
    #[serde(default = "default_10")]
    pub broker_heartbeat_interval_secs: u64,
    #[serde(default = "default_0_3")]
    pub ewma_alpha: f64,
    #[serde(default = "default_120")]
    pub unhealthy_broker_timeout_secs: u64,
    #[serde(default)]
    pub load_score_normalization: LoadScoreNormalization,
    #[serde(default)]
    pub load_score_weights: LoadScoreWeights,
    #[serde(default = "default_300")]
    pub graceful_shutdown_timeout_secs: u64,
}

fn default_60() -> u64 { 60 }
fn default_1_5() -> f64 { 1.5 }
fn default_5() -> usize { 5 }
fn default_50mb() -> u64 { 52428800 }
fn default_300() -> u64 { 300 }
fn default_10() -> u64 { 10 }
fn default_0_3() -> f64 { 0.3 }
fn default_120() -> u64 { 120 }

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
            auto_rebalance_enabled: true,
            auto_rebalance_interval_secs: 60,
            imbalance_threshold_ratio: 1.5,
            max_concurrent_moves_per_broker: 5,
            rebalance_bandwidth_limit_bytes: 52428800,
            rebalance_cooldown_secs: 300,
            broker_heartbeat_interval_secs: 10,
            ewma_alpha: 0.3,
            unhealthy_broker_timeout_secs: 120,
            load_score_normalization: LoadScoreNormalization::default(),
            load_score_weights: LoadScoreWeights::default(),
            graceful_shutdown_timeout_secs: 300,
        }
    }
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
