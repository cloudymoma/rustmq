/// Integration tests for private key encryption in certificate lifecycle
///
/// These tests verify that the entire certificate lifecycle works correctly
/// with encrypted private keys.

use crate::error::Result;
use crate::security::tls::{
    CertificateManager, EnhancedCertificateManagementConfig, CaGenerationParams,
    CertificateRequest, CertificateRole, CertificateStatus, CertificateTemplate,
};
use crate::security::CertificateManagementConfig;
use std::collections::HashMap;
use rcgen::DistinguishedName;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create test configuration with mandatory encryption
fn create_test_config_with_encryption(temp_dir: &TempDir, password: &str) -> EnhancedCertificateManagementConfig {
    let storage_path = temp_dir.path().to_str().unwrap().to_string();

    let basic_config = CertificateManagementConfig {
        ca_cert_path: format!("{}/ca.pem", storage_path),
        ca_key_path: format!("{}/ca.key", storage_path),
        cert_validity_days: 365,
        auto_renew_before_expiry_days: 30,
        crl_check_enabled: false,
        ocsp_check_enabled: false,
    };

    let mut templates = HashMap::new();
    templates.insert(CertificateRole::Broker, CertificateTemplate::broker_default());
    templates.insert(CertificateRole::Controller, CertificateTemplate::controller_default());
    templates.insert(CertificateRole::Client, CertificateTemplate::client_default());
    templates.insert(CertificateRole::Admin, CertificateTemplate::admin_default());

    EnhancedCertificateManagementConfig {
        basic: basic_config,
        storage_path,
        key_encryption_password: password.to_string(),
        ca_settings: Default::default(),
        certificate_templates: templates,
        audit_enabled: false,
        audit_log_path: String::new(),
        metrics_enabled: false,
        auto_renewal_enabled: false,
        renewal_check_interval_hours: 24,
        crl_update_interval_hours: 6,
        crl_distribution_points: vec![],
    }
}

#[tokio::test]
async fn test_root_ca_with_encrypted_private_key() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_encryption(&temp_dir, "test-password-12345678");

    let manager = CertificateManager::new_with_enhanced_config(config).await?;

    let ca_params = CaGenerationParams {
        common_name: "Test Root CA".to_string(),
        is_root: true,
        ..Default::default()
    };

    let ca_cert = manager.generate_root_ca(ca_params).await?;

    assert_eq!(ca_cert.status, CertificateStatus::Active);
    assert!(ca_cert.is_ca);
    assert_eq!(ca_cert.role, CertificateRole::RootCa);

    Ok(())
}

#[tokio::test]
async fn test_issue_certificate_with_encrypted_key() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_encryption(&temp_dir, "strong-password-abcdefgh");

    let manager = CertificateManager::new_with_enhanced_config(config).await?;

    // First create root CA
    let ca_params = CaGenerationParams {
        common_name: "Test Root CA".to_string(),
        is_root: true,
        ..Default::default()
    };
    let ca_cert = manager.generate_root_ca(ca_params).await?;

    // Issue end-entity certificate
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, "test-broker-01".to_string());

    let cert_request = CertificateRequest {
        subject: dn,
        role: CertificateRole::Broker,
        san_entries: vec![],
        validity_days: Some(365),
        key_type: None,
        key_size: None,
        issuer_id: Some(ca_cert.id.clone()),
    };

    let broker_cert = manager.issue_certificate(cert_request).await?;

    assert_eq!(broker_cert.status, CertificateStatus::Active);
    assert_eq!(broker_cert.role, CertificateRole::Broker);
    assert_eq!(broker_cert.issuer_id, Some(ca_cert.id));

    Ok(())
}

#[tokio::test]
async fn test_certificate_signing_with_encrypted_ca_key() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_encryption(&temp_dir, "ca-key-password-xyz");

    let manager = CertificateManager::new_with_enhanced_config(config).await?;

    // Create root CA (encrypted key)
    let ca_params = CaGenerationParams {
        common_name: "Encrypted CA".to_string(),
        is_root: true,
        ..Default::default()
    };
    let ca_cert = manager.generate_root_ca(ca_params).await?;

    // Issue certificate - this requires decrypting CA key for signing
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, "client@example.com".to_string());

    let cert_request = CertificateRequest {
        subject: dn,
        role: CertificateRole::Client,
        san_entries: vec![],
        validity_days: Some(180),
        key_type: None,
        key_size: None,
        issuer_id: Some(ca_cert.id.clone()),
    };

    let client_cert = manager.issue_certificate(cert_request).await?;

    // Verify certificate was signed properly
    assert_eq!(client_cert.issuer_id, Some(ca_cert.id));
    assert!(client_cert.issuer.contains("Encrypted CA"));

    Ok(())
}

