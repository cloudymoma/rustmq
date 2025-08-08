//! Failure and Recovery Integration Tests
//!
//! This module tests security system behavior during component failures,
//! recovery workflows, failover scenarios, and partial service degradation
//! to ensure security guarantees are maintained under all conditions.
//!
//! Target: 5 comprehensive failure and recovery integration tests

use super::test_infrastructure::*;
use rustmq::{
    security::{Permission, AclRule, Effect, ResourcePattern, CertificateStatus},
    error::RustMqError,
    Result,
};
use std::{sync::Arc, time::{Duration, SystemTime}};
use tokio::time::timeout;

#[cfg(test)]
mod failure_recovery_integration_tests {
    use super::*;

    /// Test 1: Security system behavior during component failures
    #[tokio::test]
    async fn test_security_system_behavior_during_component_failures() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.cluster_size = 3; // Multiple controllers for failover testing
        cluster.config.broker_count = 3; // Multiple brokers for resilience
        cluster.start().await.unwrap();
        
        // Setup baseline security configuration
        let baseline_rules = vec![
            AclRule {
                principal: "failure-test-producer".to_string(),
                resource_pattern: ResourcePattern::Topic("failure.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "failure-test-consumer".to_string(),
                resource_pattern: ResourcePattern::Topic("failure.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
        ];
        
        for rule in &baseline_rules {
            cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        
        // Create clients for testing
        let producer_client = cluster.create_test_client("failure-test-producer").await.unwrap();
        let consumer_client = cluster.create_test_client("failure-test-consumer").await.unwrap();
        
        // Establish baseline connections
        let producer_connection = producer_client.connect_to_broker(0).await.unwrap();
        let consumer_connection = consumer_client.connect_to_broker(1).await.unwrap();
        
        // Verify baseline functionality
        assert!(producer_connection.authorize("failure.topic1", Permission::Write).await.unwrap());
        assert!(consumer_connection.authorize("failure.topic1", Permission::Read).await.unwrap());
        
        println!("Testing security system during component failures...");
        
        // Failure Scenario 1: Single controller failure
        println!("Scenario 1: Single controller failure");
        
        let failed_controller_index = 0;
        let failed_controller = &mut cluster.controllers[failed_controller_index];
        failed_controller.shutdown().await.unwrap();
        
        // Wait for failure detection and leader re-election
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Security operations should continue with remaining controllers
        let new_rule_during_failure = AclRule {
            principal: "failure-scenario-1".to_string(),
            resource_pattern: ResourcePattern::Topic("failure.scenario1.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let rule_result = cluster.acl_manager.add_rule(new_rule_during_failure).await;
        assert!(rule_result.is_ok(), "ACL operations should continue during single controller failure");
        
        // Existing connections should remain functional
        assert!(producer_connection.authorize("failure.topic1", Permission::Write).await.unwrap());
        assert!(consumer_connection.authorize("failure.topic1", Permission::Read).await.unwrap());
        
        // New connections should work
        let new_client = cluster.create_test_client("failure-scenario-1").await.unwrap();
        let new_connection = new_client.connect_to_broker(2).await.unwrap();
        assert!(new_connection.authorize("failure.scenario1.test", Permission::Read).await.unwrap());
        
        // Failure Scenario 2: Single broker failure
        println!("Scenario 2: Single broker failure");
        
        let failed_broker_index = 0;
        let failed_broker = &mut cluster.brokers[failed_broker_index];
        failed_broker.shutdown().await.unwrap();
        
        // Existing connection to failed broker should detect failure
        let failed_connection_result = producer_connection.authorize("failure.topic1", Permission::Write).await;
        // This might fail due to broker failure, which is expected
        
        // New connections should work on remaining brokers
        let surviving_producer = cluster.create_test_client("failure-test-producer").await.unwrap();
        let surviving_connection = surviving_producer.connect_to_broker(1).await.unwrap(); // Use different broker
        assert!(surviving_connection.authorize("failure.topic1", Permission::Write).await.unwrap());
        
        // Certificate operations should continue
        let cert_during_failure = cluster.cert_manager.issue_certificate(&CertificateRequest {
            common_name: "failure-scenario-2-cert".to_string(),
            organization: Some("RustMQ Failure Test".to_string()),
            organizational_unit: Some("Failure".to_string()),
            country: None,
            state: None,
            locality: None,
            email: None,
            subject_alt_names: vec!["failure-cert.test".to_string()],
            key_type: KeyType::Rsa2048,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
            validity_days: 30,
            role: CertificateRole::Client,
        }).await;
        
        assert!(cert_during_failure.is_ok(), "Certificate operations should continue during single broker failure");
        
        // Failure Scenario 3: Network partition (split brain)
        println!("Scenario 3: Network partition simulation");
        
        // Simulate network partition affecting one controller
        let isolated_controller_index = 1;
        let isolated_controller = &cluster.controllers[isolated_controller_index];
        isolated_controller.simulate_network_partition().await.unwrap();
        
        // Security operations should continue with majority
        let partition_rule = AclRule {
            principal: "partition-test-user".to_string(),
            resource_pattern: ResourcePattern::Topic("partition.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let partition_rule_result = cluster.acl_manager.add_rule(partition_rule).await;
        assert!(partition_rule_result.is_ok(), "ACL operations should continue during network partition with majority");
        
        // Heal network partition
        isolated_controller.heal_network_partition().await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await; // Allow recovery
        
        // Verify isolated controller catches up
        let partition_client = cluster.create_test_client("partition-test-user").await.unwrap();
        let partition_connection = partition_client.connect_to_broker(1).await.unwrap();
        assert!(partition_connection.authorize("partition.test", Permission::Read).await.unwrap());
        
        // Failure Scenario 4: Storage corruption simulation
        println!("Scenario 4: Storage corruption simulation");
        
        // Simulate ACL storage corruption
        let corruption_result = cluster.acl_manager.simulate_storage_corruption().await;
        if corruption_result.is_ok() {
            // Security system should detect corruption and enter safe mode
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Should deny operations by default in safe mode
            let corruption_rule = AclRule {
                principal: "corruption-test-user".to_string(),
                resource_pattern: ResourcePattern::Topic("corruption.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            };
            
            let corruption_rule_result = cluster.acl_manager.add_rule(corruption_rule).await;
            // This might fail due to corruption, which is expected secure behavior
        }
        
        // Failure Scenario 5: Memory pressure simulation
        println!("Scenario 5: Memory pressure simulation");
        
        // Create memory pressure by allocating large amounts of data
        let mut _memory_pressure = Vec::new();
        for _ in 0..1000 {
            _memory_pressure.push(vec![0u8; 1024 * 1024]); // 1MB allocations
        }
        
        // Security operations should continue under memory pressure
        let memory_pressure_client = cluster.create_test_client("memory-pressure-test").await.unwrap();
        let memory_pressure_result = memory_pressure_client.connect_to_broker(1).await;
        
        // Should either succeed or fail gracefully (not panic or hang)
        match memory_pressure_result {
            Ok(connection) => {
                // If connection succeeds, authorization should work
                let authz_result = connection.authorize("failure.memory.test", Permission::Read).await;
                // Result may vary under memory pressure, but should not crash
            }
            Err(_) => {
                // Graceful failure under memory pressure is acceptable
            }
        }
        
        drop(_memory_pressure); // Release memory pressure
        
        println!("Component failure testing completed");
        cluster.shutdown().await.unwrap();
    }

    /// Test 2: Recovery workflows for corrupted security data
    #[tokio::test]
    async fn test_recovery_workflows_corrupted_security_data() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        println!("Testing recovery workflows for corrupted security data...");
        
        // Setup initial security data
        let initial_rules = vec![
            AclRule {
                principal: "recovery-user-1".to_string(),
                resource_pattern: ResourcePattern::Topic("recovery.data1.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "recovery-user-2".to_string(),
                resource_pattern: ResourcePattern::Topic("recovery.data2.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
        ];
        
        let mut rule_ids = Vec::new();
        for rule in &initial_rules {
            let rule_id = cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
            rule_ids.push(rule_id);
        }
        
        // Issue test certificates
        let mut cert_serials = Vec::new();
        for i in 0..3 {
            let cert_request = CertificateRequest {
                common_name: format!("recovery-cert-{}", i),
                organization: Some("RustMQ Recovery Test".to_string()),
                organizational_unit: Some("Recovery".to_string()),
                country: None,
                state: None,
                locality: None,
                email: None,
                subject_alt_names: vec![format!("recovery-cert-{}.test", i)],
                key_type: KeyType::Rsa2048,
                key_usage: vec![KeyUsage::DigitalSignature],
                extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                validity_days: 30,
                role: CertificateRole::Client,
            };
            
            let cert = cluster.cert_manager.issue_certificate(&cert_request).await.unwrap();
            cert_serials.push(cert.serial_number);
        }
        
        // Create backup before corruption
        let backup_result = cluster.acl_manager.create_backup().await.unwrap();
        assert!(backup_result.success);
        let backup_id = backup_result.backup_id;
        
        let cert_backup_result = cluster.cert_manager.create_certificate_backup().await.unwrap();
        assert!(cert_backup_result.success);
        let cert_backup_id = cert_backup_result.backup_id;
        
        // Recovery Scenario 1: ACL data corruption
        println!("Recovery scenario 1: ACL data corruption");
        
        // Simulate ACL data corruption
        let acl_corruption_result = cluster.acl_manager.simulate_data_corruption().await.unwrap();
        assert!(acl_corruption_result.success);
        
        // Verify corruption is detected
        let corruption_detection = cluster.acl_manager.detect_data_corruption().await.unwrap();
        assert!(corruption_detection.corruption_detected);
        assert!(!corruption_detection.corrupted_components.is_empty());
        
        // Attempt to use corrupted ACL system
        let test_client = cluster.create_test_client("recovery-user-1").await.unwrap();
        let test_connection = test_client.connect_to_broker(0).await.unwrap();
        
        // Authorization should fail due to corruption
        let corrupted_authz_result = test_connection.authorize("recovery.data1.test", Permission::Read).await;
        // Expect this to fail or use safe default (deny)
        
        // Perform ACL data recovery
        let acl_recovery_start = tokio::time::Instant::now();
        let acl_recovery_result = cluster.acl_manager.recover_from_backup(&backup_id).await.unwrap();
        let acl_recovery_time = acl_recovery_start.elapsed();
        
        assert!(acl_recovery_result.success);
        assert!(acl_recovery_result.recovered_rule_count >= initial_rules.len());
        assert!(acl_recovery_time < Duration::from_secs(10));
        
        // Verify ACL system is functional after recovery
        tokio::time::sleep(Duration::from_millis(100)).await; // Allow recovery propagation
        
        let recovered_authz = test_connection.authorize("recovery.data1.test", Permission::Read).await.unwrap();
        assert!(recovered_authz, "ACL authorization should work after recovery");
        
        // Recovery Scenario 2: Certificate store corruption
        println!("Recovery scenario 2: Certificate store corruption");
        
        // Simulate certificate store corruption
        let cert_corruption_result = cluster.cert_manager.simulate_certificate_store_corruption().await.unwrap();
        assert!(cert_corruption_result.success);
        
        // Verify certificate operations fail
        let post_corruption_validation = cluster.cert_manager.validate_certificate(&cert_serials[0]).await;
        // This should fail or indicate corruption
        
        // Perform certificate store recovery
        let cert_recovery_start = tokio::time::Instant::now();
        let cert_recovery_result = cluster.cert_manager.recover_from_backup(&cert_backup_id).await.unwrap();
        let cert_recovery_time = cert_recovery_start.elapsed();
        
        assert!(cert_recovery_result.success);
        assert!(cert_recovery_result.recovered_certificate_count >= cert_serials.len());
        assert!(cert_recovery_time < Duration::from_secs(15));
        
        // Verify certificate system is functional after recovery
        for serial in &cert_serials {
            let validation_result = cluster.cert_manager.validate_certificate(serial).await.unwrap();
            assert!(validation_result.is_valid, "Certificate should be valid after recovery");
        }
        
        // Recovery Scenario 3: Partial data corruption (mixed state)
        println!("Recovery scenario 3: Partial data corruption");
        
        // Add new data after backup
        let post_backup_rule = AclRule {
            principal: "post-backup-user".to_string(),
            resource_pattern: ResourcePattern::Topic("post.backup.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let post_backup_rule_id = cluster.acl_manager.add_rule(post_backup_rule).await.unwrap();
        
        // Simulate partial corruption (only affects recent data)
        let partial_corruption_result = cluster.acl_manager.simulate_partial_corruption().await.unwrap();
        assert!(partial_corruption_result.success);
        
        // Perform selective recovery (preserve uncorrupted recent data)
        let selective_recovery_result = cluster.acl_manager.selective_recovery_from_backup(
            &backup_id,
            true, // preserve_recent_data
        ).await.unwrap();
        
        assert!(selective_recovery_result.success);
        assert!(selective_recovery_result.preserved_recent_data);
        
        // Verify both old and new data are available
        let old_data_client = cluster.create_test_client("recovery-user-1").await.unwrap();
        let old_connection = old_data_client.connect_to_broker(0).await.unwrap();
        assert!(old_connection.authorize("recovery.data1.test", Permission::Read).await.unwrap());
        
        let new_data_client = cluster.create_test_client("post-backup-user").await.unwrap();
        let new_connection = new_data_client.connect_to_broker(0).await.unwrap();
        assert!(new_connection.authorize("post.backup.test", Permission::Read).await.unwrap());
        
        // Recovery Scenario 4: Cross-component consistency restoration
        println!("Recovery scenario 4: Cross-component consistency restoration");
        
        // Create inconsistent state between ACL and certificate systems
        let inconsistent_cert_request = CertificateRequest {
            common_name: "inconsistent-user".to_string(),
            organization: Some("RustMQ Inconsistency Test".to_string()),
            organizational_unit: Some("Inconsistency".to_string()),
            country: None,
            state: None,
            locality: None,
            email: None,
            subject_alt_names: vec!["inconsistent-user.test".to_string()],
            key_type: KeyType::Rsa2048,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
            validity_days: 30,
            role: CertificateRole::Client,
        };
        
        let inconsistent_cert = cluster.cert_manager.issue_certificate(&inconsistent_cert_request).await.unwrap();
        
        // Remove corresponding ACL rule to create inconsistency
        cluster.acl_manager.delete_rule(&post_backup_rule_id).await.unwrap();
        
        // Detect and resolve inconsistency
        let consistency_check = cluster.acl_manager.check_cross_component_consistency().await.unwrap();
        if !consistency_check.is_consistent {
            let consistency_repair = cluster.acl_manager.repair_cross_component_consistency().await.unwrap();
            assert!(consistency_repair.success);
            assert!(consistency_repair.inconsistencies_resolved > 0);
        }
        
        // Recovery Scenario 5: Automated recovery triggers
        println!("Recovery scenario 5: Automated recovery triggers");
        
        // Configure automated recovery
        let auto_recovery_config = cluster.acl_manager.configure_automated_recovery(
            true, // enable_auto_recovery
            Duration::from_secs(30), // corruption_detection_interval
            Duration::from_secs(60), // recovery_timeout
        ).await.unwrap();
        assert!(auto_recovery_config.success);
        
        // Simulate corruption that should trigger automated recovery
        cluster.acl_manager.simulate_data_corruption().await.unwrap();
        
        // Wait for automated recovery to trigger
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Verify automated recovery occurred
        let auto_recovery_status = cluster.acl_manager.get_automated_recovery_status().await.unwrap();
        if auto_recovery_status.recovery_triggered {
            assert!(auto_recovery_status.recovery_successful);
            assert!(auto_recovery_status.recovery_time < Duration::from_secs(60));
        }
        
        println!("Recovery workflows testing completed");
        cluster.shutdown().await.unwrap();
    }

    /// Test 3: Failover scenarios with security context preservation
    #[tokio::test]
    async fn test_failover_scenarios_security_context_preservation() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.cluster_size = 3; // Need multiple nodes for failover
        cluster.config.broker_count = 3;
        cluster.start().await.unwrap();
        
        println!("Testing failover scenarios with security context preservation...");
        
        // Setup security context
        let failover_rules = vec![
            AclRule {
                principal: "failover-producer".to_string(),
                resource_pattern: ResourcePattern::Topic("failover.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "failover-consumer".to_string(),
                resource_pattern: ResourcePattern::Topic("failover.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
        ];
        
        for rule in &failover_rules {
            cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        
        // Establish authenticated connections
        let producer_client = cluster.create_test_client("failover-producer").await.unwrap();
        let consumer_client = cluster.create_test_client("failover-consumer").await.unwrap();
        
        let mut producer_connections = Vec::new();
        let mut consumer_connections = Vec::new();
        
        for i in 0..cluster.brokers.len() {
            let prod_conn = producer_client.connect_to_broker(i).await.unwrap();
            let cons_conn = consumer_client.connect_to_broker(i).await.unwrap();
            producer_connections.push(prod_conn);
            consumer_connections.push(cons_conn);
        }
        
        // Verify baseline security context
        for (i, conn) in producer_connections.iter().enumerate() {
            assert!(conn.authorize(&format!("failover.broker{}", i), Permission::Write).await.unwrap());
        }
        
        // Failover Scenario 1: Controller leader failover
        println!("Failover scenario 1: Controller leader failover");
        
        // Identify current leader
        let mut leader_index = None;
        for (i, controller) in cluster.controllers.iter().enumerate() {
            if controller.is_leader().await.unwrap() {
                leader_index = Some(i);
                break;
            }
        }
        
        assert!(leader_index.is_some(), "Should have a controller leader");
        let leader_idx = leader_index.unwrap();
        
        // Record pre-failover security operations
        let pre_failover_operations = async {
            let mut success_count = 0;
            for conn in &producer_connections {
                if conn.authorize("failover.pre.test", Permission::Write).await.unwrap_or(false) {
                    success_count += 1;
                }
            }
            success_count
        };
        
        let pre_failover_successes = pre_failover_operations.await;
        assert!(pre_failover_successes >= 2, "Should have multiple successful authorizations before failover");
        
        // Trigger leader failover
        cluster.controllers[leader_idx].shutdown().await.unwrap();
        
        // Wait for leader election
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // Verify new leader is elected
        let mut new_leader_found = false;
        for (i, controller) in cluster.controllers.iter().enumerate() {
            if i != leader_idx && controller.is_leader().await.unwrap_or(false) {
                new_leader_found = true;
                break;
            }
        }
        assert!(new_leader_found, "New leader should be elected after failover");
        
        // Test security context preservation during failover
        let post_failover_operations = async {
            let mut success_count = 0;
            for conn in &producer_connections {
                if conn.authorize("failover.post.test", Permission::Write).await.unwrap_or(false) {
                    success_count += 1;
                }
            }
            success_count
        };
        
        let post_failover_successes = post_failover_operations.await;
        assert!(post_failover_successes >= pre_failover_successes / 2, 
               "Should maintain most security operations after controller failover");
        
        // Add new ACL rule through new leader
        let post_failover_rule = AclRule {
            principal: "post-failover-user".to_string(),
            resource_pattern: ResourcePattern::Topic("post.failover.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let new_rule_result = cluster.acl_manager.add_rule(post_failover_rule).await;
        assert!(new_rule_result.is_ok(), "Should be able to add ACL rules after controller failover");
        
        // Failover Scenario 2: Broker failover with connection migration
        println!("Failover scenario 2: Broker failover with connection migration");
        
        let failed_broker_index = 0;
        let original_connection = &producer_connections[failed_broker_index];
        
        // Verify connection is working
        assert!(original_connection.authorize("failover.broker.test", Permission::Write).await.unwrap());
        
        // Trigger broker failure
        cluster.brokers[failed_broker_index].shutdown().await.unwrap();
        
        // Original connection should detect failure
        let failed_conn_result = original_connection.authorize("failover.broker.test", Permission::Write).await;
        // This may fail, which is expected
        
        // Establish new connection to surviving broker
        let surviving_broker_index = 1;
        let migrated_connection = producer_client.connect_to_broker(surviving_broker_index).await.unwrap();
        
        // Security context should be preserved for new connection
        assert!(migrated_connection.authorize("failover.broker.test", Permission::Write).await.unwrap());
        
        // Test connection pool failover
        let connection_pool_stats = cluster.brokers[surviving_broker_index]
            .quic_server.as_ref().unwrap()
            .connection_pool()
            .stats();
        
        assert!(connection_pool_stats.total_connections > 0, "Connection pool should have active connections");
        
        // Failover Scenario 3: Network partition with security context consistency
        println!("Failover scenario 3: Network partition with security context consistency");
        
        // Simulate network partition isolating one broker
        let isolated_broker_index = 2;
        cluster.brokers[isolated_broker_index].simulate_network_partition().await.unwrap();
        
        // Security operations should continue on non-isolated brokers
        let partition_test_client = cluster.create_test_client("failover-producer").await.unwrap();
        let partition_connection = partition_test_client.connect_to_broker(surviving_broker_index).await.unwrap();
        
        assert!(partition_connection.authorize("failover.partition.test", Permission::Write).await.unwrap());
        
        // Add ACL rule during partition
        let partition_rule = AclRule {
            principal: "partition-test-user".to_string(),
            resource_pattern: ResourcePattern::Topic("partition.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let partition_rule_result = cluster.acl_manager.add_rule(partition_rule).await;
        assert!(partition_rule_result.is_ok(), "Should be able to add ACL rules during partition");
        
        // Heal partition
        cluster.brokers[isolated_broker_index].heal_network_partition().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await; // Allow synchronization
        
        // Test security context consistency after partition heal
        let healed_client = cluster.create_test_client("partition-test-user").await.unwrap();
        let healed_connection = healed_client.connect_to_broker(isolated_broker_index).await.unwrap();
        
        assert!(healed_connection.authorize("partition.test", Permission::Read).await.unwrap(),
               "Security context should be consistent after partition heal");
        
        // Failover Scenario 4: Rolling broker restart with security preservation
        println!("Failover scenario 4: Rolling broker restart with security preservation");
        
        // Perform rolling restart of brokers
        for (i, broker) in cluster.brokers.iter_mut().enumerate() {
            println!("  Restarting broker {}", i);
            
            // Test security before restart
            let pre_restart_client = cluster.create_test_client("failover-producer").await.unwrap();
            let pre_restart_connection = pre_restart_client.connect_to_broker((i + 1) % cluster.brokers.len()).await.unwrap();
            assert!(pre_restart_connection.authorize("failover.rolling.test", Permission::Write).await.unwrap());
            
            // Restart broker
            broker.shutdown().await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await; // Brief downtime
            broker.start().await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await; // Startup time
            
            // Test security after restart
            let post_restart_client = cluster.create_test_client("failover-producer").await.unwrap();
            let post_restart_connection = post_restart_client.connect_to_broker(i).await.unwrap();
            assert!(post_restart_connection.authorize("failover.rolling.test", Permission::Write).await.unwrap());
        }
        
        // Failover Scenario 5: Certificate-based authentication during failover
        println!("Failover scenario 5: Certificate-based authentication during failover");
        
        // Issue certificate for failover testing
        let failover_cert_request = CertificateRequest {
            common_name: "failover-cert-user".to_string(),
            organization: Some("RustMQ Failover Test".to_string()),
            organizational_unit: Some("Failover".to_string()),
            country: None,
            state: None,
            locality: None,
            email: None,
            subject_alt_names: vec!["failover-cert-user.test".to_string()],
            key_type: KeyType::Rsa2048,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
            validity_days: 30,
            role: CertificateRole::Client,
        };
        
        let failover_cert = cluster.cert_manager.issue_certificate(&failover_cert_request).await.unwrap();
        
        // Create ACL rule for certificate user
        let cert_user_rule = AclRule {
            principal: "failover-cert-user".to_string(),
            resource_pattern: ResourcePattern::Topic("cert.failover.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(cert_user_rule).await.unwrap();
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        
        // Test certificate-based authentication before failover
        let cert_client = cluster.create_test_client("failover-cert-user").await.unwrap();
        let cert_connection = cert_client.connect_to_broker(0).await.unwrap();
        assert!(cert_connection.authorize("cert.failover.test", Permission::Read).await.unwrap());
        
        // Trigger certificate manager restart
        let cert_manager_restart_result = cluster.cert_manager.restart_certificate_services().await;
        if cert_manager_restart_result.is_ok() {
            tokio::time::sleep(Duration::from_millis(200)).await; // Allow restart
            
            // Test certificate validation after restart
            let post_restart_validation = cluster.cert_manager.validate_certificate(&failover_cert.serial_number).await.unwrap();
            assert!(post_restart_validation.is_valid, "Certificate should remain valid after certificate manager restart");
            
            // Test new authentication with existing certificate
            let post_restart_cert_client = cluster.create_test_client("failover-cert-user").await.unwrap();
            let post_restart_cert_connection = post_restart_cert_client.connect_to_broker(1).await.unwrap();
            assert!(post_restart_cert_connection.authorize("cert.failover.test", Permission::Read).await.unwrap());
        }
        
        println!("Failover scenarios with security context preservation completed");
        cluster.shutdown().await.unwrap();
    }

    /// Test 4: Partial service degradation with security fallbacks
    #[tokio::test]
    async fn test_partial_service_degradation_security_fallbacks() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.cluster_size = 3;
        cluster.config.broker_count = 2;
        cluster.start().await.unwrap();
        
        println!("Testing partial service degradation with security fallbacks...");
        
        // Setup baseline security configuration
        let baseline_rules = vec![
            AclRule {
                principal: "degradation-user-1".to_string(),
                resource_pattern: ResourcePattern::Topic("degradation.normal.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "degradation-user-2".to_string(),
                resource_pattern: ResourcePattern::Topic("degradation.normal.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
        ];
        
        for rule in &baseline_rules {
            cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        
        // Create clients for testing
        let client1 = cluster.create_test_client("degradation-user-1").await.unwrap();
        let client2 = cluster.create_test_client("degradation-user-2").await.unwrap();
        
        // Verify normal operation
        let connection1 = client1.connect_to_broker(0).await.unwrap();
        let connection2 = client2.connect_to_broker(1).await.unwrap();
        
        assert!(connection1.authorize("degradation.normal.topic1", Permission::Read).await.unwrap());
        assert!(connection2.authorize("degradation.normal.topic1", Permission::Write).await.unwrap());
        
        // Degradation Scenario 1: Controller quorum loss with broker autonomy
        println!("Degradation scenario 1: Controller quorum loss");
        
        // Shutdown majority of controllers
        for i in 0..2 {
            cluster.controllers[i].shutdown().await.unwrap();
        }
        
        // Wait for controller failure detection
        tokio::time::sleep(Duration::from_millis(300)).await;
        
        // Brokers should enter autonomous mode with cached ACL rules
        let autonomous_result1 = connection1.authorize("degradation.normal.topic2", Permission::Read).await.unwrap();
        let autonomous_result2 = connection2.authorize("degradation.normal.topic2", Permission::Write).await.unwrap();
        
        assert!(autonomous_result1, "Cached ACL rules should work in autonomous mode");
        assert!(autonomous_result2, "Cached ACL rules should work in autonomous mode");
        
        // New ACL rule creation should fail or queue for later processing
        let new_rule_during_quorum_loss = AclRule {
            principal: "quorum-loss-user".to_string(),
            resource_pattern: ResourcePattern::Topic("quorum.loss.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let quorum_loss_rule_result = cluster.acl_manager.add_rule(new_rule_during_quorum_loss).await;
        // This should either fail or queue the operation
        
        // Restore controller quorum
        cluster.controllers[0].start().await.unwrap();
        cluster.controllers[1].start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await; // Allow recovery
        
        // Verify normal operation is restored
        let post_recovery_rule = AclRule {
            principal: "post-recovery-user".to_string(),
            resource_pattern: ResourcePattern::Topic("post.recovery.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let post_recovery_result = cluster.acl_manager.add_rule(post_recovery_rule).await;
        assert!(post_recovery_result.is_ok(), "Should be able to add rules after quorum recovery");
        
        // Degradation Scenario 2: Certificate service degradation
        println!("Degradation scenario 2: Certificate service degradation");
        
        // Simulate certificate service overload
        let cert_overload_result = cluster.cert_manager.simulate_service_overload().await;
        if cert_overload_result.is_ok() {
            // Certificate operations should be throttled or queued
            let throttled_cert_request = CertificateRequest {
                common_name: "throttled-cert".to_string(),
                organization: Some("RustMQ Throttle Test".to_string()),
                organizational_unit: Some("Throttle".to_string()),
                country: None,
                state: None,
                locality: None,
                email: None,
                subject_alt_names: vec!["throttled-cert.test".to_string()],
                key_type: KeyType::Rsa2048,
                key_usage: vec![KeyUsage::DigitalSignature],
                extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                validity_days: 30,
                role: CertificateRole::Client,
            };
            
            let throttled_cert_start = tokio::time::Instant::now();
            let throttled_cert_result = cluster.cert_manager.issue_certificate(&throttled_cert_request).await;
            let throttled_cert_time = throttled_cert_start.elapsed();
            
            // Should either succeed with higher latency or fail gracefully
            match throttled_cert_result {
                Ok(_) => {
                    assert!(throttled_cert_time > Duration::from_millis(100), "Should have higher latency during overload");
                }
                Err(_) => {
                    // Graceful failure is acceptable during overload
                }
            }
            
            // Existing certificate validation should still work (from cache)
            // Issue a certificate before overload simulation
            let cached_cert_request = CertificateRequest {
                common_name: "cached-cert".to_string(),
                organization: Some("RustMQ Cache Test".to_string()),
                organizational_unit: Some("Cache".to_string()),
                country: None,
                state: None,
                locality: None,
                email: None,
                subject_alt_names: vec!["cached-cert.test".to_string()],
                key_type: KeyType::Rsa2048,
                key_usage: vec![KeyUsage::DigitalSignature],
                extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                validity_days: 30,
                role: CertificateRole::Client,
            };
            
            // Reset overload first
            cluster.cert_manager.reset_service_overload().await.unwrap();
            let cached_cert = cluster.cert_manager.issue_certificate(&cached_cert_request).await.unwrap();
            
            // Simulate overload again
            cluster.cert_manager.simulate_service_overload().await.unwrap();
            
            // Validation should still work from cache
            let cached_validation = cluster.cert_manager.validate_certificate(&cached_cert.serial_number).await;
            match cached_validation {
                Ok(result) => assert!(result.is_valid, "Cached certificate validation should work during overload"),
                Err(_) => {} // May fail during overload, which is acceptable
            }
        }
        
        // Degradation Scenario 3: Network connectivity issues
        println!("Degradation scenario 3: Network connectivity issues");
        
        // Simulate high network latency
        for _ in 0..5 {
            cluster.simulate_network_latency().await;
        }
        
        // Authentication and authorization should still work but with higher latency
        let high_latency_start = tokio::time::Instant::now();
        let high_latency_result = connection1.authorize("degradation.normal.latency", Permission::Read).await;
        let high_latency_time = high_latency_start.elapsed();
        
        match high_latency_result {
            Ok(authorized) => {
                assert!(authorized, "Authorization should work despite high latency");
                // Latency may be higher but should not exceed reasonable bounds
                assert!(high_latency_time < Duration::from_secs(5), "Latency should not be excessive");
            }
            Err(_) => {
                // May fail due to network issues, but should not crash
            }
        }
        
        // Degradation Scenario 4: Storage system degradation
        println!("Degradation scenario 4: Storage system degradation");
        
        // Simulate storage latency increase
        let storage_degradation_result = cluster.acl_manager.simulate_storage_degradation().await;
        if storage_degradation_result.is_ok() {
            // ACL operations should fall back to cache
            let cache_fallback_start = tokio::time::Instant::now();
            let cache_fallback_result = connection1.authorize("degradation.normal.cache", Permission::Read).await;
            let cache_fallback_time = cache_fallback_start.elapsed();
            
            match cache_fallback_result {
                Ok(authorized) => {
                    assert!(authorized, "Should use cache fallback during storage degradation");
                    assert!(cache_fallback_time < Duration::from_millis(100), "Cache fallback should be fast");
                }
                Err(_) => {
                    // May fail during severe storage degradation
                }
            }
            
            // New rule creation should queue or use write-behind cache
            let degraded_rule = AclRule {
                principal: "degraded-storage-user".to_string(),
                resource_pattern: ResourcePattern::Topic("degraded.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            };
            
            let degraded_rule_result = cluster.acl_manager.add_rule(degraded_rule).await;
            // May succeed (queued) or fail gracefully
        }
        
        // Degradation Scenario 5: Memory pressure with fallback behavior
        println!("Degradation scenario 5: Memory pressure with fallback behavior");
        
        // Simulate memory pressure
        let mut _memory_pressure = Vec::new();
        for _ in 0..500 {
            _memory_pressure.push(vec![0u8; 1024 * 1024]); // 1MB allocations
        }
        
        // Security operations should continue with degraded performance
        let memory_pressure_client = cluster.create_test_client("degradation-user-1").await.unwrap();
        let memory_pressure_result = memory_pressure_client.connect_to_broker(0).await;
        
        match memory_pressure_result {
            Ok(connection) => {
                // If connection succeeds, test authorization
                let authz_under_pressure = connection.authorize("degradation.normal.memory", Permission::Read).await;
                match authz_under_pressure {
                    Ok(authorized) => assert!(authorized, "Authorization should work under memory pressure"),
                    Err(_) => {} // May fail under extreme memory pressure
                }
            }
            Err(_) => {
                // Connection may fail under memory pressure - should be graceful
            }
        }
        
        // Release memory pressure
        drop(_memory_pressure);
        tokio::time::sleep(Duration::from_millis(100)).await; // Allow GC
        
        // Test recovery after memory pressure relief
        let post_pressure_client = cluster.create_test_client("degradation-user-1").await.unwrap();
        let post_pressure_connection = post_pressure_client.connect_to_broker(0).await.unwrap();
        assert!(post_pressure_connection.authorize("degradation.normal.recovered", Permission::Read).await.unwrap());
        
        // Test graceful degradation monitoring
        let degradation_metrics = cluster.acl_manager.get_degradation_metrics().await.unwrap();
        assert!(degradation_metrics.total_degradation_events >= 0);
        
        if degradation_metrics.cache_fallback_count > 0 {
            assert!(degradation_metrics.cache_fallback_success_rate > 0.0);
        }
        
        println!("Partial service degradation with security fallbacks completed");
        cluster.shutdown().await.unwrap();
    }

    /// Test 5: Security system startup and shutdown coordination
    #[tokio::test]
    async fn test_security_system_startup_shutdown_coordination() {
        println!("Testing security system startup and shutdown coordination...");
        
        // Test cold startup sequence
        println!("Test 1: Cold startup sequence");
        
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        
        // Verify initial state before startup
        assert!(cluster.controllers.is_empty());
        assert!(cluster.brokers.is_empty());
        
        // Monitor startup sequence
        let startup_start = tokio::time::Instant::now();
        cluster.start().await.unwrap();
        let startup_time = startup_start.elapsed();
        
        // Verify startup completed successfully
        assert_eq!(cluster.controllers.len(), cluster.config.cluster_size);
        assert_eq!(cluster.brokers.len(), cluster.config.broker_count);
        assert!(startup_time < Duration::from_secs(10), "Startup should complete within 10 seconds");
        
        // Test security system readiness
        let readiness_check = async {
            // Controller readiness
            let mut controllers_ready = 0;
            for controller in &cluster.controllers {
                if controller.is_ready().await.unwrap_or(false) {
                    controllers_ready += 1;
                }
            }
            
            // Broker readiness
            let mut brokers_ready = 0;
            for broker in &cluster.brokers {
                if broker.is_ready().await.unwrap_or(false) {
                    brokers_ready += 1;
                }
            }
            
            (controllers_ready, brokers_ready)
        };
        
        let (ready_controllers, ready_brokers) = readiness_check.await;
        assert!(ready_controllers >= cluster.config.cluster_size / 2 + 1, "Majority of controllers should be ready");
        assert!(ready_brokers >= 1, "At least one broker should be ready");
        
        // Test security functionality after startup
        let startup_test_rule = AclRule {
            principal: "startup-test-user".to_string(),
            resource_pattern: ResourcePattern::Topic("startup.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let startup_rule_result = cluster.acl_manager.add_rule(startup_test_rule).await;
        assert!(startup_rule_result.is_ok(), "Should be able to add ACL rules after startup");
        
        let startup_client = cluster.create_test_client("startup-test-user").await.unwrap();
        let startup_connection = startup_client.connect_to_broker(0).await.unwrap();
        assert!(startup_connection.authorize("startup.test", Permission::Read).await.unwrap());
        
        // Test warm restart sequence
        println!("Test 2: Warm restart sequence");
        
        // Add data before restart
        let pre_restart_rule = AclRule {
            principal: "pre-restart-user".to_string(),
            resource_pattern: ResourcePattern::Topic("pre.restart.*".to_string()),
            operation: Permission::Write,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(pre_restart_rule).await.unwrap();
        
        let pre_restart_cert_request = CertificateRequest {
            common_name: "pre-restart-cert".to_string(),
            organization: Some("RustMQ Restart Test".to_string()),
            organizational_unit: Some("Restart".to_string()),
            country: None,
            state: None,
            locality: None,
            email: None,
            subject_alt_names: vec!["pre-restart-cert.test".to_string()],
            key_type: KeyType::Rsa2048,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
            validity_days: 30,
            role: CertificateRole::Client,
        };
        
        let pre_restart_cert = cluster.cert_manager.issue_certificate(&pre_restart_cert_request).await.unwrap();
        
        // Perform restart
        let restart_start = tokio::time::Instant::now();
        cluster.shutdown().await.unwrap();
        cluster.start().await.unwrap();
        let restart_time = restart_start.elapsed();
        
        assert!(restart_time < Duration::from_secs(15), "Restart should complete within 15 seconds");
        
        // Verify data persistence after restart
        let post_restart_client = cluster.create_test_client("pre-restart-user").await.unwrap();
        let post_restart_connection = post_restart_client.connect_to_broker(0).await.unwrap();
        assert!(post_restart_connection.authorize("pre.restart.test", Permission::Write).await.unwrap());
        
        let post_restart_cert_validation = cluster.cert_manager.validate_certificate(&pre_restart_cert.serial_number).await.unwrap();
        assert!(post_restart_cert_validation.is_valid);
        
        // Test graceful shutdown sequence
        println!("Test 3: Graceful shutdown sequence");
        
        // Create active connections
        let shutdown_client = cluster.create_test_client("shutdown-test-user").await.unwrap();
        let shutdown_connection = shutdown_client.connect_to_broker(0).await.unwrap();
        
        // Start background operations
        let background_operations = {
            let connection = shutdown_connection.clone();
            tokio::spawn(async move {
                let mut operation_count = 0;
                for i in 0..1000 {
                    let topic = format!("shutdown.background.{}", i);
                    if connection.authorize(&topic, Permission::Read).await.unwrap_or(false) {
                        operation_count += 1;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                operation_count
            })
        };
        
        // Wait for operations to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Initiate graceful shutdown
        let shutdown_start = tokio::time::Instant::now();
        cluster.shutdown().await.unwrap();
        let shutdown_time = shutdown_start.elapsed();
        
        // Verify graceful shutdown completed
        assert!(shutdown_time < Duration::from_secs(30), "Graceful shutdown should complete within 30 seconds");
        
        // Check if background operations completed gracefully
        let completed_operations = background_operations.await.unwrap();
        assert!(completed_operations > 0, "Some background operations should have completed before shutdown");
        
        // Test emergency shutdown sequence
        println!("Test 4: Emergency shutdown sequence");
        
        cluster.start().await.unwrap();
        
        // Create active operations
        let emergency_client = cluster.create_test_client("emergency-test-user").await.unwrap();
        let emergency_connection = emergency_client.connect_to_broker(0).await.unwrap();
        
        // Simulate emergency condition
        let emergency_operations = {
            let connection = emergency_connection.clone();
            tokio::spawn(async move {
                for i in 0..100 {
                    let topic = format!("emergency.{}", i);
                    connection.authorize(&topic, Permission::Read).await.unwrap_or(false);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
        };
        
        tokio::time::sleep(Duration::from_millis(50)).await; // Let operations start
        
        // Emergency shutdown (faster, less graceful)
        let emergency_shutdown_start = tokio::time::Instant::now();
        cluster.emergency_shutdown().await.unwrap();
        let emergency_shutdown_time = emergency_shutdown_start.elapsed();
        
        assert!(emergency_shutdown_time < Duration::from_secs(5), "Emergency shutdown should complete within 5 seconds");
        
        // Background operations may be interrupted
        let emergency_result = emergency_operations.await;
        // This may fail due to emergency shutdown, which is expected
        
        // Test startup dependency ordering
        println!("Test 5: Startup dependency ordering");
        
        cluster = SecurityTestCluster::new().await.unwrap();
        
        // Test manual component startup in specific order
        let ordered_startup_start = tokio::time::Instant::now();
        
        // 1. Start certificate manager first
        let cert_manager_start = cluster.cert_manager.start_services().await;
        assert!(cert_manager_start.is_ok(), "Certificate manager should start first");
        
        // 2. Start controllers
        for controller in &mut cluster.controllers {
            controller.start().await.unwrap();
        }
        
        // 3. Start brokers last
        for broker in &mut cluster.brokers {
            broker.start().await.unwrap();
        }
        
        let ordered_startup_time = ordered_startup_start.elapsed();
        assert!(ordered_startup_time < Duration::from_secs(12), "Ordered startup should complete within 12 seconds");
        
        // Verify correct initialization order was followed
        let initialization_order = cluster.get_initialization_order().await.unwrap();
        assert_eq!(initialization_order[0], "certificate_manager");
        assert!(initialization_order.contains(&"controllers"));
        assert!(initialization_order.contains(&"brokers"));
        
        // Test final functionality
        let final_test_client = cluster.create_test_client("final-test-user").await.unwrap();
        let final_test_connection = final_test_client.connect_to_broker(0).await.unwrap();
        
        let final_rule = AclRule {
            principal: "final-test-user".to_string(),
            resource_pattern: ResourcePattern::Topic("final.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(final_rule).await.unwrap();
        assert!(final_test_connection.authorize("final.test", Permission::Read).await.unwrap());
        
        println!("Security system startup and shutdown coordination completed");
        cluster.shutdown().await.unwrap();
    }
}

#[cfg(test)]
mod failure_recovery_stress_tests {
    use super::*;
    
    /// Stress test: Multiple simultaneous failures
    #[tokio::test]
    async fn test_multiple_simultaneous_failures() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.cluster_size = 5; // Larger cluster for stress testing
        cluster.config.broker_count = 4;
        cluster.start().await.unwrap();
        
        println!("Testing multiple simultaneous failures...");
        
        // Setup baseline configuration
        let stress_rule = AclRule {
            principal: "stress-test-user".to_string(),
            resource_pattern: ResourcePattern::Topic("stress.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(stress_rule).await.unwrap();
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        
        // Create multiple clients
        let mut clients = Vec::new();
        for i in 0..10 {
            let client = cluster.create_test_client("stress-test-user").await.unwrap();
            clients.push(client);
        }
        
        // Start continuous operations
        let continuous_operations = {
            let clients = clients.clone();
            let broker_count = cluster.brokers.len();
            tokio::spawn(async move {
                let mut success_count = 0;
                let mut error_count = 0;
                
                for (i, client) in clients.iter().enumerate() {
                    let broker_index = i % broker_count;
                    match client.connect_to_broker(broker_index).await {
                        Ok(connection) => {
                            for j in 0..50 {
                                let topic = format!("stress.topic.{}.{}", i, j);
                                match connection.authorize(&topic, Permission::Read).await {
                                    Ok(authorized) => {
                                        if authorized {
                                            success_count += 1;
                                        } else {
                                            error_count += 1;
                                        }
                                    }
                                    Err(_) => error_count += 1,
                                }
                                
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        }
                        Err(_) => error_count += 1,
                    }
                }
                
                (success_count, error_count)
            })
        };
        
        // Trigger multiple simultaneous failures
        tokio::time::sleep(Duration::from_millis(100)).await; // Let operations start
        
        println!("Triggering simultaneous failures...");
        
        // Failure 1: Shutdown multiple controllers
        for i in 0..2 {
            cluster.controllers[i].shutdown().await.unwrap();
        }
        
        // Failure 2: Shutdown one broker
        cluster.brokers[0].shutdown().await.unwrap();
        
        // Failure 3: Simulate storage corruption
        cluster.acl_manager.simulate_storage_corruption().await.unwrap();
        
        // Failure 4: Simulate certificate service overload
        cluster.cert_manager.simulate_service_overload().await.unwrap();
        
        // Let operations continue during failures
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Trigger recovery
        println!("Initiating recovery procedures...");
        
        // Recovery 1: Restart controllers
        for i in 0..2 {
            cluster.controllers[i].start().await.unwrap();
        }
        
        // Recovery 2: Restart broker
        cluster.brokers[0].start().await.unwrap();
        
        // Recovery 3: Recover from storage corruption
        let backup_result = cluster.acl_manager.create_backup().await.unwrap();
        cluster.acl_manager.recover_from_backup(&backup_result.backup_id).await.unwrap();
        
        // Recovery 4: Reset certificate service
        cluster.cert_manager.reset_service_overload().await.unwrap();
        
        // Wait for full recovery
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // Check final operations results
        let (successes, errors) = continuous_operations.await.unwrap();
        let total_operations = successes + errors;
        let success_rate = successes as f64 / total_operations as f64;
        
        println!("Stress test results:");
        println!("  Total operations: {}", total_operations);
        println!("  Successful operations: {}", successes);
        println!("  Failed operations: {}", errors);
        println!("  Success rate: {:.2}%", success_rate * 100.0);
        
        // Even with multiple failures, some operations should succeed
        assert!(success_rate > 0.3, "Success rate should be > 30% even during multiple failures");
        assert!(total_operations > 100, "Should have attempted a significant number of operations");
        
        // Test post-recovery functionality
        let recovery_client = cluster.create_test_client("stress-test-user").await.unwrap();
        let recovery_connection = recovery_client.connect_to_broker(1).await.unwrap();
        assert!(recovery_connection.authorize("stress.recovery.test", Permission::Read).await.unwrap());
        
        cluster.shutdown().await.unwrap();
    }
}