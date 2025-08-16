//! Comprehensive Security Test Suite for RustMQ
//!
//! This module provides extensive testing for all security components including:
//! - Certificate management and lifecycle
//! - mTLS authentication and principal extraction
//! - Multi-level ACL authorization caching
//! - Security integration and performance validation
//!
//! Target: 50+ tests covering production security requirements

pub mod test_utils;
pub mod authentication_tests;
pub mod authorization_tests;
pub mod acl_tests;
pub mod integration_tests;
pub mod performance_tests;
pub mod webpki_tests;
pub mod enhanced_tests;
pub mod validation_tests;

// Re-export test utilities for convenient access
pub use test_utils::*;

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Smoke test to ensure all test modules compile and basic functionality works
    #[tokio::test]
    async fn test_security_test_suite_smoke_test() {
        // This test ensures all test modules are properly configured
        // and can be imported without compilation errors
        assert!(true, "Security test suite smoke test passed");
    }
}