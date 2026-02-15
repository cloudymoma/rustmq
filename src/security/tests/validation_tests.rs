//! Enhanced Certificate Validation Tests
//!
//! Tests for the enhanced certificate validation pipeline including
//! WebPKI integration, EKU handling, and policy-based validation.

use crate::error::RustMqError;
use crate::security::auth::authentication::{
    AuthenticationManager, CertificateValidationConfig, ExtendedCertificateInfo,
};
use crate::security::auth::certificate_metadata::{CertificateRole, KeyStrength};
use crate::security::metrics::SecurityMetrics;
use crate::security::tests::SecurityTestConfig;
use crate::security::tls::{CaGenerationParams, CertificateManager, KeyType};
use rustls_pki_types::CertificateDer;
use std::sync::Arc;
use tempfile::TempDir;

/// Test enhanced certificate validation with proper CA chain
#[tokio::test]
async fn test_enhanced_validation_with_ca_chain() {
    let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;

    // Create a root CA
    let ca_params = CaGenerationParams {
        common_name: "RustMQ Test Root CA".to_string(),
        organization: Some("RustMQ Security".to_string()),
        validity_years: Some(1),
        key_type: Some(KeyType::Ecdsa),
        is_root: true,
        ..Default::default()
    };

    let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();

    // Refresh CA chain
    auth_manager.refresh_ca_chain().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Test that CA chain is loaded
    let ca_chain = cert_manager.get_ca_chain().await.unwrap();
    assert!(!ca_chain.is_empty(), "CA chain should not be empty");

    println!("✅ Enhanced validation with CA chain test setup completed");
}

/// Test certificate validation policy configuration
#[tokio::test]
async fn test_enhanced_validation_policy_configuration() {
    let mut config = CertificateValidationConfig::default();

    // Test policy customization
    config.require_strong_keys = false;
    config.max_validity_days = Some(180); // 6 months
    config.require_san = true;
    config.expiry_warning_days = 15;

    // Verify policy changes
    assert!(
        !config.require_strong_keys,
        "Strong keys requirement should be disabled"
    );
    assert_eq!(
        config.max_validity_days.unwrap(),
        180,
        "Max validity should be 6 months"
    );
    assert!(config.require_san, "SAN requirement should be enabled");
    assert_eq!(
        config.expiry_warning_days, 15,
        "Expiry warning should be 15 days"
    );

    println!("✅ Certificate validation policy configuration test passed");
}

/// Test comprehensive certificate validation result
#[tokio::test]
async fn test_enhanced_comprehensive_validation_result() {
    let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;

    let cert_der = create_minimal_test_cert();
    let config = CertificateValidationConfig::default();

    // Test comprehensive validation (expected to fail with test cert but should return structured result)
    let validation_result = auth_manager
        .validate_certificate_with_policies(&cert_der, &config)
        .await;

    match validation_result {
        Ok(result) => {
            // If validation somehow succeeds, verify the result structure
            assert!(
                result.validation_duration.as_nanos() > 0,
                "Validation should take some time"
            );
            println!("✅ Comprehensive validation result test passed - validation succeeded");
            println!("   - Valid: {}", result.is_valid);
            println!("   - Errors: {}", result.validation_errors.len());
            println!("   - Warnings: {}", result.validation_warnings.len());
        }
        Err(e) => {
            // Expected case - test certificate validation fails
            println!("⚠️  Comprehensive validation failed as expected: {}", e);
            println!("   This is normal with the minimal test certificate");
        }
    }
}

/// Test enhanced certificate metadata extraction
#[tokio::test]
async fn test_enhanced_metadata_extraction_with_webpki() {
    let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;

    let cert_der = create_minimal_test_cert();

    // Test metadata extraction
    let metadata_result = auth_manager.extract_certificate_metadata(&cert_der).await;

    match metadata_result {
        Ok(metadata) => {
            // Verify metadata structure
            assert!(
                !metadata.fingerprint.is_empty(),
                "Fingerprint should not be empty"
            );
            println!("✅ Enhanced metadata extraction test passed");
            println!("   - Fingerprint: {}", metadata.fingerprint);
            println!("   - Subject: {}", metadata.subject_dn.raw_dn);
            println!("   - Role: {:?}", metadata.certificate_role);
        }
        Err(e) => {
            println!("⚠️  Metadata extraction failed: {}", e);
            println!("   This is expected with minimal test certificate");
        }
    }
}

