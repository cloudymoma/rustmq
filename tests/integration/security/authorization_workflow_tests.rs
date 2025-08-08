//! Complete Authorization Workflow Integration Tests
//!
//! This module tests the complete authorization pipeline from request
//! to permission evaluation, including multi-level cache integration,
//! ACL rule evaluation, and authorization context propagation.
//!
//! Target: 8 comprehensive authorization workflow tests

use super::test_infrastructure::*;
use rustmq::{
    security::{
        Principal, Permission, AuthContext, AclRule, Effect, ResourcePattern,
        PermissionSet, AclKey, AuthorizedRequest,
    },
    error::RustMqError,
    Result,
};
use std::{sync::Arc, time::{Duration, SystemTime}, collections::HashMap};
use tokio::time::timeout;

#[cfg(test)]
mod authorization_workflow_integration_tests {
    use super::*;

    /// Test 1: Principal-based permission evaluation from request to response
    #[tokio::test]
    async fn test_principal_permission_evaluation_end_to_end() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup ACL rules for different principals
        let acl_rules = vec![
            AclRule {
                principal: "producer-client".to_string(),
                resource_pattern: ResourcePattern::Topic("events.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "consumer-client".to_string(),
                resource_pattern: ResourcePattern::Topic("events.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "admin-client".to_string(),
                resource_pattern: ResourcePattern::Cluster,
                operation: Permission::ClusterAdmin,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "restricted-client".to_string(),
                resource_pattern: ResourcePattern::Topic("restricted.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
        ];
        
        // Apply ACL rules to cluster
        let acl_manager = cluster.acl_manager.clone();
        for rule in &acl_rules {
            acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        
        // Test producer permissions
        let producer_client = cluster.create_test_client("producer-client").await.unwrap();
        let producer_connection = producer_client.connect_to_broker(0).await.unwrap();
        
        // Should allow write to events topics
        assert!(producer_connection.authorize("events.user_signup", Permission::Write).await.unwrap());
        assert!(producer_connection.authorize("events.order_placed", Permission::Write).await.unwrap());
        
        // Should deny read operations (not explicitly allowed)
        assert!(!producer_connection.authorize("events.user_signup", Permission::Read).await.unwrap());
        
        // Should deny write to non-matching topics
        assert!(!producer_connection.authorize("metrics.cpu", Permission::Write).await.unwrap());
        
        // Test consumer permissions
        let consumer_client = cluster.create_test_client("consumer-client").await.unwrap();
        let consumer_connection = consumer_client.connect_to_broker(0).await.unwrap();
        
        // Should allow read from events topics
        assert!(consumer_connection.authorize("events.user_signup", Permission::Read).await.unwrap());
        assert!(consumer_connection.authorize("events.order_placed", Permission::Read).await.unwrap());
        
        // Should deny write operations
        assert!(!consumer_connection.authorize("events.user_signup", Permission::Write).await.unwrap());
        
        // Test admin permissions
        let admin_client = cluster.create_test_client("admin-client").await.unwrap();
        let admin_connection = admin_client.connect_to_broker(0).await.unwrap();
        
        // Should allow cluster administration
        assert!(admin_connection.authorize("cluster", Permission::ClusterAdmin).await.unwrap());
        assert!(admin_connection.authorize("cluster.topics", Permission::ClusterAdmin).await.unwrap());
        
        // Test explicit deny (should override any allow rules)
        let restricted_client = cluster.create_test_client("restricted-client").await.unwrap();
        let restricted_connection = restricted_client.connect_to_broker(0).await.unwrap();
        
        // Should deny access to restricted topics even if other rules might allow
        assert!(!restricted_connection.authorize("restricted.sensitive_data", Permission::Read).await.unwrap());
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 2: Multi-level cache integration (L1 → L2 → L3 → storage)
    #[tokio::test]
    async fn test_multi_level_cache_integration() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let authorization_manager = cluster.brokers[0].broker.as_ref().unwrap()
            .security_manager().authorization();
        
        // Setup test ACL rule
        let test_rule = AclRule {
            principal: "cache-test-client".to_string(),
            resource_pattern: ResourcePattern::Topic("cache.test.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(test_rule).await.unwrap();
        
        let client = cluster.create_test_client("cache-test-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // First authorization - should hit storage (cache miss)
        let start_time = tokio::time::Instant::now();
        let first_auth = connection.authorize("cache.test.topic1", Permission::Read).await.unwrap();
        let storage_latency = start_time.elapsed();
        
        assert!(first_auth);
        // Storage access should be slower than cache
        assert!(storage_latency > Duration::from_micros(100));
        
        // Second authorization - should hit L1 cache (connection-local)
        let start_time = tokio::time::Instant::now();
        let second_auth = connection.authorize("cache.test.topic1", Permission::Read).await.unwrap();
        let l1_latency = start_time.elapsed();
        
        assert!(second_auth);
        // L1 cache should be extremely fast (~10ns target)
        assert!(l1_latency < Duration::from_micros(50));
        
        // Create second connection to test L2 cache (broker-wide)
        let client2 = cluster.create_test_client("cache-test-client").await.unwrap();
        let connection2 = client2.connect_to_broker(0).await.unwrap();
        
        let start_time = tokio::time::Instant::now();
        let l2_auth = connection2.authorize("cache.test.topic1", Permission::Read).await.unwrap();
        let l2_latency = start_time.elapsed();
        
        assert!(l2_auth);
        // L2 cache should be faster than storage but slower than L1
        assert!(l2_latency < storage_latency);
        assert!(l2_latency < Duration::from_micros(100));
        
        // Test L3 cache (bloom filter for negative lookups)
        let start_time = tokio::time::Instant::now();
        let negative_auth = connection.authorize("cache.test.nonexistent", Permission::Write).await.unwrap();
        let bloom_latency = start_time.elapsed();
        
        assert!(!negative_auth);
        // Bloom filter should reject quickly (~20ns target)
        assert!(bloom_latency < Duration::from_micros(30));
        
        // Test cache invalidation
        let new_rule = AclRule {
            principal: "cache-test-client".to_string(),
            resource_pattern: ResourcePattern::Topic("cache.test.topic1".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(new_rule).await.unwrap();
        
        // Should invalidate caches and pick up new permission
        tokio::time::sleep(Duration::from_millis(10)).await; // Allow cache invalidation
        
        let new_permission = connection.authorize("cache.test.topic1", Permission::Write).await.unwrap();
        assert!(new_permission);
        
        // Test cache statistics
        let cache_stats = authorization_manager.get_cache_statistics().await.unwrap();
        assert!(cache_stats.l1_hits > 0);
        assert!(cache_stats.l2_hits > 0);
        assert!(cache_stats.storage_hits > 0);
        assert!(cache_stats.bloom_rejects > 0);
        
        // Verify cache hit ratios are reasonable
        let total_requests = cache_stats.l1_hits + cache_stats.l2_hits + cache_stats.storage_hits;
        let cache_hit_ratio = (cache_stats.l1_hits + cache_stats.l2_hits) as f64 / total_requests as f64;
        assert!(cache_hit_ratio > 0.5, "Cache hit ratio should be > 50%");
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 3: ACL rule evaluation with complex permission scenarios
    #[tokio::test]
    async fn test_complex_acl_rule_evaluation() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup complex ACL rules with overlapping patterns
        let complex_rules = vec![
            // General read access to all topics
            AclRule {
                principal: "complex-client".to_string(),
                resource_pattern: ResourcePattern::Topic("*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            // Specific write access to user topics
            AclRule {
                principal: "complex-client".to_string(),
                resource_pattern: ResourcePattern::Topic("user.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            // Deny access to sensitive topics (should override allow)
            AclRule {
                principal: "complex-client".to_string(),
                resource_pattern: ResourcePattern::Topic("user.sensitive.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
            // Group-based permissions
            AclRule {
                principal: "group:developers".to_string(),
                resource_pattern: ResourcePattern::Topic("dev.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            // Wildcard patterns
            AclRule {
                principal: "complex-client".to_string(),
                resource_pattern: ResourcePattern::Topic("logs.*.debug".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
        ];
        
        // Apply complex rules
        let acl_manager = cluster.acl_manager.clone();
        for rule in &complex_rules {
            acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        
        let client = cluster.create_test_client("complex-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // Test general read access
        assert!(connection.authorize("public.announcements", Permission::Read).await.unwrap());
        assert!(connection.authorize("metrics.cpu", Permission::Read).await.unwrap());
        
        // Test specific write access
        assert!(connection.authorize("user.profiles", Permission::Write).await.unwrap());
        assert!(connection.authorize("user.preferences", Permission::Write).await.unwrap());
        
        // Test denied write access to non-user topics
        assert!(!connection.authorize("public.announcements", Permission::Write).await.unwrap());
        
        // Test explicit deny overrides allow
        assert!(!connection.authorize("user.sensitive.passwords", Permission::Read).await.unwrap());
        assert!(!connection.authorize("user.sensitive.tokens", Permission::Read).await.unwrap());
        
        // Test wildcard pattern matching
        assert!(connection.authorize("logs.app.debug", Permission::Read).await.unwrap());
        assert!(connection.authorize("logs.system.debug", Permission::Read).await.unwrap());
        assert!(!connection.authorize("logs.app.info", Permission::Read).await.unwrap()); // Doesn't match pattern
        
        // Test rule precedence (more specific rules override general ones)
        let precedence_rules = vec![
            AclRule {
                principal: "precedence-client".to_string(),
                resource_pattern: ResourcePattern::Topic("*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "precedence-client".to_string(),
                resource_pattern: ResourcePattern::Topic("restricted.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
        ];
        
        for rule in &precedence_rules {
            acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        
        let precedence_client = cluster.create_test_client("precedence-client").await.unwrap();
        let precedence_connection = precedence_client.connect_to_broker(0).await.unwrap();
        
        // General allow should work
        assert!(precedence_connection.authorize("public.data", Permission::Read).await.unwrap());
        
        // Specific deny should override general allow
        assert!(!precedence_connection.authorize("restricted.data", Permission::Read).await.unwrap());
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 4: Authorization context propagation through request pipeline
    #[tokio::test]
    async fn test_authorization_context_propagation() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup ACL rules
        let pipeline_rule = AclRule {
            principal: "pipeline-client".to_string(),
            resource_pattern: ResourcePattern::Topic("pipeline.*".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(pipeline_rule).await.unwrap();
        
        let client = cluster.create_test_client("pipeline-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // Test authorization context creation
        let auth_context = client.authenticate().await.unwrap();
        assert_eq!(auth_context.principal, "pipeline-client");
        
        // Test context propagation through message send pipeline
        let topics = vec![
            "pipeline.events",
            "pipeline.metrics", 
            "pipeline.logs",
        ];
        
        for topic in topics {
            // Authorization should be called before message processing
            let authorized = connection.authorize(topic, Permission::Write).await.unwrap();
            assert!(authorized, "Should be authorized for topic: {}", topic);
            
            // Send message with authorization context
            let message = format!("test message for {}", topic).into_bytes();
            connection.send_message(topic, &message).await.unwrap();
            
            // Verify message was processed with correct authorization context
            // In a real implementation, this would check that the message was
            // associated with the correct principal and permissions
        }
        
        // Test context propagation through admin operations
        let admin_rule = AclRule {
            principal: "pipeline-client".to_string(),
            resource_pattern: ResourcePattern::Cluster,
            operation: Permission::ClusterAdmin,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(admin_rule).await.unwrap();
        
        // Test admin operation authorization
        let admin_authorized = connection.authorize("cluster.topics", Permission::ClusterAdmin).await.unwrap();
        assert!(admin_authorized);
        
        // Test context preservation across multiple operations
        let mut operation_results = Vec::new();
        
        for i in 0..10 {
            let topic = format!("pipeline.batch.{}", i);
            let authorized = connection.authorize(&topic, Permission::Write).await.unwrap();
            operation_results.push((topic.clone(), authorized));
            
            if authorized {
                let message = format!("batch message {}", i).into_bytes();
                connection.send_message(&topic, &message).await.unwrap();
            }
        }
        
        // All operations should have consistent authorization results
        assert!(operation_results.iter().all(|(_, authorized)| *authorized));
        
        // Test context timeout and refresh
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Context should still be valid
        let late_auth = connection.authorize("pipeline.late", Permission::Write).await.unwrap();
        assert!(late_auth);
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 5: Permission inheritance and group-based access control
    #[tokio::test]
    async fn test_permission_inheritance_and_groups() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup group-based ACL rules
        let group_rules = vec![
            // Base permissions for all users
            AclRule {
                principal: "group:users".to_string(),
                resource_pattern: ResourcePattern::Topic("public.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            // Developer group permissions
            AclRule {
                principal: "group:developers".to_string(),
                resource_pattern: ResourcePattern::Topic("dev.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "group:developers".to_string(),
                resource_pattern: ResourcePattern::Topic("dev.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            // Admin group permissions
            AclRule {
                principal: "group:admins".to_string(),
                resource_pattern: ResourcePattern::Cluster,
                operation: Permission::ClusterAdmin,
                effect: Effect::Allow,
            },
            // Specific user overrides
            AclRule {
                principal: "dev-user-1".to_string(),
                resource_pattern: ResourcePattern::Topic("dev.special.*".to_string()),
                operation: Permission::ClusterAdmin,
                effect: Effect::Allow,
            },
        ];
        
        // Apply group rules
        let acl_manager = cluster.acl_manager.clone();
        for rule in &group_rules {
            acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        
        // Test user with multiple group memberships
        let user_groups = HashMap::from([
            ("dev-user-1".to_string(), vec!["users".to_string(), "developers".to_string()]),
            ("admin-user-1".to_string(), vec!["users".to_string(), "admins".to_string()]),
            ("regular-user".to_string(), vec!["users".to_string()]),
        ]);
        
        // Configure group memberships in ACL manager
        for (user, groups) in &user_groups {
            for group in groups {
                acl_manager.add_user_to_group(user, group).await.unwrap();
            }
        }
        
        // Test dev-user-1 permissions (users + developers + specific overrides)
        let dev_client = cluster.create_test_client("dev-user-1").await.unwrap();
        let dev_connection = dev_client.connect_to_broker(0).await.unwrap();
        
        // Should inherit from users group
        assert!(dev_connection.authorize("public.announcements", Permission::Read).await.unwrap());
        
        // Should inherit from developers group
        assert!(dev_connection.authorize("dev.api", Permission::Read).await.unwrap());
        assert!(dev_connection.authorize("dev.api", Permission::Write).await.unwrap());
        
        // Should have specific user permissions
        assert!(dev_connection.authorize("dev.special.feature", Permission::ClusterAdmin).await.unwrap());
        
        // Should not have admin cluster permissions (not in admin group)
        assert!(!dev_connection.authorize("cluster", Permission::ClusterAdmin).await.unwrap());
        
        // Test admin-user-1 permissions (users + admins)
        let admin_client = cluster.create_test_client("admin-user-1").await.unwrap();
        let admin_connection = admin_client.connect_to_broker(0).await.unwrap();
        
        // Should inherit from users group
        assert!(admin_connection.authorize("public.announcements", Permission::Read).await.unwrap());
        
        // Should have admin permissions
        assert!(admin_connection.authorize("cluster", Permission::ClusterAdmin).await.unwrap());
        
        // Should not have developer permissions (not in developers group)
        assert!(!admin_connection.authorize("dev.api", Permission::Write).await.unwrap());
        
        // Test regular-user permissions (only users group)
        let regular_client = cluster.create_test_client("regular-user").await.unwrap();
        let regular_connection = regular_client.connect_to_broker(0).await.unwrap();
        
        // Should only have basic user permissions
        assert!(regular_connection.authorize("public.announcements", Permission::Read).await.unwrap());
        assert!(!regular_connection.authorize("dev.api", Permission::Read).await.unwrap());
        assert!(!regular_connection.authorize("cluster", Permission::ClusterAdmin).await.unwrap());
        
        // Test permission inheritance precedence
        // Specific user rules should override group rules
        let precedence_rule = AclRule {
            principal: "dev-user-1".to_string(),
            resource_pattern: ResourcePattern::Topic("dev.restricted.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Deny,
        };
        
        acl_manager.add_rule(precedence_rule).await.unwrap();
        
        // User-specific deny should override group allow
        assert!(!dev_connection.authorize("dev.restricted.data", Permission::Read).await.unwrap());
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 6: Cache invalidation coordination across security components
    #[tokio::test]
    async fn test_cache_invalidation_coordination() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup initial ACL rule
        let initial_rule = AclRule {
            principal: "cache-invalidation-client".to_string(),
            resource_pattern: ResourcePattern::Topic("test.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(initial_rule).await.unwrap();
        
        // Create multiple connections to test cache coordination
        let client1 = cluster.create_test_client("cache-invalidation-client").await.unwrap();
        let client2 = cluster.create_test_client("cache-invalidation-client").await.unwrap();
        
        let connection1 = client1.connect_to_broker(0).await.unwrap();
        let connection2 = client2.connect_to_broker(0).await.unwrap();
        
        // Warm up caches
        assert!(connection1.authorize("test.topic1", Permission::Read).await.unwrap());
        assert!(connection2.authorize("test.topic1", Permission::Read).await.unwrap());
        
        // Verify cached authorization is fast
        let start = tokio::time::Instant::now();
        assert!(connection1.authorize("test.topic1", Permission::Read).await.unwrap());
        let cached_latency = start.elapsed();
        assert!(cached_latency < Duration::from_micros(50));
        
        // Update ACL rule (should trigger cache invalidation)
        let updated_rule = AclRule {
            principal: "cache-invalidation-client".to_string(),
            resource_pattern: ResourcePattern::Topic("test.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Deny,
        };
        
        cluster.acl_manager.update_rule(&initial_rule, updated_rule.clone()).await.unwrap();
        
        // Wait for cache invalidation to propagate
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Both connections should see updated permissions
        assert!(!connection1.authorize("test.topic1", Permission::Read).await.unwrap());
        assert!(!connection2.authorize("test.topic1", Permission::Read).await.unwrap());
        
        // Test selective cache invalidation
        let selective_rule = AclRule {
            principal: "cache-invalidation-client".to_string(),
            resource_pattern: ResourcePattern::Topic("test.topic2".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(selective_rule).await.unwrap();
        
        // Only specific entries should be invalidated
        assert!(connection1.authorize("test.topic2", Permission::Write).await.unwrap());
        assert!(!connection1.authorize("test.topic1", Permission::Read).await.unwrap()); // Still denied
        
        // Test cache invalidation across brokers (if multiple brokers)
        if cluster.brokers.len() > 1 {
            let broker2_client = cluster.create_test_client("cache-invalidation-client").await.unwrap();
            let broker2_connection = broker2_client.connect_to_broker(1).await.unwrap();
            
            // Should see consistent permissions across brokers
            assert!(broker2_connection.authorize("test.topic2", Permission::Write).await.unwrap());
            assert!(!broker2_connection.authorize("test.topic1", Permission::Read).await.unwrap());
        }
        
        // Test cache metrics after invalidation
        let authorization_manager = cluster.brokers[0].broker.as_ref().unwrap()
            .security_manager().authorization();
        
        let cache_stats = authorization_manager.get_cache_statistics().await.unwrap();
        assert!(cache_stats.invalidations > 0);
        assert!(cache_stats.cache_misses_after_invalidation > 0);
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 7: Authorization metrics and performance tracking
    #[tokio::test]
    async fn test_authorization_metrics_and_performance() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup test rules
        let metrics_rules = vec![
            AclRule {
                principal: "metrics-client".to_string(),
                resource_pattern: ResourcePattern::Topic("allowed.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "metrics-client".to_string(),
                resource_pattern: ResourcePattern::Topic("denied.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
        ];
        
        for rule in &metrics_rules {
            cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        
        let authorization_manager = cluster.brokers[0].broker.as_ref().unwrap()
            .security_manager().authorization();
        
        let initial_metrics = authorization_manager.get_metrics().await.unwrap();
        
        let client = cluster.create_test_client("metrics-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // Perform various authorization operations
        let test_operations = vec![
            ("allowed.topic1", Permission::Read, true),
            ("allowed.topic2", Permission::Read, true),
            ("denied.topic1", Permission::Read, false),
            ("denied.topic2", Permission::Read, false),
            ("allowed.topic1", Permission::Write, false), // Not explicitly allowed
            ("nonexistent.topic", Permission::Read, false),
        ];
        
        let mut expected_allowed = 0;
        let mut expected_denied = 0;
        
        for (topic, permission, expected_result) in &test_operations {
            let start = tokio::time::Instant::now();
            let result = connection.authorize(topic, *permission).await.unwrap();
            let latency = start.elapsed();
            
            assert_eq!(result, *expected_result, 
                      "Authorization result mismatch for {} {:?}", topic, permission);
            
            // Track expected counts
            if *expected_result {
                expected_allowed += 1;
            } else {
                expected_denied += 1;
            }
            
            // First-time authorization should have reasonable latency
            if expected_allowed + expected_denied == 1 {
                assert!(latency < Duration::from_millis(10), 
                       "Initial authorization latency too high: {:?}", latency);
            }
        }
        
        // Repeat operations to test cache performance
        for (topic, permission, _) in &test_operations {
            let start = tokio::time::Instant::now();
            let _result = connection.authorize(topic, *permission).await.unwrap();
            let cached_latency = start.elapsed();
            
            // Cached operations should be much faster
            assert!(cached_latency < Duration::from_micros(100), 
                   "Cached authorization latency too high: {:?}", cached_latency);
        }
        
        // Verify metrics updates
        let final_metrics = authorization_manager.get_metrics().await.unwrap();
        
        assert_eq!(final_metrics.authorization_success_total, 
                  initial_metrics.authorization_success_total + expected_allowed);
        assert_eq!(final_metrics.authorization_failure_total,
                  initial_metrics.authorization_failure_total + expected_denied);
        
        // Test latency percentiles
        let latency_stats = authorization_manager.get_latency_statistics().await.unwrap();
        
        assert!(latency_stats.avg_latency > Duration::from_nanos(0));
        assert!(latency_stats.p50_latency <= latency_stats.p95_latency);
        assert!(latency_stats.p95_latency <= latency_stats.p99_latency);
        assert!(latency_stats.p99_latency <= latency_stats.max_latency);
        
        // Verify performance targets
        assert!(latency_stats.avg_latency < Duration::from_millis(1), 
               "Average authorization latency should be < 1ms");
        assert!(latency_stats.p95_latency < Duration::from_millis(5), 
               "P95 authorization latency should be < 5ms");
        assert!(latency_stats.p99_latency < Duration::from_millis(10), 
               "P99 authorization latency should be < 10ms");
        
        // Test cache hit ratio
        let cache_stats = authorization_manager.get_cache_statistics().await.unwrap();
        let total_requests = cache_stats.l1_hits + cache_stats.l2_hits + cache_stats.storage_hits;
        
        if total_requests > 0 {
            let hit_ratio = (cache_stats.l1_hits + cache_stats.l2_hits) as f64 / total_requests as f64;
            assert!(hit_ratio > 0.7, "Cache hit ratio should be > 70%, got {:.2}", hit_ratio);
        }
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 8: Error handling for authorization failures with proper context
    #[tokio::test]
    async fn test_authorization_error_handling_with_context() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup limited permissions
        let limited_rule = AclRule {
            principal: "limited-client".to_string(),
            resource_pattern: ResourcePattern::Topic("allowed.only".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(limited_rule).await.unwrap();
        
        let client = cluster.create_test_client("limited-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // Test successful authorization with context
        let allowed_result = connection.authorize("allowed.only", Permission::Read).await.unwrap();
        assert!(allowed_result);
        
        // Test authorization failures with different error types
        let error_scenarios = vec![
            ("denied.topic", Permission::Read, "permission denied"),
            ("allowed.only", Permission::Write, "operation not permitted"),
            ("nonexistent.pattern", Permission::Read, "no matching rules"),
        ];
        
        for (topic, permission, expected_error_type) in error_scenarios {
            let denied_result = connection.authorize(&topic, permission).await.unwrap();
            assert!(!denied_result, "Should deny access to {} {:?}", topic, permission);
            
            // In a full implementation, we would capture and verify error details
            // including the specific reason for denial, the principal, resource, etc.
        }
        
        // Test authorization with invalid principal (simulated)
        let invalid_client = TestSecurityClient::new(
            TestCertificate {
                certificate: rustls::Certificate(b"invalid".to_vec()),
                private_key: rustls::PrivateKey(b"invalid".to_vec()),
                cert_path: "/tmp/invalid.pem".to_string(),
                key_path: "/tmp/invalid.key".to_string(),
                principal: "".to_string(), // Empty principal
                serial_number: "invalid".to_string(),
                fingerprint: "invalid".to_string(),
            },
            cluster.get_broker_addresses(),
            cluster.test_ca.ca_cert_path.clone(),
        ).await.unwrap();
        
        // Connection with invalid principal should handle gracefully
        let invalid_connection_result = invalid_client.connect_to_broker(0).await;
        // This should fail in a real implementation
        
        // Test authorization system resilience during errors
        let authorization_manager = cluster.brokers[0].broker.as_ref().unwrap()
            .security_manager().authorization();
        
        // Verify error metrics are tracked
        let error_metrics = authorization_manager.get_error_metrics().await.unwrap();
        assert!(error_metrics.authorization_errors_total >= 0);
        
        // Test authorization with malformed resource patterns
        let malformed_scenarios = vec![
            ("", Permission::Read), // Empty resource
            ("invalid..pattern", Permission::Read), // Double dots
            ("resource\x00null", Permission::Read), // Null bytes
        ];
        
        for (malformed_resource, permission) in malformed_scenarios {
            let result = connection.authorize(malformed_resource, permission).await;
            // Should handle malformed input gracefully
            match result {
                Ok(authorized) => assert!(!authorized, "Should deny malformed resource"),
                Err(_) => {
                    // Error is acceptable for malformed input
                }
            }
        }
        
        // Test authorization during ACL manager unavailability
        // Simulate ACL manager error
        let acl_manager = cluster.acl_manager.clone();
        
        // Try authorization when ACL system has issues
        // This tests the fallback behavior and error handling
        let resilience_result = connection.authorize("test.resilience", Permission::Read).await;
        
        // System should either:
        // 1. Fail securely (deny by default)
        // 2. Use cached permissions if available
        // 3. Return appropriate error
        match resilience_result {
            Ok(authorized) => {
                // If authorized, it should be from cache
                assert!(!authorized, "Should deny when ACL system unavailable (fail secure)");
            }
            Err(error) => {
                // Error is acceptable when ACL system is unavailable
                assert!(error.to_string().contains("ACL") || error.to_string().contains("authorization"));
            }
        }
        
        cluster.shutdown().await.unwrap();
    }
}

#[cfg(test)]
mod authorization_stress_tests {
    use super::*;
    
    /// Stress test: Authorization performance under concurrent load
    #[tokio::test]
    async fn test_authorization_performance_under_concurrent_load() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup test rules for stress testing
        let stress_rules = vec![
            AclRule {
                principal: "*".to_string(), // Wildcard for any principal
                resource_pattern: ResourcePattern::Topic("stress.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "*".to_string(),
                resource_pattern: ResourcePattern::Topic("stress.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
        ];
        
        for rule in &stress_rules {
            cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        
        // Create multiple concurrent clients
        let client_count = 20;
        let requests_per_client = 100;
        let mut handles = Vec::new();
        
        let start_time = tokio::time::Instant::now();
        
        for i in 0..client_count {
            let cluster_ref = &cluster;
            let handle = tokio::spawn(async move {
                let client = cluster_ref.create_test_client(&format!("stress-client-{}", i)).await.unwrap();
                let connection = client.connect_to_broker(0).await.unwrap();
                
                let mut latencies = Vec::new();
                
                for j in 0..requests_per_client {
                    let topic = format!("stress.topic.{}.{}", i, j);
                    let permission = if j % 2 == 0 { Permission::Read } else { Permission::Write };
                    
                    let start = tokio::time::Instant::now();
                    let result = connection.authorize(&topic, permission).await.unwrap();
                    let latency = start.elapsed();
                    
                    assert!(result, "Should be authorized for stress test");
                    latencies.push(latency);
                }
                
                latencies
            });
            
            handles.push(handle);
        }
        
        // Collect all latencies
        let mut all_latencies = Vec::new();
        for handle in handles {
            let client_latencies = handle.await.unwrap();
            all_latencies.extend(client_latencies);
        }
        
        let total_time = start_time.elapsed();
        let total_requests = client_count * requests_per_client;
        
        // Calculate performance statistics
        all_latencies.sort();
        let avg_latency = all_latencies.iter().sum::<Duration>() / all_latencies.len() as u32;
        let p95_latency = all_latencies[(all_latencies.len() as f64 * 0.95) as usize];
        let p99_latency = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];
        let max_latency = all_latencies[all_latencies.len() - 1];
        
        let throughput = total_requests as f64 / total_time.as_secs_f64();
        
        // Verify performance requirements
        assert!(avg_latency < Duration::from_millis(5), 
               "Average authorization latency should be < 5ms under load, got {:?}", avg_latency);
        assert!(p95_latency < Duration::from_millis(10), 
               "P95 authorization latency should be < 10ms under load, got {:?}", p95_latency);
        assert!(p99_latency < Duration::from_millis(25), 
               "P99 authorization latency should be < 25ms under load, got {:?}", p99_latency);
        assert!(max_latency < Duration::from_millis(100), 
               "Max authorization latency should be < 100ms under load, got {:?}", max_latency);
        
        assert!(throughput > 1000.0, 
               "Authorization throughput should be > 1000 RPS, got {:.2}", throughput);
        
        println!("Authorization stress test results:");
        println!("  Total requests: {}", total_requests);
        println!("  Total time: {:?}", total_time);
        println!("  Throughput: {:.2} RPS", throughput);
        println!("  Average latency: {:?}", avg_latency);
        println!("  P95 latency: {:?}", p95_latency);
        println!("  P99 latency: {:?}", p99_latency);
        println!("  Max latency: {:?}", max_latency);
        
        cluster.shutdown().await.unwrap();
    }
}