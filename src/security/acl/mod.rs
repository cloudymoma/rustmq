//! Access Control List (ACL) Management
//!
//! This module provides comprehensive ACL rule management with Raft consensus,
//! persistent storage, caching, security validation, and audit logging
//! for RustMQ's enterprise authorization system.

pub mod manager;
pub mod patterns;
pub mod policy;
pub mod raft;
pub mod rules;
pub mod storage;

// Core ACL types
pub use manager::{
    AclCondition, AclEffect, AclManager, AclManagerTrait, AclSyncPayload, ConditionOperator,
    GrpcNetworkHandlerAclExtension, Permission,
};
pub use patterns::{ResourcePattern, ResourceType};
pub use policy::{Effect, PolicyDecision};
pub use rules::{AclEntry, AclOperation, AclRule, CompiledPattern};

// Storage types
pub use storage::{
    AclPermission, AclRuleFilter, AclSnapshot, AclStorage, AclStorageConfig, InMemoryAclStorage,
    ObjectStorageAclStorage, VersionedAclRule,
};

// Raft integration types
pub use raft::{AclRaftOperation, AclRaftResult, AclStateMachine, RaftAclManager, RaftOperations};

// Re-export PermissionSet from auth module for backward compatibility
pub use crate::security::auth::PermissionSet;

/// Create a new ACL manager with all enterprise features
///
/// # Arguments
/// * `config` - ACL configuration
/// * `raft_ops` - Raft operations provider (dependency injection)
/// * `object_storage` - Object storage backend
/// * `cache` - Cache layer
/// * `network_handler` - Network handler for gRPC communication
/// * `node_id` - Node identifier
pub async fn create_acl_manager(
    config: crate::config::AclConfig,
    raft_ops: std::sync::Arc<dyn RaftOperations>,
    object_storage: std::sync::Arc<dyn crate::storage::traits::ObjectStorage>,
    cache: std::sync::Arc<dyn crate::storage::traits::Cache>,
    network_handler: std::sync::Arc<crate::network::grpc_server::GrpcNetworkHandler>,
    node_id: String,
) -> crate::Result<AclManager> {
    // Create storage layer
    let storage_config = storage::AclStorageConfig::default();
    let acl_storage = std::sync::Arc::new(
        storage::ObjectStorageAclStorage::new(object_storage, cache, storage_config).await?,
    );

    // Create Raft manager with dependency-injected raft operations
    let raft_manager =
        std::sync::Arc::new(raft::RaftAclManager::new(acl_storage, raft_ops, node_id));

    // Create enhanced ACL manager
    AclManager::new(config, raft_manager, network_handler).await
}
