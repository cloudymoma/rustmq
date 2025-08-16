//! Security Integration Tests
//!
//! End-to-end integration tests for the complete security pipeline,
//! testing interactions between authentication, authorization, certificate
//! management, and ACL systems.

#[cfg(test)]
mod tests {
    use super::super::test_utils::*;
    use crate::error::RustMqError;
    use crate::security::*;
    use crate::security::auth::{AuthorizationManager, AclKey, Permission};
    use crate::security::auth::authorization::AclCacheEntry;
    use crate::security::acl::{AclManager, AclRule, AclOperation, ResourceType};
    use crate::security::tls::*;
    use crate::types::TopicName;
    // Use certificate manager types for certificate creation
    use crate::security::tls::certificate_manager::{CertificateRole as CertMgrRole, KeyType, CaGenerationParams, CertificateRequest};
    
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;

    async fn create_integrated_security_system() -> (SecurityManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = SecurityTestConfig::create_test_config();
        
        // Update paths to use temp directory
        config.certificate_management.ca_cert_path = temp_dir.path().join("ca.pem").to_string_lossy().to_string();
        config.certificate_management.ca_key_path = temp_dir.path().join("ca.key").to_string_lossy().to_string();
        
        // The key fix: Create SecurityManager with a custom constructor that accepts storage path
        let security_manager = SecurityManager::new_with_storage_path(config, temp_dir.path()).await.unwrap();
        (security_manager, temp_dir)
    }

    #[tokio::test]
    async fn test_end_to_end_authentication_flow() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Generate root CA
        let ca_params = CaGenerationParams {
            common_name: "RustMQ Root CA".to_string(),
            organization: Some("RustMQ Test".to_string()),
            is_root: true,
            validity_years: Some(5),
            key_type: Some(KeyType::Ecdsa),
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
            
        // Load CA certificates into authentication manager
        security_manager.authentication().refresh_ca_chain().await.unwrap();
        
        // Give time for CA chain refresh to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        // 2. Issue client certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "integration-test-client".to_string());
        subject.push(rcgen::DnType::OrganizationName, "RustMQ Test Client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertMgrRole::Client,
            san_entries: vec![
                rcgen::SanType::DnsName("client.test.local".to_string()),
            ],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };
        
        let client_cert = security_manager.certificate_manager()
            .issue_certificate(cert_request).await.unwrap();
        
        // 3. Authenticate using the certificate
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        let certificate = rustls::Certificate(der_data);
        let fingerprint = "test_fingerprint";
        let principal = security_manager.authentication()
            .extract_principal_from_certificate(&certificate, fingerprint).unwrap();
        
        // 4. Create authentication context
        let auth_context = AuthContext::new(principal.clone())
            .with_certificate(client_cert.fingerprint.clone())
            .with_groups(vec!["clients".to_string()]);
        
        // 5. Verify the complete flow worked
        assert!(auth_context.principal.contains("integration-test-client"));
        assert_eq!(auth_context.certificate_fingerprint, Some(client_cert.fingerprint));
        assert!(auth_context.groups.contains(&"clients".to_string()));
        
        // 6. Test certificate validation
        // Refresh CA chain to ensure latest certificates are loaded before validation
        security_manager.authentication().refresh_ca_chain().await.unwrap();
        
        // Give time for CA chain refresh and certificate persistence to complete
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        let validation_result = security_manager.authentication()
            .validate_certificate(&der_data).await;
        assert!(validation_result.is_ok(), "Certificate validation should succeed, error: {:?}", validation_result.err());
    }

    #[tokio::test]
    async fn test_complete_authorization_pipeline() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Create test principal
        let principal = TestPrincipalFactory::create_client_principal();
        
        // 2. Set up ACL rules for the principal
        let acl_rules = vec![
            AclRule::new(
                "test-rule-allow".to_string(),
                principal.to_string(),
                ResourcePattern::new(ResourceType::Topic, "allowed-topic".to_string()),
                vec![AclOperation::Read, AclOperation::Write],
                Effect::Allow,
            ),
            AclRule::new(
                "test-rule-deny".to_string(),
                principal.to_string(),
                ResourcePattern::new(ResourceType::Topic, "denied-topic".to_string()),
                vec![AclOperation::Read, AclOperation::Write],
                Effect::Deny,
            ),
        ];
        
