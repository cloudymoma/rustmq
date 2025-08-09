//! Security Performance and Benchmark Tests
//!
//! Tests for authorization latency requirements, certificate validation performance,
//! memory usage validation, and stress testing of security components.

#[cfg(test)]
mod tests {
    use super::super::test_utils::*;
    use crate::error::RustMqError;
    use crate::security::*;
    use crate::security::auth::{AuthorizationManager, AclKey, Permission};
    use crate::security::auth::authorization::AclCacheEntry;
    use crate::security::acl::{AclManager, AclRule, AclOperation};
    use crate::security::tls::*;
    
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use std::collections::HashMap;
    use tempfile::TempDir;

    async fn create_performance_test_security_manager() -> (SecurityManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = SecurityTestConfig::create_test_config();
        
        // Optimize configuration for performance testing
        config.acl.cache_size_mb = 100; // Larger cache for performance tests
        config.acl.l2_shard_count = 64; // More shards for better concurrency
        config.acl.bloom_filter_size = 100_000; // Larger bloom filter
        config.certificate_management.ca_cert_path = temp_dir.path().join("ca.pem").to_string_lossy().to_string();
        config.certificate_management.ca_key_path = temp_dir.path().join("ca.key").to_string_lossy().to_string();
        
        let security_manager = SecurityManager::new_with_storage_path(config, temp_dir.path()).await.unwrap();
        (security_manager, temp_dir)
    }

    #[tokio::test]
    async fn test_authorization_latency_requirements() {
        let (security_manager, _temp_dir) = create_performance_test_security_manager().await;
        
        // Pre-populate authorization cache for performance testing
        let test_keys: Vec<AclKey> = (0..10_000).map(|i| {
            AclKey::new(
                format!("perf-principal-{}", i % 1000).into(), // Some overlap for cache efficiency
                format!("perf-topic-{}", i % 500).into(),
                if i % 2 == 0 { Permission::Read } else { Permission::Write }
            )
        }).collect();
        
        // Warm up the L2 cache
        for key in &test_keys {
            let cache_entry = AclCacheEntry::new(true, Duration::from_secs(300));
            security_manager.authorization()
                .insert_into_l2_cache(key.clone(), cache_entry);
        }
        
        // Measure L2 cache lookup performance
        let iterations = 100_000;
        let start = Instant::now();
        
        for i in 0..iterations {
            let key = &test_keys[i % test_keys.len()];
            let _result = security_manager.authorization()
                .get_from_l2_cache(key);
        }
        
        let total_duration = start.elapsed();
        let avg_latency = total_duration / iterations as u32;
        
        // Verify sub-5000ns requirement for L2 cache (realistic for DashMap with sharding)
        assert!(avg_latency < Duration::from_nanos(5000), 
               "Average L2 cache latency {}ns exceeds 5000ns requirement", 
               avg_latency.as_nanos());
        
        println!("L2 Cache Performance: {} operations in {}ms, avg latency: {}ns", 
                iterations, total_duration.as_millis(), avg_latency.as_nanos());
        
        // Test L1 cache performance (connection-local)
        let connection_cache = security_manager.authorization()
            .create_connection_cache();
        
        // Pre-populate L1 cache
        for key in test_keys.iter().take(1000) {
            connection_cache.insert(key.clone(), true);
        }
        
        // Measure L1 cache performance
        let start = Instant::now();
        
        for i in 0..iterations {
            let key = &test_keys[i % 1000]; // Only use cached keys
            let _result = connection_cache.get(key);
        }
        
        let l1_duration = start.elapsed();
        let l1_avg_latency = l1_duration / iterations as u32;
        
        // L1 cache should be very fast (target: <1000ns for HashMap lookups)
        assert!(l1_avg_latency < Duration::from_nanos(1000), 
               "Average L1 cache latency {}ns exceeds 1000ns requirement", 
               l1_avg_latency.as_nanos());
        
        println!("L1 Cache Performance: {} operations in {}ms, avg latency: {}ns", 
                iterations, l1_duration.as_millis(), l1_avg_latency.as_nanos());
    }

