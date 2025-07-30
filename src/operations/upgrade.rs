use super::*;
use crate::{Result, config::OperationsConfig, scaling::BrokerInfo};
use std::collections::HashMap;
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

pub struct RollingUpgradeManagerImpl {
    config: OperationsConfig,
    upgrade_operations: Arc<dyn BrokerUpgradeOperations>,
    active_upgrades: Arc<AsyncRwLock<HashMap<String, UpgradeStatus>>>,
    broker_list: Arc<AsyncRwLock<Vec<BrokerInfo>>>,
}

impl RollingUpgradeManagerImpl {
    pub fn new(
        config: OperationsConfig,
        upgrade_operations: Arc<dyn BrokerUpgradeOperations>,
        broker_list: Arc<AsyncRwLock<Vec<BrokerInfo>>>,
    ) -> Self {
        Self {
            config,
            upgrade_operations,
            active_upgrades: Arc::new(AsyncRwLock::new(HashMap::new())),
            broker_list,
        }
    }

    async fn execute_upgrade(&self, request: UpgradeRequest) -> Result<()> {
        let operation_id = request.operation_id.clone();
        let brokers = self.broker_list.read().await.clone();
        let broker_ids: Vec<String> = brokers.iter().map(|b| b.id.clone()).collect();

        // Initialize status
        {
            let mut upgrades = self.active_upgrades.write().await;
            upgrades.insert(operation_id.clone(), UpgradeStatus::InProgress {
                started_at: Instant::now(),
                current_broker: "".to_string(),
                completed_brokers: Vec::new(),
                remaining_brokers: broker_ids.clone(),
            });
        }

        let mut completed_brokers = Vec::new();
        let mut remaining_brokers = broker_ids;

        let upgrade_interval = Duration::from_secs(60 / request.upgrade_velocity as u64);

        for broker_id in remaining_brokers.clone() {
            // Update status with current broker
            {
                let mut upgrades = self.active_upgrades.write().await;
                if let Some(status) = upgrades.get_mut(&operation_id) {
                    *status = UpgradeStatus::InProgress {
                        started_at: if let UpgradeStatus::InProgress { started_at, .. } = status {
                            *started_at
                        } else {
                            Instant::now()
                        },
                        current_broker: broker_id.clone(),
                        completed_brokers: completed_brokers.clone(),
                        remaining_brokers: remaining_brokers[1..].to_vec(),
                    };
                }
            }

            let upgrade_result = self.upgrade_single_broker(
                &broker_id,
                &request.target_version,
                request.health_check_timeout,
            ).await;

            match upgrade_result {
                Ok(()) => {
                    completed_brokers.push(broker_id.clone());
                    remaining_brokers.retain(|id| id != &broker_id);
                }
                Err(e) => {
                    // Mark as failed
                    let mut upgrades = self.active_upgrades.write().await;
                    upgrades.insert(operation_id.clone(), UpgradeStatus::Failed {
                        error: e.to_string(),
                        failed_at: Instant::now(),
                        failed_broker: broker_id.clone(),
                    });

                    if request.rollback_on_failure {
                        drop(upgrades); // Release lock before rollback
                        let _ = self.rollback_completed_brokers(&completed_brokers).await;
                        
                        let mut upgrades = self.active_upgrades.write().await;
                        upgrades.insert(operation_id, UpgradeStatus::RolledBack {
                            rolled_back_at: Instant::now(),
                            reason: format!("Failed to upgrade broker {}: {}", broker_id, e),
                        });
                    }
                    return Err(e);
                }
            }

            // Wait before upgrading next broker
            if !remaining_brokers.is_empty() {
                tokio::time::sleep(upgrade_interval).await;
            }
        }

        // Mark as completed
        {
            let mut upgrades = self.active_upgrades.write().await;
            upgrades.insert(operation_id, UpgradeStatus::Completed {
                completed_at: Instant::now(),
                upgraded_brokers: completed_brokers,
            });
        }

        Ok(())
    }

