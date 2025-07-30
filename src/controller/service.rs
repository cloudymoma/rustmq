use crate::{Result, config::ScalingConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock as AsyncRwLock, Semaphore};
use tokio::time::{Duration, Instant};
use uuid::Uuid;

/// Controller service that manages cluster-wide coordination including decommissioning slots
pub struct ControllerService {
    /// Decommissioning slot manager to prevent mass decommissions
    decommission_manager: Arc<DecommissionSlotManager>,
    /// Scaling configuration
    scaling_config: Arc<AsyncRwLock<ScalingConfig>>,
}

/// Manages decommissioning slots to enforce safety constraints
pub struct DecommissionSlotManager {
    /// Semaphore controlling concurrent decommissions
    decommission_slots: Arc<Semaphore>,
    /// Active decommission operations
    active_decommissions: Arc<AsyncRwLock<HashMap<String, DecommissionSlot>>>,
    /// Maximum allowed concurrent decommissions
    max_concurrent: usize,
}

#[derive(Debug, Clone)]
pub struct DecommissionSlot {
    pub operation_id: String,
    pub broker_id: String,
    pub acquired_at: Instant,
    pub expires_at: Instant,
    pub requester: String, // Admin tool instance or user ID
}

#[derive(Debug, Clone)]
pub struct SlotAcquisitionResult {
    pub operation_id: String,
    pub slot_token: String,
    pub expires_at: Instant,
}

impl ControllerService {
    pub fn new(scaling_config: ScalingConfig) -> Self {
        let decommission_manager = Arc::new(
            DecommissionSlotManager::new(scaling_config.max_concurrent_decommissions)
        );
        
        // Start background cleanup task for expired decommission slots
        let cleanup_manager = decommission_manager.clone();
        tokio::spawn(async move {
            cleanup_manager.start_cleanup_task().await;
        });
        
        Self {
            decommission_manager,
            scaling_config: Arc::new(AsyncRwLock::new(scaling_config)),
        }
    }

    /// Acquire a decommissioning slot for a broker
    pub async fn acquire_decommission_slot(
        &self,
        broker_id: String,
        requester: String,
    ) -> Result<SlotAcquisitionResult> {
        self.decommission_manager
            .acquire_slot(broker_id, requester)
            .await
    }

    /// Release a decommissioning slot
    pub async fn release_decommission_slot(&self, operation_id: &str) -> Result<()> {
        self.decommission_manager.release_slot(operation_id).await
    }

    /// Get status of active decommissions
    pub async fn get_decommission_status(&self) -> Result<Vec<DecommissionSlot>> {
        self.decommission_manager.get_active_decommissions().await
    }

    /// Update scaling configuration at runtime
    pub async fn update_scaling_config(&self, new_config: ScalingConfig) -> Result<()> {
        // Validate the new configuration
        if new_config.max_concurrent_decommissions == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "max_concurrent_decommissions must be greater than 0".to_string(),
            ));
        }

        if new_config.max_concurrent_decommissions > 10 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "max_concurrent_decommissions should not exceed 10 for safety".to_string(),
            ));
        }

        // Update the decommission manager with new limits
        self.decommission_manager
            .update_max_concurrent(new_config.max_concurrent_decommissions)
            .await?;

        // Update the stored configuration
        let mut config = self.scaling_config.write().await;
        *config = new_config;

        Ok(())
    }
}

