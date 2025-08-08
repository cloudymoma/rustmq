//! Comprehensive Security Performance Benchmarks for RustMQ
//!
//! This benchmark suite validates that RustMQ's security system achieves sub-100ns 
//! authorization latency and meets all performance requirements for enterprise deployment.
//!
//! ## Performance Targets
//! - L1 Cache Authorization: ~10ns latency
//! - L2 Cache Authorization: ~50ns latency  
//! - L3 Bloom Filter Rejection: ~20ns latency
//! - Cache Miss Authorization: <1ms latency
//! - Overall Authorization Target: <100ns for cache hits
//! - Authentication Latency: <10ms end-to-end mTLS
//! - Certificate Validation: <5ms for full chain validation
//! - ACL Synchronization: <100ms cluster-wide

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;

mod utils;
use utils::{
    data_generators::*,
    performance_helpers::*,
    setup_test_environment,
};

// Import the actual RustMQ security modules
use rustmq::security::{
    auth::{
        AuthorizationManager, AuthenticationManager, ConnectionAclCache,
        AclKey, Principal, Permission, AclCacheEntry,
    },
    tls::{CertificateManager, CertificateConfig},
    SecurityMetrics, AclConfig,
};

/// Main authorization latency benchmarks targeting sub-100ns performance
fn authorization_latency_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("authorization_latency");
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(3));
    
    // Setup test environment
    let rt = Runtime::new().unwrap();
    let (auth_mgr, test_data) = rt.block_on(async {
        setup_authorization_manager().await
    });
    
    // L1 Cache Hit Benchmark - Target: ~10ns
    group.bench_function("l1_cache_hit", |b| {
        let cache = ConnectionAclCache::new(1000);
        let key = generate_acl_key("user1", "topic1", Permission::Read);
        
        // Pre-populate cache
        cache.insert(key.clone(), true);
        
        b.iter(|| {
            let result = cache.get(&key);
            black_box(result)
        });
    });
    
    // L2 Cache Hit Benchmark - Target: ~50ns
    group.bench_function("l2_cache_hit", |b| {
        let key = generate_acl_key("user2", "topic2", Permission::Write);
        
        // Pre-populate L2 cache
        rt.block_on(async {
            auth_mgr.populate_l2_cache(key.clone(), true).await;
        });
        
        b.to_async(&rt).iter(|| async {
            let result = auth_mgr.check_l2_cache(&key).await;
            black_box(result)
        });
    });
    
    // L3 Bloom Filter Benchmark - Target: ~20ns
    group.bench_function("l3_bloom_filter_rejection", |b| {
        let key = generate_acl_key("denied_user", "restricted_topic", Permission::Admin);
        
        // Add to negative cache
        rt.block_on(async {
            auth_mgr.add_to_negative_cache(key.clone()).await;
        });
        
        b.to_async(&rt).iter(|| async {
            let result = auth_mgr.check_negative_cache(&key).await;
            black_box(result)
        });
    });
    
    // Cache Miss with Storage Fetch - Target: <1ms
    group.bench_function("cache_miss_with_fetch", |b| {
        b.to_async(&rt).iter(|| async {
            let key = generate_random_acl_key();
            let result = auth_mgr.authorize_with_fetch(&key).await;
            black_box(result)
        });
    });
    
    // Combined Authorization Path (Realistic Scenario)
    group.bench_function("combined_authorization_path", |b| {
        let keys = generate_acl_key_batch(100);
        let mut key_iter = keys.iter().cycle();
        
        b.to_async(&rt).iter(|| async {
            let key = key_iter.next().unwrap();
            let result = auth_mgr.authorize(key).await;
            black_box(result)
        });
    });
    
    group.finish();
}

