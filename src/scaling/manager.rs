use super::*;
use crate::{Result, config::ScalingConfig, controller::ControllerService};
use std::collections::HashMap;
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

pub struct ScalingManagerImpl {
    config: ScalingConfig,
    operations: Arc<AsyncRwLock<HashMap<String, ScalingOperation>>>,
    operation_status: Arc<AsyncRwLock<HashMap<String, ScalingStatus>>>,
    brokers: Arc<AsyncRwLock<HashMap<String, BrokerInfo>>>,
    rebalancer: Arc<dyn PartitionRebalancer>,
    controller: Option<Arc<ControllerService>>,
}

impl ScalingManagerImpl {
    pub fn new(
        config: ScalingConfig,
        rebalancer: Arc<dyn PartitionRebalancer>,
    ) -> Self {
        Self {
            config,
            operations: Arc::new(AsyncRwLock::new(HashMap::new())),
            operation_status: Arc::new(AsyncRwLock::new(HashMap::new())),
            brokers: Arc::new(AsyncRwLock::new(HashMap::new())),
            rebalancer,
            controller: None,
        }
    }

    pub fn with_controller(
        config: ScalingConfig,
        rebalancer: Arc<dyn PartitionRebalancer>,
        controller: Arc<ControllerService>,
    ) -> Self {
        Self {
            config,
            operations: Arc::new(AsyncRwLock::new(HashMap::new())),
            operation_status: Arc::new(AsyncRwLock::new(HashMap::new())),
            brokers: Arc::new(AsyncRwLock::new(HashMap::new())),
            rebalancer,
            controller: Some(controller),
        }
    }

    async fn validate_add_brokers(&self, broker_ids: &[String]) -> Result<()> {
        if broker_ids.len() > self.config.max_concurrent_additions {
            return Err(crate::error::RustMqError::InvalidConfig(
                format!("Cannot add more than {} brokers at once", 
                    self.config.max_concurrent_additions)
            ));
        }

        let brokers = self.brokers.read().await;
        for broker_id in broker_ids {
            if brokers.contains_key(broker_id) {
                return Err(crate::error::RustMqError::InvalidConfig(
                    format!("Broker {} already exists", broker_id)
                ));
            }
        }

        Ok(())
    }

    async fn execute_add_brokers_operation(
        &self,
        operation_id: String,
        broker_ids: Vec<String>,
        rack_ids: Vec<String>,
    ) -> Result<()> {
        let operation_status = self.operation_status.clone();
        let brokers = self.brokers.clone();
        let config = self.config.clone();
        let rebalancer = self.rebalancer.clone();

        tokio::spawn(async move {
            // Update status to in progress
            {
                let mut status_map = operation_status.write().await;
                status_map.insert(operation_id.clone(), ScalingStatus::InProgress {
                    started_at: Instant::now(),
                    progress: 0.0,
                });
            }

            let result = Self::do_add_brokers(
                broker_ids,
                rack_ids,
                brokers.clone(),
                config,
                rebalancer,
                operation_status.clone(),
                operation_id.clone(),
            ).await;

            // Update final status
            let mut status_map = operation_status.write().await;
            match result {
                Ok(()) => {
                    status_map.insert(operation_id, ScalingStatus::Completed {
                        completed_at: Instant::now(),
                    });
                }
                Err(e) => {
                    status_map.insert(operation_id, ScalingStatus::Failed {
                        error: e.to_string(),
                        failed_at: Instant::now(),
                    });
                }
            }
        });

        Ok(())
    }

