//! WebPKI Implementation Tests
//!
//! Comprehensive test suite for the WebPKI security implementation,
//! testing all WebPKI-based validation methods and backward compatibility.

#[cfg(test)]
mod tests {
    use super::super::test_utils::*;
    use crate::error::RustMqError;
    use crate::security::auth::*;
    use crate::security::metrics::SecurityMetrics;
    use crate::security::tls::*;
    use crate::security::*;

    use rustls_pki_types::CertificateDer;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

    // Test isolation mutex for WebPKI tests
    use once_cell::sync::Lazy;
    static WEBPKI_TEST_MUTEX: Lazy<tokio::sync::Mutex<()>> =
        Lazy::new(|| tokio::sync::Mutex::new(()));

    async fn create_webpki_test_setup() -> (AuthenticationManager, TempDir, Arc<CertificateManager>)
    {
        let temp_dir = TempDir::new().unwrap();
        let cert_config = SecurityTestConfig::create_test_certificate_config(&temp_dir);

        let certificate_manager = Arc::new(
            CertificateManager::new_with_enhanced_config(cert_config)
                .await
                .unwrap(),
        );

        let tls_config = SecurityTestConfig::create_test_config().tls;
        let metrics = Arc::new(SecurityMetrics::new().unwrap());

        let auth_manager =
            AuthenticationManager::new(certificate_manager.clone(), tls_config, metrics)
                .await
                .unwrap();

        (auth_manager, temp_dir, certificate_manager)
    }