/// Authentication performance benchmarks
fn authentication_latency_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("authentication_latency");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let (auth_mgr, cert_mgr, test_certs) = rt.block_on(async {
        setup_authentication_environment().await
    });
    
    // Certificate Parsing and Caching - Target: <1ms
    group.bench_function("certificate_parsing", |b| {
        let cert_bytes = test_certs.client_cert_der.clone();
        
        b.to_async(&rt).iter(|| async {
            let result = cert_mgr.parse_certificate(&cert_bytes).await;
            black_box(result)
        });
    });
    
    // Principal Extraction Performance
    group.bench_function("principal_extraction", |b| {
        let cert = test_certs.parsed_client_cert.clone();
        
        b.iter(|| {
            let principal = extract_principal_from_cert(&cert);
            black_box(principal)
        });
    });
    
    // Certificate Chain Validation - Target: <5ms
    group.bench_function("certificate_chain_validation", |b| {
        let chain = test_certs.cert_chain.clone();
        
        b.to_async(&rt).iter(|| async {
            let result = cert_mgr.validate_chain(&chain).await;
            black_box(result)
        });
    });
    
    // mTLS Handshake Overhead - Target: <10ms
    group.bench_function("mtls_handshake", |b| {
        b.to_async(&rt).iter(|| async {
            let result = perform_mtls_handshake(&test_certs).await;
            black_box(result)
        });
    });
    
    // Certificate Revocation Check
    group.bench_function("certificate_revocation_check", |b| {
        let cert = test_certs.parsed_client_cert.clone();
        
        b.to_async(&rt).iter(|| async {
            let result = cert_mgr.check_revocation(&cert).await;
            black_box(result)
        });
    });
    
    // Authentication Context Creation
    group.bench_function("auth_context_creation", |b| {
        let cert = test_certs.parsed_client_cert.clone();
        
        b.to_async(&rt).iter(|| async {
            let context = auth_mgr.create_context(&cert).await;
            black_box(context)
        });
    });
    
    group.finish();
}

/// Memory usage benchmarks
fn memory_usage_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    group.measurement_time(Duration::from_secs(15));
    
    let rt = Runtime::new().unwrap();
    
    // String Interning Memory Savings - Target: 60-80% reduction
    group.bench_function("string_interning_savings", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = Duration::ZERO;
            
            for _ in 0..iters {
                let start = std::time::Instant::now();
                
                // Measure memory with string interning
                let interned_memory = measure_interned_string_memory(10_000);
                
                // Measure memory without string interning
                let regular_memory = measure_regular_string_memory(10_000);
                
                let savings_percent = ((regular_memory - interned_memory) as f64 
                    / regular_memory as f64) * 100.0;
                
                assert!(savings_percent >= 60.0, 
                    "String interning should save at least 60% memory, got {}%", savings_percent);
                
                total_duration += start.elapsed();
            }
            
            total_duration
        });
    });
    
    // ACL Cache Memory Footprint
    for size in [1_000, 10_000, 100_000].iter() {
        group.bench_with_input(
            BenchmarkId::new("acl_cache_memory", size),
            size,
            |b, &size| {
                b.iter_custom(|iters| {
                    let mut total_duration = Duration::ZERO;
                    
                    for _ in 0..iters {
                        let start = std::time::Instant::now();
                        let memory_used = measure_acl_cache_memory(size);
                        black_box(memory_used);
                        total_duration += start.elapsed();
                    }
                    
                    total_duration
                });
            },
        );
    }
    
    // Certificate Cache Memory Efficiency
    group.bench_function("certificate_cache_memory", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = Duration::ZERO;
            
            for _ in 0..iters {
                let start = std::time::Instant::now();
                let memory_used = measure_certificate_cache_memory(1000);
                black_box(memory_used);
                total_duration += start.elapsed();
            }
            
            total_duration
        });
    });
    
    // Memory Allocation Patterns
    group.bench_function("memory_allocation_patterns", |b| {
        b.to_async(&rt).iter(|| async {
            let patterns = analyze_memory_allocation_patterns().await;
            black_box(patterns)
        });
    });
    
    group.finish();
}