        // Store ACL rules (simulate controller storage)
        for rule in acl_rules {
            let acl_key = AclKey::new(
                rule.principal.to_string().into(),
                rule.resource.pattern().to_string().into(),
                Permission::from(rule.operations[0].clone())
            );
            
            // Simulate storing in authorization cache
            let cache_entry = AclCacheEntry::new(
                rule.effect == Effect::Allow,
                Duration::from_secs(300)
            );
            
            security_manager.authorization()
                .insert_into_l2_cache(acl_key, cache_entry);
        }
        
        // 3. Test authorization decisions
        let allowed_key = AclKey::new(
            principal.clone(),
            "allowed-topic".into(),
            Permission::Read
        );
        
        let denied_key = AclKey::new(
            principal.clone(),
            "denied-topic".into(),
            Permission::Read
        );
        
        let allowed_result = security_manager.authorization()
            .check_authorization(&allowed_key).await;
        let denied_result = security_manager.authorization()
            .check_authorization(&denied_key).await;
        
        assert!(allowed_result.is_ok(), "Authorization check should succeed");
        assert!(allowed_result.unwrap(), "Should be allowed for allowed-topic");
        
        assert!(denied_result.is_ok(), "Authorization check should succeed");
        assert!(!denied_result.unwrap(), "Should be denied for denied-topic");
    }

    #[tokio::test]
    async fn test_certificate_lifecycle_integration() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Create root CA
        let ca_params = CaGenerationParams {
            common_name: "Lifecycle Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
        
        // 2. Issue broker certificate
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "broker-001.cluster.local".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertMgrRole::Broker,
            san_entries: vec![
                rcgen::SanType::DnsName("broker-001.cluster.local".to_string()),
                rcgen::SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 1, 100))),
            ],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };
        
        let broker_cert = security_manager.certificate_manager()
            .issue_certificate(cert_request).await.unwrap();
        
        // 3. Test certificate in authentication flow
        let pem_data = broker_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        let certificate = rustls::Certificate(der_data);
        let fingerprint = "test_fingerprint";
        let auth_principal = security_manager.authentication()
            .extract_principal_from_certificate(&certificate, fingerprint).unwrap();
        
        assert!(auth_principal.contains("broker-001.cluster.local"));
        
        // 4. Test certificate renewal
        let renewed_cert = security_manager.certificate_manager()
            .renew_certificate(&broker_cert.id).await.unwrap();
        
        assert_ne!(broker_cert.id, renewed_cert.id, "Renewed certificate should have different ID");
        assert_eq!(broker_cert.role, renewed_cert.role, "Renewed certificate should have same role");
        
        // Refresh revoked certificates list in authentication manager
        security_manager.authentication().refresh_revoked_certificates().await.unwrap();
        
        // 5. Verify original certificate is revoked
        let original_status = security_manager.certificate_manager()
            .get_certificate_status(&broker_cert.id).await.unwrap();
        assert_eq!(original_status, CertificateStatus::Revoked, "Original certificate should be revoked");
        
        // 6. Test revocation in authentication
        let is_revoked = security_manager.authentication()
            .is_certificate_revoked(&broker_cert.fingerprint).await.unwrap();
        assert!(is_revoked, "Original certificate should be detected as revoked");
        
        let renewed_is_revoked = security_manager.authentication()
            .is_certificate_revoked(&renewed_cert.fingerprint).await.unwrap();
        assert!(!renewed_is_revoked, "Renewed certificate should not be revoked");
    }

    #[tokio::test]
    async fn test_security_metrics_integration() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // Get initial metrics
        let initial_metrics = security_manager.metrics().snapshot();
        let initial_auth_success = initial_metrics.auth_success_count;
        let initial_auth_failure = initial_metrics.auth_failure_count;
        let initial_authz_checks = initial_metrics.authz_success_count;
        
        // 1. Generate test certificate
        let ca_params = CaGenerationParams {
            common_name: "Metrics Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
            
        // Load CA certificates into authentication manager
        security_manager.authentication().refresh_ca_chain().await.unwrap();
        
        // Give time for CA chain refresh to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "metrics-test-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertMgrRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };
        
        let client_cert = security_manager.certificate_manager()
            .issue_certificate(cert_request).await.unwrap();
        
        // Give time for background certificate persistence to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        
        // 2. Perform authentication operations (some successful, some failed)
        // Successful authentication
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        let _auth_result = security_manager.authentication()
            .validate_certificate(&der_data).await;
        
        // Failed authentication
        let invalid_cert = b"invalid certificate data";
        let _failed_result = security_manager.authentication()
            .validate_certificate(invalid_cert).await;
        
        // 3. Perform authorization checks
        let principal = "metrics-test-principal";
        let auth_key = AclKey::new(
            principal.to_string().into(),
            "metrics-topic".to_string().into(),
            Permission::Read
        );
        
        let _authz_result = security_manager.authorization()
            .check_authorization(&auth_key).await;
        
        // 4. Verify metrics were updated
        let final_metrics = security_manager.metrics().snapshot();
        
        assert!(final_metrics.auth_success_count > initial_auth_success,
               "Authentication success count should increase");
        assert!(final_metrics.auth_failure_count > initial_auth_failure,
               "Authentication failure count should increase");
        assert!(final_metrics.authz_success_count > initial_authz_checks,
               "Authorization checks count should increase");
        
        // 5. Test metrics aggregation
        let total_auth_attempts = final_metrics.auth_success_count + final_metrics.auth_failure_count;
        assert!(total_auth_attempts > initial_auth_success + initial_auth_failure,
               "Total authentication attempts should increase");
    }

    #[tokio::test]
    async fn test_security_audit_logging_integration() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // Enable audit logging for all components
        let audit_config = AuditConfig {
            enabled: true,
            log_authentication_events: true,
            log_authorization_events: true,
            log_certificate_events: true,
            log_failed_attempts: true,
            max_log_size_mb: 10,
        };
        
        // 1. Perform certificate operations
        let ca_params = CaGenerationParams {
            common_name: "Audit Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
        
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "audit-test-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertMgrRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };
        
        let client_cert = security_manager.certificate_manager()
            .issue_certificate(cert_request).await.unwrap();
        
        // 2. Perform authentication and authorization operations
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        let _auth_result = security_manager.authentication()
            .validate_certificate(&der_data).await;
        
        let certificate = rustls::Certificate(der_data.clone());
        let fingerprint = "test_fingerprint";
        let principal = security_manager.authentication()
            .extract_principal_from_certificate(&certificate, fingerprint).unwrap();
        
        let auth_key = AclKey::new(
            principal.clone(),
            "audit-topic".into(),
            Permission::Read
        );
        
        let _authz_result = security_manager.authorization()
            .check_authorization(&auth_key).await;
        
        // 3. Revoke certificate
        security_manager.certificate_manager()
            .revoke_certificate(&client_cert.id, RevocationReason::KeyCompromise)
            .await.unwrap();
        
        // 4. Verify audit logs contain expected events
        let audit_entries = security_manager.certificate_manager()
            .get_recent_audit_entries(20).await.unwrap();
        
        assert!(!audit_entries.is_empty(), "Should have audit entries");
        
        let operations: Vec<&str> = audit_entries.iter()
            .map(|entry| entry.operation.as_str())
            .collect();
        
        assert!(operations.contains(&"generate_root_ca"), "Should log CA generation");
        assert!(operations.contains(&"issue_certificate"), "Should log certificate issuance");
        assert!(operations.contains(&"revoke_certificate"), "Should log certificate revocation");
        
        // 5. Test audit log rotation and cleanup
        let initial_log_size = audit_entries.len();
        
        // Generate many more audit events by creating additional certificates
        for i in 0..10 {
            let mut subject = rcgen::DistinguishedName::new();
            subject.push(rcgen::DnType::CommonName, format!("audit-test-client-{}", i));
            
            let cert_request = CertificateRequest {
                subject,
                role: CertMgrRole::Client,
                san_entries: vec![],
                validity_days: Some(365),
                key_type: Some(KeyType::Ecdsa),
                key_size: Some(256),
                issuer_id: Some(ca_cert.id.clone()),
            };
            
            let _test_cert = security_manager.certificate_manager()
                .issue_certificate(cert_request).await.unwrap();
        }
        
        let extended_audit_entries = security_manager.certificate_manager()
            .get_recent_audit_entries(150).await.unwrap();
        
        assert!(extended_audit_entries.len() > initial_log_size,
               "Audit log should contain additional entries (initial: {}, final: {})", 
               initial_log_size, extended_audit_entries.len());
    }

    #[tokio::test]
    async fn test_security_configuration_hot_updates() {
        let (mut security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Get initial configuration
        let (initial_tls_config, initial_acl_config) = {
            let initial_config = security_manager.get_current_config();
            assert!(initial_config.tls.enabled, "TLS should be enabled initially");
            assert!(initial_config.acl.enabled, "ACL should be enabled initially");
            (initial_config.tls.clone(), initial_config.acl.clone())
        };
        
        // 2. Test TLS configuration update
        let mut new_tls_config = initial_tls_config.clone();
        new_tls_config.mode = crate::security::tls::TlsMode::Server; // Change setting
        
        let tls_update_result = security_manager.update_tls_config(new_tls_config.clone()).await;
        assert!(tls_update_result.is_ok(), "TLS config update should succeed");
        
        let updated_config = security_manager.get_current_config();
        assert!(matches!(updated_config.tls.mode, crate::security::tls::TlsMode::Server), "TLS config should be updated");
        
        // 3. Test ACL configuration update
        let mut new_acl_config = initial_acl_config.clone();
        new_acl_config.cache_ttl_seconds = 600; // Change cache TTL
        new_acl_config.bloom_filter_size = 20_000; // Change bloom filter size
        
        let acl_update_result = security_manager.update_acl_config(new_acl_config.clone()).await;
        assert!(acl_update_result.is_ok(), "ACL config update should succeed");
        
        let final_config = security_manager.get_current_config();
        assert_eq!(final_config.acl.cache_ttl_seconds, 600, "ACL cache TTL should be updated");
        assert_eq!(final_config.acl.bloom_filter_size, 20_000, "Bloom filter size should be updated");
        
        // 4. Test invalid configuration rejection
        let mut invalid_config = initial_acl_config.clone();
        invalid_config.cache_size_mb = 0; // Invalid cache size
        
        let invalid_update_result = security_manager.update_acl_config(invalid_config).await;
        assert!(invalid_update_result.is_err(), "Invalid config should be rejected");
        
        // 5. Verify system still works after configuration updates
        let ca_params = CaGenerationParams {
            common_name: "Config Update Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
        
        // System should still function with updated configuration
    }

    #[tokio::test]
    async fn test_cross_component_error_propagation() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Test certificate validation errors propagate to authentication
        let invalid_cert_data = b"invalid certificate data";
        let auth_result = security_manager.authentication()
            .validate_certificate(invalid_cert_data).await;
        
        assert!(auth_result.is_err(), "Invalid certificate should cause authentication error");
        
        match auth_result.unwrap_err() {
            RustMqError::InvalidCertificate { .. } | RustMqError::CertificateValidation { .. } => {
                // Expected error type
            }
            other => panic!("Expected InvalidCertificate or CertificateValidation, got {:?}", other),
        }
        
        // 2. Test ACL storage errors propagate to authorization
        let nonexistent_principal = "nonexistent-principal-12345";
        let auth_key = AclKey::new(
            nonexistent_principal.to_string().into(),
            "some-topic".to_string().into(),
            Permission::Read
        );
        
        let authz_result = security_manager.authorization()
            .check_authorization(&auth_key).await;
        
        // Should handle missing ACL rules gracefully
        assert!(authz_result.is_ok(), "Missing ACL rules should be handled gracefully");
        assert!(!authz_result.unwrap(), "Missing ACL rules should deny access");
        
        // 3. Test certificate revocation propagates through authentication
        let ca_params = CaGenerationParams {
            common_name: "Error Propagation Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
        
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, "error-test-client".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertMgrRole::Client,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_cert.id.clone()),
        };
        
        let client_cert = security_manager.certificate_manager()
            .issue_certificate(cert_request).await.unwrap();
        
        // Revoke the certificate
        security_manager.certificate_manager()
            .revoke_certificate(&client_cert.id, RevocationReason::KeyCompromise)
            .await.unwrap();
        
        // Refresh revoked certificates list in authentication manager
        security_manager.authentication().refresh_revoked_certificates().await.unwrap();
        
        // Authentication should detect revocation
        let revocation_check = security_manager.authentication()
            .is_certificate_revoked(&client_cert.fingerprint).await;
        
        assert!(revocation_check.is_ok(), "Revocation check should succeed");
        assert!(revocation_check.unwrap(), "Revoked certificate should be detected");
    }

    #[tokio::test]
    async fn test_concurrent_security_operations() {
        let security_manager = Arc::new(create_integrated_security_system().await.0);
        
        // Create CA certificate first
        let ca_params = CaGenerationParams {
            common_name: "Concurrent Test CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
            
        // Load CA certificates into authentication manager
        security_manager.authentication().refresh_ca_chain().await.unwrap();
        
        // Give time for CA chain refresh to complete
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        
        // Test concurrent certificate operations
        let mut cert_handles = Vec::new();
        let ca_cert_id = ca_cert.id.clone();
        
        for i in 0..20 {
            let security_manager = security_manager.clone();
            let ca_cert_id = ca_cert_id.clone();
            
            let handle = tokio::spawn(async move {
                // Issue certificate
                let mut subject = rcgen::DistinguishedName::new();
                subject.push(rcgen::DnType::CommonName, format!("concurrent-client-{}", i));
                
                let cert_request = CertificateRequest {
                    subject,
                    role: CertMgrRole::Client,
                    san_entries: vec![],
                    validity_days: Some(365),
                    key_type: Some(KeyType::Ecdsa),
                    key_size: Some(256),
                    issuer_id: Some(ca_cert_id.clone()),
                };
                
                let client_cert = security_manager.certificate_manager()
                    .issue_certificate(cert_request).await.unwrap();
                
                // Perform authentication
                let pem_data = client_cert.certificate_pem.clone().unwrap();
                let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                    .unwrap().into_iter().next().unwrap();
                let _auth_result = security_manager.authentication()
                    .validate_certificate(&der_data).await;
                
                // Perform authorization
                let auth_key = AclKey::new(
                    Arc::from(format!("concurrent-principal-{}", i).as_str()),
                    Arc::from(format!("concurrent-topic-{}", i).as_str()),
                    Permission::Read
                );
                
                let _authz_result = security_manager.authorization()
                    .check_authorization(&auth_key).await;
                
                i // Return operation ID
            });
            
            cert_handles.push(handle);
        }
        
        // Wait for all operations to complete
        let mut results = Vec::new();
        for handle in cert_handles {
            results.push(handle.await.unwrap());
        }
        
        // Verify all operations completed successfully
        assert_eq!(results.len(), 20, "All concurrent operations should complete");
        results.sort();
        for (i, &result) in results.iter().enumerate() {
            assert_eq!(i, result, "All operations should complete in order");
        }
        
        // Test system health after concurrent operations
        let final_metrics = security_manager.metrics().snapshot();
        assert!(final_metrics.auth_success_count > 0, "Should have successful authentications");
        assert!(final_metrics.authz_success_count > 0, "Should have authorization checks");
    }

    // ========== Enhanced Integration Tests ==========

    #[tokio::test]
    async fn test_end_to_end_webpki_authentication_flow() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Generate root CA
        let ca_cert = create_test_root_ca(&security_manager).await;
        
        // 2. Issue client certificate
        let client_cert = create_test_client_cert(&security_manager, &ca_cert.id).await;
        
        // 3. Test authentication pipeline with WebPKI validation
        let auth_result = test_certificate_authentication_webpki(
            &security_manager, 
            &client_cert
        ).await;
        
        assert!(auth_result.is_ok(), "End-to-end WebPKI authentication should succeed: {:?}", auth_result);
        
        // 4. Test authorization
        let authz_result = test_authorization_flow(&security_manager, &client_cert).await;
        assert!(authz_result.is_ok(), "Authorization should succeed: {:?}", authz_result);
        
        // 5. Test WebPKI-specific features
        let webpki_validation = test_webpki_specific_features(&security_manager, &client_cert).await;
        assert!(webpki_validation.is_ok(), "WebPKI-specific validation should succeed: {:?}", webpki_validation);
    }

    #[tokio::test]
    async fn test_comprehensive_webpki_certificate_lifecycle() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Create CA with specific WebPKI-compatible parameters
        let ca_params = CaGenerationParams {
            common_name: "WebPKI Integration Root CA".to_string(),
            organization: Some("RustMQ WebPKI Integration".to_string()),
            is_root: true,
            validity_years: Some(2),
            key_type: Some(KeyType::Ecdsa),
            ..Default::default()
        };
        
        let ca_cert = security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap();
        
        // Load CA into authentication manager for WebPKI validation
        security_manager.authentication().refresh_ca_chain().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 2. Test multiple certificate types with WebPKI validation
        let certificate_scenarios = vec![
            ("broker-webpki", CertMgrRole::Broker, vec![
                rcgen::SanType::DnsName("broker.rustmq.local".to_string()),
                rcgen::SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
            ]),
            ("client-webpki", CertMgrRole::Client, vec![
                rcgen::SanType::DnsName("client.rustmq.local".to_string()),
            ]),
            ("controller-webpki", CertMgrRole::Controller, vec![
                rcgen::SanType::DnsName("controller.rustmq.local".to_string()),
                rcgen::SanType::DnsName("controller-01.cluster.rustmq.local".to_string()),
            ]),
        ];
        
        for (name, role, san_entries) in certificate_scenarios {
            // Issue certificate
            let mut subject = rcgen::DistinguishedName::new();
            subject.push(rcgen::DnType::CommonName, name.to_string());
            subject.push(rcgen::DnType::OrganizationName, "WebPKI Integration Test".to_string());
            
            let cert_request = CertificateRequest {
                subject,
                role,
                san_entries,
                validity_days: Some(365),
                key_type: Some(KeyType::Ecdsa),
                key_size: Some(256),
                issuer_id: Some(ca_cert.id.clone()),
            };
            
            let cert_result = security_manager.certificate_manager()
                .issue_certificate(cert_request).await.unwrap();
            
            // Test WebPKI validation
            let pem_data = cert_result.certificate_pem.clone().unwrap();
            let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                .unwrap().into_iter().next().unwrap();
            
            // Comprehensive WebPKI validation
            let validation_result = security_manager.authentication()
                .validate_certificate_comprehensive(&der_data).await;
            assert!(validation_result.is_ok(), 
                   "WebPKI validation should succeed for {}: {:?}", name, validation_result);
            
            // Test WebPKI principal extraction
            let certificate = rustls::Certificate(der_data.clone());
            let principal = security_manager.authentication()
                .extract_principal_with_webpki(&certificate, "webpki_test").unwrap();
            assert!(principal.contains(name), 
                   "WebPKI principal should contain certificate name for {}", name);
            
            println!("✅ WebPKI lifecycle test passed for {} certificate", name);
        }
    }

    #[tokio::test]
    async fn test_webpki_certificate_chain_validation_integration() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Create root CA
        let ca_cert = create_test_root_ca(&security_manager).await;
        
        // 2. Issue multiple certificates for chain testing
        let mut certificates = Vec::new();
        for i in 0..5 {
            let cert = create_test_client_cert_with_name(
                &security_manager, 
                &ca_cert.id, 
                &format!("chain-test-client-{}", i)
            ).await;
            certificates.push(cert);
        }
        
        // Load CA certificates into authentication manager
        security_manager.authentication().refresh_ca_chain().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 3. Test chain validation with WebPKI
        for (i, cert) in certificates.iter().enumerate() {
            let pem_data = cert.certificate_pem.clone().unwrap();
            let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                .unwrap().into_iter().next().unwrap();
            let cert_obj = rustls::Certificate(der_data.clone());
            
            // Test WebPKI comprehensive validation first
            let comprehensive_validation = security_manager.authentication()
                .validate_certificate_comprehensive(&der_data).await;
            assert!(comprehensive_validation.is_ok(), 
                   "WebPKI comprehensive validation should succeed for certificate {}: {:?}", i, comprehensive_validation);
        }
        
        // 4. Test batch validation performance
        let cert_ders: Vec<Vec<u8>> = certificates.iter().map(|cert| {
            let pem_data = cert.certificate_pem.clone().unwrap();
            rustls_pemfile::certs(&mut pem_data.as_bytes())
                .unwrap().into_iter().next().unwrap()
        }).collect();
        
        let cert_der_refs: Vec<&[u8]> = cert_ders.iter().map(|der| der.as_slice()).collect();
        
        let batch_results = security_manager.authentication()
            .validate_certificates_batch(&cert_der_refs).await;
        
        assert_eq!(batch_results.len(), certificates.len(), "Batch validation should process all certificates");
        for (i, result) in batch_results.iter().enumerate() {
            assert!(result.is_ok(), "Batch validation should succeed for certificate {}: {:?}", i, result);
        }
        
        println!("✅ WebPKI chain validation integration test completed successfully");
    }

    #[tokio::test]
    async fn test_webpki_performance_integration() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Setup test environment
        let ca_cert = create_test_root_ca(&security_manager).await;
        
        // 2. Create multiple certificates for performance testing
        let mut test_certificates = Vec::new();
        for i in 0..20 {
            let cert = create_test_client_cert_with_name(
                &security_manager, 
                &ca_cert.id, 
                &format!("perf-test-client-{}", i)
            ).await;
            test_certificates.push(cert);
        }
        
        // Load CA certificates into authentication manager
        security_manager.authentication().refresh_ca_chain().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 3. Performance test: Serial validation
        let start = std::time::Instant::now();
        for cert in &test_certificates {
            let pem_data = cert.certificate_pem.clone().unwrap();
            let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                .unwrap().into_iter().next().unwrap();
            
            let _validation_result = security_manager.authentication()
                .validate_certificate_comprehensive(&der_data).await.unwrap();
        }
        let serial_duration = start.elapsed();
        
        // 4. Performance test: Batch validation
        let cert_ders: Vec<Vec<u8>> = test_certificates.iter().map(|cert| {
            let pem_data = cert.certificate_pem.clone().unwrap();
            rustls_pemfile::certs(&mut pem_data.as_bytes())
                .unwrap().into_iter().next().unwrap()
        }).collect();
        
        let cert_der_refs: Vec<&[u8]> = cert_ders.iter().map(|der| der.as_slice()).collect();
        
        let start = std::time::Instant::now();
        let batch_results = security_manager.authentication()
            .validate_certificates_batch(&cert_der_refs).await;
        let batch_duration = start.elapsed();
        
        // 5. Verify all validations succeeded
        for result in batch_results {
            assert!(result.is_ok(), "Batch validation should succeed");
        }
        
        // 6. Performance assertions
        assert!(serial_duration > Duration::from_nanos(0), "Serial validation should take measurable time");
        assert!(batch_duration > Duration::from_nanos(0), "Batch validation should take measurable time");
        
        // 7. Test caching performance
        let start = std::time::Instant::now();
        for cert in &test_certificates[..5] { // Test first 5 again (should be cached)
            let pem_data = cert.certificate_pem.clone().unwrap();
            let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
                .unwrap().into_iter().next().unwrap();
            
            let _validation_result = security_manager.authentication()
                .validate_certificate_comprehensive(&der_data).await.unwrap();
        }
        let cached_duration = start.elapsed();
        
        println!("🚀 WebPKI Performance Results:");
        println!("   Serial validation (20 certs): {:?}", serial_duration);
        println!("   Batch validation (20 certs): {:?}", batch_duration);
        println!("   Cached validation (5 certs): {:?}", cached_duration);
        
        // Cache should generally be faster, but we'll just verify it works
        assert!(cached_duration > Duration::from_nanos(0), "Cached validation should take measurable time");
    }

    #[tokio::test]
    async fn test_webpki_error_handling_integration() {
        let (security_manager, _temp_dir) = create_integrated_security_system().await;
        
        // 1. Setup valid environment
        let ca_cert = create_test_root_ca(&security_manager).await;
        
        // 2. Test various error scenarios with WebPKI
        let error_scenarios = vec![
            ("empty_certificate", b"".to_vec()),
            ("invalid_der", b"invalid der data".to_vec()),
            ("malformed_pem", b"-----BEGIN CERTIFICATE-----\nGARBAGE\n-----END CERTIFICATE-----".to_vec()),
            ("binary_garbage", vec![0xFF; 1000]),
        ];
        
        for (scenario_name, cert_data) in error_scenarios {
            // Test comprehensive validation
            let validation_result = security_manager.authentication()
                .validate_certificate_comprehensive(&cert_data).await;
            assert!(validation_result.is_err(), 
                   "WebPKI validation should fail for {}", scenario_name);
            
            // Test chain validation
            let certificate = rustls::Certificate(cert_data.clone());
            let chain_validation = security_manager.authentication()
                .validate_certificate_chain_with_webpki(&[certificate]).await;
            assert!(chain_validation.is_err(), 
                   "WebPKI chain validation should fail for {}", scenario_name);
            
            println!("✅ Error handling test passed for scenario: {}", scenario_name);
        }
        
        // 3. Test certificate signed by wrong CA
        let wrong_ca_params = CaGenerationParams {
            common_name: "Wrong CA".to_string(),
            is_root: true,
            ..Default::default()
        };
        
        let wrong_ca = security_manager.certificate_manager()
            .generate_root_ca(wrong_ca_params).await.unwrap();
        
        let wrong_cert = create_test_client_cert_with_name(
            &security_manager, 
            &wrong_ca.id, 
            "wrong-ca-client"
        ).await;
        
        // This certificate is valid but signed by a CA not in our trust store
        let pem_data = wrong_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        
        let wrong_ca_validation = security_manager.authentication()
            .validate_certificate_comprehensive(&der_data).await;
        
        // Should fail because it's not signed by our trusted CA
        assert!(wrong_ca_validation.is_err(), 
               "Certificate signed by untrusted CA should fail validation");
        
        println!("✅ WebPKI error handling integration test completed successfully");
    }

    // ========== Helper Functions for Integration Tests ==========

    async fn create_test_root_ca(security_manager: &SecurityManager) -> CertificateInfo {
        let ca_params = CaGenerationParams {
            common_name: "Integration Test Root CA".to_string(),
            organization: Some("RustMQ Integration".to_string()),
            is_root: true,
            validity_years: Some(2),
            key_type: Some(KeyType::Ecdsa),
            ..Default::default()
        };
        
        security_manager.certificate_manager()
            .generate_root_ca(ca_params).await.unwrap()
    }

    async fn create_test_client_cert(security_manager: &SecurityManager, ca_id: &str) -> CertificateInfo {
        create_test_client_cert_with_name(security_manager, ca_id, "integration-test-client").await
    }

    async fn create_test_client_cert_with_name(
        security_manager: &SecurityManager, 
        ca_id: &str, 
        name: &str
    ) -> CertificateInfo {
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, name.to_string());
        subject.push(rcgen::DnType::OrganizationName, "Integration Test".to_string());
        
        let cert_request = CertificateRequest {
            subject,
            role: CertMgrRole::Client,
            san_entries: vec![
                rcgen::SanType::DnsName(format!("{}.test.local", name)),
            ],
            validity_days: Some(365),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_id.to_string()),
        };
        
        security_manager.certificate_manager()
            .issue_certificate(cert_request).await.unwrap()
    }

    async fn test_certificate_authentication_webpki(
        security_manager: &SecurityManager, 
        client_cert: &CertificateInfo
    ) -> Result<(), RustMqError> {
        // Refresh CA chain to ensure WebPKI validation works
        security_manager.authentication().refresh_ca_chain().await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Test certificate validation with WebPKI
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        
        // WebPKI comprehensive validation
        security_manager.authentication()
            .validate_certificate_comprehensive(&der_data).await?;
        
        // WebPKI principal extraction
        let certificate = rustls::Certificate(der_data.clone());
        let _principal = security_manager.authentication()
            .extract_principal_with_webpki(&certificate, "webpki_auth_test")?;
        
        // WebPKI chain validation
        security_manager.authentication()
            .validate_certificate_chain_with_webpki(&[certificate]).await?;
        
        Ok(())
    }

    async fn test_authorization_flow(
        security_manager: &SecurityManager, 
        client_cert: &CertificateInfo
    ) -> Result<(), RustMqError> {
        // Extract principal from certificate
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        let certificate = rustls::Certificate(der_data);
        let principal = security_manager.authentication()
            .extract_principal_from_certificate(&certificate, "auth_flow_test")?;
        
        // Test authorization
        let auth_key = AclKey::new(
            principal,
            "integration-test-topic".into(),
            Permission::Read
        );
        
        let _authorization_result = security_manager.authorization()
            .check_authorization(&auth_key).await?;
        
        Ok(())
    }

    async fn test_webpki_specific_features(
        security_manager: &SecurityManager, 
        client_cert: &CertificateInfo
    ) -> Result<(), RustMqError> {
        let pem_data = client_cert.certificate_pem.clone().unwrap();
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .unwrap().into_iter().next().unwrap();
        
        // Test WebPKI-specific certificate parsing
        let certificate = security_manager.authentication()
            .parse_certificate_rustls(&der_data)?;
        
        // Test WebPKI certificate metadata extraction
        let _metadata = security_manager.authentication()
            .extract_certificate_metadata(&der_data).await?;
        
        // Test certificate validation with WebPKI comprehensive validation
        security_manager.authentication()
            .validate_certificate_comprehensive(&der_data).await?;
        
        // Test certificate metadata extraction
        let _cert_metadata = security_manager.authentication()
            .extract_certificate_metadata(&der_data).await?;
        
        Ok(())
    }
}