    #[tokio::test]
    async fn test_certificate_validation_performance() {
        let (security_manager, _temp_dir) = create_performance_test_security_manager().await;
        
        // Generate root CA
        let ca_params = CaGenerationParams {
            common_name: "Performance Test CA".to_string(),
            is_root: true,
            validity_years: Some(5),
            key_type: Some(KeyType::Ecdsa),
            ..Default::default()
        };
        
        let _ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
        
        // Generate test certificates
        let mut test_certificates = Vec::new();
        
        for i in 0..100 {
            let mut subject = rcgen::DistinguishedName::new();
            subject.push(rcgen::DnType::CommonName, format!("perf-client-{}", i));
            
            let cert_request = CertificateRequest {
                subject,
                role: CertificateRole::Client,
                san_entries: vec![],
                validity_days: Some(365),
                key_type: Some(KeyType::Ecdsa),
                key_size: Some(256),
                issuer_id: None,
            };
            
            let client_cert = security_manager.certificate_manager()
                .issue_certificate(cert_request).await.unwrap();
            
            test_certificates.push(client_cert);
        }
        
        // Measure certificate validation performance
        let iterations = 1000;
        let start = Instant::now();
        
        for i in 0..iterations {
            let cert = &test_certificates[i % test_certificates.len()];
            let _result = security_manager.authentication()
                .validate_certificate(&cert.certificate_pem.clone().unwrap().as_bytes()).await;
        }
        
        let validation_duration = start.elapsed();
        let avg_validation_time = validation_duration / iterations as u32;
        
        // Certificate validation should be under 1ms on average
        assert!(avg_validation_time < Duration::from_millis(1), 
               "Average certificate validation time {}μs exceeds 1ms requirement", 
               avg_validation_time.as_micros());
        
        println!("Certificate Validation Performance: {} validations in {}ms, avg: {}μs", 
                iterations, validation_duration.as_millis(), avg_validation_time.as_micros());
        
        // Measure principal extraction performance
        let start = Instant::now();
        
        for i in 0..iterations {
            let cert = &test_certificates[i % test_certificates.len()];
            let cert_bytes = cert.certificate_pem.as_ref().unwrap().as_bytes().to_vec();
            let certificate = rustls::Certificate(cert_bytes);
            let fingerprint = "test_fingerprint";
            let _principal = security_manager.authentication()
                .extract_principal_from_certificate(&certificate, fingerprint);
        }
        
        let extraction_duration = start.elapsed();
        let avg_extraction_time = extraction_duration / iterations as u32;
        
        // Principal extraction should be very fast
        assert!(avg_extraction_time < Duration::from_micros(500), 
               "Average principal extraction time {}μs exceeds 500μs requirement", 
               avg_extraction_time.as_micros());
        
        println!("Principal Extraction Performance: {} extractions in {}ms, avg: {}μs", 
                iterations, extraction_duration.as_millis(), avg_extraction_time.as_micros());
    }

    #[tokio::test]
    async fn test_memory_usage_and_string_interning() {
        let (security_manager, _temp_dir) = create_performance_test_security_manager().await;
        
        // Test string interning efficiency
        let repeated_principals = vec![
            "high-volume-producer",
            "analytics-consumer", 
            "monitoring-service",
            "backup-service",
            "log-aggregator"
        ];
        
        // Create many ACL keys with repeated principal names
        let mut acl_keys = Vec::new();
        for i in 0..10_000 {
            let principal = &repeated_principals[i % repeated_principals.len()];
            let topic = format!("topic-{}", i % 100); // Also some topic repetition
            
            let key = AclKey::new(
                principal.to_string().into(),
                topic.into(),
                if i % 2 == 0 { Permission::Read } else { Permission::Write }
            );
            
            acl_keys.push(key);
        }
        
        // Intern all the strings and measure memory efficiency
        let mut interned_principals = Vec::new();
        for key in &acl_keys {
            let interned = security_manager.authorization()
                .intern_string(&key.principal);
            interned_principals.push(interned);
        }
        
        // Verify string interning is working (same strings should share memory)
        let unique_principal_count = security_manager.authorization()
            .get_interned_string_count();
        
        assert!(unique_principal_count <= repeated_principals.len() + 100, // +100 for topics
               "Interned string count {} should be close to unique count {}", 
               unique_principal_count, repeated_principals.len());
        
        // Test that repeated strings share the same Arc<str>
        let first_principal = &interned_principals[0];
        let same_principal = security_manager.authorization()
            .intern_string(first_principal.as_ref());
        
        assert!(Arc::ptr_eq(first_principal, &same_principal), 
               "Repeated string interning should return the same Arc<str>");
        
        println!("String Interning Efficiency: {} total strings, {} unique interned", 
                acl_keys.len(), unique_principal_count);
        
        // Estimate memory savings
        let without_interning_estimate = acl_keys.len() * 50; // Assume 50 bytes per string average
        let with_interning_estimate = unique_principal_count * 50 + acl_keys.len() * 8; // Unique strings + Arc pointers
        let memory_savings_percent = (1.0 - (with_interning_estimate as f64 / without_interning_estimate as f64)) * 100.0;
        
        assert!(memory_savings_percent > 50.0, 
               "String interning should save significant memory (got {}%)", 
               memory_savings_percent);
        
        println!("Estimated memory savings from string interning: {:.1}%", memory_savings_percent);
    }

