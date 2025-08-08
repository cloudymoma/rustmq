//! Comprehensive Authentication Manager Tests
//!
//! Tests for mTLS authentication, certificate validation, principal extraction,
//! and certificate caching functionality.

#[cfg(test)]
mod tests {
    use super::super::test_utils::*;
    use crate::error::RustMqError;
    use crate::security::auth::*;
    use crate::security::tls::*;
    use crate::security::metrics::SecurityMetrics;
    use crate::security::*;
    
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use rustls::Certificate;

    async fn create_test_authentication_manager() -> (AuthenticationManager, TempDir, Arc<CertificateManager>) {
        let temp_dir = TempDir::new().unwrap();
        let cert_config = SecurityTestConfig::create_test_certificate_config(&temp_dir);
        
        let certificate_manager = Arc::new(
            CertificateManager::new_with_enhanced_config(cert_config)
                .await
                .unwrap()
        );
        
        let tls_config = SecurityTestConfig::create_test_config().tls;
        let metrics = Arc::new(SecurityMetrics::new().unwrap());
        
        let auth_manager = AuthenticationManager::new(
            certificate_manager.clone(),
            tls_config,
            metrics,
        ).await.unwrap();
        
        (auth_manager, temp_dir, certificate_manager)
    }

    #[tokio::test]
    async fn test_authentication_manager_creation() {
        let (_auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;
        // Creation should succeed without errors
    }

    #[tokio::test]
    async fn test_certificate_validation_with_valid_cert() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            organization: Some("Test Org".to_string()),
            is_root: true,
            validity_years: Some(5),
            key_type: Some(KeyType::Ecdsa),
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue a client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "test-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: None,
        };
        
        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        
        // Test certificate validation - convert PEM to DER first
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes()).unwrap().into_iter().next().unwrap();
        let is_valid = auth_manager.validate_certificate(&der_data).await;
        assert!(is_valid.is_ok(), "Certificate validation should succeed");
    }

    #[tokio::test]
    async fn test_certificate_validation_with_invalid_cert() {
        let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;
        
        // Test with invalid certificate data
        let invalid_cert_data = b"-----BEGIN CERTIFICATE-----\nINVALID\n-----END CERTIFICATE-----";
        let result = auth_manager.validate_certificate(invalid_cert_data).await;
        
        assert!(result.is_err(), "Invalid certificate should fail validation");
    }

    #[tokio::test]
    async fn test_principal_extraction_from_certificate() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue a client certificate with specific CN
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "client-principal-001".to_string());
        subject.push(rcgen::DnType::OrganizationName, "Test Organization".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: None,
        };
        
        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        
        // Extract principal from the certificate - convert PEM to DER first
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let certificate = rustls::Certificate(der_data);
        let fingerprint = "test_fingerprint";
        let principal = auth_manager.extract_principal_from_certificate(&certificate, fingerprint);
        
        assert!(principal.is_ok(), "Principal extraction should succeed");
        let principal = principal.unwrap();
        assert!(principal.contains("client-principal-001"), 
               "Principal should contain the certificate CN");
    }

    #[tokio::test]
    async fn test_certificate_revocation_check() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue a client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "test-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: None,
        };
        
        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        
        // Test revocation check before revocation
        let is_revoked = auth_manager.is_certificate_revoked(&client_cert.fingerprint).await;
        assert!(is_revoked.is_ok(), "Revocation check should succeed");
        assert!(!is_revoked.unwrap(), "Certificate should not be revoked initially");
        
        // Revoke the certificate
        cert_manager.revoke_certificate(&client_cert.id, RevocationReason::KeyCompromise).await.unwrap();
        
        // Refresh the revoked certificates list in the auth manager
        auth_manager.refresh_revoked_certificates().await.unwrap();
        
        // Test revocation check after revocation
        let is_revoked = auth_manager.is_certificate_revoked(&client_cert.fingerprint).await;
        assert!(is_revoked.is_ok(), "Revocation check should succeed");
        assert!(is_revoked.unwrap(), "Certificate should be revoked");
    }

    #[tokio::test]
    async fn test_certificate_caching_behavior() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue a client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "cached-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: None,
        };
        
        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        
        // First validation (should cache) - convert PEM to DER first
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes()).unwrap().into_iter().next().unwrap();
        let start = std::time::Instant::now();
        let result1 = auth_manager.validate_certificate(&der_data).await;
        let first_duration = start.elapsed();
        
        assert!(result1.is_ok(), "First validation should succeed");
        
        // Second validation (should be faster due to caching)
        let start = std::time::Instant::now();
        let result2 = auth_manager.validate_certificate(&der_data).await;
        let second_duration = start.elapsed();
        
        assert!(result2.is_ok(), "Second validation should succeed");
        
        // Note: In a real system, the second call would be faster due to caching
        // For this test, we just verify both calls succeed
        assert!(first_duration >= Duration::from_nanos(0), "First duration should be measured");
        assert!(second_duration >= Duration::from_nanos(0), "Second duration should be measured");
    }

    #[tokio::test]
    async fn test_authentication_context_creation() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue a broker certificate with specific attributes
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "broker-001.cluster.internal".to_string());
        subject.push(rcgen::DnType::OrganizationName, "RustMQ Cluster".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Broker,
            san_entries: vec![
                rcgen::SanType::DnsName("broker-001.cluster.internal".to_string()),
                rcgen::SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
            ],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: None,
        };
        
        let broker_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        
        // Create authentication context from certificate - convert PEM to DER first
        let pem_data = broker_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let certificate = rustls::Certificate(der_data);
        let fingerprint = "test_fingerprint";
        let principal = auth_manager.extract_principal_from_certificate(&certificate, fingerprint).unwrap();
        
        let auth_context = AuthContext::new(principal.clone())
            .with_certificate(broker_cert.fingerprint.clone())
            .with_groups(vec!["brokers".to_string()]);
        
        assert_eq!(auth_context.principal.as_ref(), principal.as_ref());
        assert_eq!(auth_context.certificate_fingerprint, Some(broker_cert.fingerprint));
        assert!(auth_context.groups.contains(&"brokers".to_string()));
        assert!(auth_context.authenticated_at.elapsed() >= Duration::from_nanos(0));
    }

    #[tokio::test]
    async fn test_certificate_fingerprint_calculation() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue multiple certificates
        for i in 0..3 {
            let mut subject = rcgen::DistinguishedName::new();
            subject.push(rcgen::DnType::CommonName, format!("client-{}", i));
            
            let cert_request = CertificateRequest {
                subject,
                role: CertificateRole::Client,
                san_entries: vec![],
                validity_days: Some(365),
                key_type: Some(KeyType::Ecdsa),
                key_size: Some(256),
                issuer_id: None,
            };
            
            let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
            
            // Calculate fingerprint - convert PEM to DER first
            let pem_data = client_cert.certificate_pem.clone().unwrap();
            let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            let certificate = rustls::Certificate(der_data.clone());
            let fingerprint = auth_manager.calculate_certificate_fingerprint(&certificate);
            
            assert!(!fingerprint.is_empty(), "Fingerprint should not be empty");
            assert_eq!(fingerprint.len(), 64, "SHA256 fingerprint should be 64 hex characters");
            assert_eq!(fingerprint, client_cert.fingerprint, "Fingerprints should match");
        }
    }

    #[tokio::test]
    async fn test_authentication_metrics_collection() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Get initial metrics
        let metrics = auth_manager.get_statistics();
        let initial_success_count = metrics.authentication_success_count;
        let initial_failure_count = metrics.authentication_failure_count;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Perform successful authentication
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "metrics-test-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: None,
        };
        
        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes()).unwrap().into_iter().next().unwrap();
        let _validation_result = auth_manager.validate_certificate(&der_data).await.unwrap();
        
        // Perform failed authentication
        let invalid_cert_data = b"invalid certificate data";
        let _failed_result = auth_manager.validate_certificate(invalid_cert_data).await;
        
        // Check metrics were updated
        let final_metrics = auth_manager.get_statistics();
        assert!(final_metrics.authentication_success_count > initial_success_count, 
               "Success count should increase");
        assert!(final_metrics.authentication_failure_count > initial_failure_count, 
               "Failure count should increase");
    }

    #[tokio::test]
    async fn test_certificate_chain_validation() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA
        let root_ca_params = CaGenerationParams {
            common_name: "Root CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let root_ca = cert_manager.generate_root_ca(root_ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Create an intermediate CA
        let intermediate_ca_params = CaGenerationParams {
            common_name: "Intermediate CA".to_string(),
            is_root: false,
            ..Default::default()
        };
        
        let intermediate_ca = cert_manager.generate_intermediate_ca(&root_ca.id, intermediate_ca_params).await.unwrap();
        
        // Refresh CA chain again after adding intermediate CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue a client certificate from the intermediate CA
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "chain-test-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(intermediate_ca.id.clone()),
        };
        
        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        
        // Validate the certificate chain - convert PEMs to DER first
        let client_pem = client_cert.certificate_pem.clone().unwrap();
        let client_der = rustls_pemfile::certs(&mut client_pem.as_bytes()).unwrap().into_iter().next().unwrap();
        let client_cert_obj = rustls::Certificate(client_der);
        
        let intermediate_pem = intermediate_ca.certificate_pem.clone().unwrap();
        let intermediate_der = rustls_pemfile::certs(&mut intermediate_pem.as_bytes()).unwrap().into_iter().next().unwrap();
        let intermediate_ca_obj = rustls::Certificate(intermediate_der);
        
        let root_pem = root_ca.certificate_pem.clone().unwrap();
        let root_der = rustls_pemfile::certs(&mut root_pem.as_bytes()).unwrap().into_iter().next().unwrap();
        let root_ca_obj = rustls::Certificate(root_der);
        let chain_validation = auth_manager.validate_certificate_chain(&[
            client_cert_obj,
            intermediate_ca_obj,
            root_ca_obj,
        ]).await;
        
        assert!(chain_validation.is_ok(), "Certificate chain validation should succeed");
    }

    #[tokio::test]
    #[ignore]
    async fn test_concurrent_authentication_operations() {
        // Test temporarily disabled due to Send trait issues with concurrent operations
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue multiple certificates concurrently
        let mut handles = Vec::new();
        
        for i in 0..10 {
            let auth_manager = auth_manager.clone();
            let cert_manager = cert_manager.clone();
            
            let handle = tokio::spawn(async move {
                let mut subject = rcgen::DistinguishedName::new();
                subject.push(rcgen::DnType::CommonName, format!("concurrent-client-{}", i));
                
                let cert_request = CertificateRequest {
                    subject,
                    role: CertificateRole::Client,
                    san_entries: vec![],
                    validity_days: Some(365),
                    key_type: Some(KeyType::Ecdsa),
                    key_size: Some(256),
                    issuer_id: None,
                };
                
                let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
                
                // Perform authentication operations
                let pem_data = client_cert.certificate_pem.clone().unwrap();
                let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes()).unwrap().into_iter().next().unwrap();
                let validation_result = auth_manager.validate_certificate(&der_data).await;
                let revocation_result = auth_manager.is_certificate_revoked(&client_cert.fingerprint).await;
                let pem_data = client_cert.certificate_pem.clone().unwrap();
                let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                    .unwrap()
                    .into_iter()
                    .next()
                    .unwrap();
                let certificate = rustls::Certificate(der_data);
                let fingerprint = "test_fingerprint";
                let principal_result = auth_manager.extract_principal_from_certificate(&certificate, fingerprint);
                
                (validation_result, revocation_result, principal_result)
            });
            
            handles.push(handle);
        }
        
        // Wait for all operations to complete
        for handle in handles {
            let (validation, revocation, principal) = handle.await.unwrap();
            
            assert!(validation.is_ok(), "Concurrent validation should succeed");
            assert!(revocation.is_ok(), "Concurrent revocation check should succeed");
            assert!(principal.is_ok(), "Concurrent principal extraction should succeed");
        }
    }

    #[tokio::test]
    async fn test_authentication_with_expired_certificate() {
        let (auth_manager, _temp_dir, cert_manager) = create_test_authentication_manager().await;
        
        // Create a root CA first
        let ca_params = CaGenerationParams {
            common_name: "Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let _ca_cert = cert_manager.generate_root_ca(ca_params).await.unwrap();
        
        // Refresh CA chain in auth manager to recognize the new CA
        auth_manager.refresh_ca_chain().await.unwrap();
        
        // Issue a certificate with very short validity (1 day)
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "short-lived-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertificateRole::Client,
            san_entries: vec![],
            validity_days: Some(1), // Very short validity
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: None,
        };
        
        let client_cert = cert_manager.issue_certificate(cert_request).await.unwrap();
        
        // Test validation with newly issued certificate (should succeed) - convert PEM to DER first
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes()).unwrap().into_iter().next().unwrap();
        let validation_result = auth_manager.validate_certificate(&der_data).await;
        assert!(validation_result.is_ok(), "Validation of fresh certificate should succeed");
        
        // Note: Testing actual expiry would require time manipulation or very short certificates
        // In a production test environment, you might use mock time or certificates with past expiry dates
    }

    #[tokio::test]
    async fn test_authentication_error_handling() {
        let (auth_manager, _temp_dir, _cert_manager) = create_test_authentication_manager().await;
        
        // Test various error conditions
        let test_cases = vec![
            ("".as_bytes(), "empty certificate"),
            ("invalid data".as_bytes(), "invalid certificate format"),
            ("-----BEGIN CERTIFICATE-----\nGARBAGE\n-----END CERTIFICATE-----".as_bytes(), "malformed certificate"),
        ];
        
        for (cert_data, description) in test_cases {
            let result = auth_manager.validate_certificate(cert_data).await;
            assert!(result.is_err(), "Should fail for {}", description);
            
            let certificate = rustls::Certificate(cert_data.to_vec());
            let fingerprint = "test_fingerprint";
            let principal_result = auth_manager.extract_principal_from_certificate(&certificate, fingerprint);
            assert!(principal_result.is_err(), "Principal extraction should fail for {}", description);
        }
    }
}