/// Scalability benchmarks
fn scalability_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");
    group.measurement_time(Duration::from_secs(20));
    group.sample_size(50);
    
    let rt = Runtime::new().unwrap();
    let auth_mgr = rt.block_on(async {
        setup_authorization_manager().await.0
    });
    
    // Performance with varying number of ACL rules
    for rule_count in [1_000, 10_000, 100_000].iter() {
        group.throughput(Throughput::Elements(*rule_count as u64));
        group.bench_with_input(
            BenchmarkId::new("acl_rules", rule_count),
            rule_count,
            |b, &rule_count| {
                let rules = generate_acl_rules(rule_count);
                
                b.to_async(&rt).iter(|| async {
                    let result = auth_mgr.authorize_with_rules(&rules).await;
                    black_box(result)
                });
            },
        );
    }
    
    // Performance with varying number of principals
    for principal_count in [1_000, 10_000, 100_000].iter() {
        group.throughput(Throughput::Elements(*principal_count as u64));
        group.bench_with_input(
            BenchmarkId::new("principals", principal_count),
            principal_count,
            |b, &principal_count| {
                let principals = generate_principals(principal_count);
                
                b.to_async(&rt).iter(|| async {
                    for principal in principals.iter().take(100) {
                        let result = auth_mgr.authorize_principal(principal).await;
                        black_box(result);
                    }
                });
            },
        );
    }
    
    // Concurrent authorization throughput - Target: >100K ops/sec
    for thread_count in [1, 10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_threads", thread_count),
            thread_count,
            |b, &thread_count| {
                b.to_async(&rt).iter(|| async {
                    let throughput = measure_concurrent_throughput(
                        auth_mgr.clone(),
                        thread_count,
                        Duration::from_secs(1)
                    ).await;
                    
                    assert!(throughput > 100_000, 
                        "Should achieve >100K ops/sec, got {}", throughput);
                    
                    black_box(throughput)
                });
            },
        );
    }
    
    // Cache contention measurement
    group.bench_function("cache_contention", |b| {
        b.to_async(&rt).iter(|| async {
            let contention = measure_cache_contention(100).await;
            black_box(contention)
        });
    });
    
    // Distributed ACL synchronization - Target: <100ms
    group.bench_function("acl_synchronization", |b| {
        b.to_async(&rt).iter(|| async {
            let sync_time = measure_acl_sync_time(10).await;
            assert!(sync_time.as_millis() < 100, 
                "ACL sync should complete in <100ms, took {:?}", sync_time);
            black_box(sync_time)
        });
    });
    
    group.finish();
}

/// Certificate management benchmarks
fn certificate_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("certificate_management");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    let cert_mgr = rt.block_on(async {
        setup_certificate_manager().await
    });
    
    // RSA Certificate Generation
    group.bench_function("rsa_certificate_generation", |b| {
        b.to_async(&rt).iter(|| async {
            let cert = cert_mgr.generate_rsa_certificate(2048).await;
            black_box(cert)
        });
    });
    
    // ECDSA Certificate Generation (should be faster)
    group.bench_function("ecdsa_certificate_generation", |b| {
        b.to_async(&rt).iter(|| async {
            let cert = cert_mgr.generate_ecdsa_certificate().await;
            black_box(cert)
        });
    });
    
    // Certificate Storage and Retrieval
    group.bench_function("certificate_storage_retrieval", |b| {
        let cert_id = "test_cert_001";
        let cert_data = generate_test_certificate();
        
        b.to_async(&rt).iter(|| async {
            cert_mgr.store_certificate(cert_id, &cert_data).await.unwrap();
            let retrieved = cert_mgr.retrieve_certificate(cert_id).await;
            black_box(retrieved)
        });
    });
    
    // Certificate Validation Against CA Chain
    group.bench_function("ca_chain_validation", |b| {
        let cert = generate_test_certificate();
        let ca_chain = generate_ca_chain();
        
        b.to_async(&rt).iter(|| async {
            let result = cert_mgr.validate_against_ca(&cert, &ca_chain).await;
            black_box(result)
        });
    });
    
    // Certificate Renewal Performance
    group.bench_function("certificate_renewal", |b| {
        let old_cert = generate_test_certificate();
        
        b.to_async(&rt).iter(|| async {
            let new_cert = cert_mgr.renew_certificate(&old_cert).await;
            black_box(new_cert)
        });
    });
    
    // CRL Operations
    group.bench_function("crl_operations", |b| {
        let crl = generate_test_crl(1000);
        let cert = generate_test_certificate();
        
        b.to_async(&rt).iter(|| async {
            let is_revoked = cert_mgr.check_crl(&cert, &crl).await;
            black_box(is_revoked)
        });
    });
    
    group.finish();
}