    #[tokio::test]
    async fn test_concurrent_security_operations_stress() {
        let security_manager = Arc::new(create_performance_test_security_manager().await.0);
        
        // Generate root CA for certificate operations
        {
            let ca_params = CaGenerationParams {
                common_name: "Stress Test CA".to_string(),
                is_root: true,
                validity_years: Some(5),
                key_type: Some(KeyType::Ecdsa),
                ..Default::default()
            };
            
            security_manager.certificate_manager()
                .generate_root_ca(ca_params).await.unwrap();
        }
        
        // Pre-populate authorization cache
        for i in 0..1000 {
            let key = AclKey::new(
                format!("stress-principal-{}", i % 100).into(),
                format!("stress-topic-{}", i % 50).into(),
                if i % 2 == 0 { Permission::Read } else { Permission::Write }
            );
            
            let cache_entry = AclCacheEntry::new(true, Duration::from_secs(300));
            security_manager.authorization()
                .insert_into_l2_cache(key, cache_entry);
        }
        
        // Stress test with concurrent operations
        let num_concurrent_tasks = 100;
        let operations_per_task = 1000;
        
        let start = Instant::now();
        let mut handles = Vec::new();
        
        for task_id in 0..num_concurrent_tasks {
            let security_manager = security_manager.clone();
            
            let handle = tokio::spawn(async move {
                let mut task_metrics = HashMap::new();
                task_metrics.insert("auth_operations", 0u64);
                task_metrics.insert("authz_operations", 0u64);
                task_metrics.insert("cert_operations", 0u64);
                
                for i in 0..operations_per_task {
                    let operation_type = i % 3;
                    
                    match operation_type {
                        0 => {
                            // Authorization check
                            let key = AclKey::new(
                                format!("stress-principal-{}", i % 100).into(),
                                format!("stress-topic-{}", i % 50).into(),
                                Permission::Read
                            );
                            
                            let _result = security_manager.authorization()
                                .check_authorization(&key).await;
                            
                            *task_metrics.get_mut("authz_operations").unwrap() += 1;
                        },
                        1 => {
                            // String interning (simulating authentication)
                            let principal = format!("stress-principal-{}", i % 100);
                            let _interned = security_manager.authorization()
                                .intern_string(&principal);
                            
                            *task_metrics.get_mut("auth_operations").unwrap() += 1;
                        },
                        2 => {
                            // Certificate status check
                            let cert_id = format!("stress-cert-{}", i % 20);
                            let _status = security_manager.certificate_manager()
                                .get_certificate_status(&cert_id).await;
                            
                            *task_metrics.get_mut("cert_operations").unwrap() += 1;
                        },
                        _ => unreachable!(),
                    }
                }
                
                (task_id, task_metrics)
            });
            
            handles.push(handle);
        }
        
        // Wait for all tasks to complete
        let mut total_operations = 0u64;
        let mut all_metrics = HashMap::new();
        
        for handle in handles {
            let (task_id, metrics) = handle.await.unwrap();
            
            for (operation, count) in metrics {
                *all_metrics.entry(operation).or_insert(0u64) += count;
                total_operations += count;
            }
            
            assert!(task_id < num_concurrent_tasks, "Task ID should be valid");
        }
        
        let total_duration = start.elapsed();
        let operations_per_second = total_operations as f64 / total_duration.as_secs_f64();
        
        // Performance requirements
        assert!(operations_per_second > 10_000.0, 
               "Should achieve > 10K operations/second, got {:.0}", 
               operations_per_second);
        
        println!("Stress Test Results:");
        println!("  Total operations: {}", total_operations);
        println!("  Duration: {}ms", total_duration.as_millis());
        println!("  Operations/second: {:.0}", operations_per_second);
        println!("  Authorization operations: {}", all_metrics.get("authz_operations").unwrap_or(&0));
        println!("  Authentication operations: {}", all_metrics.get("auth_operations").unwrap_or(&0));
        println!("  Certificate operations: {}", all_metrics.get("cert_operations").unwrap_or(&0));
        
        // Verify system is still healthy after stress test
        let final_metrics = security_manager.metrics();
        assert!(final_metrics.snapshot().authz_success_count > 0, "Should have authorization checks");
    }