impl DecommissionSlotManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            decommission_slots: Arc::new(Semaphore::new(max_concurrent)),
            active_decommissions: Arc::new(AsyncRwLock::new(HashMap::new())),
            max_concurrent,
        }
    }

    /// Acquire a decommissioning slot for a broker
    pub async fn acquire_slot(
        &self,
        broker_id: String,
        requester: String,
    ) -> Result<SlotAcquisitionResult> {
        // Check if this broker is already being decommissioned
        {
            let active = self.active_decommissions.read().await;
            if active.values().any(|slot| slot.broker_id == broker_id) {
                return Err(crate::error::RustMqError::InvalidOperation(
                    format!("Broker {} is already being decommissioned", broker_id),
                ));
            }
        }

        // Try to acquire a semaphore permit
        let permit = self.decommission_slots
            .try_acquire()
            .map_err(|_| crate::error::RustMqError::ResourceExhausted(
                format!(
                    "Maximum concurrent decommissions ({}) reached. Please wait for ongoing operations to complete.",
                    self.max_concurrent
                )
            ))?;

        // Generate operation ID and slot token
        let operation_id = Uuid::new_v4().to_string();
        let slot_token = Uuid::new_v4().to_string();
        let acquired_at = Instant::now();
        let expires_at = acquired_at + Duration::from_secs(3600); // 1 hour timeout

        // Create slot record
        let slot = DecommissionSlot {
            operation_id: operation_id.clone(),
            broker_id: broker_id.clone(),
            acquired_at,
            expires_at,
            requester: requester.clone(),
        };

        // Store the active decommission operation
        {
            let mut active = self.active_decommissions.write().await;
            active.insert(operation_id.clone(), slot);
        }

        // Forget the permit (it will be released when slot is released)
        permit.forget();

        tracing::info!(
            "Decommission slot acquired for broker {} by {} (operation: {})",
            broker_id,
            requester,
            operation_id
        );

        Ok(SlotAcquisitionResult {
            operation_id,
            slot_token,
            expires_at,
        })
    }

    /// Release a decommissioning slot
    pub async fn release_slot(&self, operation_id: &str) -> Result<()> {
        let slot = {
            let mut active = self.active_decommissions.write().await;
            active.remove(operation_id)
                .ok_or_else(|| crate::error::RustMqError::NotFound(
                    format!("Decommission operation {} not found", operation_id)
                ))?
        };

        // Release the semaphore permit
        self.decommission_slots.add_permits(1);

        tracing::info!(
            "Decommission slot released for broker {} (operation: {})",
            slot.broker_id,
            operation_id
        );

        Ok(())
    }

    /// Get all active decommission operations
    pub async fn get_active_decommissions(&self) -> Result<Vec<DecommissionSlot>> {
        let active = self.active_decommissions.read().await;
        Ok(active.values().cloned().collect())
    }

    /// Update the maximum concurrent decommissions limit
    pub async fn update_max_concurrent(&self, new_max: usize) -> Result<()> {
        let current_active = {
            let active = self.active_decommissions.read().await;
            active.len()
        };

        if current_active > new_max {
            return Err(crate::error::RustMqError::InvalidOperation(
                format!(
                    "Cannot reduce limit to {} while {} decommissions are active",
                    new_max, current_active
                )
            ));
        }

        // Calculate the difference and adjust semaphore permits
        let current_available = self.decommission_slots.available_permits();
        let current_max = current_available + current_active;
        
        if new_max > current_max {
            // Increase permits
            self.decommission_slots.add_permits(new_max - current_max);
        } else if new_max < current_max {
            // Decrease permits - acquire the difference
            let permits_to_remove = current_max - new_max;
            for _ in 0..permits_to_remove {
                self.decommission_slots
                    .try_acquire()
                    .map_err(|_| crate::error::RustMqError::InvalidOperation(
                        "Cannot reduce decommission limit while operations are active".to_string()
                    ))?
                    .forget();
            }
        }

        tracing::info!(
            "Updated max concurrent decommissions from {} to {}",
            current_max,
            new_max
        );

        Ok(())
    }

    /// Start background task to clean up expired decommission slots
    pub async fn start_cleanup_task(&self) {
        let active_decommissions = self.active_decommissions.clone();
        let decommission_slots = self.decommission_slots.clone();

        let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute
        
        loop {
            interval.tick().await;
            
            let now = Instant::now();
            let mut expired_slots = Vec::new();
            
            // Find expired slots
            {
                let active = active_decommissions.read().await;
                for (operation_id, slot) in active.iter() {
                    if now > slot.expires_at {
                        expired_slots.push(operation_id.clone());
                    }
                }
            }
            
            // Remove expired slots and release semaphore permits
            if !expired_slots.is_empty() {
                let mut active = active_decommissions.write().await;
                for operation_id in &expired_slots {
                    if let Some(slot) = active.remove(operation_id) {
                        // Release the semaphore permit
                        decommission_slots.add_permits(1);
                        
                        tracing::warn!(
                            "Auto-expired decommission slot for broker {} (operation: {}, expired after 1 hour)",
                            slot.broker_id,
                            operation_id
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_decommission_slot_acquisition() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire first slot
        let result1 = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result1.operation_id.is_empty());

        // Acquire second slot
        let result2 = manager
            .acquire_slot("broker-2".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result2.operation_id.is_empty());

        // Third slot should fail (limit reached)
        let result3 = manager
            .acquire_slot("broker-3".to_string(), "admin-1".to_string())
            .await;
        assert!(result3.is_err());

        // Release first slot
        manager.release_slot(&result1.operation_id).await.unwrap();

        // Now third slot should succeed
        let result3 = manager
            .acquire_slot("broker-3".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result3.operation_id.is_empty());
    }

    #[tokio::test]
    async fn test_duplicate_broker_decommission_prevention() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire slot for broker-1
        let _result1 = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Try to acquire another slot for the same broker - should fail
        let result2 = manager
            .acquire_slot("broker-1".to_string(), "admin-2".to_string())
            .await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_max_concurrent_update() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire one slot
        let result1 = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Increase limit - should succeed
        manager.update_max_concurrent(3).await.unwrap();

        // Acquire two more slots (total 3 active)
        let _result2 = manager
            .acquire_slot("broker-2".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        let _result3 = manager
            .acquire_slot("broker-3".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Try to reduce limit to 2 while 3 are active - should fail
        let update_result = manager.update_max_concurrent(2).await;
        assert!(update_result.is_err());

        // Release one slot
        manager.release_slot(&result1.operation_id).await.unwrap();

        // Now reducing to 2 should succeed
        manager.update_max_concurrent(2).await.unwrap();
    }

    #[tokio::test]
    async fn test_controller_service_integration() {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let controller = ControllerService::new(scaling_config);

        // Acquire slot
        let result = controller
            .acquire_decommission_slot("broker-1".to_string(), "admin-tool".to_string())
            .await
            .unwrap();

        // Check status
        let status = controller.get_decommission_status().await.unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].broker_id, "broker-1");

        // Release slot
        controller.release_decommission_slot(&result.operation_id).await.unwrap();

        // Check status again
        let status = controller.get_decommission_status().await.unwrap();
        assert_eq!(status.len(), 0);
    }

    #[tokio::test]
    async fn test_decommission_slot_expiration() {
        let manager = DecommissionSlotManager::new(2);

        // Acquire slot
        let result = manager
            .acquire_slot("broker-1".to_string(), "admin-1".to_string())
            .await
            .unwrap();

        // Verify slot exists
        let status = manager.get_active_decommissions().await.unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].broker_id, "broker-1");

        // Simulate expired slot by manually removing and releasing
        {
            let mut active = manager.active_decommissions.write().await;
            active.remove(&result.operation_id);
            manager.decommission_slots.add_permits(1);
        }

        // Verify slot is cleaned up
        let status = manager.get_active_decommissions().await.unwrap();
        assert_eq!(status.len(), 0);

        // Verify we can acquire new slots after cleanup
        let result2 = manager
            .acquire_slot("broker-2".to_string(), "admin-1".to_string())
            .await
            .unwrap();
        assert!(!result2.operation_id.is_empty());
    }

    #[tokio::test]
    async fn test_decommission_slot_timeout_integration() {
        use tokio::time::{timeout, Duration as TokioDuration};
        
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };

        let controller = ControllerService::new(scaling_config);

        // Acquire slot
        let result = controller
            .acquire_decommission_slot("broker-1".to_string(), "admin-tool".to_string())
            .await
            .unwrap();

        // Verify slot exists
        let status = controller.get_decommission_status().await.unwrap();
        assert_eq!(status.len(), 1);

        // Test that the background cleanup task exists by ensuring
        // the controller maintains state correctly over time
        timeout(TokioDuration::from_millis(100), async {
            tokio::time::sleep(TokioDuration::from_millis(50)).await;
            let status = controller.get_decommission_status().await.unwrap();
            assert_eq!(status.len(), 1); // Still there after short time
        })
        .await
        .unwrap();
    }
}