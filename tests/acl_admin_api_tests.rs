//! Comprehensive tests for ACL Admin API
//!
//! This module tests the full ACL administration API including:
//! - CRUD operations (create, read, update, delete)
//! - Cache invalidation after mutations
//! - Principal permissions query
//! - ACL synchronization
//! - Filtering and pagination

use rustmq::admin::acl_handlers::{AclHandlers, AclRuleCreationResponse, PrincipalAnalysis};
use rustmq::security::{
    AclManager, AuthorizationManager, AclRule, AclOperation, Effect, PermissionSet,
    acl::{ResourcePattern, ResourceType, raft::RaftAclManager, storage::AclRuleFilter},
};
use rustmq::config::{AclConfig, SecurityConfig};
use rustmq::network::grpc_server::GrpcNetworkHandler;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use chrono::Utc;

/// Create a test ACL manager with in-memory storage
async fn create_test_acl_manager() -> Arc<AclManager> {
    let config = AclConfig {
        enabled: true,
        cache_size_mb: 10,
        cache_ttl_seconds: 300,
        negative_cache_enabled: true,
        bloom_filter_size: 10000,
        l2_shard_count: Some(32),
        l2_shard_multiplier: 2.0,
        enable_audit_logging: false,
        batch_fetch_size: 100,
        max_rules_per_principal: 1000,
    };

    // Create a mock RaftAclManager for testing
    let raft_manager = Arc::new(RaftAclManager::new_for_testing());
    let network_handler = Arc::new(GrpcNetworkHandler::new_for_testing());

    Arc::new(
        AclManager::new(config, raft_manager, network_handler)
            .await
            .expect("Failed to create ACL manager")
    )
}

/// Create a test authorization manager
async fn create_test_authorization_manager(acl_manager: Arc<AclManager>) -> Arc<AuthorizationManager> {
    let config = AclConfig {
        enabled: true,
        cache_size_mb: 10,
        cache_ttl_seconds: 300,
        negative_cache_enabled: true,
        bloom_filter_size: 10000,
        l2_shard_count: Some(32),
        l2_shard_multiplier: 2.0,
        enable_audit_logging: false,
        batch_fetch_size: 100,
        max_rules_per_principal: 1000,
    };

    let metrics = Arc::new(rustmq::security::SecurityMetrics::new());

    Arc::new(
        AuthorizationManager::new(config, metrics, Some(acl_manager))
            .await
            .expect("Failed to create authorization manager")
    )
}

/// Create test ACL handlers
async fn create_test_handlers() -> AclHandlers {
    let acl_manager = create_test_acl_manager().await;
    let auth_manager = create_test_authorization_manager(acl_manager.clone()).await;

    AclHandlers::new(acl_manager, auth_manager)
}

#[tokio::test]
async fn test_create_acl_rule() {
    let handlers = create_test_handlers().await;

    // Create a new ACL rule
    let result = handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.events.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await;

    assert!(result.is_ok(), "Failed to create ACL rule: {:?}", result);

    let response = result.unwrap();
    assert!(!response.rule_id.is_empty(), "Rule ID should not be empty");
    assert_eq!(response.principal, "user@example.com");
    assert_eq!(response.resource_pattern, "topic.events.*");
    assert_eq!(response.resource_type, "topic");
    assert_eq!(response.operation, "read");
    assert_eq!(response.effect, "allow");
}

#[tokio::test]
async fn test_update_acl_rule() {
    let handlers = create_test_handlers().await;

    // First create a rule
    let create_result = handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.events.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create rule");

    let rule_id = create_result.rule_id;

    // Update the rule
    let update_result = handlers.update_acl_rule(
        rule_id.clone(),
        Some("user2@example.com".to_string()),
        None,
        None,
        Some("write".to_string()),
        Some("deny".to_string()),
        None,
    ).await;

    assert!(update_result.is_ok(), "Failed to update ACL rule: {:?}", update_result);

    let response = update_result.unwrap();
    assert_eq!(response.rule_id, rule_id);
    assert_eq!(response.principal, "user2@example.com");
    assert_eq!(response.operation, "write");
    assert_eq!(response.effect, "deny");
}

#[tokio::test]
async fn test_delete_acl_rule() {
    let handlers = create_test_handlers().await;

    // First create a rule
    let create_result = handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.events.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create rule");

    let rule_id = create_result.rule_id;

    // Delete the rule
    let delete_result = handlers.delete_acl_rule(rule_id.clone()).await;

    assert!(delete_result.is_ok(), "Failed to delete ACL rule: {:?}", delete_result);

    let message = delete_result.unwrap();
    assert!(message.contains("deleted successfully"));
}