/// Network performance benchmarks with security
fn network_performance_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("network_security");
    group.measurement_time(Duration::from_secs(10));
    
    let rt = Runtime::new().unwrap();
    
    // mTLS Connection Establishment - Target: <10ms
    group.bench_function("mtls_connection_establishment", |b| {
        b.to_async(&rt).iter(|| async {
            let conn_time = measure_mtls_connection_time().await;
            assert!(conn_time.as_millis() < 10, 
                "mTLS connection should establish in <10ms, took {:?}", conn_time);
            black_box(conn_time)
        });
    });
    
    // Authenticated Request Processing Overhead
    group.bench_function("authenticated_request_overhead", |b| {
        let request = generate_authenticated_request();
        
        b.to_async(&rt).iter(|| async {
            let overhead = measure_auth_request_overhead(&request).await;
            black_box(overhead)
        });
    });
    
    // QUIC Protocol Performance with Security
    group.bench_function("quic_with_security", |b| {
        b.to_async(&rt).iter(|| async {
            let throughput = measure_quic_throughput_with_security().await;
            black_box(throughput)
        });
    });
    
    // Security Metadata Transmission Efficiency
    group.bench_function("security_metadata_transmission", |b| {
        let metadata = generate_security_metadata();
        
        b.to_async(&rt).iter(|| async {
            let transmission_time = measure_metadata_transmission(&metadata).await;
            black_box(transmission_time)
        });
    });
    
    group.finish();
}

/// Performance validation and reporting
fn performance_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("performance_validation");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(100);
    
    let rt = Runtime::new().unwrap();
    
    // Comprehensive Performance Validation
    group.bench_function("comprehensive_validation", |b| {
        b.to_async(&rt).iter(|| async {
            let report = run_comprehensive_validation().await;
            
            // Validate all performance targets
            assert!(report.l1_cache_latency_ns <= 15, 
                "L1 cache latency exceeds target: {}ns > 15ns", report.l1_cache_latency_ns);
            
            assert!(report.l2_cache_latency_ns <= 60, 
                "L2 cache latency exceeds target: {}ns > 60ns", report.l2_cache_latency_ns);
            
            assert!(report.l3_bloom_latency_ns <= 25, 
                "L3 bloom filter latency exceeds target: {}ns > 25ns", report.l3_bloom_latency_ns);
            
            assert!(report.cache_miss_latency_us < 1000, 
                "Cache miss latency exceeds target: {}μs > 1000μs", report.cache_miss_latency_us);
            
            assert!(report.auth_throughput_ops_sec > 100_000, 
                "Authorization throughput below target: {} < 100K ops/sec", 
                report.auth_throughput_ops_sec);
            
            assert!(report.memory_reduction_percent >= 60.0, 
                "Memory reduction below target: {}% < 60%", report.memory_reduction_percent);
            
            assert!(report.mtls_handshake_ms < 10, 
                "mTLS handshake exceeds target: {}ms > 10ms", report.mtls_handshake_ms);
            
            assert!(report.cert_validation_ms < 5, 
                "Certificate validation exceeds target: {}ms > 5ms", report.cert_validation_ms);
            
            assert!(report.acl_sync_ms < 100, 
                "ACL synchronization exceeds target: {}ms > 100ms", report.acl_sync_ms);
            
            println!("\n✅ All performance targets achieved!");
            println!("{}", report);
            
            black_box(report)
        });
    });
    
    group.finish();
}

// Configure and run all benchmark groups
criterion_group! {
    name = security_benchmarks;
    config = Criterion::default()
        .significance_level(0.05)
        .noise_threshold(0.03)
        .with_profiler(perf::FlamegraphProfiler::new(100));
    targets = 
        authorization_latency_benchmarks,
        authentication_latency_benchmarks,
        memory_usage_benchmarks,
        scalability_benchmarks,
        certificate_benchmarks,
        network_performance_benchmarks,
        performance_validation
}

criterion_main!(security_benchmarks);