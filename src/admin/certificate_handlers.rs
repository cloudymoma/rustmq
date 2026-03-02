use crate::Result;
use crate::security::{
    CaGenerationParams, CertificateInfo, CertificateManager, CertificateRequest, CertificateRole,
    CertificateStatus, ExtendedKeyUsage, KeyType, KeyUsage, RevocationReason, ValidationResult,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Certificate management handlers for the Security API
pub struct CertificateHandlers {
    certificate_manager: Arc<CertificateManager>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertificateCreationResponse {
    pub certificate_id: String,
    pub subject: String,
    pub serial_number: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub status: String,
    pub fingerprint: String,
    pub certificate_pem: String,
    pub private_key_pem: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertificateOperationResult {
    pub certificate_id: String,
    pub operation: String,
    pub status: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

impl CertificateHandlers {
    pub fn new(certificate_manager: Arc<CertificateManager>) -> Self {
        Self {
            certificate_manager,
        }
    }

    /// Generate a new root CA certificate
    pub async fn generate_ca(
        &self,
        common_name: String,
        organization: Option<String>,
        country: Option<String>,
        validity_days: Option<u32>,
        key_size: Option<u32>,
    ) -> Result<CertificateCreationResponse> {
        info!("Generating root CA certificate: {}", common_name);

        // Create CA generation parameters
        let ca_params = CaGenerationParams {
            common_name: common_name.clone(),
            organization: organization.clone(),
            organizational_unit: None,
            country: country.clone(),
            state_province: None,
            locality: None,
            validity_years: validity_days.map(|days| days / 365),
            key_size: Some(key_size.unwrap_or(256)),
            key_type: Some(KeyType::Ecdsa),
            is_root: true,
        };

        // Generate the CA certificate
        let ca_info = self.certificate_manager.generate_root_ca(ca_params).await?;

        // Convert to response format
        let response = CertificateCreationResponse {
            certificate_id: ca_info.id.clone(),
            subject: ca_info.subject.clone(),
            serial_number: ca_info.serial_number.clone(),
            not_before: ca_info.not_before.into(),
            not_after: ca_info.not_after.into(),
            status: "Active".to_string(),
            fingerprint: ca_info.fingerprint.clone(),
            certificate_pem: ca_info.certificate_pem.clone().unwrap_or_default(),
            private_key_pem: ca_info.private_key_pem.clone(),
        };

        info!("Successfully generated root CA: {}", ca_info.id);
        Ok(response)
    }

    // Intermediate CA generation removed - only root CA supported for simplicity

    /// Issue a new certificate
    pub async fn issue_certificate(
        &self,
        ca_id: String,
        common_name: String,
        subject_alt_names: Option<Vec<String>>,
        organization: Option<String>,
        role: Option<String>,
        validity_days: Option<u32>,
        key_usage: Option<Vec<String>>,
        extended_key_usage: Option<Vec<String>>,
    ) -> Result<CertificateCreationResponse> {
        info!(
            "Issuing certificate for: {} using CA: {}",
            common_name, ca_id
        );

        // Verify CA exists and is active
        let ca_status = self
            .certificate_manager
            .get_certificate_status(&ca_id)
            .await?;
        if ca_status != CertificateStatus::Active {
            return Err(crate::error::RustMqError::CaNotAvailable(format!(
                "CA '{}' is not active",
                ca_id
            )));
        }

        // Parse role to determine certificate template
        let cert_role = role
            .as_ref()
            .and_then(|r| match r.as_str() {
                "broker" => Some(CertificateRole::Broker),
                "client" => Some(CertificateRole::Client),
                "admin" => Some(CertificateRole::Admin),
                _ => None,
            })
            .unwrap_or(CertificateRole::Client);

        // Create distinguished name
        let mut subject = rcgen::DistinguishedName::new();
        subject.push(rcgen::DnType::CommonName, common_name.clone());
        if let Some(org) = organization {
            subject.push(rcgen::DnType::OrganizationName, org);
        }

        // Convert subject_alt_names to SanType vector
        let san_entries: Vec<rcgen::SanType> = subject_alt_names
            .unwrap_or_default()
            .into_iter()
            .map(|san| rcgen::SanType::DnsName(san))
            .collect();

        // Create certificate request
        let cert_request = CertificateRequest {
            subject,
            role: cert_role,
            san_entries,
            validity_days: Some(validity_days.unwrap_or(365)),
            key_type: Some(KeyType::Ecdsa),
            key_size: Some(256),
            issuer_id: Some(ca_id.clone()),
        };

        // Issue the certificate
        let cert_info = self
            .certificate_manager
            .issue_certificate(cert_request)
            .await?;

        let response = CertificateCreationResponse {
            certificate_id: cert_info.id.clone(),
            subject: cert_info.subject.clone(),
            serial_number: cert_info.serial_number.clone(),
            not_before: cert_info.not_before.into(),
            not_after: cert_info.not_after.into(),
            status: "Active".to_string(),
            fingerprint: cert_info.fingerprint.clone(),
            certificate_pem: cert_info.certificate_pem.clone().unwrap_or_default(),
            private_key_pem: cert_info.private_key_pem.clone(),
        };

        info!("Successfully issued certificate: {}", cert_info.id);
        Ok(response)
    }

    /// Renew an existing certificate
    pub async fn renew_certificate(&self, cert_id: String) -> Result<CertificateOperationResult> {
        info!("Renewing certificate: {}", cert_id);

        // Get existing certificate info
        let cert_info = self
            .certificate_manager
            .get_certificate_by_id(&cert_id)
            .await?
            .ok_or_else(|| crate::error::RustMqError::CertificateNotFound {
                identifier: cert_id.clone(),
            })?;
        if cert_info.status == CertificateStatus::Revoked {
            return Err(crate::error::RustMqError::CertificateRevoked {
                subject: cert_id.clone(),
            });
        }

        // Perform renewal
        let renewed_cert = self.certificate_manager.renew_certificate(&cert_id).await?;

        let result = CertificateOperationResult {
            certificate_id: renewed_cert.id,
            operation: "renew".to_string(),
            status: "success".to_string(),
            message: format!(
                "Certificate renewed successfully. New expiry: {:?}",
                renewed_cert.not_after
            ),
            timestamp: Utc::now(),
        };

        info!("Successfully renewed certificate: {}", cert_id);
        Ok(result)
    }

    /// Rotate a certificate (new key pair)
    pub async fn rotate_certificate(&self, cert_id: String) -> Result<CertificateOperationResult> {
        info!("Rotating certificate: {}", cert_id);

        // Get existing certificate info
        let cert_info = self
            .certificate_manager
            .get_certificate_by_id(&cert_id)
            .await?
            .ok_or_else(|| crate::error::RustMqError::CertificateNotFound {
                identifier: cert_id.clone(),
            })?;
        if cert_info.status == CertificateStatus::Revoked {
            return Err(crate::error::RustMqError::CertificateRevoked {
                subject: cert_id.clone(),
            });
        }

        // Perform rotation (this creates a new certificate with new key pair)
        let rotated_cert = self
            .certificate_manager
            .rotate_certificate(&cert_id)
            .await?;

        let result = CertificateOperationResult {
            certificate_id: rotated_cert.id.clone(),
            operation: "rotate".to_string(),
            status: "success".to_string(),
            message: format!(
                "Certificate rotated successfully. New certificate ID: {}",
                rotated_cert.id
            ),
            timestamp: Utc::now(),
        };

        info!("Successfully rotated certificate: {}", cert_id);
        Ok(result)
    }

    /// Revoke a certificate
    pub async fn revoke_certificate(
        &self,
        cert_id: String,
        reason: String,
        reason_code: Option<u32>,
    ) -> Result<CertificateOperationResult> {
        warn!("Revoking certificate: {} with reason: {}", cert_id, reason);

        // Parse revocation reason
        let revocation_reason = match reason.to_lowercase().as_str() {
            "unspecified" => RevocationReason::Unspecified,
            "key_compromise" => RevocationReason::KeyCompromise,
            "ca_compromise" => RevocationReason::CaCompromise,
            "affiliation_changed" => RevocationReason::AffiliationChanged,
            "superseded" => RevocationReason::Superseded,
            "cessation_of_operation" => RevocationReason::CessationOfOperation,
            "certificate_hold" => RevocationReason::CertificateHold,
            "privilege_withdrawn" => RevocationReason::PrivilegeWithdrawn,
            "aa_compromise" => RevocationReason::AaCompromise,
            _ => RevocationReason::Unspecified,
        };

        // Revoke the certificate
        self.certificate_manager
            .revoke_certificate(&cert_id, revocation_reason)
            .await?;

        let result = CertificateOperationResult {
            certificate_id: cert_id.clone(),
            operation: "revoke".to_string(),
            status: "success".to_string(),
            message: format!("Certificate revoked successfully with reason: {}", reason),
            timestamp: Utc::now(),
        };

        warn!("Successfully revoked certificate: {}", cert_id);
        Ok(result)
    }

    /// List certificates with filtering
    pub async fn list_certificates(
        &self,
        filters: HashMap<String, String>,
    ) -> Result<Vec<CertificateInfo>> {
        debug!("Listing certificates with filters: {:?}", filters);

        // Parse filter parameters
        let status_filter = filters
            .get("status")
            .and_then(|s| match s.to_lowercase().as_str() {
                "active" => Some(CertificateStatus::Active),
                "expired" => Some(CertificateStatus::Expired),
                "revoked" => Some(CertificateStatus::Revoked),
                _ => None,
            });

        let role_filter = filters.get("role").and_then(|r| match r.as_str() {
            "broker" => Some(CertificateRole::Broker),
            "client" => Some(CertificateRole::Client),
            "admin" => Some(CertificateRole::Admin),
            _ => None,
        });

        let limit = filters
            .get("limit")
            .and_then(|l| l.parse::<usize>().ok())
            .unwrap_or(100);

        let offset = filters
            .get("offset")
            .and_then(|o| o.parse::<usize>().ok())
            .unwrap_or(0);

        // Get certificates from manager
        let mut certificates = self.certificate_manager.list_all_certificates().await?;

        // Apply filters
        if let Some(status) = status_filter {
            certificates.retain(|cert| cert.status == status);
        }

        if let Some(role) = role_filter {
            certificates.retain(|cert| cert.role == role);
        }

        // Apply pagination
        let total = certificates.len();
        let start = offset.min(total);
        let end = (offset + limit).min(total);
        certificates = certificates[start..end].to_vec();

        debug!(
            "Found {} certificates (showing {} from offset {})",
            total,
            certificates.len(),
            offset
        );
        Ok(certificates)
    }

    /// Get expiring certificates
    pub async fn get_expiring_certificates(
        &self,
        threshold_days: i64,
    ) -> Result<Vec<CertificateInfo>> {
        debug!(
            "Getting certificates expiring within {} days",
            threshold_days
        );

        let expiry_threshold = Utc::now() + chrono::Duration::days(threshold_days);

        // Get all certificates from the certificate manager
        let certificates = self
            .certificate_manager
            .list_all_certificates()
            .await
            .map_err(|e| crate::error::RustMqError::CertificateGeneration {
                reason: format!("Failed to list certificates: {}", e),
            })?;

        let expiring_certs: Vec<CertificateInfo> = certificates
            .into_iter()
            .filter(|cert| {
                cert.status == CertificateStatus::Active && {
                    if let Ok(duration) = cert.not_after.duration_since(std::time::UNIX_EPOCH) {
                        let cert_expiry = Utc.timestamp_opt(duration.as_secs() as i64, 0).single();
                        cert_expiry.map_or(false, |expiry| expiry <= expiry_threshold)
                    } else {
                        false
                    }
                }
            })
            .collect();

        info!(
            "Found {} certificates expiring within {} days",
            expiring_certs.len(),
            threshold_days
        );
        Ok(expiring_certs)
    }

    /// Validate a certificate from PEM and optionally its chain
    pub async fn validate_certificate(
        &self,
        certificate_pem: String,
        _chain_pem: Option<Vec<String>>,
        check_revocation: bool,
    ) -> Result<ValidationResult> {
        debug!("Validating certificate from PEM");

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Parse the PEM certificate
        let der_certs: Vec<_> = rustls_pemfile::certs(&mut certificate_pem.as_bytes())
            .filter_map(|r| r.ok())
            .collect();

        if der_certs.is_empty() {
            return Ok(ValidationResult {
                is_valid: false,
                errors: vec!["No valid certificates found in PEM data".to_string()],
                warnings: Vec::new(),
                chain_length: 0,
                trust_anchor: None,
            });
        }

        let cert_der = &der_certs[0];

        // Parse with x509_parser for validation
        let (_, parsed_cert) = x509_parser::parse_x509_certificate(cert_der).map_err(|e| {
            crate::error::RustMqError::CertificateValidation {
                reason: format!("Failed to parse certificate: {}", e),
            }
        })?;

        // Check validity period
        let now = chrono::Utc::now();
        let not_before_ts = parsed_cert.validity().not_before.timestamp();
        let not_after_ts = parsed_cert.validity().not_after.timestamp();

        if now.timestamp() < not_before_ts {
            errors.push("Certificate is not yet valid (not_before is in the future)".to_string());
        }
        if now.timestamp() > not_after_ts {
            errors.push("Certificate has expired".to_string());
        }

        // Check for weak key sizes
        let spki = parsed_cert.public_key();
        let key_bits = spki.subject_public_key.data.len() * 8;
        let alg_oid = spki.algorithm.algorithm.to_id_string();
        if alg_oid == "1.2.840.113549.1.1.1" && key_bits < 2048 {
            warnings.push(format!("RSA key size {} bits is below recommended minimum of 2048", key_bits));
        }

        // Check expiry warning (30 days)
        let days_until_expiry = (not_after_ts - now.timestamp()) / 86400;
        if days_until_expiry > 0 && days_until_expiry <= 30 {
            warnings.push(format!("Certificate expires in {} days", days_until_expiry));
        }

        // Check revocation if requested
        if check_revocation {
            let fingerprint = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(cert_der.as_ref());
                hex::encode(hasher.finalize())
            };
            let revoked_certs = self.certificate_manager.get_revoked_certificates().await?;
            if revoked_certs.iter().any(|rc| {
                hex::encode(parsed_cert.serial.to_bytes_be()) == rc.serial_number
            }) {
                errors.push("Certificate has been revoked".to_string());
            }
        }

        let trust_anchor = parsed_cert.issuer().to_string();

        Ok(ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
            chain_length: der_certs.len(),
            trust_anchor: Some(trust_anchor),
        })
    }

    /// Get certificate chain (simplified: end-entity cert + root CA)
    pub async fn get_certificate_chain(&self, cert_id: String) -> Result<Vec<String>> {
        debug!("Getting certificate chain for: {}", cert_id);

        let mut chain = Vec::new();

        // Get the end-entity certificate
        if let Some(cert_info) = self.certificate_manager.get_certificate_by_id(&cert_id).await? {
            if let Some(pem) = &cert_info.certificate_pem {
                chain.push(pem.clone());
            }

            // Get the CA certificate (issuer) if available
            if let Some(issuer_id) = &cert_info.issuer_id {
                if let Some(issuer_info) = self.certificate_manager.get_certificate_by_id(issuer_id).await? {
                    if let Some(issuer_pem) = &issuer_info.certificate_pem {
                        chain.push(issuer_pem.clone());
                    }
                }
            }
        }

        Ok(chain)
    }

    // Helper methods

    fn parse_key_usage(&self, key_usage: Option<Vec<String>>) -> Vec<KeyUsage> {
        key_usage
            .unwrap_or_default()
            .into_iter()
            .filter_map(|usage| match usage.to_lowercase().as_str() {
                "digital_signature" => Some(KeyUsage::DigitalSignature),
                "content_commitment" => Some(KeyUsage::NonRepudiation),
                "key_encipherment" => Some(KeyUsage::KeyEncipherment),
                "data_encipherment" => Some(KeyUsage::DataEncipherment),
                "key_agreement" => Some(KeyUsage::KeyAgreement),
                "key_cert_sign" => Some(KeyUsage::KeyCertSign),
                "crl_sign" => Some(KeyUsage::CrlSign),
                "encipher_only" => Some(KeyUsage::EncipherOnly),
                "decipher_only" => Some(KeyUsage::DecipherOnly),
                _ => None,
            })
            .collect()
    }

    fn parse_extended_key_usage(
        &self,
        extended_key_usage: Option<Vec<String>>,
    ) -> Vec<ExtendedKeyUsage> {
        extended_key_usage
            .unwrap_or_default()
            .into_iter()
            .filter_map(|usage| match usage.to_lowercase().as_str() {
                "server_auth" => Some(ExtendedKeyUsage::ServerAuth),
                "client_auth" => Some(ExtendedKeyUsage::ClientAuth),
                "code_signing" => Some(ExtendedKeyUsage::CodeSigning),
                "email_protection" => Some(ExtendedKeyUsage::EmailProtection),
                "time_stamping" => Some(ExtendedKeyUsage::TimeStamping),
                "ocsp_signing" => Some(ExtendedKeyUsage::OcspSigning),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::EnhancedCertificateManagementConfig;
    use tempfile::TempDir;

    async fn create_test_handlers() -> (CertificateHandlers, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = EnhancedCertificateManagementConfig {
            storage_path: temp_dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let cert_manager = CertificateManager::new_with_enhanced_config(config)
            .await
            .unwrap();
        let handlers = CertificateHandlers::new(Arc::new(cert_manager));
        (handlers, temp_dir)
    }

    #[tokio::test]
    async fn test_generate_ca() {
        let (handlers, _temp_dir) = create_test_handlers().await;

        let result = handlers
            .generate_ca(
                "Test Root CA".to_string(),
                Some("Test Corp".to_string()),
                Some("US".to_string()),
                Some(365),
                Some(256),
            )
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, "Active");
        assert!(response.certificate_pem.contains("BEGIN CERTIFICATE"));
        // Private key is not returned in API response for security (stored encrypted)
        assert!(response.private_key_pem.is_none());
    }

    #[tokio::test]
    async fn test_issue_certificate() {
        let (handlers, _temp_dir) = create_test_handlers().await;

        // First generate a CA
        let ca_response = handlers
            .generate_ca(
                "Test Root CA".to_string(),
                Some("Test Corp".to_string()),
                None,
                Some(365),
                None,
            )
            .await
            .unwrap();

        // Then issue a certificate
        let result = handlers
            .issue_certificate(
                ca_response.certificate_id,
                "test.example.com".to_string(),
                Some(vec![
                    "test.example.com".to_string(),
                    "192.168.1.100".to_string(),
                ]),
                Some("Test Corp".to_string()),
                Some("broker".to_string()),
                Some(90),
                Some(vec![
                    "digital_signature".to_string(),
                    "key_encipherment".to_string(),
                ]),
                Some(vec!["server_auth".to_string(), "client_auth".to_string()]),
            )
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, "Active");
        assert!(response.certificate_pem.contains("BEGIN CERTIFICATE"));
    }

    #[tokio::test]
    async fn test_list_certificates_with_filters() {
        let (handlers, _temp_dir) = create_test_handlers().await;

        // Generate a CA and issue some certificates first
        let ca_response = handlers
            .generate_ca("Test Root CA".to_string(), None, None, None, None)
            .await
            .unwrap();

        let _cert1 = handlers
            .issue_certificate(
                ca_response.certificate_id.clone(),
                "broker1.example.com".to_string(),
                None,
                None,
                Some("broker".to_string()),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let _cert2 = handlers
            .issue_certificate(
                ca_response.certificate_id,
                "client1.example.com".to_string(),
                None,
                None,
                Some("client".to_string()),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        // Test listing with filters
        let mut filters = HashMap::new();
        filters.insert("status".to_string(), "active".to_string());
        filters.insert("limit".to_string(), "10".to_string());

        let result = handlers.list_certificates(filters).await;
        assert!(result.is_ok());

        let certificates = result.unwrap();
        assert!(!certificates.is_empty());
        assert!(certificates.len() <= 10);
    }

    #[tokio::test]
    async fn test_get_expiring_certificates() {
        let (handlers, _temp_dir) = create_test_handlers().await;

        // Generate a CA
        let ca_response = handlers
            .generate_ca("Test Root CA".to_string(), None, None, None, None)
            .await
            .unwrap();

        // Issue a certificate with short validity
        let cert = handlers
            .issue_certificate(
                ca_response.certificate_id.clone(),
                "expiring.example.com".to_string(),
                None,
                None,
                None,
                Some(1), // 1 day validity
                None,
                None,
            )
            .await
            .unwrap();

        // Verify the certificate was created
        println!("Created certificate with ID: {}", cert.certificate_id);
        println!("Certificate expires at: {:?}", cert.not_after);

        // Check for expiring certificates
        let result = handlers.get_expiring_certificates(7).await; // Within 7 days
        assert!(result.is_ok());

        let expiring_certs = result.unwrap();
        println!("Found {} expiring certificates", expiring_certs.len());
        for cert in &expiring_certs {
            println!(
                "  - Certificate {}: expires at {:?}",
                cert.id, cert.not_after
            );
        }

        // We should have at least one expiring certificate (the one we just created)
        assert!(
            !expiring_certs.is_empty(),
            "Expected at least one expiring certificate but found none"
        );
    }

    #[tokio::test]
    async fn test_revoke_certificate() {
        let (handlers, _temp_dir) = create_test_handlers().await;

        // Generate a CA and issue a certificate
        let ca_response = handlers
            .generate_ca("Test Root CA".to_string(), None, None, None, None)
            .await
            .unwrap();

        let cert_response = handlers
            .issue_certificate(
                ca_response.certificate_id,
                "revoke-test.example.com".to_string(),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        // Revoke the certificate
        let result = handlers
            .revoke_certificate(
                cert_response.certificate_id,
                "key_compromise".to_string(),
                Some(1),
            )
            .await;

        assert!(result.is_ok());
        let operation_result = result.unwrap();
        assert_eq!(operation_result.operation, "revoke");
        assert_eq!(operation_result.status, "success");
    }

    #[tokio::test]
    async fn test_parse_key_usage() {
        let (handlers, _temp_dir) = create_test_handlers().await;

        let key_usages = vec![
            "digital_signature".to_string(),
            "key_encipherment".to_string(),
            "invalid_usage".to_string(), // Should be filtered out
        ];

        let parsed = handlers.parse_key_usage(Some(key_usages));
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains(&KeyUsage::DigitalSignature));
        assert!(parsed.contains(&KeyUsage::KeyEncipherment));
    }

    #[tokio::test]
    async fn test_parse_extended_key_usage() {
        let (handlers, _temp_dir) = create_test_handlers().await;

        let extended_key_usages = vec![
            "server_auth".to_string(),
            "client_auth".to_string(),
            "invalid_extended_usage".to_string(), // Should be filtered out
        ];

        let parsed = handlers.parse_extended_key_usage(Some(extended_key_usages));
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains(&ExtendedKeyUsage::ServerAuth));
        assert!(parsed.contains(&ExtendedKeyUsage::ClientAuth));
    }
}