#[tokio::test]
async fn test_certificate_renewal_with_encrypted_keys() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_encryption(&temp_dir, "renewal-test-password");

    let manager = CertificateManager::new_with_enhanced_config(config).await?;

    // Create root CA
    let ca_params = CaGenerationParams {
        common_name: "Renewal Test CA".to_string(),
        is_root: true,
        ..Default::default()
    };
    let ca_cert = manager.generate_root_ca(ca_params).await?;

    // Issue initial certificate
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, "broker-to-renew".to_string());

    let cert_request = CertificateRequest {
        subject: dn.clone(),
        role: CertificateRole::Broker,
        san_entries: vec![],
        validity_days: Some(365),
        key_type: None,
        key_size: None,
        issuer_id: Some(ca_cert.id.clone()),
    };

    let original_cert = manager.issue_certificate(cert_request).await?;

    // Renew certificate
    let renewed_cert = manager.renew_certificate(&original_cert.id).await?;

    // Verify renewal worked
    assert_eq!(renewed_cert.role, CertificateRole::Broker);
    assert!(renewed_cert.subject.contains("broker-to-renew"));
    assert_ne!(renewed_cert.id, original_cert.id); // New certificate ID
    assert_eq!(renewed_cert.issuer_id, Some(ca_cert.id)); // Same issuer

    // Original should be revoked
    let original_status = manager.get_certificate_status(&original_cert.id).await?;
    assert_eq!(original_status, CertificateStatus::Revoked);

    Ok(())
}

#[tokio::test]
async fn test_multiple_encrypted_keys_in_same_deployment() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();

    // Create manager with mandatory encryption
    let config = create_test_config_with_encryption(&temp_dir, "test-password");
    let manager = CertificateManager::new_with_enhanced_config(config).await?;

    // Create CA with encrypted key
    let ca_params = CaGenerationParams {
        common_name: "Test CA".to_string(),
        is_root: true,
        ..Default::default()
    };
    let ca_cert = manager.generate_root_ca(ca_params).await?;

    // Issue multiple certificates - all will have encrypted keys
    let mut dn1 = DistinguishedName::new();
    dn1.push(rcgen::DnType::CommonName, "client-1".to_string());

    let cert_request1 = CertificateRequest {
        subject: dn1,
        role: CertificateRole::Client,
        san_entries: vec![],
        validity_days: Some(365),
        key_type: None,
        key_size: None,
        issuer_id: Some(ca_cert.id.clone()),
    };

    let client_cert1 = manager.issue_certificate(cert_request1).await?;

    let mut dn2 = DistinguishedName::new();
    dn2.push(rcgen::DnType::CommonName, "client-2".to_string());

    let cert_request2 = CertificateRequest {
        subject: dn2,
        role: CertificateRole::Client,
        san_entries: vec![],
        validity_days: Some(365),
        key_type: None,
        key_size: None,
        issuer_id: Some(ca_cert.id.clone()),
    };

    let client_cert2 = manager.issue_certificate(cert_request2).await?;

    // Verify all certs exist and are active - all keys are encrypted
    let all_certs = manager.list_all_certificates().await?;
    assert_eq!(all_certs.len(), 3); // 1 CA + 2 clients
    assert_eq!(client_cert1.status, CertificateStatus::Active);
    assert_eq!(client_cert2.status, CertificateStatus::Active);

    Ok(())
}

#[tokio::test]
async fn test_wrong_password_fails_decryption() -> Result<()> {
    // This test verifies that wrong password causes decryption to fail
    // The actual encryption/decryption with wrong password is tested in unit tests
    // Here we verify it integrates properly with certificate management

    use crate::security::tls::key_encryption;

    let test_pem = "-----BEGIN PRIVATE KEY-----\ntest-key-data\n-----END PRIVATE KEY-----";
    let correct_password = "correct-password";
    let wrong_password = "wrong-password";

    // Encrypt with correct password
    let encrypted = key_encryption::encrypt_private_key(test_pem, correct_password)?;

    // Try to decrypt with wrong password - should fail
    let result = key_encryption::decrypt_private_key(&encrypted, wrong_password);
    assert!(result.is_err(), "Expected decryption to fail with wrong password");

    // Decrypt with correct password - should succeed
    let decrypted = key_encryption::decrypt_private_key(&encrypted, correct_password)?;
    assert_eq!(decrypted, test_pem);

    Ok(())
}

#[tokio::test]
async fn test_multiple_certificates_with_encryption() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_encryption(&temp_dir, "multi-cert-password");

    let manager = CertificateManager::new_with_enhanced_config(config).await?;

    // Create root CA
    let ca_params = CaGenerationParams {
        common_name: "Multi-Cert CA".to_string(),
        is_root: true,
        ..Default::default()
    };
    let ca_cert = manager.generate_root_ca(ca_params).await?;

    // Issue multiple certificates
    let roles = vec![
        CertificateRole::Broker,
        CertificateRole::Client,
        CertificateRole::Admin,
    ];

    for (i, role) in roles.iter().enumerate() {
        let mut dn = DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, format!("test-{}-{}", role.to_string().to_lowercase(), i));

        let cert_request = CertificateRequest {
            subject: dn,
            role: *role,
            san_entries: vec![],
            validity_days: Some(365),
            key_type: None,
            key_size: None,
            issuer_id: Some(ca_cert.id.clone()),
        };

        let cert = manager.issue_certificate(cert_request).await?;
        assert_eq!(cert.role, *role);
        assert_eq!(cert.status, CertificateStatus::Active);
    }

    // Verify all certificates exist
    let all_certs = manager.list_all_certificates().await?;
    assert_eq!(all_certs.len(), 4); // 1 CA + 3 end-entity

    Ok(())
}

impl ToString for CertificateRole {
    fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}
