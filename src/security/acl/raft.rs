//! ACL Raft Consensus Integration
//!
//! Integrates ACL operations with RustMQ's Raft consensus mechanism to ensure
//! consistent ACL state across the controller cluster.

use super::{AclRule, AclStorage, VersionedAclRule, AclRuleFilter};
use crate::error::{Result, RustMqError};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use async_trait::async_trait;
use uuid::Uuid;
use std::collections::HashMap;

// ==================== Raft Operations Trait ====================

/// Abstraction for Raft consensus operations needed by ACL management.
///
/// This trait decouples the ACL subsystem from the controller implementation,
/// breaking the circular dependency between security and controller modules.
/// The controller provides an implementation of this trait.
#[async_trait]
pub trait RaftOperations: Send + Sync {
    /// Check if this node is currently the Raft leader
    async fn is_leader(&self) -> bool;

    /// Get the current Raft term
    async fn get_current_term(&self) -> u64;

    /// Append an ACL operation to the Raft log
    ///
    /// # Arguments
    /// * `operation_data` - Serialized ACL operation data
    ///
    /// # Returns
    /// * `Ok(())` if the operation was successfully appended
    /// * `Err` if the operation could not be appended
    async fn append_acl_operation(&self, operation_data: Vec<u8>) -> Result<()>;
}

/// ACL operation that can be applied through Raft consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AclRaftOperation {
    /// Create a new ACL rule
    CreateRule {
        rule: AclRule,
        created_by: String,
        description: String,
    },
    /// Update an existing ACL rule
    UpdateRule {
        rule_id: String,
        rule: AclRule,
        updated_by: String,
        description: String,
    },
    /// Delete an ACL rule
    DeleteRule {
        rule_id: String,
        deleted_by: String,
        description: String,
    },
    /// Batch operation for multiple ACL changes
    BatchOperation {
        operations: Vec<AclRaftOperation>,
        batch_id: String,
        description: String,
    },
}

/// Result of applying an ACL operation through Raft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AclRaftResult {
    /// Operation succeeded
    Success {
        operation_id: String,
        rule_id: Option<String>,
        version: u64,
    },
    /// Operation failed
    Failure {
        operation_id: String,
        error: String,
    },
    /// Batch operation result
    BatchResult {
        batch_id: String,
        results: Vec<AclRaftResult>,
    },
}

/// ACL state machine for Raft consensus
pub struct AclStateMachine {
    /// Underlying ACL storage
    storage: Arc<dyn AclStorage>,
    /// Last applied Raft index
    last_applied_index: Arc<AsyncRwLock<u64>>,
    /// Pending operations waiting for consensus
    pending_operations: Arc<AsyncRwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<Result<AclRaftResult>>>>>,
}