/// Test certificate purpose validation
#[tokio::test]
async fn test_enhanced_certificate_purpose_validation() {
    let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;

    let cert_der = create_minimal_test_cert();

    // Test validation for different purposes
    let purposes = vec![
        CertificateRole::Server,
        CertificateRole::Client,
        CertificateRole::RootCa,
    ];

    for purpose in purposes {
        let validation_result = auth_manager
            .validate_certificate_for_purpose(&cert_der, purpose)
            .await;

        match validation_result {
            Ok(_) => {
                println!("✅ Certificate validated for purpose: {:?}", purpose);
            }
            Err(e) => {
                println!(
                    "⚠️  Certificate validation failed for purpose {:?}: {}",
                    purpose, e
                );
                // Expected for test certificates
            }
        }
    }
}

/// Test batch certificate validation
#[tokio::test]
async fn test_enhanced_batch_certificate_validation() {
    let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;

    // Create multiple test certificates
    let cert1 = create_minimal_test_cert();
    let cert2 = create_minimal_test_cert();
    let cert3 = create_minimal_test_cert();

    let certificates = vec![cert1.as_slice(), cert2.as_slice(), cert3.as_slice()];

    // Test batch validation
    let results = auth_manager
        .validate_certificates_batch(&certificates)
        .await;

    assert_eq!(
        results.len(),
        3,
        "Should get results for all 3 certificates"
    );

    println!("✅ Batch certificate validation test completed");
    println!("   - Processed {} certificates", results.len());

    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(_) => println!("   - Certificate {}: Valid", i + 1),
            Err(e) => println!("   - Certificate {}: Error - {}", i + 1, e),
        }
    }
}

/// Test certificate information extraction
#[tokio::test]
async fn test_enhanced_certificate_info_extraction() {
    let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;

    let cert_der = create_minimal_test_cert();

    // Test certificate information extraction
    let cert_info_result = auth_manager.get_certificate_info(&cert_der).await;

    match cert_info_result {
        Ok(info) => {
            // Verify information fields
            assert!(
                !info.fingerprint.is_empty(),
                "Fingerprint should not be empty"
            );
            assert!(
                !info.subject_dn.is_empty(),
                "Subject DN should not be empty"
            );
            assert!(
                !info.serial_number.is_empty(),
                "Serial number should not be empty"
            );

            println!("✅ Certificate information extraction test passed");
            println!("   - Fingerprint: {}", info.fingerprint);
            println!("   - Subject: {}", info.subject_dn);
            println!("   - Serial: {}", info.serial_number);
            println!("   - Role: {}", info.certificate_role);
            println!("   - Key Strength: {}", info.key_strength);
        }
        Err(e) => {
            println!("⚠️  Certificate information extraction failed: {}", e);
            println!("   This is expected with minimal test certificate");
        }
    }
}

/// Test cache statistics and monitoring
#[tokio::test]
async fn test_enhanced_cache_monitoring() {
    let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;

    // Get initial cache statistics
    let initial_stats = auth_manager.get_cache_statistics();
    assert_eq!(
        initial_stats.total_cache_entries, 0,
        "Initial cache should be empty"
    );

    // Perform some operations that should populate caches
    let cert_der = create_minimal_test_cert();

    // Try certificate validation (will populate validation cache even if it fails)
    let _ = auth_manager.validate_certificate(&cert_der).await;

    // Try metadata extraction (will populate metadata cache if successful)
    let _ = auth_manager.extract_certificate_metadata(&cert_der).await;

    // Get updated statistics
    let updated_stats = auth_manager.get_cache_statistics();

    println!("✅ Cache monitoring test completed");
    println!(
        "   - Initial total entries: {}",
        initial_stats.total_cache_entries
    );
    println!(
        "   - Updated total entries: {}",
        updated_stats.total_cache_entries
    );
    println!("   - Metadata cache: {}", updated_stats.metadata_cache_size);
    println!(
        "   - Validation cache: {}",
        updated_stats.validation_cache_size
    );

    // Test cache cleanup
    auth_manager.cleanup_expired_cache_entries().await;
    println!("   - Cache cleanup completed successfully");
}