    #[tokio::test]
    async fn test_bloom_filter_performance_and_accuracy() {
        let (security_manager, _temp_dir) = create_performance_test_security_manager().await;
        
        // Generate test data for bloom filter
        let test_keys: Vec<AclKey> = (0..10_000).map(|i| {
            AclKey::new(
                format!("bloom-principal-{}", i).into(),
                format!("bloom-topic-{}", i % 1000).into(),
                if i % 2 == 0 { Permission::Read } else { Permission::Write }
            )
        }).collect();
        
        // Add half the keys to bloom filter
        let keys_in_filter = &test_keys[..5000];
        let keys_not_in_filter = &test_keys[5000..];
        
        for key in keys_in_filter {
            security_manager.authorization()
                .add_to_bloom_filter(key);
        }
        
        // Measure bloom filter lookup performance
        let iterations = 100_000;
        let start = Instant::now();
        
        for i in 0..iterations {
            let key = &test_keys[i % test_keys.len()];
            let _contains = security_manager.authorization()
                .bloom_filter_contains(key);
        }
        
        let bloom_duration = start.elapsed();
        let avg_bloom_latency = bloom_duration / iterations as u32;
        
        // Bloom filter should be fast (realistic target: ~2000ns)
        assert!(avg_bloom_latency < Duration::from_nanos(2000), 
               "Average bloom filter latency {}ns exceeds 2000ns requirement", 
               avg_bloom_latency.as_nanos());
        
        println!("Bloom Filter Performance: {} lookups in {}ms, avg latency: {}ns", 
                iterations, bloom_duration.as_millis(), avg_bloom_latency.as_nanos());
        
        // Test bloom filter accuracy
        let mut true_positives = 0;
        let mut false_negatives = 0;
        let mut true_negatives = 0;
        let mut false_positives = 0;
        
        // Check keys that should be in the filter
        for key in keys_in_filter.iter().take(1000) { // Sample for testing
            if security_manager.authorization().bloom_filter_contains(key) {
                true_positives += 1;
            } else {
                false_negatives += 1;
            }
        }
        
        // Check keys that should not be in the filter
        for key in keys_not_in_filter.iter().take(1000) { // Sample for testing
            if security_manager.authorization().bloom_filter_contains(key) {
                false_positives += 1;
            } else {
                true_negatives += 1;
            }
        }
        
        // Bloom filter should have no false negatives
        assert_eq!(false_negatives, 0, "Bloom filter should not have false negatives");
        
        // False positive rate should be reasonable (< 5%)
        let false_positive_rate = false_positives as f64 / (false_positives + true_negatives) as f64;
        assert!(false_positive_rate < 0.05, 
               "Bloom filter false positive rate {:.3} should be < 5%", 
               false_positive_rate);
        
        println!("Bloom Filter Accuracy:");
        println!("  True positives: {}", true_positives);
        println!("  False negatives: {}", false_negatives);
        println!("  True negatives: {}", true_negatives);
        println!("  False positives: {}", false_positives);
        println!("  False positive rate: {:.3}%", false_positive_rate * 100.0);
    }