impl AclStateMachine {
    /// Create a new ACL state machine
    pub fn new(storage: Arc<dyn AclStorage>) -> Self {
        Self {
            storage,
            last_applied_index: Arc::new(AsyncRwLock::new(0)),
            pending_operations: Arc::new(AsyncRwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Apply an ACL operation through the state machine
    fn apply_operation(&self, operation: AclRaftOperation) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AclRaftResult>> + Send + '_>> {
        Box::pin(async move {
            self.apply_operation_inner(operation).await
        })
    }
    
    async fn apply_operation_inner(&self, operation: AclRaftOperation) -> Result<AclRaftResult> {
        match operation {
            AclRaftOperation::CreateRule { rule, created_by, description } => {
                let operation_id = Uuid::new_v4().to_string();
                match self.storage.create_rule(rule, &created_by, &description).await {
                    Ok(rule_id) => {
                        let version = self.storage.get_version().await?;
                        Ok(AclRaftResult::Success {
                            operation_id,
                            rule_id: Some(rule_id),
                            version,
                        })
                    }
                    Err(e) => Ok(AclRaftResult::Failure {
                        operation_id,
                        error: e.to_string(),
                    }),
                }
            }
            AclRaftOperation::UpdateRule { rule_id, rule, updated_by, description } => {
                let operation_id = Uuid::new_v4().to_string();
                match self.storage.update_rule(&rule_id, rule, &updated_by, &description).await {
                    Ok(_) => {
                        let version = self.storage.get_version().await?;
                        Ok(AclRaftResult::Success {
                            operation_id,
                            rule_id: Some(rule_id),
                            version,
                        })
                    }
                    Err(e) => Ok(AclRaftResult::Failure {
                        operation_id,
                        error: e.to_string(),
                    }),
                }
            }
            AclRaftOperation::DeleteRule { rule_id, deleted_by, description } => {
                let operation_id = Uuid::new_v4().to_string();
                match self.storage.delete_rule(&rule_id, &deleted_by, &description).await {
                    Ok(_) => {
                        let version = self.storage.get_version().await?;
                        Ok(AclRaftResult::Success {
                            operation_id,
                            rule_id: Some(rule_id),
                            version,
                        })
                    }
                    Err(e) => Ok(AclRaftResult::Failure {
                        operation_id,
                        error: e.to_string(),
                    }),
                }
            }
            AclRaftOperation::BatchOperation { operations, batch_id, description: _ } => {
                let mut results = Vec::new();
                for op in operations {
                    // Handle batch operations non-recursively
                    let result = match op {
                        AclRaftOperation::CreateRule { rule, created_by, description } => {
                            let operation_id = uuid::Uuid::new_v4().to_string();
                            match self.storage.create_rule(rule, &created_by, &description).await {
                                Ok(rule_id) => {
                                    let version = self.storage.get_version().await?;
                                    AclRaftResult::Success {
                                        operation_id,
                                        rule_id: Some(rule_id),
                                        version,
                                    }
                                }
                                Err(e) => AclRaftResult::Failure {
                                    operation_id,
                                    error: e.to_string(),
                                },
                            }
                        }
                        AclRaftOperation::UpdateRule { rule_id, rule, updated_by, description } => {
                            let operation_id = uuid::Uuid::new_v4().to_string();
                            match self.storage.update_rule(&rule_id, rule, &updated_by, &description).await {
                                Ok(_) => {
                                    let version = self.storage.get_version().await?;
                                    AclRaftResult::Success {
                                        operation_id,
                                        rule_id: Some(rule_id),
                                        version,
                                    }
                                }
                                Err(e) => AclRaftResult::Failure {
                                    operation_id,
                                    error: e.to_string(),
                                },
                            }
                        }
                        AclRaftOperation::DeleteRule { rule_id, deleted_by, description } => {
                            let operation_id = uuid::Uuid::new_v4().to_string();
                            match self.storage.delete_rule(&rule_id, &deleted_by, &description).await {
                                Ok(_) => {
                                    let version = self.storage.get_version().await?;
                                    AclRaftResult::Success {
                                        operation_id,
                                        rule_id: Some(rule_id),
                                        version,
                                    }
                                }
                                Err(e) => AclRaftResult::Failure {
                                    operation_id,
                                    error: e.to_string(),
                                },
                            }
                        }
                        AclRaftOperation::BatchOperation { .. } => {
                            // Nested batch operations not supported
                            AclRaftResult::Failure {
                                operation_id: uuid::Uuid::new_v4().to_string(),
                                error: "Nested batch operations not supported".to_string(),
                            }
                        }
                    };
                    results.push(result);
                }
                Ok(AclRaftResult::BatchResult { batch_id, results })
            }
        }
    }

    /// Get the last applied Raft index
    pub async fn get_last_applied_index(&self) -> u64 {
        *self.last_applied_index.read().await
    }

    /// Set the last applied Raft index
    pub async fn set_last_applied_index(&self, index: u64) {
        *self.last_applied_index.write().await = index;
    }
}

/// ACL manager with Raft consensus integration
pub struct RaftAclManager {
    /// Underlying ACL state machine
    state_machine: Arc<AclStateMachine>,
    /// Raft operations provider (dependency injection)
    raft_ops: Arc<dyn RaftOperations>,
    /// Node ID of this controller
    node_id: String,
}

impl RaftAclManager {
    /// Create a new Raft ACL manager
    ///
    /// # Arguments
    /// * `storage` - ACL storage backend
    /// * `raft_ops` - Raft operations provider (typically ControllerRaftOperations)
    /// * `node_id` - Node identifier
    pub fn new(
        storage: Arc<dyn AclStorage>,
        raft_ops: Arc<dyn RaftOperations>,
        node_id: String,
    ) -> Self {
        let state_machine = Arc::new(AclStateMachine::new(storage));

        Self {
            state_machine,
            raft_ops,
            node_id,
        }
    }

    /// Submit an ACL operation through Raft consensus
    async fn submit_operation(&self, operation: AclRaftOperation) -> Result<AclRaftResult> {
        // Check if this node is the leader
        if !self.raft_ops.is_leader().await {
            return Err(RustMqError::NotLeader("ACL operations must be submitted to the leader".to_string()));
        }

        let operation_id = Uuid::new_v4().to_string();

        // Create a channel to receive the result
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Store the pending operation
        {
            let mut pending = self.state_machine.pending_operations.write().await;
            pending.insert(operation_id.clone(), tx);
        }

        // Serialize the operation for the Raft log
        let operation_data = serde_json::to_vec(&operation)
            .map_err(|e| RustMqError::Serialization(Box::new(bincode::ErrorKind::Custom(e.to_string()))))?;

        // Submit ACL operation to Raft log through the abstraction
        self.raft_ops.append_acl_operation(operation_data).await?;

        // Wait for the operation to be applied
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(RustMqError::AclEvaluation("Operation was cancelled".to_string())),
        }
    }

    /// Apply a Raft log entry to the ACL state machine
    ///
    /// Deserializes and applies ACL operations from Raft log entry data.
    ///
    /// # Arguments
    /// * `entry_data` - Serialized ACL operation data from Raft log
    pub async fn apply_log_entry(&self, entry_data: &[u8]) -> Result<()> {
        // Deserialize the ACL operation
        let operation: AclRaftOperation = serde_json::from_slice(entry_data)
            .map_err(|e| RustMqError::Serialization(Box::new(bincode::ErrorKind::Custom(e.to_string()))))?;

        // Apply the operation through the state machine
        self.state_machine.apply_operation(operation).await?;

        Ok(())
    }

    /// Create a snapshot of the ACL state
    pub async fn create_snapshot(&self) -> Result<Vec<u8>> {
        let snapshot = self.state_machine.storage.create_snapshot().await?;
        let last_applied = self.state_machine.get_last_applied_index().await;

        let snapshot_data = AclSnapshotData {
            snapshot,
            last_applied_index: last_applied,
        };

        serde_json::to_vec(&snapshot_data)
            .map_err(|e| RustMqError::Config(format!("Failed to serialize snapshot: {}", e)))
    }

    /// Restore from a snapshot
    pub async fn restore_snapshot(&self, data: &[u8]) -> Result<()> {
        let snapshot_data: AclSnapshotData = serde_json::from_slice(data)
            .map_err(|e| RustMqError::Config(format!("Failed to deserialize snapshot: {}", e)))?;

        self.state_machine.storage.restore_snapshot(snapshot_data.snapshot).await?;
        self.state_machine.set_last_applied_index(snapshot_data.last_applied_index).await;

        Ok(())
    }

    /// Get ACL storage reference for read operations
    pub fn get_storage(&self) -> Arc<dyn AclStorage> {
        self.state_machine.storage.clone()
    }
}

/// ACL manager interface that routes operations through Raft
#[async_trait]
impl super::manager::AclManagerTrait for RaftAclManager {
    async fn create_acl_rule(&self, rule: AclRule, created_by: &str, description: &str) -> Result<String> {
        let operation = AclRaftOperation::CreateRule {
            rule,
            created_by: created_by.to_string(),
            description: description.to_string(),
        };

        match self.submit_operation(operation).await? {
            AclRaftResult::Success { rule_id: Some(rule_id), .. } => Ok(rule_id),
            AclRaftResult::Failure { error, .. } => Err(RustMqError::AclEvaluation(error)),
            _ => Err(RustMqError::AclEvaluation("Unexpected result type".to_string())),
        }
    }

