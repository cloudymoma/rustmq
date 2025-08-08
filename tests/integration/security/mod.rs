//! Comprehensive Security Integration Tests for RustMQ
//!
//! This module provides end-to-end security integration tests that validate
//! the complete authentication and authorization flows across all RustMQ components.
//! These tests ensure the entire security system works correctly as an integrated whole.
//!
//! ## Test Coverage
//!
//! - **End-to-End Authentication Flows**: Complete mTLS handshake and principal extraction
//! - **Authorization Workflows**: Multi-level cache integration and permission evaluation
//! - **Certificate Lifecycle Integration**: Complete certificate management flows
//! - **ACL Management Integration**: Distributed ACL operations with Raft consensus
//! - **Security API Integration**: Admin APIs and CLI security management
//! - **Performance Integration**: Security operations under realistic load
//! - **Failure and Recovery**: Security system resilience and recovery workflows
//!
//! ## Total Test Count: 42 tests
//!
//! These integration tests simulate realistic production scenarios with full
//! RustMQ cluster setup, ensuring security guarantees are maintained under
//! all operational conditions.

pub mod test_infrastructure;
pub mod e2e_auth_tests;
pub mod authorization_workflow_tests;
pub mod certificate_lifecycle_tests;
pub mod acl_integration_tests;
pub mod api_integration_tests;
pub mod performance_integration_tests;
pub mod failure_recovery_tests;

// Re-export test infrastructure for convenient access
pub use test_infrastructure::*;

#[cfg(test)]
mod integration_tests {
    use super::*;
    
    /// Comprehensive smoke test to ensure all integration test modules are properly configured
    #[tokio::test]
    async fn test_security_integration_suite_smoke_test() {
        // Verify all test modules can be imported and basic infrastructure works
        let test_cluster = SecurityTestCluster::new_test_config().await;
        assert!(test_cluster.is_ok(), "Security test cluster infrastructure should be available");
        
        // Basic connectivity test
        if let Ok(cluster) = test_cluster {
            assert!(cluster.brokers.is_empty()); // Should start with empty cluster
            assert!(cluster.controllers.is_empty()); // Should start with empty cluster
        }
    }
}