//! Access Control List (ACL) Management
//!
//! This module provides comprehensive ACL rule management with Raft consensus,
//! persistent storage, caching, security validation, and audit logging
//! for RustMQ's enterprise authorization system.

pub mod manager;
pub mod rules;
pub mod patterns;
pub mod policy;
pub mod storage;
pub mod raft;

// Core ACL types
pub use manager::{
    AclManager, AclManagerTrait, Permission, AclEffect, AclCondition, ConditionOperator,
    AclSyncPayload, GrpcNetworkHandlerAclExtension
};
pub use rules::{AclRule, AclEntry, AclOperation, CompiledPattern};
pub use patterns::{ResourcePattern, ResourceType};
pub use policy::{Effect, PolicyDecision};

// Storage types
pub use storage::{
    AclStorage, ObjectStorageAclStorage, AclStorageConfig, VersionedAclRule, 
    AclRuleFilter, AclPermission, AclSnapshot, InMemoryAclStorage
};

// Raft integration types
pub use raft::{
    RaftAclManager, AclRaftOperation, AclRaftResult, AclStateMachine,
    ControllerAclExtension
};

// Re-export PermissionSet from auth module for backward compatibility
pub use crate::security::auth::PermissionSet;

/// Create a new ACL manager with all enterprise features
pub async fn create_acl_manager(
    config: crate::config::AclConfig,
    controller: std::sync::Arc<crate::controller::service::ControllerService>,
    object_storage: std::sync::Arc<dyn crate::storage::traits::ObjectStorage>,
    cache: std::sync::Arc<dyn crate::storage::traits::Cache>,
    network_handler: std::sync::Arc<crate::network::grpc_server::GrpcNetworkHandler>,
    node_id: String,
) -> crate::Result<AclManager> {
    // Create storage layer
    let storage_config = storage::AclStorageConfig::default();
    let acl_storage = std::sync::Arc::new(
        storage::ObjectStorageAclStorage::new(object_storage, cache, storage_config).await?
    );
    
    // Create Raft manager
    let raft_manager = std::sync::Arc::new(
        raft::RaftAclManager::new(acl_storage, controller, node_id)
    );
    
    // Create enhanced ACL manager
    AclManager::new(config, raft_manager, network_handler).await
}