    async fn update_acl_rule(&self, rule_id: &str, rule: AclRule, updated_by: &str, description: &str) -> Result<()> {
        let operation = AclRaftOperation::UpdateRule {
            rule_id: rule_id.to_string(),
            rule,
            updated_by: updated_by.to_string(),
            description: description.to_string(),
        };

        match self.submit_operation(operation).await? {
            AclRaftResult::Success { .. } => Ok(()),
            AclRaftResult::Failure { error, .. } => Err(RustMqError::AclEvaluation(error)),
            _ => Err(RustMqError::AclEvaluation("Unexpected result type".to_string())),
        }
    }

    async fn delete_acl_rule(&self, rule_id: &str, deleted_by: &str, description: &str) -> Result<()> {
        let operation = AclRaftOperation::DeleteRule {
            rule_id: rule_id.to_string(),
            deleted_by: deleted_by.to_string(),
            description: description.to_string(),
        };

        match self.submit_operation(operation).await? {
            AclRaftResult::Success { .. } => Ok(()),
            AclRaftResult::Failure { error, .. } => Err(RustMqError::AclEvaluation(error)),
            _ => Err(RustMqError::AclEvaluation("Unexpected result type".to_string())),
        }
    }

    async fn get_acl_rule(&self, rule_id: &str) -> Result<Option<VersionedAclRule>> {
        // Read operations can go directly to storage
        self.state_machine.storage.get_rule(rule_id).await
    }

    async fn list_acl_rules(&self, filter: AclRuleFilter) -> Result<Vec<VersionedAclRule>> {
        // Read operations can go directly to storage
        self.state_machine.storage.list_rules(filter).await
    }

    async fn get_rules_for_principal(&self, principal: &str) -> Result<Vec<VersionedAclRule>> {
        // Read operations can go directly to storage
        self.state_machine.storage.get_rules_for_principal(principal).await
    }

    async fn get_acl_version(&self) -> Result<u64> {
        self.state_machine.storage.get_version().await
    }

    async fn get_acls_since_version(&self, version: u64) -> Result<Vec<VersionedAclRule>> {
        self.state_machine.storage.get_rules_since_version(version).await
    }

