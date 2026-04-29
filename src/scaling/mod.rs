use crate::{Result, types::TopicPartition};
use crate::controller::service::PartitionAssignment;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::time::{Duration, Instant};
use chrono::{DateTime, Utc};

pub mod manager;
pub mod operations;

pub use manager::*;
pub use operations::*;

#[derive(Debug, Clone, PartialEq)]
pub enum ScalingOperation {
    AddBrokers {
        broker_ids: Vec<String>,
        rack_ids: Vec<String>,
    },
    RemoveBroker {
        broker_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScalingStatus {
    NotStarted,
    InProgress {
        started_at: Instant,
        progress: f64,
    },
    Completed {
        completed_at: Instant,
    },
    Failed {
        error: String,
        failed_at: Instant,
    },
}

#[derive(Debug, Clone)]
pub struct BrokerInfo {
    pub id: String,
    pub rack_id: String,
    pub endpoints: Vec<String>,
    pub status: BrokerStatus,
    pub load_metrics: LoadMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrokerStatus {
    Healthy,
    Draining,
    Unhealthy,
    Removed,
}

#[derive(Debug, Clone)]
pub struct LoadMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_usage: f64,
    pub network_tx_bytes_sec: u64,
    pub network_rx_bytes_sec: u64,
    pub partition_count: usize,
    pub leader_partition_count: usize,
    pub message_rate: u64,
    pub consumer_lag_total: u64,
    pub timestamp: DateTime<Utc>,
}

impl Default for LoadMetrics {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_usage: 0.0,
            network_tx_bytes_sec: 0,
            network_rx_bytes_sec: 0,
            partition_count: 0,
            leader_partition_count: 0,
            message_rate: 0,
            consumer_lag_total: 0,
            timestamp: Utc::now(),
        }
    }
}

#[async_trait]
pub trait ScalingManager: Send + Sync {
    async fn add_brokers(&self, broker_ids: Vec<String>, rack_ids: Vec<String>) -> Result<String>;
    async fn remove_broker(&self, broker_id: String) -> Result<String>;
    async fn get_scaling_status(&self, operation_id: &str) -> Result<ScalingStatus>;
    async fn list_brokers(&self) -> Result<Vec<BrokerInfo>>;
    async fn health_check_broker(&self, broker_id: &str) -> Result<bool>;
    async fn rebalance_partitions(&self) -> Result<()>;
}

#[async_trait]
pub trait PartitionRebalancer: Send + Sync {
    async fn calculate_rebalance_plan(
        &self,
        brokers: Vec<BrokerInfo>,
        assignments: HashMap<TopicPartition, PartitionAssignment>,
    ) -> Result<RebalancePlan>;
    async fn execute_rebalance(
        &self,
        plan: RebalancePlan,
        assignments: HashMap<TopicPartition, PartitionAssignment>,
    ) -> Result<()>;
    async fn get_rebalance_progress(&self, operation_id: &str) -> Result<f64>;
}

#[derive(Debug, Clone)]
pub struct RebalancePlan {
    pub operation_id: String,
    pub moves: Vec<PartitionMove>,
    pub estimated_duration: Duration,
}

#[derive(Debug, Clone)]
pub struct PartitionMove {
    pub topic_partition: TopicPartition,
    pub from_broker: String,
    pub to_broker: String,
    pub estimated_bytes: u64,
}