    async fn do_add_brokers(
        broker_ids: Vec<String>,
        rack_ids: Vec<String>,
        brokers: Arc<AsyncRwLock<HashMap<String, BrokerInfo>>>,
        config: ScalingConfig,
        rebalancer: Arc<dyn PartitionRebalancer>,
        operation_status: Arc<AsyncRwLock<HashMap<String, ScalingStatus>>>,
        operation_id: String,
    ) -> Result<()> {
        // Step 1: Add brokers to cluster (25% progress)
        for (i, (broker_id, rack_id)) in broker_ids.iter().zip(rack_ids.iter()).enumerate() {
            let broker_info = BrokerInfo {
                id: broker_id.clone(),
                rack_id: rack_id.clone(),
                endpoints: vec![format!("{}:9092", broker_id)],
                status: BrokerStatus::Healthy,
                load_metrics: LoadMetrics::default(),
            };

            brokers.write().await.insert(broker_id.clone(), broker_info);

            let progress = 0.25 * (i + 1) as f64 / broker_ids.len() as f64;
            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = progress;
                }
            }
        }

        // Step 2: Health checks (50% progress)
        for (i, _broker_id) in broker_ids.iter().enumerate() {
            let start_time = Instant::now();
            let timeout = Duration::from_millis(config.health_check_timeout_ms);
            
            while start_time.elapsed() < timeout {
                // Simulate health check
                tokio::time::sleep(Duration::from_millis(100)).await;
                break; // For testing, assume health check passes
            }

            let progress = 0.25 + 0.25 * (i + 1) as f64 / broker_ids.len() as f64;
            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = progress;
                }
            }
        }

        // Step 3: Calculate rebalance plan (75% progress)
        let all_brokers: Vec<BrokerInfo> = brokers.read().await.values().cloned().collect();
        let rebalance_plan = rebalancer.calculate_rebalance_plan(all_brokers).await?;

        {
            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = 0.75;
                }
            }
        }

        // Step 4: Execute rebalance (100% progress)
        rebalancer.execute_rebalance(rebalance_plan).await?;

        {
            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = 1.0;
                }
            }
        }

        Ok(())
    }

    async fn execute_remove_broker_operation(
        &self,
        operation_id: String,
        broker_id: String,
        decommission_slot: Option<crate::controller::SlotAcquisitionResult>,
    ) -> Result<()> {
        let operation_status = self.operation_status.clone();
        let brokers = self.brokers.clone();
        let rebalancer = self.rebalancer.clone();
        let controller = self.controller.clone();

        tokio::spawn(async move {
            // Update status to in progress
            {
                let mut status_map = operation_status.write().await;
                status_map.insert(operation_id.clone(), ScalingStatus::InProgress {
                    started_at: Instant::now(),
                    progress: 0.0,
                });
            }

            let result = Self::do_remove_broker(
                broker_id.clone(),
                brokers.clone(),
                rebalancer,
                operation_status.clone(),
                operation_id.clone(),
            ).await;

            // Release decommission slot if we have one
            if let (Some(controller), Some(_slot)) = (controller, decommission_slot) {
                if let Err(e) = controller.release_decommission_slot(&operation_id).await {
                    tracing::error!("Failed to release decommission slot for operation {}: {}", operation_id, e);
                }
            }

            // Update final status
            let mut status_map = operation_status.write().await;
            match result {
                Ok(()) => {
                    status_map.insert(operation_id, ScalingStatus::Completed {
                        completed_at: Instant::now(),
                    });
                }
                Err(e) => {
                    status_map.insert(operation_id, ScalingStatus::Failed {
                        error: e.to_string(),
                        failed_at: Instant::now(),
                    });
                }
            }
        });

        Ok(())
    }

    async fn do_remove_broker(
        broker_id: String,
        brokers: Arc<AsyncRwLock<HashMap<String, BrokerInfo>>>,
        rebalancer: Arc<dyn PartitionRebalancer>,
        operation_status: Arc<AsyncRwLock<HashMap<String, ScalingStatus>>>,
        operation_id: String,
    ) -> Result<()> {
        // Step 1: Mark broker as draining (20% progress)
        {
            let mut brokers_map = brokers.write().await;
            if let Some(broker) = brokers_map.get_mut(&broker_id) {
                broker.status = BrokerStatus::Draining;
            } else {
                return Err(crate::error::RustMqError::Storage(
                    format!("Broker {} not found", broker_id)
                ));
            }

            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = 0.2;
                }
            }
        }

        // Step 2: Calculate rebalance plan to move partitions away (40% progress)
        let all_brokers: Vec<BrokerInfo> = brokers.read().await.values().cloned().collect();
        let rebalance_plan = rebalancer.calculate_rebalance_plan(all_brokers).await?;

        {
            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = 0.4;
                }
            }
        }

        // Step 3: Execute rebalance (80% progress)
        rebalancer.execute_rebalance(rebalance_plan).await?;

        {
            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = 0.8;
                }
            }
        }

        // Step 4: Remove broker from cluster (100% progress)
        {
            let mut brokers_map = brokers.write().await;
            if let Some(mut broker) = brokers_map.remove(&broker_id) {
                broker.status = BrokerStatus::Removed;
            }

            let mut status_map = operation_status.write().await;
            if let Some(status) = status_map.get_mut(&operation_id) {
                if let ScalingStatus::InProgress { started_at: _, progress: ref mut p } = status {
                    *p = 1.0;
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ScalingManager for ScalingManagerImpl {
    async fn add_brokers(&self, broker_ids: Vec<String>, rack_ids: Vec<String>) -> Result<String> {
        if broker_ids.len() != rack_ids.len() {
            return Err(crate::error::RustMqError::InvalidConfig(
                "Number of broker IDs must match number of rack IDs".to_string()
            ));
        }

        self.validate_add_brokers(&broker_ids).await?;

        let operation_id = Uuid::new_v4().to_string();
        let operation = ScalingOperation::AddBrokers { broker_ids: broker_ids.clone(), rack_ids: rack_ids.clone() };

        {
            let mut operations = self.operations.write().await;
            operations.insert(operation_id.clone(), operation);
        }

        {
            let mut status_map = self.operation_status.write().await;
            status_map.insert(operation_id.clone(), ScalingStatus::NotStarted);
        }

        self.execute_add_brokers_operation(operation_id.clone(), broker_ids, rack_ids).await?;

        Ok(operation_id)
    }

    async fn remove_broker(&self, broker_id: String) -> Result<String> {
        let brokers = self.brokers.read().await;
        if !brokers.contains_key(&broker_id) {
            return Err(crate::error::RustMqError::InvalidConfig(
                format!("Broker {} does not exist", broker_id)
            ));
        }
        drop(brokers); // Release the lock early

        // If controller is available, acquire decommission slot first
        let decommission_slot = if let Some(controller) = &self.controller {
            Some(controller.acquire_decommission_slot(
                broker_id.clone(),
                "scaling-manager".to_string(),
            ).await?)
        } else {
            tracing::warn!(
                "No controller available - proceeding with broker removal without slot management. \
                This may result in unsafe concurrent decommissions."
            );
            None
        };

        let operation_id = if let Some(ref slot) = decommission_slot {
            slot.operation_id.clone()
        } else {
            Uuid::new_v4().to_string()
        };

        let operation = ScalingOperation::RemoveBroker { broker_id: broker_id.clone() };

        {
            let mut operations = self.operations.write().await;
            operations.insert(operation_id.clone(), operation);
        }

        {
            let mut status_map = self.operation_status.write().await;
            status_map.insert(operation_id.clone(), ScalingStatus::NotStarted);
        }

        // Pass the decommission slot info to the operation
        self.execute_remove_broker_operation(
            operation_id.clone(), 
            broker_id,
            decommission_slot
        ).await?;

        Ok(operation_id)
    }

    async fn get_scaling_status(&self, operation_id: &str) -> Result<ScalingStatus> {
        let status_map = self.operation_status.read().await;
        status_map.get(operation_id)
            .cloned()
            .ok_or_else(|| crate::error::RustMqError::Storage(
                format!("Operation {} not found", operation_id)
            ))
    }

    async fn list_brokers(&self) -> Result<Vec<BrokerInfo>> {
        let brokers = self.brokers.read().await;
        Ok(brokers.values().cloned().collect())
    }

    async fn health_check_broker(&self, broker_id: &str) -> Result<bool> {
        let brokers = self.brokers.read().await;
        if let Some(broker) = brokers.get(broker_id) {
            Ok(matches!(broker.status, BrokerStatus::Healthy))
        } else {
            Ok(false)
        }
    }

    async fn rebalance_partitions(&self) -> Result<()> {
        let all_brokers: Vec<BrokerInfo> = self.brokers.read().await.values().cloned().collect();
        let rebalance_plan = self.rebalancer.calculate_rebalance_plan(all_brokers).await?;
        self.rebalancer.execute_rebalance(rebalance_plan).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaling::operations::MockPartitionRebalancer;

    #[tokio::test]
    async fn test_add_brokers() {
        let config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let rebalancer = Arc::new(MockPartitionRebalancer::new());
        let scaling_manager = ScalingManagerImpl::new(config, rebalancer);

        let broker_ids = vec!["broker-1".to_string(), "broker-2".to_string()];
        let rack_ids = vec!["rack-1".to_string(), "rack-2".to_string()];

        let operation_id = scaling_manager.add_brokers(broker_ids, rack_ids).await.unwrap();
        assert!(!operation_id.is_empty());

        // Wait for operation to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        let status = scaling_manager.get_scaling_status(&operation_id).await.unwrap();
        match status {
            ScalingStatus::InProgress { .. } | ScalingStatus::Completed { .. } => {
                // Operation is in progress or completed
            }
            _ => panic!("Unexpected status: {:?}", status),
        }

        let brokers = scaling_manager.list_brokers().await.unwrap();
        assert_eq!(brokers.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_broker() {
        let config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let rebalancer = Arc::new(MockPartitionRebalancer::new());
        let scaling_manager = ScalingManagerImpl::new(config, rebalancer);

        // First add a broker
        let broker_ids = vec!["broker-1".to_string()];
        let rack_ids = vec!["rack-1".to_string()];
        scaling_manager.add_brokers(broker_ids, rack_ids).await.unwrap();

        // Wait for add operation to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Now remove the broker
        let operation_id = scaling_manager.remove_broker("broker-1".to_string()).await.unwrap();
        assert!(!operation_id.is_empty());

        // Wait for operation to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        let status = scaling_manager.get_scaling_status(&operation_id).await.unwrap();
        match status {
            ScalingStatus::InProgress { .. } | ScalingStatus::Completed { .. } => {
                // Operation is in progress or completed
            }
            _ => panic!("Unexpected status: {:?}", status),
        }
    }

    #[tokio::test]
    async fn test_concurrent_decommission_prevention() {
        let config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1, // Only allow 1 concurrent decommission
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        // Create controller with the same config
        let controller = Arc::new(crate::controller::ControllerService::new(config.clone()));
        let rebalancer = Arc::new(MockPartitionRebalancer::new());
        let scaling_manager = ScalingManagerImpl::with_controller(config, rebalancer, controller);

        // Add two brokers first
        let broker_ids = vec!["broker-1".to_string(), "broker-2".to_string()];
        let rack_ids = vec!["rack-1".to_string(), "rack-2".to_string()];
        scaling_manager.add_brokers(broker_ids, rack_ids).await.unwrap();

        // Wait for add operation to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to remove both brokers simultaneously - should enforce safety constraint
        let remove1_future = scaling_manager.remove_broker("broker-1".to_string());
        let remove2_future = scaling_manager.remove_broker("broker-2".to_string());

        let (result1, result2) = tokio::join!(remove1_future, remove2_future);

        // One should succeed, one should fail due to slot exhaustion
        let success_count = [result1.is_ok(), result2.is_ok()].iter().filter(|&&x| x).count();
        let failure_count = [result1.is_err(), result2.is_err()].iter().filter(|&&x| x).count();

        assert_eq!(success_count, 1, "Exactly one decommission should succeed");
        assert_eq!(failure_count, 1, "Exactly one decommission should fail due to safety constraint");

        // Check that the failure is due to resource exhaustion (slot limit)
        let error_msg = if result1.is_err() {
            result1.unwrap_err().to_string()
        } else {
            result2.unwrap_err().to_string()
        };
        
        assert!(
            error_msg.contains("Maximum concurrent decommissions") || 
            error_msg.contains("Resource exhausted"),
            "Error should indicate decommission limit reached: {}", error_msg
        );
    }
}