    async fn validate_integrity(&self) -> Result<bool> {
        self.state_machine.storage.validate_integrity().await
    }
}

/// Snapshot data for ACL state
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AclSnapshotData {
    snapshot: super::storage::AclSnapshot,
    last_applied_index: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::acl::{ResourcePattern, ResourceType, Effect, AclOperation};
    use crate::security::acl::storage::{ObjectStorageAclStorage, AclStorageConfig};
    use crate::storage::cache::LruCache;
    use crate::storage::object_storage::LocalObjectStorage;
    use tempfile::TempDir;

    /// Mock implementation of RaftOperations for testing
    struct MockRaftOperations {
        is_leader: bool,
        current_term: u64,
    }

    impl MockRaftOperations {
        fn new() -> Self {
            Self {
                is_leader: true,
                current_term: 1,
            }
        }
    }

    #[async_trait]
    impl RaftOperations for MockRaftOperations {
        async fn is_leader(&self) -> bool {
            self.is_leader
        }

        async fn get_current_term(&self) -> u64 {
            self.current_term
        }

        async fn append_acl_operation(&self, _operation_data: Vec<u8>) -> Result<()> {
            // Mock implementation - just return Ok
            Ok(())
        }
    }

    async fn create_test_raft_manager() -> (RaftAclManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let object_storage = Arc::new(LocalObjectStorage::new(temp_dir.path().to_path_buf()).unwrap());
        let cache = Arc::new(LruCache::new(1024 * 1024));
        let config = AclStorageConfig::default();

        let storage = Arc::new(ObjectStorageAclStorage::new(object_storage, cache, config).await.unwrap());

        // Mock raft operations
        let raft_ops = Arc::new(MockRaftOperations::new());

        let manager = RaftAclManager::new(storage, raft_ops, "test-node".to_string());
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_acl_raft_operation_serialization() {
        let rule = AclRule::new(
            "test-rule".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        let operation = AclRaftOperation::CreateRule {
            rule: rule.clone(),
            created_by: "admin".to_string(),
            description: "Test rule".to_string(),
        };

        let serialized = serde_json::to_vec(&operation).unwrap();
        let deserialized: AclRaftOperation = serde_json::from_slice(&serialized).unwrap();

        match deserialized {
            AclRaftOperation::CreateRule { rule: deserialized_rule, created_by, description } => {
                assert_eq!(deserialized_rule.id, rule.id);
                assert_eq!(created_by, "admin");
                assert_eq!(description, "Test rule");
            }
            _ => panic!("Unexpected operation type"),
        }
    }

    #[tokio::test]
    #[ignore] // Requires full controller service setup
    async fn test_state_machine_apply_operation() {
        // This test would verify the state machine operation application
        // but requires a complete controller service implementation
        // let (_manager, _temp_dir) = create_test_raft_manager().await;
    }

    #[tokio::test]
    async fn test_batch_operation() {
        let rule1 = AclRule::new(
            "rule-1".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "topic-1".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        let rule2 = AclRule::new(
            "rule-2".to_string(),
            "user-456".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "topic-2".to_string(),
            },
            vec![AclOperation::Write],
            Effect::Deny,
        );

        let batch_operation = AclRaftOperation::BatchOperation {
            operations: vec![
                AclRaftOperation::CreateRule {
                    rule: rule1,
                    created_by: "admin".to_string(),
                    description: "Rule 1".to_string(),
                },
                AclRaftOperation::CreateRule {
                    rule: rule2,
                    created_by: "admin".to_string(),
                    description: "Rule 2".to_string(),
                },
            ],
            batch_id: "batch-1".to_string(),
            description: "Batch creation".to_string(),
        };

        let serialized = serde_json::to_vec(&batch_operation).unwrap();
        let deserialized: AclRaftOperation = serde_json::from_slice(&serialized).unwrap();

        match deserialized {
            AclRaftOperation::BatchOperation { operations, batch_id, .. } => {
                assert_eq!(operations.len(), 2);
                assert_eq!(batch_id, "batch-1");
            }
            _ => panic!("Unexpected operation type"),
        }
    }

    #[tokio::test]
    async fn test_snapshot_serialization() {
        use crate::security::acl::storage::AclSnapshot;
        use std::collections::HashMap;
        
        let snapshot = AclSnapshot {
            rules: vec![],
            version_counter: 42,
            timestamp: 1234567890,
            snapshot_id: "snap-1".to_string(),
            metadata: HashMap::new(),
        };

        let snapshot_data = AclSnapshotData {
            snapshot,
            last_applied_index: 100,
        };

        let serialized = serde_json::to_vec(&snapshot_data).unwrap();
        let deserialized: AclSnapshotData = serde_json::from_slice(&serialized).unwrap();

        assert_eq!(deserialized.snapshot.version_counter, 42);
        assert_eq!(deserialized.last_applied_index, 100);
    }
}