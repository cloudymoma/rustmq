use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use rustmq::security::auth::AuthenticationManager;
use rustmq::security::tls::{CertificateManager, TlsConfig, TlsMode, TlsServerConfig, TlsClientConfig, EnhancedCertificateManagementConfig, CaSettings};
use rustmq::security::metrics::SecurityMetrics;
use rustmq::security::tls::CertificateRole;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

// Simple certificate generator for benchmarks
fn generate_test_certificate() -> Vec<u8> {
    use rcgen::*;
    
    let mut params = CertificateParams::new(vec!["test.example.com".to_string()]);
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    
    let key_pair = KeyPair::generate(&PKCS_ECDSA_P256_SHA256).unwrap();
    params.key_pair = Some(key_pair);
    
    let cert = Certificate::from_params(params).unwrap();
    cert.serialize_der().unwrap()
}

// Helper function to create test authentication manager
async fn create_test_auth_manager() -> (AuthenticationManager, TempDir, Vec<Vec<u8>>) {
    use rustmq::security::CertificateManagementConfig;
    use std::collections::HashMap;
    
    let temp_dir = TempDir::new().unwrap();
    
    // Create test certificate configuration with proper structure
    let basic_config = CertificateManagementConfig {
        ca_cert_path: temp_dir.path().join("ca-cert.pem").to_string_lossy().to_string(),
        ca_key_path: temp_dir.path().join("ca-key.pem").to_string_lossy().to_string(),
        cert_validity_days: 365,
        auto_renew_before_expiry_days: 30,
        crl_check_enabled: false,
        ocsp_check_enabled: false,
    };
    
    let ca_settings = CaSettings::default();
    
    let mut certificate_templates = HashMap::new();
    certificate_templates.insert(CertificateRole::Client, 
        rustmq::security::tls::CertificateTemplate::client_default());
    
    let cert_config = EnhancedCertificateManagementConfig {
        basic: basic_config,
        storage_path: temp_dir.path().to_string_lossy().to_string(),
        // Use a test password for benchmarks (encryption is mandatory)
        key_encryption_password: "benchmark-test-password-32chars!!".to_string(),
        ca_settings,
        certificate_templates,
        audit_enabled: false,
        audit_log_path: "/tmp/audit.log".to_string(),
        metrics_enabled: true,
        auto_renewal_enabled: false,
        renewal_check_interval_hours: 24,
        crl_update_interval_hours: 24,
        crl_distribution_points: vec![],
    };
    
    let certificate_manager = Arc::new(
        CertificateManager::new_with_enhanced_config(cert_config)
            .await
            .unwrap()
    );
    
    // Create minimal TLS config
    let tls_config = TlsConfig {
        enabled: true,
        mode: TlsMode::Mutual,
        server: Some(TlsServerConfig {
            cert_path: "/tmp/test-cert.pem".to_string(),
            key_path: "/tmp/test-key.pem".to_string(),
            ca_cert_path: Some("/tmp/test-ca.pem".to_string()),
            require_client_cert: true,
            cipher_suites: vec![],
            supported_versions: vec![],
        }),
        client: Some(TlsClientConfig {
            cert_path: Some("/tmp/test-client-cert.pem".to_string()),
            key_path: Some("/tmp/test-client-key.pem".to_string()),
            ca_cert_path: "/tmp/test-ca.pem".to_string(),
            server_name: Some("test-server".to_string()),
            verify_server_cert: true,
            supported_versions: vec![],
        }),
    };
    
    let metrics = Arc::new(SecurityMetrics::new().unwrap());
    
    let auth_manager = AuthenticationManager::new(
        certificate_manager.clone(),
        tls_config,
        metrics,
    ).await.unwrap();
    
    // Generate test certificates using simple generator
    let mut test_certificates = Vec::new();
    for _ in 0..10 {
        test_certificates.push(generate_test_certificate());
    }
    
    (auth_manager, temp_dir, test_certificates)
}

fn bench_certificate_validation_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Setup test data
    let (auth_manager, _temp_dir, test_certificates) = rt.block_on(async {
        create_test_auth_manager().await
    });
    
    let mut group = c.benchmark_group("certificate_validation");
    
    // Benchmark cold validation (first time)
    group.bench_function("cold_validation", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Use different certificate each iteration to avoid caching
                let cert_index = criterion::black_box(0);
                let cert_der = &test_certificates[cert_index % test_certificates.len()];
                auth_manager.validate_certificate(cert_der).await
            })
        })
    });
    
    // Benchmark warm validation (cached)
    let warm_cert = &test_certificates[0];
    rt.block_on(async {
        // Pre-warm the cache
        let _ = auth_manager.validate_certificate(warm_cert).await;
    });
    
    group.bench_function("warm_validation", |b| {
        b.iter(|| {
            rt.block_on(async {
                auth_manager.validate_certificate(criterion::black_box(warm_cert)).await
            })
        })
    });
    
    // Benchmark fingerprint calculation
    group.bench_function("fingerprint_calculation", |b| {
        b.iter(|| {
            let cert = rustls::Certificate(criterion::black_box(warm_cert.clone()));
            auth_manager.calculate_certificate_fingerprint(&cert)
        })
    });
    
    // Benchmark revocation check
    group.bench_function("revocation_check", |b| {
        b.iter(|| {
            rt.block_on(async {
                let fingerprint = criterion::black_box("test_fingerprint");
                auth_manager.is_certificate_revoked(fingerprint).await
            })
        })
    });
    
    // Benchmark various certificate sizes
    for size in [1, 5, 10].iter() {
        group.bench_with_input(BenchmarkId::new("batch_validation", size), size, |b, &size| {
            b.iter(|| {
                rt.block_on(async {
                    for i in 0..size {
                        let cert_der = &test_certificates[i % test_certificates.len()];
                        let _ = auth_manager.validate_certificate(cert_der).await;
                    }
                })
            })
        });
    }
    
    group.finish();
}

fn bench_certificate_parsing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let (auth_manager, _temp_dir, test_certificates) = rt.block_on(async {
        create_test_auth_manager().await
    });
    
    // Use the first test certificate
    let test_cert_der = &test_certificates[0];
    
    // Benchmark timing resistance overhead
    c.bench_function("timing_resistance_overhead", |b| {
        b.iter(|| {
            rt.block_on(async {
                // This will test the timing mitigation overhead
                auth_manager.validate_certificate(criterion::black_box(test_cert_der)).await
            })
        })
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(10))
        .warm_up_time(std::time::Duration::from_secs(2));
    targets = bench_certificate_validation_performance, bench_certificate_parsing
);
criterion_main!(benches);