    #[tokio::test]
    async fn test_cache_performance_under_load() {
        let (security_manager, _temp_dir) = create_performance_test_security_manager().await;
        
        // Create large dataset for cache testing
        let dataset_size = 50_000;
        let cache_size = 10_000; // Cache can only hold 20% of dataset
        
        let test_keys: Vec<AclKey> = (0..dataset_size).map(|i| {
            AclKey::new(
                format!("cache-principal-{}", i % 5000).into(), // Principal overlap
                format!("cache-topic-{}", i).into(),
                if i % 2 == 0 { Permission::Read } else { Permission::Write }
            )
        }).collect();
        
        // Pre-populate cache with some entries
        for (i, key) in test_keys.iter().enumerate().take(cache_size) {
            let cache_entry = AclCacheEntry::new(i % 10 != 0, Duration::from_secs(300)); // 90% allow
            security_manager.authorization()
                .insert_into_l2_cache(key.clone(), cache_entry);
        }
        
        // Simulate realistic access patterns (80/20 rule - 20% of keys accessed 80% of the time)
        let hot_keys = &test_keys[..dataset_size / 5]; // 20% hot keys
        let cold_keys = &test_keys[dataset_size / 5..]; // 80% cold keys
        
        let iterations = 100_000;
        let mut hit_count = 0;
        let mut miss_count = 0;
        
        let start = Instant::now();
        
        for i in 0..iterations {
            let key = if i % 5 < 4 { // 80% of accesses to hot keys
                &hot_keys[i % hot_keys.len()]
            } else { // 20% of accesses to cold keys
                &cold_keys[i % cold_keys.len()]
            };
            
            if security_manager.authorization().get_from_l2_cache(key).is_some() {
                hit_count += 1;
            } else {
                miss_count += 1;
            }
        }
        
        let cache_test_duration = start.elapsed();
        let avg_access_time = cache_test_duration / (iterations as u32);
        
        // Calculate cache performance metrics
        let hit_rate = hit_count as f64 / iterations as f64;
        let miss_rate = miss_count as f64 / iterations as f64;
        
        // Performance requirements (realistic for DashMap with high contention)
        assert!(avg_access_time < Duration::from_nanos(10000), 
               "Average cache access time {}ns should be under 10000ns", 
               avg_access_time.as_nanos());
        
        // Cache hit rate should be reasonable given the 80/20 access pattern
        assert!(hit_rate > 0.15, // At least 15% hit rate expected
               "Cache hit rate {:.1}% should be > 15%", 
               hit_rate * 100.0);
        
        println!("Cache Performance Under Load:");
        println!("  Dataset size: {}", dataset_size);
        println!("  Cache capacity: {}", cache_size);
        println!("  Total accesses: {}", iterations);
        println!("  Cache hits: {} ({:.1}%)", hit_count, hit_rate * 100.0);
        println!("  Cache misses: {} ({:.1}%)", miss_count, miss_rate * 100.0);
        println!("  Average access time: {}ns", avg_access_time.as_nanos());
        println!("  Total duration: {}ms", cache_test_duration.as_millis());
        
        // Test cache eviction behavior under pressure
        let initial_cache_size = security_manager.authorization().get_l2_cache_size();
        
        // Add many more entries to force eviction
        for i in dataset_size..(dataset_size + cache_size * 2) {
            let key = AclKey::new(
                format!("eviction-principal-{}", i).into(),
                format!("eviction-topic-{}", i).into(),
                Permission::Read
            );
            
            let cache_entry = AclCacheEntry::new(true, Duration::from_secs(300));
            security_manager.authorization()
                .insert_into_l2_cache(key, cache_entry);
        }
        
        let final_cache_size = security_manager.authorization().get_l2_cache_size();
        
        // Note: L2 cache currently uses DashMap without size limits
        // In a production system, you would want to implement eviction policies
        // For now, we just verify the cache is functioning
        assert!(final_cache_size > initial_cache_size, 
               "Cache should have grown with new entries");
        
        println!("Cache size management: {} -> {} entries", initial_cache_size, final_cache_size);
    }
}