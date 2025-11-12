//! Raft Operations Implementation for Controller
//!
//! Provides the implementation of RaftOperations trait for ControllerService.
//! This breaks the circular dependency between security and controller modules
//! by implementing the abstraction defined in the security module.

use crate::error::Result;
use crate::security::acl::RaftOperations;
use crate::controller::service::ControllerService;
use async_trait::async_trait;
use std::sync::Arc;

/// Controller-side implementation of RaftOperations trait
///
/// This struct wraps a ControllerService and provides the Raft operations
/// needed by the ACL subsystem, implementing the dependency inversion principle.
pub struct ControllerRaftOperations {
    controller: Arc<ControllerService>,
}

impl ControllerRaftOperations {
    /// Create a new ControllerRaftOperations wrapper
    ///
    /// # Arguments
    /// * `controller` - The controller service to wrap
    pub fn new(controller: Arc<ControllerService>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl RaftOperations for ControllerRaftOperations {
    async fn is_leader(&self) -> bool {
        self.controller.is_leader()
    }

    async fn get_current_term(&self) -> u64 {
        self.controller.get_current_term()
    }

    async fn append_acl_operation(&self, operation_data: Vec<u8>) -> Result<()> {
        // This method appends an ACL operation to the Raft log
        // The implementation should:
        // 1. Wrap the operation_data in a ClusterCommand::AclOperation variant
        // 2. Create a log entry with the current term
        // 3. Append it to the Raft log through the consensus mechanism
        //
        // For now, this is a placeholder that logs the operation
        tracing::debug!(
            "Appending ACL operation to Raft log (size: {} bytes)",
            operation_data.len()
        );

        // TODO: Implement actual Raft log appending once ClusterCommand::AclOperation is added
        // self.controller.append_log(LogEntry {
        //     term: self.get_current_term().await,
        //     command: ClusterCommand::AclOperation(operation_data),
        //     ...
        // }).await

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock ControllerService for testing
    struct MockControllerService {
        is_leader: bool,
        current_term: u64,
    }

    impl MockControllerService {
        fn new() -> Self {
            Self {
                is_leader: true,
                current_term: 42,
            }
        }
    }

    #[async_trait]
    impl RaftOperations for MockControllerService {
        async fn is_leader(&self) -> bool {
            self.is_leader
        }

        async fn get_current_term(&self) -> u64 {
            self.current_term
        }

        async fn append_acl_operation(&self, _operation_data: Vec<u8>) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_mock_raft_operations() {
        let mock = MockControllerService::new();

        assert!(mock.is_leader().await);
        assert_eq!(mock.get_current_term().await, 42);

        let operation_data = vec![1, 2, 3, 4];
        assert!(mock.append_acl_operation(operation_data).await.is_ok());
    }
}
