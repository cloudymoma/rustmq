//! ACL Management Integration Tests
//!
//! This module tests end-to-end ACL rule creation, distribution, and enforcement
//! across controllers and brokers, including Raft consensus integration and
//! real-time ACL updates without service interruption.
//!
//! Target: 6 comprehensive ACL management integration tests

use super::test_infrastructure::*;
use rustmq::{
    security::{
        AclManager, AclRule, AclEntry, Effect, ResourcePattern, Permission,
        PermissionSet, Principal,
    },
    controller::service::Controller,
    error::RustMqError,
    Result,
};
use std::{sync::Arc, time::{Duration, SystemTime}, collections::HashMap};
use tokio::time::timeout;

#[cfg(test)]
mod acl_management_integration_tests {
    use super::*;

    /// Test 1: End-to-end ACL rule creation, distribution, and enforcement
    #[tokio::test]
    async fn test_e2e_acl_rule_lifecycle() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let acl_manager = cluster.acl_manager.clone();
        
        // Create comprehensive ACL rules for different scenarios
        let test_rules = vec![
            AclRule {
                principal: "producer-service".to_string(),
                resource_pattern: ResourcePattern::Topic("events.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "consumer-service".to_string(),
                resource_pattern: ResourcePattern::Topic("events.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "analytics-service".to_string(),
                resource_pattern: ResourcePattern::Topic("metrics.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "admin-user".to_string(),
                resource_pattern: ResourcePattern::Cluster,
                operation: Permission::ClusterAdmin,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "restricted-user".to_string(),
                resource_pattern: ResourcePattern::Topic("sensitive.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
        ];
        
        // Test ACL rule creation and storage
        let rule_creation_start = tokio::time::Instant::now();
        let mut created_rule_ids = Vec::new();
        
        for rule in &test_rules {
            let rule_id = acl_manager.add_rule(rule.clone()).await.unwrap();
            created_rule_ids.push(rule_id);
        }
        
        let rule_creation_time = rule_creation_start.elapsed();
        assert!(rule_creation_time < Duration::from_secs(5));
        
        // Test ACL rule distribution to brokers
        let distribution_start = tokio::time::Instant::now();
        let distribution_results = acl_manager.distribute_rules_to_brokers().await.unwrap();
        let distribution_time = distribution_start.elapsed();
        
        assert_eq!(distribution_results.len(), cluster.brokers.len());
        assert!(distribution_results.iter().all(|result| result.success));
        assert!(distribution_time < Duration::from_secs(2));
        
        // Wait for distribution to complete
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Test rule enforcement on brokers
        let test_scenarios = vec![
            ("producer-service", "events.user_signup", Permission::Write, true),
            ("producer-service", "events.order_placed", Permission::Write, true),
            ("producer-service", "metrics.cpu", Permission::Write, false),
            ("consumer-service", "events.user_signup", Permission::Read, true),
            ("consumer-service", "events.user_signup", Permission::Write, false),
            ("analytics-service", "metrics.cpu", Permission::Read, true),
            ("analytics-service", "events.user_signup", Permission::Read, false),
            ("admin-user", "cluster", Permission::ClusterAdmin, true),
            ("admin-user", "cluster.topics", Permission::ClusterAdmin, true),
            ("restricted-user", "sensitive.passwords", Permission::Read, false),
            ("restricted-user", "public.data", Permission::Read, false), // No explicit allow
        ];
        
        for (principal, resource, operation, expected_result) in test_scenarios {
            let client = cluster.create_test_client(principal).await.unwrap();
            let connection = client.connect_to_broker(0).await.unwrap();
            
            let actual_result = connection.authorize(resource, operation).await.unwrap();
            assert_eq!(actual_result, expected_result, 
                      "ACL enforcement failed for {} on {} with {:?}", principal, resource, operation);
        }
        
        // Test ACL rule query and retrieval
        let rules_for_producer = acl_manager.get_rules_for_principal("producer-service").await.unwrap();
        assert_eq!(rules_for_producer.len(), 1);
        assert_eq!(rules_for_producer[0].operation, Permission::Write);
        
        let rules_for_topic = acl_manager.get_rules_for_resource(&ResourcePattern::Topic("events.*".to_string())).await.unwrap();
        assert!(rules_for_topic.len() >= 2); // producer and consumer rules
        
        // Test ACL rule modification
        let mut modified_rule = test_rules[0].clone();
        modified_rule.resource_pattern = ResourcePattern::Topic("events.important.*".to_string());
        
        let modification_result = acl_manager.update_rule(&created_rule_ids[0], modified_rule.clone()).await.unwrap();
        assert!(modification_result.success);
        
        // Verify modification is enforced
        tokio::time::sleep(Duration::from_millis(50)).await; // Allow cache invalidation
        
        let producer_client = cluster.create_test_client("producer-service").await.unwrap();
        let producer_connection = producer_client.connect_to_broker(0).await.unwrap();
        
        // Should still allow access to new pattern
        assert!(producer_connection.authorize("events.important.alert", Permission::Write).await.unwrap());
        
        // Should deny access to old pattern (unless covered by other rules)
        let old_pattern_result = producer_connection.authorize("events.general", Permission::Write).await.unwrap();
        assert!(!old_pattern_result);
        
        // Test ACL rule deletion
        let deletion_result = acl_manager.delete_rule(&created_rule_ids[4]).await.unwrap(); // restricted-user rule
        assert!(deletion_result.success);
        
        tokio::time::sleep(Duration::from_millis(50)).await; // Allow cache invalidation
        
        // Verify rule deletion is enforced
        let restricted_client = cluster.create_test_client("restricted-user").await.unwrap();
        let restricted_connection = restricted_client.connect_to_broker(0).await.unwrap();
        
        // Should no longer be explicitly denied (but may still be denied due to no allow rule)
        let restricted_result = restricted_connection.authorize("sensitive.passwords", Permission::Read).await.unwrap();
        assert!(!restricted_result); // Still denied due to default deny
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 2: Raft consensus integration for ACL updates across controllers
    #[tokio::test]
    async fn test_raft_consensus_acl_updates() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.cluster_size = 3; // Ensure we have a proper Raft cluster
        cluster.start().await.unwrap();
        
        let acl_manager = cluster.acl_manager.clone();
        
        // Identify the Raft leader controller
        let mut leader_controller_index = 0;
        for (i, controller) in cluster.controllers.iter().enumerate() {
            if controller.is_leader().await.unwrap() {
                leader_controller_index = i;
                break;
            }
        }
        
        // Test ACL rule creation through Raft consensus
        let consensus_rule = AclRule {
            principal: "consensus-test-client".to_string(),
            resource_pattern: ResourcePattern::Topic("consensus.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let consensus_start = tokio::time::Instant::now();
        let consensus_result = acl_manager.add_rule_with_consensus(consensus_rule.clone()).await.unwrap();
        let consensus_time = consensus_start.elapsed();
        
        assert!(consensus_result.success);
        assert!(consensus_result.committed_to_majority);
        assert!(consensus_time < Duration::from_secs(2));
        
        // Verify rule is replicated to all controllers
        tokio::time::sleep(Duration::from_millis(200)).await; // Allow replication
        
        for (i, controller) in cluster.controllers.iter().enumerate() {
            let controller_acl_manager = controller.get_acl_manager().await.unwrap();
            let rules = controller_acl_manager.get_rules_for_principal("consensus-test-client").await.unwrap();
            
            assert_eq!(rules.len(), 1, "Rule not replicated to controller {}", i);
            assert_eq!(rules[0].principal, "consensus-test-client");
            assert_eq!(rules[0].operation, Permission::Read);
        }
        
        // Test Raft leader failover scenario
        let original_leader = &cluster.controllers[leader_controller_index];
        
        // Simulate leader failure (graceful shutdown)
        original_leader.shutdown().await.unwrap();
        
        // Wait for new leader election
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Find new leader
        let mut new_leader_index = None;
        for (i, controller) in cluster.controllers.iter().enumerate() {
            if i != leader_controller_index && controller.is_leader().await.unwrap_or(false) {
                new_leader_index = Some(i);
                break;
            }
        }
        
        assert!(new_leader_index.is_some(), "No new leader elected after failover");
        
        // Test ACL operations with new leader
        let failover_rule = AclRule {
            principal: "failover-test-client".to_string(),
            resource_pattern: ResourcePattern::Topic("failover.*".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        let failover_result = acl_manager.add_rule_with_consensus(failover_rule.clone()).await.unwrap();
        assert!(failover_result.success);
        
        // Verify rule is still replicated to remaining controllers
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        for (i, controller) in cluster.controllers.iter().enumerate() {
            if i == leader_controller_index {
                continue; // Skip failed controller
            }
            
            let controller_acl_manager = controller.get_acl_manager().await.unwrap();
            let rules = controller_acl_manager.get_rules_for_principal("failover-test-client").await.unwrap();
            
            assert_eq!(rules.len(), 1, "Failover rule not replicated to controller {}", i);
        }
        
        // Test network partition tolerance
        let partition_rule = AclRule {
            principal: "partition-test-client".to_string(),
            resource_pattern: ResourcePattern::Topic("partition.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        // Simulate network partition (isolate one controller)
        let isolated_controller_index = cluster.controllers.len() - 1;
        let isolated_controller = &cluster.controllers[isolated_controller_index];
        
        // Simulate network isolation
        isolated_controller.simulate_network_partition().await.unwrap();
        
        // Add rule during partition
        let partition_result = acl_manager.add_rule_with_consensus(partition_rule.clone()).await.unwrap();
        assert!(partition_result.success); // Should succeed with majority
        
        // Verify rule is replicated to non-isolated controllers
        for (i, controller) in cluster.controllers.iter().enumerate() {
            if i == leader_controller_index || i == isolated_controller_index {
                continue; // Skip failed and isolated controllers
            }
            
            let controller_acl_manager = controller.get_acl_manager().await.unwrap();
            let rules = controller_acl_manager.get_rules_for_principal("partition-test-client").await.unwrap();
            
            assert_eq!(rules.len(), 1);
        }
        
        // Heal network partition
        isolated_controller.heal_network_partition().await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await; // Allow catch-up
        
        // Verify isolated controller catches up
        let isolated_acl_manager = isolated_controller.get_acl_manager().await.unwrap();
        let catch_up_rules = isolated_acl_manager.get_rules_for_principal("partition-test-client").await.unwrap();
        assert_eq!(catch_up_rules.len(), 1);
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 3: ACL synchronization from controllers to brokers
    #[tokio::test]
    async fn test_acl_synchronization_controllers_to_brokers() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.broker_count = 3; // Multiple brokers for comprehensive testing
        cluster.start().await.unwrap();
        
        let acl_manager = cluster.acl_manager.clone();
        
        // Create ACL rules on controller
        let sync_rules = vec![
            AclRule {
                principal: "sync-producer".to_string(),
                resource_pattern: ResourcePattern::Topic("sync.events.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "sync-consumer".to_string(),
                resource_pattern: ResourcePattern::Topic("sync.events.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "sync-admin".to_string(),
                resource_pattern: ResourcePattern::Cluster,
                operation: Permission::ClusterAdmin,
                effect: Effect::Allow,
            },
        ];
        
        // Add rules to controller
        let mut rule_ids = Vec::new();
        for rule in &sync_rules {
            let rule_id = acl_manager.add_rule(rule.clone()).await.unwrap();
            rule_ids.push(rule_id);
        }
        
        // Test initial synchronization to all brokers
        let initial_sync_start = tokio::time::Instant::now();
        let sync_results = acl_manager.synchronize_acl_to_brokers().await.unwrap();
        let initial_sync_time = initial_sync_start.elapsed();
        
        assert_eq!(sync_results.len(), cluster.brokers.len());
        assert!(sync_results.iter().all(|result| result.success));
        assert!(initial_sync_time < Duration::from_secs(3));
        
        // Wait for synchronization to complete
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Verify rules are enforced consistently across all brokers
        for (broker_index, broker) in cluster.brokers.iter().enumerate() {
            let client = cluster.create_test_client("sync-producer").await.unwrap();
            let connection = client.connect_to_broker(broker_index).await.unwrap();
            
            assert!(connection.authorize("sync.events.test", Permission::Write).await.unwrap(),
                   "Rule not synchronized to broker {}", broker_index);
            
            assert!(!connection.authorize("sync.events.test", Permission::Read).await.unwrap(),
                   "Incorrect permission on broker {}", broker_index);
        }
        
        // Test incremental synchronization
        let incremental_rule = AclRule {
            principal: "incremental-client".to_string(),
            resource_pattern: ResourcePattern::Topic("incremental.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let incremental_rule_id = acl_manager.add_rule(incremental_rule.clone()).await.unwrap();
        
        // Perform incremental sync
        let incremental_sync_start = tokio::time::Instant::now();
        let incremental_sync_results = acl_manager.incremental_sync_to_brokers().await.unwrap();
        let incremental_sync_time = incremental_sync_start.elapsed();
        
        assert!(incremental_sync_results.iter().all(|result| result.success));
        assert!(incremental_sync_time < Duration::from_secs(1)); // Should be faster than full sync
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Verify incremental rule is enforced on all brokers
        for broker_index in 0..cluster.brokers.len() {
            let client = cluster.create_test_client("incremental-client").await.unwrap();
            let connection = client.connect_to_broker(broker_index).await.unwrap();
            
            assert!(connection.authorize("incremental.test", Permission::Read).await.unwrap(),
                   "Incremental rule not synchronized to broker {}", broker_index);
        }
        
        // Test selective synchronization (specific brokers)
        let selective_rule = AclRule {
            principal: "selective-client".to_string(),
            resource_pattern: ResourcePattern::Topic("selective.*".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        let selective_rule_id = acl_manager.add_rule(selective_rule.clone()).await.unwrap();
        
        // Sync only to first two brokers
        let target_brokers = vec![
            cluster.brokers[0].config.broker_id.clone(),
            cluster.brokers[1].config.broker_id.clone(),
        ];
        
        let selective_sync_results = acl_manager.sync_to_specific_brokers(&target_brokers).await.unwrap();
        assert_eq!(selective_sync_results.len(), 2);
        assert!(selective_sync_results.iter().all(|result| result.success));
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Verify selective synchronization
        for (broker_index, broker) in cluster.brokers.iter().enumerate() {
            let client = cluster.create_test_client("selective-client").await.unwrap();
            let connection = client.connect_to_broker(broker_index).await.unwrap();
            
            let has_permission = connection.authorize("selective.test", Permission::Write).await.unwrap();
            
            if broker_index < 2 {
                assert!(has_permission, "Rule should be synchronized to broker {}", broker_index);
            } else {
                assert!(!has_permission, "Rule should not be synchronized to broker {}", broker_index);
            }
        }
        
        // Test synchronization failure handling
        let failing_broker_id = "non-existent-broker".to_string();
        let failure_sync_results = acl_manager.sync_to_specific_brokers(&vec![failing_broker_id]).await.unwrap();
        assert_eq!(failure_sync_results.len(), 1);
        assert!(!failure_sync_results[0].success);
        
        // Test synchronization with broker restart
        let restart_broker_index = 0;
        let restart_broker = &mut cluster.brokers[restart_broker_index];
        
        // Simulate broker restart
        restart_broker.shutdown().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        restart_broker.start().await.unwrap();
        
        // Perform resynchronization after restart
        let resync_results = acl_manager.resync_after_broker_restart(&restart_broker.config.broker_id).await.unwrap();
        assert!(resync_results.success);
        
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Verify all rules are present after restart
        let client_after_restart = cluster.create_test_client("sync-producer").await.unwrap();
        let connection_after_restart = client_after_restart.connect_to_broker(restart_broker_index).await.unwrap();
        
        assert!(connection_after_restart.authorize("sync.events.test", Permission::Write).await.unwrap());
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 4: Real-time ACL rule updates without service interruption
    #[tokio::test]
    async fn test_realtime_acl_updates_no_interruption() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let acl_manager = cluster.acl_manager.clone();
        
        // Setup initial ACL rules
        let initial_rule = AclRule {
            principal: "realtime-client".to_string(),
            resource_pattern: ResourcePattern::Topic("realtime.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let initial_rule_id = acl_manager.add_rule(initial_rule.clone()).await.unwrap();
        
        // Distribute to brokers
        acl_manager.synchronize_acl_to_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Establish active connections
        let client = cluster.create_test_client("realtime-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // Verify initial access
        assert!(connection.authorize("realtime.topic1", Permission::Read).await.unwrap());
        
        // Start continuous operation simulation
        let operation_handle = {
            let connection_clone = connection.clone();
            tokio::spawn(async move {
                let mut operation_count = 0;
                let mut successful_operations = 0;
                
                for i in 0..1000 {
                    let topic = format!("realtime.operation.{}", i);
                    
                    match connection_clone.authorize(&topic, Permission::Read).await {
                        Ok(authorized) => {
                            operation_count += 1;
                            if authorized {
                                successful_operations += 1;
                            }
                        }
                        Err(_) => {
                            // Count errors but continue
                            operation_count += 1;
                        }
                    }
                    
                    // Simulate realistic operation frequency
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                
                (operation_count, successful_operations)
            })
        };
        
        // Perform real-time ACL updates while operations are running
        tokio::time::sleep(Duration::from_millis(50)).await; // Let operations start
        
        // Update 1: Add write permission
        let updated_rule_1 = AclRule {
            principal: "realtime-client".to_string(),
            resource_pattern: ResourcePattern::Topic("realtime.*".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        let update_1_start = tokio::time::Instant::now();
        let update_1_id = acl_manager.add_rule(updated_rule_1.clone()).await.unwrap();
        let update_1_propagation = acl_manager.hot_update_brokers().await.unwrap();
        let update_1_time = update_1_start.elapsed();
        
        assert!(update_1_propagation.iter().all(|result| result.success));
        assert!(update_1_time < Duration::from_millis(100)); // Hot update should be fast
        
        tokio::time::sleep(Duration::from_millis(20)).await; // Allow cache invalidation
        
        // Verify write permission is immediately available
        assert!(connection.authorize("realtime.write_test", Permission::Write).await.unwrap());
        
        // Update 2: Modify resource pattern
        let updated_rule_2 = AclRule {
            principal: "realtime-client".to_string(),
            resource_pattern: ResourcePattern::Topic("realtime.restricted.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let update_2_start = tokio::time::Instant::now();
        let update_2_result = acl_manager.update_rule(&initial_rule_id, updated_rule_2.clone()).await.unwrap();
        let update_2_propagation = acl_manager.hot_update_brokers().await.unwrap();
        let update_2_time = update_2_start.elapsed();
        
        assert!(update_2_result.success);
        assert!(update_2_propagation.iter().all(|result| result.success));
        assert!(update_2_time < Duration::from_millis(100));
        
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // Verify pattern change is enforced
        assert!(connection.authorize("realtime.restricted.data", Permission::Read).await.unwrap());
        assert!(!connection.authorize("realtime.general.data", Permission::Read).await.unwrap());
        
        // Update 3: Add deny rule (should take precedence)
        let deny_rule = AclRule {
            principal: "realtime-client".to_string(),
            resource_pattern: ResourcePattern::Topic("realtime.restricted.sensitive.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Deny,
        };
        
        let deny_rule_id = acl_manager.add_rule(deny_rule.clone()).await.unwrap();
        acl_manager.hot_update_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // Verify deny rule takes precedence
        assert!(!connection.authorize("realtime.restricted.sensitive.data", Permission::Read).await.unwrap());
        assert!(connection.authorize("realtime.restricted.normal.data", Permission::Read).await.unwrap());
        
        // Update 4: Bulk rule update
        let bulk_rules = vec![
            AclRule {
                principal: "realtime-client".to_string(),
                resource_pattern: ResourcePattern::Topic("bulk.topic1.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "realtime-client".to_string(),
                resource_pattern: ResourcePattern::Topic("bulk.topic2.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
        ];
        
        let bulk_update_start = tokio::time::Instant::now();
        let bulk_update_results = acl_manager.bulk_add_rules(bulk_rules.clone()).await.unwrap();
        let bulk_propagation = acl_manager.hot_update_brokers().await.unwrap();
        let bulk_update_time = bulk_update_start.elapsed();
        
        assert_eq!(bulk_update_results.len(), 2);
        assert!(bulk_update_results.iter().all(|result| result.success));
        assert!(bulk_update_time < Duration::from_millis(200));
        
        tokio::time::sleep(Duration::from_millis(30)).await;
        
        // Verify bulk updates are enforced
        assert!(connection.authorize("bulk.topic1.test", Permission::Read).await.unwrap());
        assert!(connection.authorize("bulk.topic2.test", Permission::Write).await.unwrap());
        assert!(!connection.authorize("bulk.topic1.test", Permission::Write).await.unwrap());
        
        // Wait for continuous operations to complete
        let (total_operations, successful_operations) = operation_handle.await.unwrap();
        
        // Verify service continuity during updates
        assert!(total_operations > 500, "Not enough operations completed");
        let success_rate = successful_operations as f64 / total_operations as f64;
        assert!(success_rate > 0.7, "Success rate too low during updates: {:.2}", success_rate);
        
        // Test update rollback capability
        let rollback_result = acl_manager.rollback_rule_update(&initial_rule_id).await.unwrap();
        assert!(rollback_result.success);
        
        acl_manager.hot_update_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // Verify rollback is enforced
        assert!(connection.authorize("realtime.general.data", Permission::Read).await.unwrap());
        assert!(!connection.authorize("realtime.restricted.normal.data", Permission::Read).await.unwrap());
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 5: ACL conflict resolution and precedence handling
    #[tokio::test]
    async fn test_acl_conflict_resolution_and_precedence() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let acl_manager = cluster.acl_manager.clone();
        
        // Create conflicting ACL rules to test precedence
        let conflicting_rules = vec![
            // General allow rule
            AclRule {
                principal: "conflict-client".to_string(),
                resource_pattern: ResourcePattern::Topic("*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            // Specific deny rule (should take precedence)
            AclRule {
                principal: "conflict-client".to_string(),
                resource_pattern: ResourcePattern::Topic("restricted.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
            // More specific allow rule (should override deny)
            AclRule {
                principal: "conflict-client".to_string(),
                resource_pattern: ResourcePattern::Topic("restricted.public.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            // Most specific deny rule (should take final precedence)
            AclRule {
                principal: "conflict-client".to_string(),
                resource_pattern: ResourcePattern::Topic("restricted.public.sensitive.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
        ];
        
        // Add rules with different precedence levels
        let mut rule_ids = Vec::new();
        for (index, rule) in conflicting_rules.iter().enumerate() {
            let rule_id = acl_manager.add_rule_with_precedence(
                rule.clone(),
                index as u32, // Higher index = higher precedence
            ).await.unwrap();
            rule_ids.push(rule_id);
        }
        
        // Distribute rules to brokers
        acl_manager.synchronize_acl_to_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let client = cluster.create_test_client("conflict-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // Test precedence resolution
        let precedence_tests = vec![
            // General topics should be allowed (rule 0)
            ("general.topic", Permission::Read, true),
            ("public.data", Permission::Read, true),
            
            // Restricted topics should be denied (rule 1 overrides rule 0)
            ("restricted.data", Permission::Read, false),
            ("restricted.private", Permission::Read, false),
            
            // Restricted public topics should be allowed (rule 2 overrides rule 1)
            ("restricted.public.data", Permission::Read, true),
            ("restricted.public.info", Permission::Read, true),
            
            // Restricted public sensitive topics should be denied (rule 3 overrides rule 2)
            ("restricted.public.sensitive.data", Permission::Read, false),
            ("restricted.public.sensitive.passwords", Permission::Read, false),
        ];
        
        for (topic, operation, expected_result) in precedence_tests {
            let actual_result = connection.authorize(topic, operation).await.unwrap();
            assert_eq!(actual_result, expected_result,
                      "Precedence resolution failed for {}: expected {}, got {}", 
                      topic, expected_result, actual_result);
        }
        
        // Test conflict resolution with group-based rules
        let group_rules = vec![
            AclRule {
                principal: "group:users".to_string(),
                resource_pattern: ResourcePattern::Topic("shared.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "conflict-client".to_string(), // User-specific rule
                resource_pattern: ResourcePattern::Topic("shared.private.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Deny,
            },
        ];
        
        // Add user to group
        acl_manager.add_user_to_group("conflict-client", "users").await.unwrap();
        
        // Add group rules
        for rule in group_rules {
            acl_manager.add_rule(rule).await.unwrap();
        }
        
        acl_manager.synchronize_acl_to_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Test group vs user-specific rule precedence
        // User-specific rules should take precedence over group rules
        assert!(connection.authorize("shared.public", Permission::Read).await.unwrap()); // Group rule
        assert!(!connection.authorize("shared.private.data", Permission::Read).await.unwrap()); // User-specific deny overrides group allow
        
        // Test time-based rule precedence (newer rules override older ones with same specificity)
        let time_based_rule_1 = AclRule {
            principal: "time-test-client".to_string(),
            resource_pattern: ResourcePattern::Topic("time.test.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let rule_1_id = acl_manager.add_rule(time_based_rule_1.clone()).await.unwrap();
        
        // Wait a moment to ensure different timestamps
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        let time_based_rule_2 = AclRule {
            principal: "time-test-client".to_string(),
            resource_pattern: ResourcePattern::Topic("time.test.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Deny,
        };
        
        let rule_2_id = acl_manager.add_rule(time_based_rule_2.clone()).await.unwrap();
        
        acl_manager.synchronize_acl_to_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let time_client = cluster.create_test_client("time-test-client").await.unwrap();
        let time_connection = time_client.connect_to_broker(0).await.unwrap();
        
        // Newer deny rule should take precedence
        assert!(!time_connection.authorize("time.test.data", Permission::Read).await.unwrap());
        
        // Test rule precedence with wildcards
        let wildcard_rules = vec![
            AclRule {
                principal: "*".to_string(), // Any user
                resource_pattern: ResourcePattern::Topic("public.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "wildcard-client".to_string(), // Specific user
                resource_pattern: ResourcePattern::Topic("*".to_string()), // Any topic
                operation: Permission::Read,
                effect: Effect::Deny,
            },
        ];
        
        for rule in wildcard_rules {
            acl_manager.add_rule(rule).await.unwrap();
        }
        
        acl_manager.synchronize_acl_to_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let wildcard_client = cluster.create_test_client("wildcard-client").await.unwrap();
        let wildcard_connection = wildcard_client.connect_to_broker(0).await.unwrap();
        
        // Specific user rule should override wildcard principal rule
        assert!(!wildcard_connection.authorize("public.data", Permission::Read).await.unwrap());
        
        // Test conflict resolution audit logging
        let conflict_audit = acl_manager.get_conflict_resolution_audit().await.unwrap();
        assert!(!conflict_audit.is_empty());
        
        for audit_entry in &conflict_audit {
            assert!(!audit_entry.principal.is_empty());
            assert!(!audit_entry.resource.is_empty());
            assert!(!audit_entry.conflicting_rules.is_empty());
            assert!(!audit_entry.resolution_rule_id.is_empty());
            assert!(!audit_entry.resolution_reason.is_empty());
        }
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 6: ACL audit trail and change tracking
    #[tokio::test]
    async fn test_acl_audit_trail_and_change_tracking() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let acl_manager = cluster.acl_manager.clone();
        
        // Enable comprehensive audit logging
        acl_manager.configure_audit_logging(true, true, true).await.unwrap(); // Enable all audit types
        
        // Test audit logging for rule creation
        let audit_rule_1 = AclRule {
            principal: "audit-client-1".to_string(),
            resource_pattern: ResourcePattern::Topic("audit.topic1.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let create_time_1 = SystemTime::now();
        let rule_1_id = acl_manager.add_rule_with_audit(
            audit_rule_1.clone(),
            "test-admin".to_string(),
            "Initial rule creation for audit testing".to_string(),
        ).await.unwrap();
        
        // Test audit logging for rule modification
        let modified_rule_1 = AclRule {
            principal: "audit-client-1".to_string(),
            resource_pattern: ResourcePattern::Topic("audit.topic1.restricted.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let modify_time_1 = SystemTime::now();
        let modify_result = acl_manager.update_rule_with_audit(
            &rule_1_id,
            modified_rule_1.clone(),
            "test-admin".to_string(),
            "Restricted scope of audit rule".to_string(),
        ).await.unwrap();
        assert!(modify_result.success);
        
        // Test audit logging for rule deletion
        let audit_rule_2 = AclRule {
            principal: "temporary-client".to_string(),
            resource_pattern: ResourcePattern::Topic("temp.*".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        let rule_2_id = acl_manager.add_rule_with_audit(
            audit_rule_2.clone(),
            "test-admin".to_string(),
            "Temporary rule for deletion testing".to_string(),
        ).await.unwrap();
        
        let delete_time = SystemTime::now();
        let delete_result = acl_manager.delete_rule_with_audit(
            &rule_2_id,
            "test-admin".to_string(),
            "Removing temporary rule after testing".to_string(),
        ).await.unwrap();
        assert!(delete_result.success);
        
        // Test bulk operations audit
        let bulk_rules = vec![
            AclRule {
                principal: "bulk-client-1".to_string(),
                resource_pattern: ResourcePattern::Topic("bulk.data1.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "bulk-client-2".to_string(),
                resource_pattern: ResourcePattern::Topic("bulk.data2.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
        ];
        
        let bulk_time = SystemTime::now();
        let bulk_results = acl_manager.bulk_add_rules_with_audit(
            bulk_rules.clone(),
            "test-admin".to_string(),
            "Bulk rule creation for performance testing".to_string(),
        ).await.unwrap();
        assert_eq!(bulk_results.len(), 2);
        
        // Retrieve and verify audit trail
        let audit_entries = acl_manager.get_audit_trail().await.unwrap();
        assert!(audit_entries.len() >= 4); // At least 4 operations recorded
        
        // Verify rule creation audit
        let create_entry = audit_entries.iter()
            .find(|entry| entry.operation_type == "create" && entry.rule_id == rule_1_id)
            .expect("Create audit entry not found");
        
        assert_eq!(create_entry.principal, "audit-client-1");
        assert_eq!(create_entry.operator, "test-admin");
        assert_eq!(create_entry.description, "Initial rule creation for audit testing");
        assert!(create_entry.timestamp >= create_time_1);
        assert!(create_entry.timestamp <= SystemTime::now());
        
        // Verify rule modification audit
        let modify_entry = audit_entries.iter()
            .find(|entry| entry.operation_type == "update" && entry.rule_id == rule_1_id)
            .expect("Modify audit entry not found");
        
        assert_eq!(modify_entry.operator, "test-admin");
        assert_eq!(modify_entry.description, "Restricted scope of audit rule");
        assert!(modify_entry.timestamp >= modify_time_1);
        assert!(!modify_entry.old_rule_json.is_empty());
        assert!(!modify_entry.new_rule_json.is_empty());
        assert_ne!(modify_entry.old_rule_json, modify_entry.new_rule_json);
        
        // Verify rule deletion audit
        let delete_entry = audit_entries.iter()
            .find(|entry| entry.operation_type == "delete" && entry.rule_id == rule_2_id)
            .expect("Delete audit entry not found");
        
        assert_eq!(delete_entry.principal, "temporary-client");
        assert_eq!(delete_entry.operator, "test-admin");
        assert_eq!(delete_entry.description, "Removing temporary rule after testing");
        assert!(delete_entry.timestamp >= delete_time);
        
        // Verify bulk operations audit
        let bulk_entries: Vec<_> = audit_entries.iter()
            .filter(|entry| entry.operation_type == "bulk_create")
            .collect();
        assert_eq!(bulk_entries.len(), 2);
        
        for bulk_entry in bulk_entries {
            assert_eq!(bulk_entry.operator, "test-admin");
            assert_eq!(bulk_entry.description, "Bulk rule creation for performance testing");
            assert!(bulk_entry.timestamp >= bulk_time);
        }
        
        // Test audit trail filtering and querying
        let filtered_by_principal = acl_manager.get_audit_trail_for_principal("audit-client-1").await.unwrap();
        assert!(filtered_by_principal.len() >= 2); // Create and update operations
        
        let filtered_by_operator = acl_manager.get_audit_trail_for_operator("test-admin").await.unwrap();
        assert!(filtered_by_operator.len() >= 4); // All operations performed by test-admin
        
        let filtered_by_time_range = acl_manager.get_audit_trail_in_time_range(
            create_time_1,
            SystemTime::now(),
        ).await.unwrap();
        assert!(filtered_by_time_range.len() >= 4);
        
        // Test change tracking and rule history
        let rule_history = acl_manager.get_rule_change_history(&rule_1_id).await.unwrap();
        assert_eq!(rule_history.len(), 2); // Create and update
        
        assert_eq!(rule_history[0].operation_type, "create");
        assert_eq!(rule_history[1].operation_type, "update");
        
        // Verify rule history shows progression
        let original_rule = serde_json::from_str::<AclRule>(&rule_history[0].rule_json).unwrap();
        let modified_rule = serde_json::from_str::<AclRule>(&rule_history[1].rule_json).unwrap();
        
        assert_eq!(original_rule.resource_pattern, ResourcePattern::Topic("audit.topic1.*".to_string()));
        assert_eq!(modified_rule.resource_pattern, ResourcePattern::Topic("audit.topic1.restricted.*".to_string()));
        
        // Test audit log export and backup
        let audit_export = acl_manager.export_audit_log(
            create_time_1,
            SystemTime::now(),
            "json".to_string(),
        ).await.unwrap();
        
        assert!(audit_export.success);
        assert!(!audit_export.export_path.is_empty());
        assert!(audit_export.entry_count >= 4);
        assert!(audit_export.file_size > 0);
        
        // Test audit log integrity verification
        let integrity_check = acl_manager.verify_audit_log_integrity().await.unwrap();
        assert!(integrity_check.is_valid);
        assert!(integrity_check.entry_count >= 4);
        assert!(integrity_check.checksum_valid);
        assert!(integrity_check.signature_valid);
        
        // Test audit log retention and cleanup
        let retention_config = acl_manager.configure_audit_retention(
            Duration::from_secs(90 * 24 * 60 * 60), // 90 days
            1000000, // 1M entries max
        ).await.unwrap();
        assert!(retention_config.success);
        
        let cleanup_result = acl_manager.cleanup_old_audit_entries().await.unwrap();
        assert!(cleanup_result.success);
        assert!(cleanup_result.entries_removed >= 0);
        
        // Test audit alerting for sensitive operations
        let sensitive_rule = AclRule {
            principal: "admin-escalation".to_string(),
            resource_pattern: ResourcePattern::Cluster,
            operation: Permission::ClusterAdmin,
            effect: Effect::Allow,
        };
        
        let alert_rule_id = acl_manager.add_rule_with_audit(
            sensitive_rule.clone(),
            "security-admin".to_string(),
            "Emergency admin access escalation".to_string(),
        ).await.unwrap();
        
        // Check if alert was triggered for sensitive operation
        let alerts = acl_manager.get_security_alerts().await.unwrap();
        let escalation_alert = alerts.iter()
            .find(|alert| alert.rule_id == alert_rule_id && alert.alert_type == "privilege_escalation");
        
        assert!(escalation_alert.is_some(), "Security alert should be triggered for admin escalation");
        
        cluster.shutdown().await.unwrap();
    }
}

#[cfg(test)]
mod acl_performance_tests {
    use super::*;
    
    /// Performance test: ACL operations under load
    #[tokio::test]
    async fn test_acl_operations_performance() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let acl_manager = cluster.acl_manager.clone();
        
        // Test ACL rule creation performance
        let rule_count = 1000;
        let creation_start = tokio::time::Instant::now();
        
        let mut rule_ids = Vec::new();
        for i in 0..rule_count {
            let rule = AclRule {
                principal: format!("perf-client-{}", i),
                resource_pattern: ResourcePattern::Topic(format!("perf.topic.{}.*", i)),
                operation: if i % 2 == 0 { Permission::Read } else { Permission::Write },
                effect: Effect::Allow,
            };
            
            let rule_id = acl_manager.add_rule(rule).await.unwrap();
            rule_ids.push(rule_id);
        }
        
        let creation_time = creation_start.elapsed();
        let avg_creation_time = creation_time / rule_count as u32;
        
        // Performance targets
        assert!(avg_creation_time < Duration::from_millis(5), 
               "Average ACL rule creation should be < 5ms, got {:?}", avg_creation_time);
        
        // Test ACL rule synchronization performance
        let sync_start = tokio::time::Instant::now();
        let sync_results = acl_manager.synchronize_acl_to_brokers().await.unwrap();
        let sync_time = sync_start.elapsed();
        
        assert!(sync_results.iter().all(|result| result.success));
        assert!(sync_time < Duration::from_secs(10), 
               "ACL synchronization should complete < 10s for {} rules, got {:?}", rule_count, sync_time);
        
        // Test ACL rule query performance
        let query_start = tokio::time::Instant::now();
        
        for i in 0..100 { // Test subset for query performance
            let principal = format!("perf-client-{}", i);
            let rules = acl_manager.get_rules_for_principal(&principal).await.unwrap();
            assert_eq!(rules.len(), 1);
        }
        
        let query_time = query_start.elapsed();
        let avg_query_time = query_time / 100;
        
        assert!(avg_query_time < Duration::from_millis(1), 
               "Average ACL rule query should be < 1ms, got {:?}", avg_query_time);
        
        println!("ACL performance test results:");
        println!("  Rule count: {}", rule_count);
        println!("  Average creation time: {:?}", avg_creation_time);
        println!("  Synchronization time: {:?}", sync_time);
        println!("  Average query time: {:?}", avg_query_time);
        
        cluster.shutdown().await.unwrap();
    }
}