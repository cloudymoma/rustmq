//! Shared benchmark utilities for security performance testing

pub mod data_generators;
pub mod performance_helpers;

use rustmq::security::{
    auth::{AuthorizationManager, AuthenticationManager, AclKey, Principal},
    tls::{CertificateManager, CertificateConfig},
    AclConfig, SecurityMetrics,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Test environment setup for benchmarks
pub struct TestEnvironment {
    pub auth_mgr: Arc<AuthorizationManager>,
    pub authn_mgr: Arc<AuthenticationManager>,
    pub cert_mgr: Arc<CertificateManager>,
    pub metrics: Arc<SecurityMetrics>,
}

/// Setup a complete test environment with all security components
pub async fn setup_test_environment() -> TestEnvironment {
    // Initialize configuration
    let acl_config = AclConfig {
        l1_cache_size: 1000,
        l2_cache_shards: 32,
        l2_cache_ttl_ms: 300_000, // 5 minutes
        l3_bloom_filter_size: 1_000_000,
        l3_false_positive_rate: 0.001,
        batch_fetch_size: 100,
        batch_fetch_timeout_ms: 50,
        enable_string_interning: true,
        sync_interval_ms: 1000,
        cleanup_interval_ms: 60_000,
    };
    
    let cert_config = CertificateConfig {
        ca_cert_path: "/tmp/ca.crt".into(),
        ca_key_path: "/tmp/ca.key".into(),
        cert_validity_days: 365,
        enable_crl_check: true,
        crl_refresh_interval_ms: 3600_000,
        certificate_cache_size: 1000,
        certificate_cache_ttl_ms: 600_000,
    };
    
    // Initialize metrics
    let metrics = Arc::new(SecurityMetrics::new());
    
    // Create managers
    let auth_mgr = Arc::new(
        AuthorizationManager::new(acl_config.clone(), metrics.clone()).await.unwrap()
    );
    
    let authn_mgr = Arc::new(
        AuthenticationManager::new(cert_config.clone(), metrics.clone()).await.unwrap()
    );
    
    let cert_mgr = Arc::new(
        CertificateManager::new(cert_config, metrics.clone()).await.unwrap()
    );
    
    TestEnvironment {
        auth_mgr,
        authn_mgr,
        cert_mgr,
        metrics,
    }
}

/// Setup authorization manager for benchmarks
pub async fn setup_authorization_manager() -> (Arc<AuthorizationManager>, TestData) {
    let config = AclConfig {
        l1_cache_size: 1000,
        l2_cache_shards: 32,
        l2_cache_ttl_ms: 300_000,
        l3_bloom_filter_size: 1_000_000,
        l3_false_positive_rate: 0.001,
        batch_fetch_size: 100,
        batch_fetch_timeout_ms: 50,
        enable_string_interning: true,
        sync_interval_ms: 1000,
        cleanup_interval_ms: 60_000,
    };
    
    let metrics = Arc::new(SecurityMetrics::new());
    let mgr = Arc::new(
        AuthorizationManager::new(config, metrics).await.unwrap()
    );
    
    // Pre-populate with test data
    let test_data = generate_test_data();
    populate_manager(&mgr, &test_data).await;
    
    (mgr, test_data)
}

/// Setup authentication environment for benchmarks
pub async fn setup_authentication_environment() -> (
    Arc<AuthenticationManager>,
    Arc<CertificateManager>,
    TestCertificates,
) {
    let cert_config = CertificateConfig {
        ca_cert_path: "/tmp/ca.crt".into(),
        ca_key_path: "/tmp/ca.key".into(),
        cert_validity_days: 365,
        enable_crl_check: true,
        crl_refresh_interval_ms: 3600_000,
        certificate_cache_size: 1000,
        certificate_cache_ttl_ms: 600_000,
    };
    
    let metrics = Arc::new(SecurityMetrics::new());
    
    let authn_mgr = Arc::new(
        AuthenticationManager::new(cert_config.clone(), metrics.clone()).await.unwrap()
    );
    
    let cert_mgr = Arc::new(
        CertificateManager::new(cert_config, metrics).await.unwrap()
    );
    
    let test_certs = generate_test_certificates().await;
    
    (authn_mgr, cert_mgr, test_certs)
}

/// Setup certificate manager for benchmarks
pub async fn setup_certificate_manager() -> Arc<CertificateManager> {
    let config = CertificateConfig {
        ca_cert_path: "/tmp/ca.crt".into(),
        ca_key_path: "/tmp/ca.key".into(),
        cert_validity_days: 365,
        enable_crl_check: true,
        crl_refresh_interval_ms: 3600_000,
        certificate_cache_size: 1000,
        certificate_cache_ttl_ms: 600_000,
    };
    
    let metrics = Arc::new(SecurityMetrics::new());
    Arc::new(CertificateManager::new(config, metrics).await.unwrap())
}

/// Test data structure for benchmarks
pub struct TestData {
    pub principals: Vec<Principal>,
    pub topics: Vec<String>,
    pub acl_keys: Vec<AclKey>,
    pub permissions: Vec<Permission>,
}

/// Test certificates for authentication benchmarks
pub struct TestCertificates {
    pub ca_cert: Vec<u8>,
    pub ca_key: Vec<u8>,
    pub client_cert_der: Vec<u8>,
    pub client_key_der: Vec<u8>,
    pub parsed_client_cert: ParsedCertificate,
    pub cert_chain: Vec<Vec<u8>>,
}

/// Generate test data for benchmarks
fn generate_test_data() -> TestData {
    use data_generators::*;
    
    TestData {
        principals: generate_principals(1000),
        topics: generate_topics(100),
        acl_keys: generate_acl_key_batch(10000),
        permissions: vec![
            Permission::Read,
            Permission::Write,
            Permission::Admin,
            Permission::Create,
            Permission::Delete,
        ],
    }
}

/// Populate authorization manager with test data
async fn populate_manager(mgr: &AuthorizationManager, data: &TestData) {
    for key in data.acl_keys.iter().take(1000) {
        mgr.cache_permission(key.clone(), true).await;
    }
    
    // Add some entries to negative cache
    for key in data.acl_keys.iter().skip(1000).take(100) {
        mgr.add_to_negative_cache(key.clone()).await;
    }
}

/// Generate test certificates for benchmarks
async fn generate_test_certificates() -> TestCertificates {
    use rcgen::{generate_simple_self_signed_cert, Certificate, CertificateParams, DistinguishedName};
    use data_generators::*;
    
    // Generate CA certificate
    let mut ca_params = CertificateParams::default();
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params.distinguished_name.push(rcgen::DnType::CommonName, "Test CA");
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    
    let ca_cert = Certificate::from_params(ca_params).unwrap();
    let ca_cert_der = ca_cert.serialize_der().unwrap();
    let ca_key_der = ca_cert.serialize_private_key_der();
    
    // Generate client certificate
    let mut client_params = CertificateParams::default();
    client_params.distinguished_name = DistinguishedName::new();
    client_params.distinguished_name.push(rcgen::DnType::CommonName, "test-client");
    
    let client_cert = Certificate::from_params(client_params).unwrap();
    let client_cert_der = client_cert.serialize_der_with_signer(&ca_cert).unwrap();
    let client_key_der = client_cert.serialize_private_key_der();
    
    // Create certificate chain
    let cert_chain = vec![client_cert_der.clone(), ca_cert_der.clone()];
    
    TestCertificates {
        ca_cert: ca_cert_der,
        ca_key: ca_key_der,
        client_cert_der: client_cert_der.clone(),
        client_key_der,
        parsed_client_cert: ParsedCertificate {
            subject: "CN=test-client".to_string(),
            issuer: "CN=Test CA".to_string(),
            serial: "123456".to_string(),
            not_before: chrono::Utc::now(),
            not_after: chrono::Utc::now() + chrono::Duration::days(365),
            raw_der: client_cert_der,
        },
        cert_chain,
    }
}

/// Parsed certificate structure
#[derive(Clone)]
pub struct ParsedCertificate {
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub not_before: chrono::DateTime<chrono::Utc>,
    pub not_after: chrono::DateTime<chrono::Utc>,
    pub raw_der: Vec<u8>,
}

/// Flamegraph profiler for performance analysis
pub mod perf {
    use std::fs::File;
    use std::os::raw::c_int;
    use std::path::Path;
    
    pub struct FlamegraphProfiler {
        frequency: c_int,
    }
    
    impl FlamegraphProfiler {
        pub fn new(frequency: c_int) -> Self {
            Self { frequency }
        }
    }
    
    impl criterion::profiler::Profiler for FlamegraphProfiler {
        fn start_profiling(&mut self, _benchmark_id: &str, _benchmark_dir: &Path) {
            // Start perf profiling
            // In production, this would interface with perf or other profiling tools
        }
        
        fn stop_profiling(&mut self, _benchmark_id: &str, benchmark_dir: &Path) {
            // Stop profiling and generate flamegraph
            // In production, this would generate actual flamegraphs
            let _ = File::create(benchmark_dir.join("flamegraph.svg"));
        }
    }
}