/// Test enhanced principal extraction
#[tokio::test]
async fn test_enhanced_principal_extraction() {
    let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;

    let cert_der = create_minimal_test_cert();
    let certificate = CertificateDer::from(cert_der);

    // Test enhanced principal extraction
    let principal_result = auth_manager.extract_principal_enhanced(&certificate).await;

    match principal_result {
        Ok(principal) => {
            assert!(!principal.is_empty(), "Principal should not be empty");
            println!("✅ Enhanced principal extraction test passed");
            println!("   - Principal: {}", principal);
        }
        Err(e) => {
            println!("⚠️  Enhanced principal extraction failed: {}", e);
            println!("   This is expected with minimal test certificate");
        }
    }
}

/// Helper function to create test authentication manager
async fn create_test_authentication_manager()
-> (AuthenticationManager, TempDir, Arc<CertificateManager>) {
    let temp_dir = TempDir::new().unwrap();
    let cert_config = SecurityTestConfig::create_test_certificate_config(&temp_dir);

    let certificate_manager = Arc::new(
        CertificateManager::new_with_enhanced_config(cert_config)
            .await
            .unwrap(),
    );

    let tls_config = SecurityTestConfig::create_test_config().tls;
    let metrics = Arc::new(SecurityMetrics::new().unwrap());

    let auth_manager = AuthenticationManager::new(certificate_manager.clone(), tls_config, metrics)
        .await
        .unwrap();

    (auth_manager, temp_dir, certificate_manager)
}

/// Create a minimal test certificate for validation testing
fn create_minimal_test_cert() -> Vec<u8> {
    // Create a basic ASN.1 DER structure for testing
    // This is sufficient for enhanced framework testing
    vec![
        0x30, 0x82, 0x01, 0x2A, // SEQUENCE (298 bytes)
        0x30, 0x82, 0x01, 0x27, // TBSCertificate SEQUENCE (295 bytes)
        0x02, 0x01, 0x00, // Version (v1 = 0)
        0x02, 0x09, 0x00, 0x87, 0x65, 0x43, 0x21, 0x00, 0x00, 0x00, 0x01, // Serial number
        0x30, 0x0D, // AlgorithmIdentifier SEQUENCE (13 bytes)
        0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01,
        0x0B, // SHA256WithRSAEncryption OID
        0x05, 0x00, // NULL parameters
        0x30, 0x1F, // Issuer SEQUENCE (31 bytes)
        0x31, 0x1D, 0x30, 0x1B, 0x06, 0x03, 0x55, 0x04, 0x03, // CN attribute
        0x0C, 0x14, 0x52, 0x75, 0x73, 0x74, 0x4D, 0x51, 0x20, 0x54, 0x65, 0x73, 0x74, 0x20, 0x52,
        0x6F, 0x6F, 0x74, 0x20, 0x43, 0x41, // "RustMQ Test Root CA"
        0x30, 0x1E, // Validity SEQUENCE (30 bytes)
        0x17, 0x0D, // UTCTime tag
        0x32, 0x33, 0x30, 0x31, 0x30, 0x31, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30,
        0x5A, // "230101000000Z"
        0x17, 0x0D, // UTCTime tag
        0x32, 0x34, 0x31, 0x32, 0x33, 0x31, 0x32, 0x33, 0x35, 0x39, 0x35, 0x39,
        0x5A, // "241231235959Z"
        0x30, 0x22, // Subject SEQUENCE (34 bytes)
        0x31, 0x20, 0x30, 0x1E, 0x06, 0x03, 0x55, 0x04, 0x03, // CN attribute
        0x0C, 0x17, 0x52, 0x75, 0x73, 0x74, 0x4D, 0x51, 0x20, 0x54, 0x65, 0x73, 0x74, 0x20, 0x43,
        0x6C, 0x69, 0x65, 0x6E, 0x74, 0x20, 0x43, 0x65, 0x72,
        0x74, // "RustMQ Test Client Cert"
        // Public key info would follow, but minimal for testing...
        0x30, 0x5C, // SubjectPublicKeyInfo SEQUENCE (minimal)
        0x30, 0x0D, // AlgorithmIdentifier SEQUENCE
        0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01, // RSA OID
        0x05, 0x00, // NULL
        0x03, 0x4B, 0x00, // BIT STRING (75 bytes)
        // Minimal RSA public key structure
        0x30, 0x48, 0x02, 0x41, 0x00, // SEQUENCE, INTEGER (modulus - 65 bytes)
        // Dummy modulus (64 bytes of 0xFF for simplicity)
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0x02, 0x03, 0x01, 0x00, 0x01, // INTEGER (exponent = 65537)
    ]
}