#[tokio::test]
async fn test_list_acl_rules() {
    let handlers = create_test_handlers().await;

    // Create multiple rules
    for i in 0..5 {
        handlers.create_acl_rule(
            format!("user{}@example.com", i),
            "topic.events.*".to_string(),
            "topic".to_string(),
            "read".to_string(),
            "allow".to_string(),
            None,
        ).await.expect("Failed to create rule");
    }

    // List all rules
    let filters = HashMap::new();
    let list_result = handlers.list_acl_rules(filters).await;

    assert!(list_result.is_ok(), "Failed to list ACL rules: {:?}", list_result);

    let rules = list_result.unwrap();
    assert!(rules.len() >= 5, "Should have at least 5 rules");
}

#[tokio::test]
async fn test_list_acl_rules_with_filters() {
    let handlers = create_test_handlers().await;

    // Create rules with different principals
    handlers.create_acl_rule(
        "admin@example.com".to_string(),
        "topic.admin.*".to_string(),
        "topic".to_string(),
        "admin".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create admin rule");

    handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.user.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create user rule");

    // Filter by principal
    let mut filters = HashMap::new();
    filters.insert("principal".to_string(), "admin@example.com".to_string());

    let list_result = handlers.list_acl_rules(filters).await;
    assert!(list_result.is_ok(), "Failed to list filtered rules");

    let rules = list_result.unwrap();
    assert!(rules.iter().all(|r| r.principal.contains("admin")), "All rules should be for admin principal");
}

#[tokio::test]
async fn test_evaluate_acl() {
    let handlers = create_test_handlers().await;

    // Create an allow rule
    handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.events.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create rule");

    // Evaluate ACL - should allow
    let eval_result = handlers.evaluate_acl(
        "user@example.com".to_string(),
        "topic.events.test".to_string(),
        "read".to_string(),
        None,
    ).await;

    assert!(eval_result.is_ok(), "Failed to evaluate ACL: {:?}", eval_result);

    let (allowed, effect, _, _) = eval_result.unwrap();
    assert!(allowed, "Should be allowed");
    assert_eq!(effect, "allow");
}

#[tokio::test]
async fn test_get_principal_permissions() {
    let handlers = create_test_handlers().await;

    let principal = "user@example.com".to_string();

    // Create multiple rules for the principal
    handlers.create_acl_rule(
        principal.clone(),
        "topic.read.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create read rule");

    handlers.create_acl_rule(
        principal.clone(),
        "topic.write.*".to_string(),
        "topic".to_string(),
        "write".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create write rule");

    handlers.create_acl_rule(
        principal.clone(),
        "topic.deny.*".to_string(),
        "topic".to_string(),
        "admin".to_string(),
        "deny".to_string(),
        None,
    ).await.expect("Failed to create deny rule");

    // Get principal permissions
    let permissions_result = handlers.get_principal_permissions(principal.clone()).await;

    assert!(permissions_result.is_ok(), "Failed to get principal permissions: {:?}", permissions_result);

    let analysis = permissions_result.unwrap();
    assert_eq!(analysis.principal, principal);
    assert_eq!(analysis.total_rules, 3);
    assert_eq!(analysis.allow_rules, 2);
    assert_eq!(analysis.deny_rules, 1);
}

#[tokio::test]
async fn test_cache_invalidation() {
    let handlers = create_test_handlers().await;

    // Invalidate all caches
    let invalidate_result = handlers.invalidate_acl_cache().await;
    assert!(invalidate_result.is_ok(), "Failed to invalidate caches");

    let message = invalidate_result.unwrap();
    assert!(message.contains("invalidated successfully"));
}

#[tokio::test]
async fn test_cache_warming() {
    let handlers = create_test_handlers().await;

    // Create some rules first
    let principals = vec!["user1@example.com".to_string(), "user2@example.com".to_string()];

    for principal in &principals {
        handlers.create_acl_rule(
            principal.clone(),
            "topic.*".to_string(),
            "topic".to_string(),
            "read".to_string(),
            "allow".to_string(),
            None,
        ).await.expect("Failed to create rule");
    }

    // Warm cache for these principals
    let warm_result = handlers.warm_acl_cache(principals).await;
    assert!(warm_result.is_ok(), "Failed to warm cache");

    let message = warm_result.unwrap();
    assert!(message.contains("warmed successfully"));
}

#[tokio::test]
async fn test_acl_sync() {
    let handlers = create_test_handlers().await;

    // Force ACL synchronization
    let sync_result = handlers.sync_acl().await;

    assert!(sync_result.is_ok(), "Failed to sync ACLs: {:?}", sync_result);

    let status = sync_result.unwrap();
    assert!(status.version >= 0);
    assert!(!status.broker_sync_status.is_empty());
}

#[tokio::test]
async fn test_get_acl_version() {
    let handlers = create_test_handlers().await;

    // Get ACL version
    let version_result = handlers.get_acl_version().await;

    assert!(version_result.is_ok(), "Failed to get ACL version: {:?}", version_result);

    let (version, last_updated, rules_count) = version_result.unwrap();
    assert!(version >= 0);
    assert!(rules_count >= 0);
}

#[tokio::test]
async fn test_pagination() {
    let handlers = create_test_handlers().await;

    // Create 20 rules
    for i in 0..20 {
        handlers.create_acl_rule(
            format!("user{}@example.com", i),
            "topic.test.*".to_string(),
            "topic".to_string(),
            "read".to_string(),
            "allow".to_string(),
            None,
        ).await.expect("Failed to create rule");
    }

    // Test pagination - first page
    let mut filters = HashMap::new();
    filters.insert("limit".to_string(), "10".to_string());
    filters.insert("offset".to_string(), "0".to_string());

    let page1 = handlers.list_acl_rules(filters.clone()).await.unwrap();
    assert!(page1.len() <= 10, "First page should have at most 10 items");

    // Test pagination - second page
    filters.insert("offset".to_string(), "10".to_string());

    let page2 = handlers.list_acl_rules(filters).await.unwrap();
    assert!(page2.len() <= 10, "Second page should have at most 10 items");
}

#[tokio::test]
async fn test_bulk_evaluate_acl() {
    let handlers = create_test_handlers().await;

    // Create rules
    handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.read.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await.expect("Failed to create rule");

    handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.write.*".to_string(),
        "topic".to_string(),
        "write".to_string(),
        "deny".to_string(),
        None,
    ).await.expect("Failed to create rule");

    // Bulk evaluate
    let evaluations = vec![
        ("user@example.com".to_string(), "topic.read.test".to_string(), "read".to_string(), None),
        ("user@example.com".to_string(), "topic.write.test".to_string(), "write".to_string(), None),
        ("other@example.com".to_string(), "topic.read.test".to_string(), "read".to_string(), None),
    ];

    let bulk_result = handlers.bulk_evaluate_acl(evaluations).await;

    assert!(bulk_result.is_ok(), "Failed to bulk evaluate: {:?}", bulk_result);

    let (results, metrics) = bulk_result.unwrap();
    assert_eq!(results.len(), 3);

    // First should be allowed (matches allow rule)
    assert_eq!(results[0].0, true);
    // Second should be denied (matches deny rule)
    assert_eq!(results[1].0, false);
    // Third should be denied (no matching rule)
    assert_eq!(results[2].0, false);
}

#[tokio::test]
async fn test_conditions() {
    let handlers = create_test_handlers().await;

    // Create rule with conditions
    let mut conditions = HashMap::new();
    conditions.insert("client_ip".to_string(), "192.168.1.0/24".to_string());
    conditions.insert("time_of_day".to_string(), "business_hours".to_string());

    let result = handlers.create_acl_rule(
        "conditional@example.com".to_string(),
        "topic.secure.*".to_string(),
        "topic".to_string(),
        "write".to_string(),
        "allow".to_string(),
        Some(conditions.clone()),
    ).await;

    assert!(result.is_ok(), "Failed to create conditional rule: {:?}", result);

    let response = result.unwrap();
    assert_eq!(response.conditions, conditions);
}

#[tokio::test]
async fn test_input_validation() {
    let handlers = create_test_handlers().await;

    // Test empty principal
    let result = handlers.create_acl_rule(
        "".to_string(),
        "topic.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await;
    assert!(result.is_err(), "Should fail with empty principal");

    // Test invalid resource type
    let result = handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.*".to_string(),
        "invalid_type".to_string(),
        "read".to_string(),
        "allow".to_string(),
        None,
    ).await;
    assert!(result.is_err(), "Should fail with invalid resource type");

    // Test invalid operation
    let result = handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.*".to_string(),
        "topic".to_string(),
        "invalid_op".to_string(),
        "allow".to_string(),
        None,
    ).await;
    assert!(result.is_err(), "Should fail with invalid operation");

    // Test invalid effect
    let result = handlers.create_acl_rule(
        "user@example.com".to_string(),
        "topic.*".to_string(),
        "topic".to_string(),
        "read".to_string(),
        "maybe".to_string(),
        None,
    ).await;
    assert!(result.is_err(), "Should fail with invalid effect");
}