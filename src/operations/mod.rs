use crate::{Result, config::OperationsConfig, scaling::BrokerInfo};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

pub mod upgrade;
pub mod kubernetes;

pub use upgrade::*;
pub use kubernetes::*;

#[derive(Debug, Clone, PartialEq)]
pub enum UpgradeStatus {
    NotStarted,
    InProgress {
        started_at: Instant,
        current_broker: String,
        completed_brokers: Vec<String>,
        remaining_brokers: Vec<String>,
    },
    Completed {
        completed_at: Instant,
        upgraded_brokers: Vec<String>,
    },
    Failed {
        error: String,
        failed_at: Instant,
        failed_broker: String,
    },
    RolledBack {
        rolled_back_at: Instant,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct UpgradeRequest {
    pub operation_id: String,
    pub target_version: String,
    pub upgrade_velocity: usize, // brokers per minute
    pub health_check_timeout: Duration,
    pub rollback_on_failure: bool,
}

#[async_trait]
pub trait RollingUpgradeManager: Send + Sync {
    async fn start_upgrade(&self, request: UpgradeRequest) -> Result<String>;
    async fn get_upgrade_status(&self, operation_id: &str) -> Result<UpgradeStatus>;
    async fn pause_upgrade(&self, operation_id: &str) -> Result<()>;
    async fn resume_upgrade(&self, operation_id: &str) -> Result<()>;
    async fn rollback_upgrade(&self, operation_id: &str) -> Result<()>;
}

#[async_trait]
pub trait BrokerUpgradeOperations: Send + Sync {
    async fn drain_broker(&self, broker_id: &str) -> Result<()>;
    async fn upgrade_broker(&self, broker_id: &str, target_version: &str) -> Result<()>;
    async fn health_check_broker(&self, broker_id: &str) -> Result<bool>;
    async fn restore_broker_traffic(&self, broker_id: &str) -> Result<()>;
    async fn get_broker_version(&self, broker_id: &str) -> Result<String>;
}

#[derive(Debug, Clone)]
pub struct RuntimeConfigUpdate {
    pub section: String, // e.g., "wal", "cache", "replication"
    pub key: String,
    pub value: serde_json::Value,
    pub apply_to_brokers: Vec<String>, // empty means all brokers
}

#[async_trait]
pub trait RuntimeConfigManager: Send + Sync {
    async fn update_config(&self, update: RuntimeConfigUpdate) -> Result<String>;
    async fn get_config_update_status(&self, operation_id: &str) -> Result<ConfigUpdateStatus>;
    async fn validate_config_update(&self, update: &RuntimeConfigUpdate) -> Result<()>;
    async fn rollback_config_update(&self, operation_id: &str) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigUpdateStatus {
    NotStarted,
    InProgress {
        started_at: Instant,
        completed_brokers: Vec<String>,
        remaining_brokers: Vec<String>,
    },
    Completed {
        completed_at: Instant,
        applied_brokers: Vec<String>,
    },
    Failed {
        error: String,
        failed_at: Instant,
        failed_broker: String,
    },
}