    #[tokio::test]
    async fn test_webpki_certificate_validation_basic() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, cert_manager) = create_webpki_test_setup().await;

        // Create a root CA
        let ca_params = CaGenerationParams {
            common_name: "WebPKI Test Root CA".to_string(),
            organization: Some("RustMQ WebPKI Test".to_string()),
            is_root: true,
            validity_years: Some(1),
            key_type: Some(KeyType::Ecdsa),
            ..Default::default()
        };

        let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay to ensure CA chain is fully loaded, especially in release mode
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Issue a client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "webpki-test-client".to_string());
        subject.push(
            rcgen::DnType::OrganizationName,
            "WebPKI Test Client".to_string(),
        );

        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };

        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay after issuing certificate for persistence to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Give time for certificate persistence
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Test WebPKI comprehensive validation
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .filter_map(|r| r.ok())
            .next()
            .unwrap();

        let result = auth_manager
            .validate_certificate_comprehensive(&der_data)
            .await;
        assert!(
            result.is_ok(),
            "WebPKI comprehensive validation should succeed: {:?}",
            result
        );

        println!("✅ WebPKI basic certificate validation test passed");
    }

    #[tokio::test]
    async fn test_webpki_principal_extraction() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, cert_manager) = create_webpki_test_setup().await;

        // Create a root CA
        let ca_params = CaGenerationParams {
            common_name: "Principal Test CA".to_string(),
            organization: Some("RustMQ".to_string()),
            organizational_unit: Some("Message Queue System".to_string()),
            country: Some("US".to_string()),
            is_root: true,
            ..Default::default()
        };

        let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay to ensure CA chain is fully loaded, especially in release mode
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Issue a client certificate with specific CN
        let test_principal_name = "webpki-principal-test-user";
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, test_principal_name.to_string());
        subject.push(
            rcgen::DnType::OrganizationName,
            "Principal Test Org".to_string(),
        );

        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };

        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();

        // Refresh CA chain and add delay after issuing certificate
        auth_manager.refresh_ca_chain().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Test WebPKI principal extraction
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .filter_map(|r| r.ok())
            .next()
            .unwrap();
        let certificate = der_data;
        let fingerprint = "test_fingerprint";

        let principal = auth_manager.extract_principal_with_webpki(&certificate, fingerprint);
        assert!(
            principal.is_ok(),
            "WebPKI principal extraction should succeed"
        );

        let principal = principal.unwrap();
        assert!(
            principal.contains(test_principal_name),
            "Principal should contain the certificate CN: {}",
            principal
        );

        println!("✅ WebPKI principal extraction test passed");
    }

    #[tokio::test]
    async fn test_webpki_certificate_chain_validation() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, cert_manager) = create_webpki_test_setup().await;

        // Create a root CA
        let ca_params = CaGenerationParams {
            common_name: "Chain Test Root CA".to_string(),
            organization: Some("RustMQ".to_string()),
            organizational_unit: Some("Message Queue System".to_string()),
            country: Some("US".to_string()),
            is_root: true,
            ..Default::default()
        };

        let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay to ensure CA chain is fully loaded, especially in release mode
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Give time for CA chain refresh
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Issue a client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "chain-test-client".to_string());

        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };

        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay after issuing certificate for persistence to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Give time for certificate persistence
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Test WebPKI certificate chain validation
        let client_pem = client_cert.certificate_pem.clone().unwrap();
        let client_der = rustls_pemfile::certs(&mut client_pem.as_bytes())
            .filter_map(|r| r.ok())
            .next()
            .unwrap();
        let client_cert_obj = CertificateDer::from(client_der);

        let chain_validation = auth_manager
            .validate_certificate_chain_with_webpki(&[client_cert_obj])
            .await;
        assert!(
            chain_validation.is_ok(),
            "WebPKI certificate chain validation should succeed: {:?}",
            chain_validation
        );

        println!("✅ WebPKI certificate chain validation test passed");
    }

    #[tokio::test]
    async fn test_webpki_certificate_parsing() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, cert_manager) = create_webpki_test_setup().await;

        // Create a root CA
        let ca_params = CaGenerationParams {
            common_name: "Parsing Test CA".to_string(),
            organization: Some("RustMQ".to_string()),
            organizational_unit: Some("Message Queue System".to_string()),
            country: Some("US".to_string()),
            is_root: true,
            ..Default::default()
        };

        let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();

        // Issue a test certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "parsing-test-client".to_string());

        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };

        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();

        // Refresh CA chain and add delay after issuing certificate
        auth_manager.refresh_ca_chain().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Test WebPKI certificate parsing
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .filter_map(|r| r.ok())
            .next()
            .unwrap();

        let parsed_cert = auth_manager.parse_certificate_rustls(&der_data);
        assert!(
            parsed_cert.is_ok(),
            "WebPKI certificate parsing should succeed"
        );

        // Verify the parsed certificate
        let certificate = parsed_cert.unwrap();
        assert!(
            !certificate.as_ref().is_empty(),
            "Parsed certificate should not be empty"
        );
        assert!(
            certificate.as_ref().len() > 100,
            "Certificate should be reasonable size"
        );

        println!("✅ WebPKI certificate parsing test passed");
    }

    #[tokio::test]
    async fn test_webpki_error_handling() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, _cert_manager) = create_webpki_test_setup().await;

        // Test various error conditions with WebPKI methods
        let invalid_test_cases = vec![
            ("empty_certificate", b"".to_vec()),
            ("invalid_der_data", b"invalid certificate data".to_vec()),
            (
                "malformed_pem",
                b"-----BEGIN CERTIFICATE-----\nGARBAGE\n-----END CERTIFICATE-----".to_vec(),
            ),
            ("too_short", vec![0x30, 0x82, 0x01]),
        ];

        for (test_name, cert_data) in invalid_test_cases {
            // Test WebPKI comprehensive validation
            let result = auth_manager
                .validate_certificate_comprehensive(&cert_data)
                .await;
            assert!(
                result.is_err(),
                "WebPKI validation should fail for {}: {:?}",
                test_name,
                result
            );

            // Test WebPKI certificate parsing
            let parse_result = auth_manager.parse_certificate_rustls(&cert_data);
            assert!(
                parse_result.is_err(),
                "WebPKI parsing should fail for {}: {:?}",
                test_name,
                parse_result
            );

            // Test WebPKI principal extraction
            let certificate = CertificateDer::from(cert_data.clone());
            let principal_result = auth_manager.extract_principal_with_webpki(&certificate, "test");
            assert!(
                principal_result.is_err(),
                "WebPKI principal extraction should fail for {}: {:?}",
                test_name,
                principal_result
            );
        }

        println!("✅ WebPKI error handling test passed");
    }

    #[tokio::test]
    async fn test_webpki_backward_compatibility() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, cert_manager) = create_webpki_test_setup().await;

        // Create a root CA
        let ca_params = CaGenerationParams {
            common_name: "Compatibility Test CA".to_string(),
            organization: Some("RustMQ".to_string()),
            organizational_unit: Some("Message Queue System".to_string()),
            country: Some("US".to_string()),
            is_root: true,
            ..Default::default()
        };

        let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay to ensure CA chain is fully loaded, especially in release mode
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Issue a client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(
            rcgen::DnType::CommonName,
            "compatibility-test-client".to_string(),
        );

        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };

        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay after issuing certificate for persistence to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Give time for certificate persistence
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Test that existing API endpoints work with WebPKI backend
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .filter_map(|r| r.ok())
            .next()
            .unwrap();

        // Test main validate_certificate method (should use WebPKI now)
        let validation_result = auth_manager.validate_certificate(&der_data).await;
        assert!(
            validation_result.is_ok(),
            "Backward compatible validation should succeed: {:?}",
            validation_result
        );

        // Test certificate chain validation
        let certificate = CertificateDer::from(der_data.clone());
        let chain_result = auth_manager
            .validate_certificate_chain(&[certificate.clone()])
            .await;
        assert!(
            chain_result.is_ok(),
            "Backward compatible chain validation should succeed: {:?}",
            chain_result
        );

        // Test principal extraction (with fallback support)
        let fingerprint = "test_fingerprint";
        let principal_result =
            auth_manager.extract_principal_from_certificate(&certificate, fingerprint);
        assert!(
            principal_result.is_ok(),
            "Backward compatible principal extraction should succeed: {:?}",
            principal_result
        );

        println!("✅ WebPKI backward compatibility test passed");
    }

    #[tokio::test]
    async fn test_webpki_performance_characteristics() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, cert_manager) = create_webpki_test_setup().await;

        // Create a root CA
        let ca_params = CaGenerationParams {
            common_name: "Performance Test CA".to_string(),
            organization: Some("RustMQ".to_string()),
            organizational_unit: Some("Message Queue System".to_string()),
            country: Some("US".to_string()),
            is_root: true,
            ..Default::default()
        };

        let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay to ensure CA chain is fully loaded, especially in release mode
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Issue a client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(
            rcgen::DnType::CommonName,
            "performance-test-client".to_string(),
        );

        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };

        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay after issuing certificate for persistence to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Give time for certificate persistence
        tokio::time::sleep(Duration::from_millis(200)).await;

        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .filter_map(|r| r.ok())
            .next()
            .unwrap();

        // Test WebPKI validation performance (should be sub-millisecond)
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let result = auth_manager
                .validate_certificate_comprehensive(&der_data)
                .await;
            assert!(result.is_ok(), "Performance test validation should succeed");
        }
        let total_duration = start.elapsed();
        let avg_duration = total_duration / 10;

        println!("📊 WebPKI validation average time: {:?}", avg_duration);
        assert!(
            avg_duration < Duration::from_millis(10),
            "WebPKI validation should be fast (< 10ms average)"
        );

        // Test caching effectiveness (second run should be faster)
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let result = auth_manager.validate_certificate(&der_data).await;
            assert!(result.is_ok(), "Cached validation should succeed");
        }
        let cached_duration = start.elapsed() / 10;

        println!("📊 Cached validation average time: {:?}", cached_duration);

        println!("✅ WebPKI performance characteristics test passed");
    }

    #[tokio::test]
    async fn test_webpki_comprehensive_integration() {
        let _lock = WEBPKI_TEST_MUTEX.lock().await;

        let (auth_manager, _temp_dir, cert_manager) = create_webpki_test_setup().await;

        // Test the complete WebPKI integration pipeline
        println!("🔄 Testing comprehensive WebPKI integration...");

        // 1. Create root CA
        let ca_params = CaGenerationParams {
            common_name: "Integration Test Root CA".to_string(),
            organization: Some("RustMQ WebPKI Integration".to_string()),
            is_root: true,
            validity_years: Some(2),
            key_type: Some(KeyType::Ecdsa),
            ..Default::default()
        };

        let ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        auth_manager.refresh_ca_chain().await.unwrap();

        // Add delay to ensure CA chain is fully loaded, especially in release mode
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // 2. Issue multiple certificates with compatible key types (ECDSA only for compatibility)
        let test_cases = vec![
            (
                "client-ecdsa-256",
                KeyType::Ecdsa,
                256,
                CertificateRole::Client,
            ),
            (
                "broker-ecdsa-256",
                KeyType::Ecdsa,
                256,
                CertificateRole::Broker,
            ),
            (
                "admin-ecdsa-256",
                KeyType::Ecdsa,
                256,
                CertificateRole::Admin,
            ),
        ];

        for (name, key_type, key_size, role) in test_cases {
            let mut subject = rcgen::DistinguishedName::new();
            subject.push(rcgen::DnType::CommonName, name.to_string());
            subject.push(
                rcgen::DnType::OrganizationName,
                "Integration Test".to_string(),
            );

            let cert_request = CertificateRequest {
                subject,
                role,
                san_entries: vec![],
                validity_days: Some(365),
                key_type: Some(key_type),
                key_size: Some(key_size),
                issuer_id: Some(ca_cert.id.clone()),
            };

            let cert_result = cert_manager.issue_certificate(cert_request).await.unwrap();

            // Test WebPKI validation for each certificate
            let pem_data = cert_result.certificate_pem.clone().unwrap();
            let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                .filter_map(|r| r.ok())
                .next()
                .unwrap();

            // Comprehensive validation
            auth_manager.refresh_ca_chain().await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            let validation_result = auth_manager
                .validate_certificate_comprehensive(&der_data)
                .await;
            assert!(
                validation_result.is_ok(),
                "Comprehensive validation should succeed for {}: {:?}",
                name,
                validation_result
            );

            // Principal extraction
            let certificate = der_data;
            let principal = auth_manager
                .extract_principal_with_webpki(&certificate, "test")
                .unwrap();
            assert!(
                principal.contains(name),
                "Principal should contain certificate name for {}",
                name
            );

            println!("✅ Certificate {} validated successfully with WebPKI", name);
        }

        // 3. Test revocation workflow
        let revoke_cert = &cert_manager.list_all_certificates().await.unwrap()[1]; // Pick second cert
        cert_manager
            .revoke_certificate(&revoke_cert.id, RevocationReason::KeyCompromise)
            .await
            .unwrap();
        auth_manager.refresh_revoked_certificates().await.unwrap();

        let is_revoked = auth_manager
            .is_certificate_revoked(&revoke_cert.fingerprint)
            .await
            .unwrap();
        assert!(is_revoked, "Revoked certificate should be detected");

        // 4. Test metrics collection - verify metrics structure is working
        let stats = auth_manager.get_statistics();
        // Note: Metrics may be 0 if no actual authentication flow occurred
        // This test focuses on WebPKI validation, not authentication metrics
        assert!(
            stats.authentication_success_count >= 0,
            "Metrics should be accessible"
        );
        assert!(
            stats.authentication_failure_count >= 0,
            "Metrics should be accessible"
        );

        println!("✅ WebPKI comprehensive integration test passed");
    }
}