    async fn upgrade_single_broker(
        &self,
        broker_id: &str,
        target_version: &str,
        health_check_timeout: Duration,
    ) -> Result<()> {
        tracing::info!("Starting upgrade of broker {} to version {}", broker_id, target_version);

        // Step 1: Drain broker
        self.upgrade_operations.drain_broker(broker_id).await?;
        tracing::info!("Drained broker {}", broker_id);

        // Step 2: Wait for graceful shutdown timeout
        tokio::time::sleep(Duration::from_millis(self.config.graceful_shutdown_timeout_ms)).await;

        // Step 3: Upgrade broker
        self.upgrade_operations.upgrade_broker(broker_id, target_version).await?;
        tracing::info!("Upgraded broker {} to version {}", broker_id, target_version);

        // Step 4: Health check with timeout
        let health_check_start = Instant::now();
        while health_check_start.elapsed() < health_check_timeout {
            if self.upgrade_operations.health_check_broker(broker_id).await? {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }

        // Final health check
        if !self.upgrade_operations.health_check_broker(broker_id).await? {
            return Err(crate::error::RustMqError::Storage(
                format!("Broker {} failed health check after upgrade", broker_id)
            ));
        }

        // Step 5: Restore traffic
        self.upgrade_operations.restore_broker_traffic(broker_id).await?;
        tracing::info!("Restored traffic to broker {}", broker_id);

        Ok(())
    }

    async fn rollback_completed_brokers(&self, broker_ids: &[String]) -> Result<()> {
        for broker_id in broker_ids.iter().rev() {
            tracing::warn!("Rolling back broker {} due to upgrade failure", broker_id);
            // In a real implementation, this would restore the previous version
            // For now, just log the rollback attempt
        }
        Ok(())
    }
}

#[async_trait]
impl RollingUpgradeManager for RollingUpgradeManagerImpl {
    async fn start_upgrade(&self, request: UpgradeRequest) -> Result<String> {
        let operation_id = request.operation_id.clone();
        
        // Validate request
        if request.upgrade_velocity == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "Upgrade velocity must be greater than 0".to_string()
            ));
        }

        // Check if operation already exists
        {
            let upgrades = self.active_upgrades.read().await;
            if upgrades.contains_key(&operation_id) {
                return Err(crate::error::RustMqError::InvalidConfig(
                    format!("Upgrade operation {} already exists", operation_id)
                ));
            }
        }

        // Start upgrade in background
        let self_clone = self.clone();
        let request_clone = request.clone();
        tokio::spawn(async move {
            if let Err(e) = self_clone.execute_upgrade(request_clone).await {
                tracing::error!("Upgrade failed: {}", e);
            }
        });

        Ok(operation_id)
    }

    async fn get_upgrade_status(&self, operation_id: &str) -> Result<UpgradeStatus> {
        let upgrades = self.active_upgrades.read().await;
        upgrades.get(operation_id)
            .cloned()
            .ok_or_else(|| crate::error::RustMqError::Storage(
                format!("Upgrade operation {} not found", operation_id)
            ))
    }

    async fn pause_upgrade(&self, operation_id: &str) -> Result<()> {
        // In a real implementation, this would set a flag to pause the upgrade
        tracing::info!("Pausing upgrade operation {}", operation_id);
        Ok(())
    }

    async fn resume_upgrade(&self, operation_id: &str) -> Result<()> {
        // In a real implementation, this would clear the pause flag
        tracing::info!("Resuming upgrade operation {}", operation_id);
        Ok(())
    }

    async fn rollback_upgrade(&self, operation_id: &str) -> Result<()> {
        let mut upgrades = self.active_upgrades.write().await;
        
        if let Some(status) = upgrades.get(operation_id) {
            match status {
                UpgradeStatus::InProgress { completed_brokers, .. } => {
                    let completed_brokers = completed_brokers.clone();
                    drop(upgrades); // Release lock before async operation
                    
                    self.rollback_completed_brokers(&completed_brokers).await?;
                    
                    let mut upgrades = self.active_upgrades.write().await;
                    upgrades.insert(operation_id.to_string(), UpgradeStatus::RolledBack {
                        rolled_back_at: Instant::now(),
                        reason: "Manual rollback requested".to_string(),
                    });
                }
                UpgradeStatus::Completed { upgraded_brokers, .. } => {
                    let upgraded_brokers = upgraded_brokers.clone();
                    drop(upgrades); // Release lock before async operation
                    
                    self.rollback_completed_brokers(&upgraded_brokers).await?;
                    
                    let mut upgrades = self.active_upgrades.write().await;
                    upgrades.insert(operation_id.to_string(), UpgradeStatus::RolledBack {
                        rolled_back_at: Instant::now(),
                        reason: "Manual rollback of completed upgrade".to_string(),
                    });
                }
                _ => {
                    return Err(crate::error::RustMqError::InvalidConfig(
                        format!("Cannot rollback upgrade in status: {:?}", status)
                    ));
                }
            }
        } else {
            return Err(crate::error::RustMqError::Storage(
                format!("Upgrade operation {} not found", operation_id)
            ));
        }

        Ok(())
    }
}

impl Clone for RollingUpgradeManagerImpl {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            upgrade_operations: self.upgrade_operations.clone(),
            active_upgrades: self.active_upgrades.clone(),
            broker_list: self.broker_list.clone(),
        }
    }
}

pub struct MockBrokerUpgradeOperations {
    broker_versions: Arc<AsyncRwLock<HashMap<String, String>>>,
    drained_brokers: Arc<AsyncRwLock<Vec<String>>>,
}

