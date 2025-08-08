//! Performance and Stress Integration Tests
//!
//! This module tests security system performance under realistic load conditions,
//! including high-throughput authentication, authorization, concurrent operations,
//! and stress testing across all security components.
//!
//! Target: 4 comprehensive performance and stress integration tests

use super::test_infrastructure::*;
use rustmq::{
    security::{Permission, AclRule, Effect, ResourcePattern},
    error::RustMqError,
    Result,
};
use std::{sync::{Arc, atomic::{AtomicU64, Ordering}}, time::{Duration, SystemTime, Instant}};
use tokio::{sync::Semaphore, time::timeout};

#[cfg(test)]
mod performance_integration_tests {
    use super::*;

    /// Test 1: High-throughput authentication and authorization under load
    #[tokio::test]
    async fn test_high_throughput_security_operations_under_load() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.broker_count = 2; // Multiple brokers for load distribution
        cluster.start().await.unwrap();
        
        // Setup ACL rules for load testing
        let load_test_rules = vec![
            AclRule {
                principal: "*".to_string(), // Wildcard for any principal
                resource_pattern: ResourcePattern::Topic("load.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "*".to_string(),
                resource_pattern: ResourcePattern::Topic("load.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
        ];
        
        for rule in &load_test_rules {
            cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Performance test parameters
        let client_count = 200;
        let operations_per_client = 100;
        let total_operations = client_count * operations_per_client;
        
        // Metrics collection
        let auth_latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let authz_latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let success_counter = Arc::new(AtomicU64::new(0));
        let error_counter = Arc::new(AtomicU64::new(0));
        
        // Rate limiting to prevent overwhelming the system
        let semaphore = Arc::new(Semaphore::new(50)); // Max 50 concurrent operations
        
        println!("Starting high-throughput security load test...");
        println!("Clients: {}, Operations per client: {}, Total operations: {}", 
                client_count, operations_per_client, total_operations);
        
        let test_start = tokio::time::Instant::now();
        let mut handles = Vec::new();
        
        for client_id in 0..client_count {
            let cluster_ref = &cluster;
            let auth_latencies_clone = auth_latencies.clone();
            let authz_latencies_clone = authz_latencies.clone();
            let success_counter_clone = success_counter.clone();
            let error_counter_clone = error_counter.clone();
            let semaphore_clone = semaphore.clone();
            
            let handle = tokio::spawn(async move {
                let client = cluster_ref.create_test_client(&format!("load-client-{}", client_id)).await.unwrap();
                let broker_index = client_id % cluster_ref.brokers.len();
                
                let mut client_auth_latencies = Vec::new();
                let mut client_authz_latencies = Vec::new();
                let mut client_successes = 0;
                let mut client_errors = 0;
                
                for op_id in 0..operations_per_client {
                    let _permit = semaphore_clone.acquire().await.unwrap();
                    
                    // Authentication operation
                    let auth_start = Instant::now();
                    match client.connect_to_broker(broker_index).await {
                        Ok(connection) => {
                            let auth_latency = auth_start.elapsed();
                            client_auth_latencies.push(auth_latency);
                            
                            // Authorization operation
                            let topic = format!("load.client{}.topic{}", client_id, op_id);
                            let permission = if op_id % 2 == 0 { Permission::Read } else { Permission::Write };
                            
                            let authz_start = Instant::now();
                            match connection.authorize(&topic, permission).await {
                                Ok(authorized) => {
                                    let authz_latency = authz_start.elapsed();
                                    client_authz_latencies.push(authz_latency);
                                    
                                    if authorized {
                                        client_successes += 1;
                                    } else {
                                        client_errors += 1;
                                    }
                                }
                                Err(_) => {
                                    client_errors += 1;
                                }
                            }
                        }
                        Err(_) => {
                            client_errors += 1;
                        }
                    }
                    
                    // Small delay to simulate realistic client behavior
                    if op_id % 10 == 0 {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                }
                
                (client_auth_latencies, client_authz_latencies, client_successes, client_errors)
            });
            
            handles.push(handle);
        }
        
        // Collect results
        let mut all_auth_latencies = Vec::new();
        let mut all_authz_latencies = Vec::new();
        let mut total_successes = 0;
        let mut total_errors = 0;
        
        for handle in handles {
            let (auth_lats, authz_lats, successes, errors) = handle.await.unwrap();
            all_auth_latencies.extend(auth_lats);
            all_authz_latencies.extend(authz_lats);
            total_successes += successes;
            total_errors += errors;
        }
        
        let total_test_time = test_start.elapsed();
        
        // Calculate performance statistics
        all_auth_latencies.sort();
        all_authz_latencies.sort();
        
        let auth_avg = all_auth_latencies.iter().sum::<Duration>() / all_auth_latencies.len() as u32;
        let auth_p95 = all_auth_latencies[(all_auth_latencies.len() as f64 * 0.95) as usize];
        let auth_p99 = all_auth_latencies[(all_auth_latencies.len() as f64 * 0.99) as usize];
        let auth_max = all_auth_latencies[all_auth_latencies.len() - 1];
        
        let authz_avg = all_authz_latencies.iter().sum::<Duration>() / all_authz_latencies.len() as u32;
        let authz_p95 = all_authz_latencies[(all_authz_latencies.len() as f64 * 0.95) as usize];
        let authz_p99 = all_authz_latencies[(all_authz_latencies.len() as f64 * 0.99) as usize];
        let authz_max = all_authz_latencies[all_authz_latencies.len() - 1];
        
        let total_throughput = total_operations as f64 / total_test_time.as_secs_f64();
        let success_rate = total_successes as f64 / (total_successes + total_errors) as f64;
        
        // Performance assertions
        assert!(auth_avg < Duration::from_millis(50), 
               "Average authentication latency should be < 50ms under load, got {:?}", auth_avg);
        assert!(auth_p95 < Duration::from_millis(100), 
               "P95 authentication latency should be < 100ms under load, got {:?}", auth_p95);
        assert!(auth_p99 < Duration::from_millis(200), 
               "P99 authentication latency should be < 200ms under load, got {:?}", auth_p99);
        
        assert!(authz_avg < Duration::from_millis(10), 
               "Average authorization latency should be < 10ms under load, got {:?}", authz_avg);
        assert!(authz_p95 < Duration::from_millis(25), 
               "P95 authorization latency should be < 25ms under load, got {:?}", authz_p95);
        assert!(authz_p99 < Duration::from_millis(50), 
               "P99 authorization latency should be < 50ms under load, got {:?}", authz_p99);
        
        assert!(total_throughput > 500.0, 
               "Total throughput should be > 500 ops/sec, got {:.2}", total_throughput);
        assert!(success_rate > 0.95, 
               "Success rate should be > 95%, got {:.2}", success_rate);
        
        println!("High-throughput performance results:");
        println!("  Total test time: {:?}", total_test_time);
        println!("  Total throughput: {:.2} operations/sec", total_throughput);
        println!("  Success rate: {:.2}%", success_rate * 100.0);
        println!("  Authentication - Avg: {:?}, P95: {:?}, P99: {:?}, Max: {:?}", auth_avg, auth_p95, auth_p99, auth_max);
        println!("  Authorization - Avg: {:?}, P95: {:?}, P99: {:?}, Max: {:?}", authz_avg, authz_p95, authz_p99, authz_max);
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 2: Concurrent security operations across multiple components
    #[tokio::test]
    async fn test_concurrent_security_operations_multiple_components() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.cluster_size = 3; // Multiple controllers for concurrency
        cluster.config.broker_count = 2; // Multiple brokers
        cluster.start().await.unwrap();
        
        let concurrent_operations = 50;
        let operation_types = 6; // Different types of concurrent operations
        
        println!("Starting concurrent security operations test...");
        println!("Concurrent operations: {}, Operation types: {}", concurrent_operations, operation_types);
        
        let test_start = tokio::time::Instant::now();
        let mut handles = Vec::new();
        
        // Type 1: Concurrent ACL rule creation
        for i in 0..concurrent_operations {
            let acl_manager = cluster.acl_manager.clone();
            let handle = tokio::spawn(async move {
                let rule = AclRule {
                    principal: format!("concurrent-acl-user-{}", i),
                    resource_pattern: ResourcePattern::Topic(format!("concurrent.acl.{}.*", i)),
                    operation: Permission::Read,
                    effect: Effect::Allow,
                };
                
                let start = Instant::now();
                let result = acl_manager.add_rule(rule).await;
                let latency = start.elapsed();
                
                (1, result.is_ok(), latency)
            });
            handles.push(handle);
        }
        
        // Type 2: Concurrent certificate issuance
        for i in 0..concurrent_operations {
            let cert_manager = cluster.cert_manager.clone();
            let handle = tokio::spawn(async move {
                let cert_request = CertificateRequest {
                    common_name: format!("concurrent-cert-{}", i),
                    organization: Some("RustMQ Concurrent Test".to_string()),
                    organizational_unit: Some("Concurrency".to_string()),
                    country: None,
                    state: None,
                    locality: None,
                    email: None,
                    subject_alt_names: vec![format!("concurrent-cert-{}.test", i)],
                    key_type: KeyType::Rsa2048,
                    key_usage: vec![KeyUsage::DigitalSignature],
                    extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                    validity_days: 30,
                    role: CertificateRole::Client,
                };
                
                let start = Instant::now();
                let result = cert_manager.issue_certificate(&cert_request).await;
                let latency = start.elapsed();
                
                (2, result.is_ok(), latency)
            });
            handles.push(handle);
        }
        
        // Type 3: Concurrent authentication operations
        for i in 0..concurrent_operations {
            let cluster_ref = &cluster;
            let handle = tokio::spawn(async move {
                let start = Instant::now();
                let client_result = cluster_ref.create_test_client(&format!("concurrent-auth-{}", i)).await;
                
                let success = match client_result {
                    Ok(client) => {
                        match client.connect_to_broker(i % cluster_ref.brokers.len()).await {
                            Ok(_) => true,
                            Err(_) => false,
                        }
                    }
                    Err(_) => false,
                };
                
                let latency = start.elapsed();
                (3, success, latency)
            });
            handles.push(handle);
        }
        
        // Type 4: Concurrent authorization checks
        for i in 0..concurrent_operations {
            let cluster_ref = &cluster;
            let handle = tokio::spawn(async move {
                let client = cluster_ref.create_test_client(&format!("concurrent-authz-{}", i)).await.unwrap();
                let connection = client.connect_to_broker(i % cluster_ref.brokers.len()).await.unwrap();
                
                let topic = format!("concurrent.authz.{}", i);
                let start = Instant::now();
                let result = connection.authorize(&topic, Permission::Read).await;
                let latency = start.elapsed();
                
                (4, result.is_ok(), latency)
            });
            handles.push(handle);
        }
        
        // Type 5: Concurrent ACL rule queries
        for i in 0..concurrent_operations {
            let acl_manager = cluster.acl_manager.clone();
            let handle = tokio::spawn(async move {
                let principal = format!("concurrent-query-{}", i % 10); // Query existing principals
                let start = Instant::now();
                let result = acl_manager.get_rules_for_principal(&principal).await;
                let latency = start.elapsed();
                
                (5, result.is_ok(), latency)
            });
            handles.push(handle);
        }
        
        // Type 6: Concurrent certificate validation
        for i in 0..concurrent_operations {
            let cert_manager = cluster.cert_manager.clone();
            let handle = tokio::spawn(async move {
                // Use a test certificate serial number
                let serial = format!("test-serial-{}", i);
                let start = Instant::now();
                let result = cert_manager.validate_certificate(&serial).await;
                let latency = start.elapsed();
                
                // This may fail for non-existent certificates, which is expected
                (6, true, latency) // Always count as success for latency measurement
            });
            handles.push(handle);
        }
        
        // Collect results by operation type
        let mut results_by_type: std::collections::HashMap<u8, Vec<(bool, Duration)>> = std::collections::HashMap::new();
        
        for handle in handles {
            let (op_type, success, latency) = handle.await.unwrap();
            results_by_type.entry(op_type).or_insert_with(Vec::new).push((success, latency));
        }
        
        let total_concurrent_time = test_start.elapsed();
        
        // Analyze results for each operation type
        for (op_type, results) in &results_by_type {
            let operation_name = match op_type {
                1 => "ACL Rule Creation",
                2 => "Certificate Issuance",
                3 => "Authentication",
                4 => "Authorization",
                5 => "ACL Rule Query",
                6 => "Certificate Validation",
                _ => "Unknown",
            };
            
            let success_count = results.iter().filter(|(success, _)| *success).count();
            let success_rate = success_count as f64 / results.len() as f64;
            
            let latencies: Vec<Duration> = results.iter().map(|(_, latency)| *latency).collect();
            let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
            let max_latency = latencies.iter().max().unwrap();
            
            println!("  {}: Success rate: {:.2}%, Avg latency: {:?}, Max latency: {:?}", 
                    operation_name, success_rate * 100.0, avg_latency, max_latency);
            
            // Performance assertions based on operation type
            match op_type {
                1 => { // ACL Rule Creation
                    assert!(success_rate > 0.9, "ACL creation success rate should be > 90%");
                    assert!(avg_latency < Duration::from_millis(100), "ACL creation avg latency should be < 100ms");
                }
                2 => { // Certificate Issuance
                    assert!(success_rate > 0.8, "Certificate issuance success rate should be > 80%");
                    assert!(avg_latency < Duration::from_secs(2), "Certificate issuance avg latency should be < 2s");
                }
                3 => { // Authentication
                    assert!(success_rate > 0.9, "Authentication success rate should be > 90%");
                    assert!(avg_latency < Duration::from_millis(200), "Authentication avg latency should be < 200ms");
                }
                4 => { // Authorization
                    assert!(success_rate > 0.95, "Authorization success rate should be > 95%");
                    assert!(avg_latency < Duration::from_millis(50), "Authorization avg latency should be < 50ms");
                }
                5 => { // ACL Rule Query
                    assert!(success_rate > 0.95, "ACL query success rate should be > 95%");
                    assert!(avg_latency < Duration::from_millis(20), "ACL query avg latency should be < 20ms");
                }
                6 => { // Certificate Validation
                    assert!(avg_latency < Duration::from_millis(100), "Certificate validation avg latency should be < 100ms");
                }
                _ => {}
            }
        }
        
        // Overall concurrency performance
        let total_operations = concurrent_operations * operation_types;
        let concurrent_throughput = total_operations as f64 / total_concurrent_time.as_secs_f64();
        
        assert!(concurrent_throughput > 100.0, 
               "Concurrent throughput should be > 100 ops/sec, got {:.2}", concurrent_throughput);
        assert!(total_concurrent_time < Duration::from_secs(30), 
               "Concurrent operations should complete < 30s, got {:?}", total_concurrent_time);
        
        println!("Concurrent operations performance results:");
        println!("  Total operations: {}", total_operations);
        println!("  Total time: {:?}", total_concurrent_time);
        println!("  Concurrent throughput: {:.2} operations/sec", concurrent_throughput);
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 3: Memory usage validation for large-scale deployments
    #[tokio::test]
    async fn test_memory_usage_validation_large_scale() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        println!("Starting memory usage validation test...");
        
        // Baseline memory usage
        let initial_memory = get_memory_usage();
        println!("Initial memory usage: {} MB", initial_memory);
        
        // Create large number of ACL rules
        let rule_count = 10000;
        println!("Creating {} ACL rules...", rule_count);
        
        let rule_creation_start = Instant::now();
        for i in 0..rule_count {
            let rule = AclRule {
                principal: format!("memory-test-user-{}", i),
                resource_pattern: ResourcePattern::Topic(format!("memory.test.{}.*", i)),
                operation: if i % 2 == 0 { Permission::Read } else { Permission::Write },
                effect: Effect::Allow,
            };
            
            cluster.acl_manager.add_rule(rule).await.unwrap();
            
            // Check memory periodically
            if i % 1000 == 0 {
                let current_memory = get_memory_usage();
                println!("  After {} rules: {} MB", i, current_memory);
            }
        }
        
        let rule_creation_time = rule_creation_start.elapsed();
        let post_rules_memory = get_memory_usage();
        
        // Synchronize rules to brokers
        println!("Synchronizing rules to brokers...");
        let sync_start = Instant::now();
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        let sync_time = sync_start.elapsed();
        let post_sync_memory = get_memory_usage();
        
        // Create large number of certificates
        let cert_count = 1000;
        println!("Creating {} certificates...", cert_count);
        
        let cert_creation_start = Instant::now();
        for i in 0..cert_count {
            let cert_request = CertificateRequest {
                common_name: format!("memory-test-cert-{}", i),
                organization: Some("RustMQ Memory Test".to_string()),
                organizational_unit: Some("Memory".to_string()),
                country: None,
                state: None,
                locality: None,
                email: None,
                subject_alt_names: vec![format!("memory-cert-{}.test", i)],
                key_type: KeyType::Rsa2048,
                key_usage: vec![KeyUsage::DigitalSignature],
                extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                validity_days: 30,
                role: CertificateRole::Client,
            };
            
            cluster.cert_manager.issue_certificate(&cert_request).await.unwrap();
            
            if i % 100 == 0 {
                let current_memory = get_memory_usage();
                println!("  After {} certificates: {} MB", i, current_memory);
            }
        }
        
        let cert_creation_time = cert_creation_start.elapsed();
        let post_certs_memory = get_memory_usage();
        
        // Perform high-volume authorization operations
        let authz_count = 50000;
        println!("Performing {} authorization operations...", authz_count);
        
        let client = cluster.create_test_client("memory-test-authz-client").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        let authz_start = Instant::now();
        for i in 0..authz_count {
            let topic = format!("memory.test.{}", i % rule_count);
            connection.authorize(&topic, Permission::Read).await.unwrap();
            
            if i % 10000 == 0 && i > 0 {
                let current_memory = get_memory_usage();
                println!("  After {} authorizations: {} MB", i, current_memory);
            }
        }
        
        let authz_time = authz_start.elapsed();
        let final_memory = get_memory_usage();
        
        // Memory usage analysis
        let rules_memory_increase = post_rules_memory - initial_memory;
        let sync_memory_increase = post_sync_memory - post_rules_memory;
        let certs_memory_increase = post_certs_memory - post_sync_memory;
        let authz_memory_increase = final_memory - post_certs_memory;
        let total_memory_increase = final_memory - initial_memory;
        
        println!("Memory usage analysis:");
        println!("  Initial: {} MB", initial_memory);
        println!("  After {} ACL rules: {} MB (+{} MB)", rule_count, post_rules_memory, rules_memory_increase);
        println!("  After sync: {} MB (+{} MB)", post_sync_memory, sync_memory_increase);
        println!("  After {} certificates: {} MB (+{} MB)", cert_count, post_certs_memory, certs_memory_increase);
        println!("  After {} authorizations: {} MB (+{} MB)", authz_count, final_memory, authz_memory_increase);
        println!("  Total increase: {} MB", total_memory_increase);
        
        // Memory efficiency assertions
        let memory_per_rule = rules_memory_increase as f64 / rule_count as f64;
        let memory_per_cert = certs_memory_increase as f64 / cert_count as f64;
        
        assert!(memory_per_rule < 0.1, 
               "Memory per ACL rule should be < 0.1 MB, got {:.3} MB", memory_per_rule);
        assert!(memory_per_cert < 0.5, 
               "Memory per certificate should be < 0.5 MB, got {:.3} MB", memory_per_cert);
        assert!(authz_memory_increase < 50, 
               "Authorization operations should not significantly increase memory, got {} MB", authz_memory_increase);
        assert!(total_memory_increase < 1000, 
               "Total memory increase should be < 1GB for large scale test, got {} MB", total_memory_increase);
        
        // Performance assertions
        assert!(rule_creation_time < Duration::from_secs(30), 
               "Rule creation should complete < 30s, got {:?}", rule_creation_time);
        assert!(sync_time < Duration::from_secs(10), 
               "Rule synchronization should complete < 10s, got {:?}", sync_time);
        assert!(cert_creation_time < Duration::from_secs(60), 
               "Certificate creation should complete < 60s, got {:?}", cert_creation_time);
        assert!(authz_time < Duration::from_secs(20), 
               "Authorization operations should complete < 20s, got {:?}", authz_time);
        
        println!("Performance summary:");
        println!("  Rule creation: {:.2} rules/sec", rule_count as f64 / rule_creation_time.as_secs_f64());
        println!("  Certificate creation: {:.2} certs/sec", cert_count as f64 / cert_creation_time.as_secs_f64());
        println!("  Authorization: {:.2} authz/sec", authz_count as f64 / authz_time.as_secs_f64());
        
        cluster.shutdown().await.unwrap();
    }

    /// Test 4: Security component resilience under stress conditions
    #[tokio::test]
    async fn test_security_component_resilience_under_stress() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.config.cluster_size = 3;
        cluster.config.broker_count = 3;
        cluster.start().await.unwrap();
        
        println!("Starting security component resilience stress test...");
        
        // Setup baseline ACL rules
        let baseline_rules = vec![
            AclRule {
                principal: "stress-producer".to_string(),
                resource_pattern: ResourcePattern::Topic("stress.*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "stress-consumer".to_string(),
                resource_pattern: ResourcePattern::Topic("stress.*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
        ];
        
        for rule in &baseline_rules {
            cluster.acl_manager.add_rule(rule.clone()).await.unwrap();
        }
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        
        // Create clients for stress testing
        let producer_client = cluster.create_test_client("stress-producer").await.unwrap();
        let consumer_client = cluster.create_test_client("stress-consumer").await.unwrap();
        
        // Test resilience under various stress conditions
        
        // Stress Condition 1: Rapid ACL rule changes
        println!("Stress test 1: Rapid ACL rule changes");
        let rapid_changes_start = Instant::now();
        
        let rapid_changes_handle = {
            let acl_manager = cluster.acl_manager.clone();
            tokio::spawn(async move {
                let mut rule_ids = Vec::new();
                
                for i in 0..100 {
                    // Add rule
                    let rule = AclRule {
                        principal: format!("rapid-change-user-{}", i),
                        resource_pattern: ResourcePattern::Topic(format!("rapid.{}.*", i)),
                        operation: Permission::Read,
                        effect: Effect::Allow,
                    };
                    
                    let rule_id = acl_manager.add_rule(rule).await.unwrap();
                    rule_ids.push(rule_id);
                    
                    // Immediately modify rule
                    let modified_rule = AclRule {
                        principal: format!("rapid-change-user-{}", i),
                        resource_pattern: ResourcePattern::Topic(format!("rapid.{}.modified.*", i)),
                        operation: Permission::Write,
                        effect: Effect::Allow,
                    };
                    
                    acl_manager.update_rule(&rule_id, modified_rule).await.unwrap();
                    
                    // Every 10 rules, delete some older ones
                    if i % 10 == 0 && rule_ids.len() > 5 {
                        for _ in 0..5 {
                            if let Some(old_rule_id) = rule_ids.pop() {
                                acl_manager.delete_rule(&old_rule_id).await.unwrap();
                            }
                        }
                    }
                    
                    // Small delay to allow processing
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                
                (true, rule_ids.len())
            })
        };
        
        // Stress Condition 2: High-frequency authentication/authorization
        let auth_stress_handle = {
            let producer_client = producer_client.clone();
            let consumer_client = consumer_client.clone();
            let broker_count = cluster.brokers.len();
            tokio::spawn(async move {
                let mut success_count = 0;
                let mut error_count = 0;
                
                for i in 0..1000 {
                    let broker_index = i % broker_count;
                    
                    // Producer operations
                    match producer_client.connect_to_broker(broker_index).await {
                        Ok(connection) => {
                            let topic = format!("stress.topic.{}", i);
                            match connection.authorize(&topic, Permission::Write).await {
                                Ok(authorized) => {
                                    if authorized {
                                        success_count += 1;
                                    } else {
                                        error_count += 1;
                                    }
                                }
                                Err(_) => error_count += 1,
                            }
                        }
                        Err(_) => error_count += 1,
                    }
                    
                    // Consumer operations
                    match consumer_client.connect_to_broker(broker_index).await {
                        Ok(connection) => {
                            let topic = format!("stress.topic.{}", i);
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
                        }
                        Err(_) => error_count += 1,
                    }
                    
                    // Occasional delay
                    if i % 100 == 0 {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
                
                (success_count, error_count)
            })
        };
        
        // Stress Condition 3: Certificate churn
        let cert_stress_handle = {
            let cert_manager = cluster.cert_manager.clone();
            tokio::spawn(async move {
                let mut issued_certs = Vec::new();
                let mut issued_count = 0;
                let mut revoked_count = 0;
                
                for i in 0..50 {
                    // Issue certificate
                    let cert_request = CertificateRequest {
                        common_name: format!("stress-cert-{}", i),
                        organization: Some("RustMQ Stress Test".to_string()),
                        organizational_unit: Some("Stress".to_string()),
                        country: None,
                        state: None,
                        locality: None,
                        email: None,
                        subject_alt_names: vec![format!("stress-cert-{}.test", i)],
                        key_type: KeyType::Rsa2048,
                        key_usage: vec![KeyUsage::DigitalSignature],
                        extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                        validity_days: 30,
                        role: CertificateRole::Client,
                    };
                    
                    match cert_manager.issue_certificate(&cert_request).await {
                        Ok(cert) => {
                            issued_count += 1;
                            issued_certs.push(cert.serial_number);
                        }
                        Err(_) => {}
                    }
                    
                    // Revoke some certificates
                    if i % 5 == 0 && !issued_certs.is_empty() {
                        let serial_to_revoke = issued_certs.remove(0);
                        match cert_manager.revoke_certificate(
                            &serial_to_revoke,
                            RevocationReason::CessationOfOperation,
                            "Stress test revocation".to_string(),
                        ).await {
                            Ok(_) => revoked_count += 1,
                            Err(_) => {}
                        }
                    }
                    
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                
                (issued_count, revoked_count)
            })
        };
        
        // Stress Condition 4: Simulated network issues
        let network_stress_handle = {
            let cluster_ref = &cluster;
            tokio::spawn(async move {
                let mut network_success = 0;
                let mut network_failures = 0;
                
                for i in 0..200 {
                    // Simulate network latency
                    cluster_ref.simulate_network_latency().await;
                    
                    // Simulate occasional network failures
                    if cluster_ref.simulate_network_failure().await {
                        network_failures += 1;
                        tokio::time::sleep(Duration::from_millis(100)).await; // Failure recovery time
                    } else {
                        network_success += 1;
                    }
                    
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                
                (network_success, network_failures)
            })
        };
        
        // Wait for all stress conditions to complete
        let (rapid_result, auth_result, cert_result, network_result) = tokio::join!(
            rapid_changes_handle,
            auth_stress_handle,
            cert_stress_handle,
            network_stress_handle
        );
        
        let total_stress_time = rapid_changes_start.elapsed();
        
        // Analyze stress test results
        let (rapid_success, remaining_rules) = rapid_result.unwrap();
        let (auth_success, auth_errors) = auth_result.unwrap();
        let (issued_certs, revoked_certs) = cert_result.unwrap();
        let (network_success, network_failures) = network_result.unwrap();
        
        println!("Stress test results:");
        println!("  Total stress time: {:?}", total_stress_time);
        println!("  Rapid ACL changes: Success={}, Remaining rules={}", rapid_success, remaining_rules);
        println!("  Auth operations: Success={}, Errors={}", auth_success, auth_errors);
        println!("  Certificate operations: Issued={}, Revoked={}", issued_certs, revoked_certs);
        println!("  Network operations: Success={}, Failures={}", network_success, network_failures);
        
        // Resilience assertions
        assert!(rapid_success, "Rapid ACL changes should complete successfully");
        
        let auth_success_rate = auth_success as f64 / (auth_success + auth_errors) as f64;
        assert!(auth_success_rate > 0.8, 
               "Auth success rate should be > 80% under stress, got {:.2}", auth_success_rate);
        
        assert!(issued_certs >= 40, 
               "Should issue at least 40 certificates under stress, got {}", issued_certs);
        assert!(revoked_certs >= 8, 
               "Should revoke at least 8 certificates under stress, got {}", revoked_certs);
        
        // Test system recovery after stress
        println!("Testing system recovery after stress...");
        
        // Verify ACL system is still functional
        let recovery_rule = AclRule {
            principal: "recovery-test-user".to_string(),
            resource_pattern: ResourcePattern::Topic("recovery.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        let recovery_rule_id = cluster.acl_manager.add_rule(recovery_rule).await.unwrap();
        assert!(!recovery_rule_id.is_empty());
        
        // Verify authentication/authorization still works
        let recovery_client = cluster.create_test_client("recovery-test-user").await.unwrap();
        let recovery_connection = recovery_client.connect_to_broker(0).await.unwrap();
        let recovery_authz = recovery_connection.authorize("recovery.test", Permission::Read).await.unwrap();
        assert!(recovery_authz);
        
        // Verify certificate system is still functional
        let recovery_cert_request = CertificateRequest {
            common_name: "recovery-test-cert".to_string(),
            organization: Some("RustMQ Recovery Test".to_string()),
            organizational_unit: Some("Recovery".to_string()),
            country: None,
            state: None,
            locality: None,
            email: None,
            subject_alt_names: vec!["recovery-test-cert.test".to_string()],
            key_type: KeyType::Rsa2048,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
            validity_days: 30,
            role: CertificateRole::Client,
        };
        
        let recovery_cert = cluster.cert_manager.issue_certificate(&recovery_cert_request).await.unwrap();
        assert!(!recovery_cert.serial_number.is_empty());
        
        println!("System recovery verified - all security components functional after stress");
        
        cluster.shutdown().await.unwrap();
    }
}

/// Helper function to get current memory usage in MB
fn get_memory_usage() -> u64 {
    use std::fs;
    
    // Read from /proc/self/status on Linux
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return kb / 1024; // Convert KB to MB
                    }
                }
            }
        }
    }
    
    // Fallback: estimate based on allocation (not accurate but useful for testing)
    std::hint::black_box(100) // Return a placeholder value
}

#[cfg(test)]
mod performance_benchmark_tests {
    use super::*;
    
    /// Benchmark test: Security operation latencies
    #[tokio::test]
    async fn benchmark_security_operation_latencies() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Setup for benchmarking
        let warmup_rule = AclRule {
            principal: "benchmark-user".to_string(),
            resource_pattern: ResourcePattern::Topic("benchmark.*".to_string()),
            operation: Permission::Read,
            effect: Effect::Allow,
        };
        
        cluster.acl_manager.add_rule(warmup_rule).await.unwrap();
        cluster.acl_manager.synchronize_acl_to_brokers().await.unwrap();
        
        let client = cluster.create_test_client("benchmark-user").await.unwrap();
        let connection = client.connect_to_broker(0).await.unwrap();
        
        // Warmup
        for _ in 0..100 {
            connection.authorize("benchmark.warmup", Permission::Read).await.unwrap();
        }
        
        // Benchmark authentication
        let auth_iterations = 1000;
        let mut auth_latencies = Vec::new();
        
        for _ in 0..auth_iterations {
            let start = Instant::now();
            let _auth_result = client.authenticate().await.unwrap();
            auth_latencies.push(start.elapsed());
        }
        
        // Benchmark authorization
        let authz_iterations = 10000;
        let mut authz_latencies = Vec::new();
        
        for i in 0..authz_iterations {
            let topic = format!("benchmark.topic.{}", i);
            let start = Instant::now();
            connection.authorize(&topic, Permission::Read).await.unwrap();
            authz_latencies.push(start.elapsed());
        }
        
        // Calculate benchmark statistics
        auth_latencies.sort();
        authz_latencies.sort();
        
        let auth_min = auth_latencies[0];
        let auth_median = auth_latencies[auth_latencies.len() / 2];
        let auth_p95 = auth_latencies[(auth_latencies.len() as f64 * 0.95) as usize];
        let auth_p99 = auth_latencies[(auth_latencies.len() as f64 * 0.99) as usize];
        let auth_max = auth_latencies[auth_latencies.len() - 1];
        
        let authz_min = authz_latencies[0];
        let authz_median = authz_latencies[authz_latencies.len() / 2];
        let authz_p95 = authz_latencies[(authz_latencies.len() as f64 * 0.95) as usize];
        let authz_p99 = authz_latencies[(authz_latencies.len() as f64 * 0.99) as usize];
        let authz_max = authz_latencies[authz_latencies.len() - 1];
        
        println!("Security Operation Latency Benchmarks:");
        println!("Authentication ({} iterations):", auth_iterations);
        println!("  Min: {:?}, Median: {:?}, P95: {:?}, P99: {:?}, Max: {:?}", 
                auth_min, auth_median, auth_p95, auth_p99, auth_max);
        
        println!("Authorization ({} iterations):", authz_iterations);
        println!("  Min: {:?}, Median: {:?}, P95: {:?}, P99: {:?}, Max: {:?}", 
                authz_min, authz_median, authz_p95, authz_p99, authz_max);
        
        // Performance targets for benchmarking
        assert!(auth_median < Duration::from_millis(10), 
               "Authentication median latency target: < 10ms, got {:?}", auth_median);
        assert!(auth_p95 < Duration::from_millis(25), 
               "Authentication P95 latency target: < 25ms, got {:?}", auth_p95);
        
        assert!(authz_median < Duration::from_micros(500), 
               "Authorization median latency target: < 500µs, got {:?}", authz_median);
        assert!(authz_p95 < Duration::from_millis(2), 
               "Authorization P95 latency target: < 2ms, got {:?}", authz_p95);
        assert!(authz_p99 < Duration::from_millis(5), 
               "Authorization P99 latency target: < 5ms, got {:?}", authz_p99);
        
        cluster.shutdown().await.unwrap();
    }
}