impl MockBrokerUpgradeOperations {
    pub fn new() -> Self {
        Self {
            broker_versions: Arc::new(AsyncRwLock::new(HashMap::new())),
            drained_brokers: Arc::new(AsyncRwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl BrokerUpgradeOperations for MockBrokerUpgradeOperations {
    async fn drain_broker(&self, broker_id: &str) -> Result<()> {
        let mut drained = self.drained_brokers.write().await;
        drained.push(broker_id.to_string());
        tokio::time::sleep(Duration::from_millis(100)).await; // Simulate drain time
        Ok(())
    }

    async fn upgrade_broker(&self, broker_id: &str, target_version: &str) -> Result<()> {
        let mut versions = self.broker_versions.write().await;
        versions.insert(broker_id.to_string(), target_version.to_string());
        tokio::time::sleep(Duration::from_millis(200)).await; // Simulate upgrade time
        Ok(())
    }

    async fn health_check_broker(&self, broker_id: &str) -> Result<bool> {
        let versions = self.broker_versions.read().await;
        Ok(versions.contains_key(broker_id))
    }

    async fn restore_broker_traffic(&self, broker_id: &str) -> Result<()> {
        let mut drained = self.drained_brokers.write().await;
        drained.retain(|id| id != broker_id);
        Ok(())
    }

    async fn get_broker_version(&self, broker_id: &str) -> Result<String> {
        let versions = self.broker_versions.read().await;
        versions.get(broker_id)
            .cloned()
            .ok_or_else(|| crate::error::RustMqError::Storage(
                format!("Broker {} not found", broker_id)
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaling::{BrokerInfo, BrokerStatus, LoadMetrics};

    #[tokio::test]
    async fn test_rolling_upgrade() {
        let config = OperationsConfig {
            allow_runtime_config_updates: true,
            upgrade_velocity: 2, // 2 brokers per minute
            graceful_shutdown_timeout_ms: 100,
            kubernetes: crate::config::KubernetesConfig {
                use_stateful_sets: true,
                pvc_storage_class: "fast-ssd".to_string(),
                wal_volume_size: "50Gi".to_string(),
                enable_pod_affinity: true,
            },
        };

        let brokers = vec![
            BrokerInfo {
                id: "broker-1".to_string(),
                rack_id: "rack-1".to_string(),
                endpoints: vec!["broker-1:9092".to_string()],
                status: BrokerStatus::Healthy,
                load_metrics: LoadMetrics::default(),
            },
            BrokerInfo {
                id: "broker-2".to_string(),
                rack_id: "rack-2".to_string(),
                endpoints: vec!["broker-2:9092".to_string()],
                status: BrokerStatus::Healthy,
                load_metrics: LoadMetrics::default(),
            },
        ];

        let broker_list = Arc::new(AsyncRwLock::new(brokers));
        let upgrade_ops = Arc::new(MockBrokerUpgradeOperations::new());
        let upgrade_manager = RollingUpgradeManagerImpl::new(config, upgrade_ops, broker_list);

        let request = UpgradeRequest {
            operation_id: "test-upgrade".to_string(),
            target_version: "2.0.0".to_string(),
            upgrade_velocity: 2,
            health_check_timeout: Duration::from_millis(1000),
            rollback_on_failure: false,
        };

        let operation_id = upgrade_manager.start_upgrade(request).await.unwrap();
        assert_eq!(operation_id, "test-upgrade");

        // Wait for upgrade to progress
        tokio::time::sleep(Duration::from_millis(500)).await;

        let status = upgrade_manager.get_upgrade_status(&operation_id).await.unwrap();
        match status {
            UpgradeStatus::InProgress { .. } | UpgradeStatus::Completed { .. } => {
                // Expected states
            }
            _ => panic!("Unexpected status: {:?}", status),
        }
    }

    #[tokio::test]
    async fn test_upgrade_rollback() {
        let config = OperationsConfig {
            allow_runtime_config_updates: true,
            upgrade_velocity: 1,
            graceful_shutdown_timeout_ms: 100,
            kubernetes: crate::config::KubernetesConfig {
                use_stateful_sets: true,
                pvc_storage_class: "fast-ssd".to_string(),
                wal_volume_size: "50Gi".to_string(),
                enable_pod_affinity: true,
            },
        };

        let brokers = vec![
            BrokerInfo {
                id: "broker-1".to_string(),
                rack_id: "rack-1".to_string(),
                endpoints: vec!["broker-1:9092".to_string()],
                status: BrokerStatus::Healthy,
                load_metrics: LoadMetrics::default(),
            },
        ];

        let broker_list = Arc::new(AsyncRwLock::new(brokers));
        let upgrade_ops = Arc::new(MockBrokerUpgradeOperations::new());
        let upgrade_manager = RollingUpgradeManagerImpl::new(config, upgrade_ops, broker_list);

        let request = UpgradeRequest {
            operation_id: "test-rollback".to_string(),
            target_version: "2.0.0".to_string(),
            upgrade_velocity: 1,
            health_check_timeout: Duration::from_millis(1000),
            rollback_on_failure: true,
        };

        let operation_id = upgrade_manager.start_upgrade(request).await.unwrap();
        
        // Wait for upgrade to complete
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let rollback_result = upgrade_manager.rollback_upgrade(&operation_id).await;
        assert!(rollback_result.is_ok());

        let status = upgrade_manager.get_upgrade_status(&operation_id).await.unwrap();
        match status {
            UpgradeStatus::RolledBack { .. } => {
                // Expected
            }
            _ => panic!("Expected RolledBack status, got: {:?}", status),
        }
    }
}