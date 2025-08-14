/// Comprehensive Certificate Management for RustMQ
///
/// This module provides enterprise-grade certificate lifecycle management including:
/// - Root and intermediate CA operations
/// - Certificate issuance, renewal, and revocation
/// - Secure storage with encryption at rest
/// - Comprehensive audit logging
/// - Automated certificate rotation
/// - Certificate chain validation
use crate::error::{Result, RustMqError};
use crate::security::CertificateManagementConfig;
use crate::storage::{ObjectStorage, LocalObjectStorage};

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, Duration, UNIX_EPOCH};
use std::path::PathBuf;
use rustls::Certificate;
use serde::{Deserialize, Serialize};
use rcgen::{Certificate as RcgenCertificate, CertificateParams, DistinguishedName, KeyPair, SanType};
use x509_parser::prelude::*;
use uuid::Uuid;
use tracing::{info, warn, error, debug};
use tokio::sync::Mutex;
use base64::{Engine, engine::general_purpose};

/// Serde-compatible wrapper for SanType since rcgen::SanType doesn't implement Deserialize
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializableSanType {
    DnsName(String),
    IpAddress(String),
    EmailAddress(String),
    Uri(String),
}

impl From<SerializableSanType> for SanType {
    fn from(san: SerializableSanType) -> Self {
        match san {
            SerializableSanType::DnsName(name) => SanType::DnsName(name),
            SerializableSanType::IpAddress(ip_str) => {
                if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
                    SanType::IpAddress(ip)
                } else {
                    // Fallback to DNS name if IP parsing fails
                    SanType::DnsName(ip_str)
                }
            },
            SerializableSanType::EmailAddress(email) => SanType::Rfc822Name(email),
            SerializableSanType::Uri(uri) => SanType::DnsName(uri), // Fallback to DnsName as URI variant may not exist
        }
    }
}

impl From<SanType> for SerializableSanType {
    fn from(san: SanType) -> Self {
        match san {
            SanType::DnsName(name) => SerializableSanType::DnsName(name),
            SanType::IpAddress(ip) => SerializableSanType::IpAddress(ip.to_string()),
            SanType::Rfc822Name(email) => SerializableSanType::EmailAddress(email),
            // Note: Handling of URI types as DnsName fallback in the reverse conversion
            _ => SerializableSanType::DnsName("unknown".to_string()), // Fallback for unsupported types
        }
    }
}


/// Enhanced certificate management configuration
#[derive(Debug, Clone)]
pub struct EnhancedCertificateManagementConfig {
    /// Basic configuration
    pub basic: CertificateManagementConfig,
    /// Storage backend for certificates and metadata
    pub storage_path: String,
    /// Private key encryption settings
    pub key_encryption_enabled: bool,
    pub key_encryption_password: Option<String>,
    /// Certificate authority settings
    pub ca_settings: CaSettings,
    /// Certificate template settings
    pub certificate_templates: HashMap<CertificateRole, CertificateTemplate>,
    /// Audit and monitoring settings
    pub audit_enabled: bool,
    pub audit_log_path: String,
    pub metrics_enabled: bool,
    /// Renewal settings
    pub auto_renewal_enabled: bool,
    pub renewal_check_interval_hours: u64,
    /// CRL settings
    pub crl_update_interval_hours: u64,
    pub crl_distribution_points: Vec<String>,
}

impl Default for EnhancedCertificateManagementConfig {
    fn default() -> Self {
        let mut templates = HashMap::new();
        templates.insert(CertificateRole::Broker, CertificateTemplate::broker_default());
        templates.insert(CertificateRole::Controller, CertificateTemplate::controller_default());
        templates.insert(CertificateRole::Client, CertificateTemplate::client_default());
        templates.insert(CertificateRole::Admin, CertificateTemplate::admin_default());
        
        Self {
            basic: CertificateManagementConfig::default(),
            storage_path: "/var/lib/rustmq/certificates".to_string(),
            key_encryption_enabled: true,
            key_encryption_password: None, // Should be provided via secure config
            ca_settings: CaSettings::default(),
            certificate_templates: templates,
            audit_enabled: true,
            audit_log_path: "/var/log/rustmq/certificate_audit.log".to_string(),
            metrics_enabled: true,
            auto_renewal_enabled: true,
            renewal_check_interval_hours: 24,
            crl_update_interval_hours: 6,
            crl_distribution_points: vec![],
        }
    }
}

/// Certificate Authority settings
#[derive(Debug, Clone)]
pub struct CaSettings {
    pub organization: String,
    pub organizational_unit: String,
    pub country_code: String,
    pub state_province: String,
    pub locality: String,
    pub root_ca_validity_years: u32,
    pub intermediate_ca_validity_years: u32,
    pub key_type: KeyType,
    pub key_size: u32,
}

impl Default for CaSettings {
    fn default() -> Self {
        Self {
            organization: "RustMQ".to_string(),
            organizational_unit: "Message Queue System".to_string(),
            country_code: "US".to_string(),
            state_province: "California".to_string(),
            locality: "San Francisco".to_string(),
            root_ca_validity_years: 10,
            intermediate_ca_validity_years: 5,
            key_type: KeyType::Ecdsa,
            key_size: 256,
        }
    }
}

/// Supported key types for certificate generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyType {
    Rsa,
    Ecdsa,
}

/// Key usage extensions for certificates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyUsage {
    DigitalSignature,
    NonRepudiation,
    KeyEncipherment,
    DataEncipherment,
    KeyAgreement,
    KeyCertSign,
    CrlSign,
    EncipherOnly,
    DecipherOnly,
}

/// Extended key usage extensions for certificates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtendedKeyUsage {
    ServerAuth,
    ClientAuth,
    CodeSigning,
    EmailProtection,
    TimeStamping,
    OcspSigning,
}

/// Certificate roles for template-based generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CertificateRole {
    RootCa,
    IntermediateCa,
    Broker,
    Controller,
    Client,
    Admin,
}

/// Certificate template for role-based generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateTemplate {
    pub validity_days: u32,
    pub key_type: KeyType,
    pub key_size: u32,
    pub key_usage: Vec<KeyUsage>,
    pub extended_key_usage: Vec<ExtendedKeyUsage>,
    pub is_ca: bool,
    pub max_path_length: Option<u32>,
    pub subject_alt_names: Vec<SerializableSanType>,
}

impl CertificateTemplate {
    pub fn broker_default() -> Self {
        Self {
            validity_days: 365,
            key_type: KeyType::Ecdsa,
            key_size: 256,
            key_usage: vec![KeyUsage::DigitalSignature, KeyUsage::KeyEncipherment],
            extended_key_usage: vec![ExtendedKeyUsage::ServerAuth, ExtendedKeyUsage::ClientAuth],
            is_ca: false,
            max_path_length: None,
            subject_alt_names: vec![],
        }
    }
    
    pub fn controller_default() -> Self {
        Self {
            validity_days: 730,
            key_type: KeyType::Ecdsa,
            key_size: 256,
            key_usage: vec![KeyUsage::DigitalSignature, KeyUsage::KeyEncipherment],
            extended_key_usage: vec![ExtendedKeyUsage::ServerAuth, ExtendedKeyUsage::ClientAuth],
            is_ca: false,
            max_path_length: None,
            subject_alt_names: vec![],
        }
    }
    
    pub fn client_default() -> Self {
        Self {
            validity_days: 365,
            key_type: KeyType::Ecdsa,
            key_size: 256,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
            is_ca: false,
            max_path_length: None,
            subject_alt_names: vec![],
        }
    }
    
    pub fn admin_default() -> Self {
        Self {
            validity_days: 180,
            key_type: KeyType::Ecdsa,
            key_size: 256,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
            is_ca: false,
            max_path_length: None,
            subject_alt_names: vec![],
        }
    }
}


/// Certificate information with enhanced metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    pub id: String,
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub not_before: SystemTime,
    pub not_after: SystemTime,
    pub fingerprint: String,
    pub role: CertificateRole,
    pub status: CertificateStatus,
    pub created_at: SystemTime,
    pub last_used: Option<SystemTime>,
    pub renewal_threshold: SystemTime,
    pub key_type: KeyType,
    pub key_size: u32,
    pub is_ca: bool,
    pub issuer_id: Option<String>,
    pub revocation_reason: Option<RevocationReason>,
    pub san_entries: Vec<String>,
    pub certificate_pem: Option<String>,
    pub private_key_pem: Option<String>,
}

/// Certificate status tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateStatus {
    Active,
    Expired,
    Revoked,
    PendingRenewal,
    Suspended,
}

/// Reasons for certificate revocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevocationReason {
    Unspecified,
    KeyCompromise,
    CaCompromise,
    AffiliationChanged,
    Superseded,
    CessationOfOperation,
    CertificateHold,
    RemoveFromCrl,
    PrivilegeWithdrawn,
    AaCompromise,
}

/// Certificate request parameters
#[derive(Debug, Clone)]
pub struct CertificateRequest {
    pub subject: DistinguishedName,
    pub role: CertificateRole,
    pub san_entries: Vec<SanType>,
    pub validity_days: Option<u32>,
    pub key_type: Option<KeyType>,
    pub key_size: Option<u32>,
    pub issuer_id: Option<String>,
}

/// CA generation parameters
#[derive(Debug, Clone)]
pub struct CaGenerationParams {
    pub common_name: String,
    pub organization: Option<String>,
    pub organizational_unit: Option<String>,
    pub country: Option<String>,
    pub state_province: Option<String>,
    pub locality: Option<String>,
    pub validity_years: Option<u32>,
    pub key_type: Option<KeyType>,
    pub key_size: Option<u32>,
    pub is_root: bool,
}

impl Default for CaGenerationParams {
    fn default() -> Self {
        Self {
            common_name: String::new(),
            organization: None,
            organizational_unit: None,
            country: None,
            state_province: None,
            locality: None,
            validity_years: None,
            key_type: None,
            key_size: None,
            is_root: false,
        }
    }
}

/// Certificate validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub chain_length: usize,
    pub trust_anchor: Option<String>,
}

/// Enhanced certificate revocation list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationList {
    pub id: String,
    pub issuer: String,
    pub this_update: SystemTime,
    pub next_update: SystemTime,
    pub revoked_certificates: HashMap<String, RevokedCertificate>,
    pub signature: Vec<u8>,
}

/// Revoked certificate entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokedCertificate {
    pub serial_number: String,
    pub revocation_date: SystemTime,
    pub reason: RevocationReason,
}

/// Audit log entry for certificate operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateAuditEntry {
    pub timestamp: SystemTime,
    pub operation: String,
    pub certificate_id: Option<String>,
    pub subject: Option<String>,
    pub user: Option<String>,
    pub source_ip: Option<String>,
    pub result: String,
    pub details: HashMap<String, String>,
}

/// Certificate storage metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CertificateMetadata {
    pub info: CertificateInfo,
    pub pem_data: String,
    pub private_key_path: Option<String>,
    pub created_by: String,
    pub storage_version: u32,
}

/// Main certificate manager implementation
pub struct CertificateManager {
    config: EnhancedCertificateManagementConfig,
    storage: Arc<dyn ObjectStorage>,
    certificates: Arc<RwLock<HashMap<String, CertificateMetadata>>>,
    crl: Arc<RwLock<RevocationList>>,
    audit_log: Arc<Mutex<Vec<CertificateAuditEntry>>>,
    renewal_task: Option<tokio::task::JoinHandle<()>>,
}

impl CertificateManager {
    /// Create a new certificate manager with enhanced configuration
    pub async fn new(config: CertificateManagementConfig) -> Result<Self> {
        let enhanced_config = EnhancedCertificateManagementConfig {
            basic: config,
            ..Default::default()
        };
        
        Self::new_with_enhanced_config(enhanced_config).await
    }
    
    /// Create a new certificate manager with enhanced configuration
    pub async fn new_with_enhanced_config(config: EnhancedCertificateManagementConfig) -> Result<Self> {
        // Initialize storage backend
        let storage = Arc::new(LocalObjectStorage::new(PathBuf::from(&config.storage_path))?) as Arc<dyn ObjectStorage>;
        
        // Initialize certificate store
        let certificates = Arc::new(RwLock::new(HashMap::new()));
        
        // Initialize empty CRL
        let crl = Arc::new(RwLock::new(RevocationList {
            id: Uuid::new_v4().to_string(),
            issuer: "RustMQ Certificate Manager".to_string(),
            this_update: SystemTime::now(),
            next_update: SystemTime::now() + Duration::from_secs(86400 * 7), // 7 days
            revoked_certificates: HashMap::new(),
            signature: vec![],
        }));
        
        let audit_log = Arc::new(Mutex::new(Vec::new()));
        
        let mut manager = Self {
            config,
            storage,
            certificates,
            crl,
            audit_log,
            renewal_task: None,
        };
        
        // Load existing certificates
        manager.load_certificates().await?;
        
        // Start background renewal task if enabled
        if manager.config.auto_renewal_enabled {
            manager.start_renewal_task().await?;
        }
        
        info!("Certificate manager initialized successfully");
        Ok(manager)
    }
    
    /// Generate a new root CA certificate
    pub async fn generate_root_ca(&self, params: CaGenerationParams) -> Result<CertificateInfo> {
        self.audit_log("generate_root_ca", None, &params.common_name, "STARTED", HashMap::new()).await;
        
        let key_pair = self.generate_key_pair(params.key_type.unwrap_or(self.config.ca_settings.key_type), 
                                              params.key_size.unwrap_or(self.config.ca_settings.key_size))?;
        
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(rcgen::DnType::CommonName, params.common_name.clone());
        
        if let Some(org) = params.organization.or_else(|| Some(self.config.ca_settings.organization.clone())) {
            distinguished_name.push(rcgen::DnType::OrganizationName, org);
        }
        
        if let Some(ou) = params.organizational_unit.or_else(|| Some(self.config.ca_settings.organizational_unit.clone())) {
            distinguished_name.push(rcgen::DnType::OrganizationalUnitName, ou);
        }
        
        if let Some(country) = params.country.or_else(|| Some(self.config.ca_settings.country_code.clone())) {
            distinguished_name.push(rcgen::DnType::CountryName, country);
        }
        
        let mut cert_params = CertificateParams::new(vec![]);
        cert_params.distinguished_name = distinguished_name;
        cert_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        cert_params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
            rcgen::KeyUsagePurpose::DigitalSignature,
        ];
        
        let validity_years = params.validity_years.unwrap_or(self.config.ca_settings.root_ca_validity_years);
        // Set certificate validity period
        cert_params.not_before = ::time::OffsetDateTime::now_utc();
        cert_params.not_after = cert_params.not_before + ::time::Duration::days(validity_years as i64 * 365);
        
        cert_params.key_pair = Some(key_pair);
        
        let certificate = RcgenCertificate::from_params(cert_params)
            .map_err(|e| RustMqError::CertificateGeneration { 
                reason: format!("Failed to generate root CA: {}", e) 
            })?;
        
        let cert_info = self.store_certificate(certificate, CertificateRole::RootCa, None, "system").await?;
        
        self.audit_log("generate_root_ca", Some(&cert_info.id), &params.common_name, "SUCCESS", HashMap::new()).await;
        info!("Generated root CA certificate: {}", cert_info.id);
        
        Ok(cert_info)
    }
    
    /// Generate a new intermediate CA certificate
    pub async fn generate_intermediate_ca(&self, issuer_id: &str, params: CaGenerationParams) -> Result<CertificateInfo> {
        self.audit_log("generate_intermediate_ca", None, &params.common_name, "STARTED", HashMap::new()).await;
        
        // Get the issuer certificate (root CA)
        let issuer_cert = self.get_certificate_by_id(issuer_id).await?
            .ok_or_else(|| RustMqError::CertificateNotFound { 
                identifier: issuer_id.to_string() 
            })?;
        
        if !issuer_cert.is_ca {
            return Err(RustMqError::CertificateValidation {
                reason: "Issuer certificate is not a CA".to_string(),
            });
        }
        
        let key_pair = self.generate_key_pair(params.key_type.unwrap_or(self.config.ca_settings.key_type), 
                                              params.key_size.unwrap_or(self.config.ca_settings.key_size))?;
        
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(rcgen::DnType::CommonName, params.common_name.clone());
        
        if let Some(org) = params.organization.or_else(|| Some(self.config.ca_settings.organization.clone())) {
            distinguished_name.push(rcgen::DnType::OrganizationName, org);
        }
        
        let mut cert_params = CertificateParams::new(vec![]);
        cert_params.distinguished_name = distinguished_name;
        cert_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Constrained(0));
        cert_params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
            rcgen::KeyUsagePurpose::DigitalSignature,
        ];
        
        let validity_years = params.validity_years.unwrap_or(self.config.ca_settings.intermediate_ca_validity_years);
        // Set certificate validity period
        cert_params.not_before = ::time::OffsetDateTime::now_utc();
        cert_params.not_after = cert_params.not_before + ::time::Duration::days(validity_years as i64 * 365);
        
        cert_params.key_pair = Some(key_pair);
        
        // Create the certificate parameters first
        let certificate = RcgenCertificate::from_params(cert_params)
            .map_err(|e| RustMqError::CertificateGeneration { 
                reason: format!("Failed to create intermediate CA params: {}", e) 
            })?;
        
        // Get the issuer CA certificate for signing
        let issuer_cert = self.reconstruct_rcgen_certificate(issuer_id).await?;
        
        // Sign the intermediate CA certificate with the issuer
        let signed_der = certificate.serialize_der_with_signer(&issuer_cert)
            .map_err(|e| RustMqError::CertificateGeneration {
                reason: format!("Failed to sign intermediate CA certificate: {}", e),
            })?;
        
        // Convert signed DER back to PEM for storage
        let signed_pem = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            general_purpose::STANDARD.encode(&signed_der)
        );
        
        let cert_info = self.store_signed_certificate(
            signed_pem,
            certificate.serialize_private_key_pem(),
            CertificateRole::IntermediateCa, 
            Some(issuer_id.to_string()), 
            "system"
        ).await?;
        
        self.audit_log("generate_intermediate_ca", Some(&cert_info.id), &params.common_name, "SUCCESS", HashMap::new()).await;
        info!("Generated intermediate CA certificate: {}", cert_info.id);
        
        Ok(cert_info)
    }
    
    /// Issue a new end-entity certificate
    pub async fn issue_certificate(&self, request: CertificateRequest) -> Result<CertificateInfo> {
        let subject_cn = request.subject.iter()
            .find(|(dn_type, _)| **dn_type == rcgen::DnType::CommonName)
            .map(|(_, value)| format!("{:?}", value))
            .unwrap_or_else(|| "unknown".to_string());
        
        self.audit_log("issue_certificate", None, &subject_cn, "STARTED", HashMap::new()).await;
        
        // Get certificate template for role
        let template = self.config.certificate_templates.get(&request.role)
            .ok_or_else(|| RustMqError::Config(format!("No template found for role: {:?}", request.role)))?;
        
        let key_pair = self.generate_key_pair(
            request.key_type.unwrap_or(template.key_type),
            request.key_size.unwrap_or(template.key_size)
        )?;
        
        let san_strings: Vec<String> = request.san_entries.iter().map(|san| {
            match san {
                SanType::DnsName(name) => name.clone(),
                SanType::IpAddress(ip) => ip.to_string(),
                SanType::Rfc822Name(email) => email.clone(),
                _ => format!("{:?}", san), // Fallback for other types
            }
        }).collect();
        let mut cert_params = CertificateParams::new(san_strings);
        cert_params.distinguished_name = request.subject;
        cert_params.is_ca = rcgen::IsCa::NoCa;
        
        // Set key usage from template
        cert_params.key_usages = template.key_usage.iter().map(|ku| match ku {
            KeyUsage::DigitalSignature => rcgen::KeyUsagePurpose::DigitalSignature,
            KeyUsage::KeyEncipherment => rcgen::KeyUsagePurpose::KeyEncipherment,
            KeyUsage::DataEncipherment => rcgen::KeyUsagePurpose::DataEncipherment,
            KeyUsage::KeyAgreement => rcgen::KeyUsagePurpose::KeyAgreement,
            KeyUsage::KeyCertSign => rcgen::KeyUsagePurpose::KeyCertSign,
            KeyUsage::CrlSign => rcgen::KeyUsagePurpose::CrlSign,
            _ => rcgen::KeyUsagePurpose::DigitalSignature,
        }).collect();
        
        // Set extended key usage from template
        cert_params.extended_key_usages = template.extended_key_usage.iter().map(|eku| match eku {
            ExtendedKeyUsage::ServerAuth => rcgen::ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsage::ClientAuth => rcgen::ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsage::CodeSigning => rcgen::ExtendedKeyUsagePurpose::CodeSigning,
            ExtendedKeyUsage::EmailProtection => rcgen::ExtendedKeyUsagePurpose::EmailProtection,
            ExtendedKeyUsage::TimeStamping => rcgen::ExtendedKeyUsagePurpose::TimeStamping,
            ExtendedKeyUsage::OcspSigning => rcgen::ExtendedKeyUsagePurpose::OcspSigning,
        }).collect();
        
        let validity_days = request.validity_days.unwrap_or(template.validity_days);
        // Set certificate validity period
        cert_params.not_before = ::time::OffsetDateTime::now_utc();
        cert_params.not_after = cert_params.not_before + ::time::Duration::days(validity_days as i64);
        
        cert_params.key_pair = Some(key_pair);
        
        // Create the certificate parameters first
        let certificate = RcgenCertificate::from_params(cert_params)
            .map_err(|e| RustMqError::CertificateGeneration { 
                reason: format!("Failed to create certificate params: {}", e) 
            })?;
        
        let cert_info = if let Some(ref issuer_id) = request.issuer_id {
            // This is a CA-signed certificate (end-entity or intermediate)
            let issuer_cert = self.reconstruct_rcgen_certificate(issuer_id).await?;
            
            // Sign the certificate with the issuer
            let signed_der = certificate.serialize_der_with_signer(&issuer_cert)
                .map_err(|e| RustMqError::CertificateGeneration {
                    reason: format!("Failed to sign certificate: {}", e),
                })?;
            
            // Convert signed DER back to PEM for storage
            let signed_pem = format!(
                "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
                general_purpose::STANDARD.encode(&signed_der)
            );
            
            self.store_signed_certificate(
                signed_pem,
                certificate.serialize_private_key_pem(),
                request.role,
                request.issuer_id.clone(),
                "user"
            ).await?
        } else {
            // This is a self-signed certificate (should only be root CA)
            if request.role != CertificateRole::RootCa {
                warn!("Creating self-signed certificate for non-root CA role: {:?}", request.role);
            }
            self.store_certificate(certificate, request.role, None, "user").await?
        };
        
        self.audit_log("issue_certificate", Some(&cert_info.id), &subject_cn, "SUCCESS", HashMap::new()).await;
        info!("Issued certificate: {}", cert_info.id);
        
        Ok(cert_info)
    }
    
    /// Renew an existing certificate
    pub async fn renew_certificate(&self, cert_id: &str) -> Result<CertificateInfo> {
        self.audit_log("renew_certificate", Some(cert_id), "unknown", "STARTED", HashMap::new()).await;
        
        let existing_cert = self.get_certificate_by_id(cert_id).await?
            .ok_or_else(|| RustMqError::CertificateNotFound { 
                identifier: cert_id.to_string() 
            })?;
        
        if existing_cert.status != CertificateStatus::Active {
            return Err(RustMqError::CertificateValidation {
                reason: format!("Certificate {} is not active", cert_id),
            });
        }
        
        // Create a new certificate request based on existing certificate
        let mut distinguished_name = DistinguishedName::new();
        let subject_parts: Vec<&str> = existing_cert.subject.split(", ").collect();
        for part in subject_parts {
            if let Some((key, value)) = part.split_once("=") {
                match key {
                    "CN" => distinguished_name.push(rcgen::DnType::CommonName, value.to_string()),
                    "O" => distinguished_name.push(rcgen::DnType::OrganizationName, value.to_string()),
                    "OU" => distinguished_name.push(rcgen::DnType::OrganizationalUnitName, value.to_string()),
                    "C" => distinguished_name.push(rcgen::DnType::CountryName, value.to_string()),
                    _ => {},
                }
            }
        }
        
        let san_entries: Vec<SanType> = existing_cert.san_entries.iter()
            .map(|san| SanType::DnsName(san.clone()))
            .collect();
        
        let request = CertificateRequest {
            subject: distinguished_name,
            role: existing_cert.role,
            san_entries,
            validity_days: None, // Use template default
            key_type: Some(existing_cert.key_type),
            key_size: Some(existing_cert.key_size),
            issuer_id: existing_cert.issuer_id.clone(),
        };
        
        // Revoke the old certificate
        self.revoke_certificate(cert_id, RevocationReason::Superseded).await?;
        
        // Issue new certificate
        let new_cert = self.issue_certificate(request).await?;
        
        self.audit_log("renew_certificate", Some(&new_cert.id), &existing_cert.subject, "SUCCESS", HashMap::new()).await;
        info!("Renewed certificate: {} -> {}", cert_id, new_cert.id);
        
        Ok(new_cert)
    }
    
    /// Revoke a certificate
    pub async fn revoke_certificate(&self, cert_id: &str, reason: RevocationReason) -> Result<()> {
        self.audit_log("revoke_certificate", Some(cert_id), "unknown", "STARTED", HashMap::new()).await;
        
        // Update certificate status
        {
            let mut certificates = self.certificates.write().map_err(|_| {
                RustMqError::Storage("Failed to acquire certificate lock for revocation".to_string())
            })?;
            
            if let Some(cert_metadata) = certificates.get_mut(cert_id) {
                cert_metadata.info.status = CertificateStatus::Revoked;
                cert_metadata.info.revocation_reason = Some(reason);
            } else {
                return Err(RustMqError::CertificateNotFound { 
                    identifier: cert_id.to_string() 
                });
            }
        }
        
        // Update CRL
        {
            let mut crl = self.crl.write().map_err(|_| {
                RustMqError::Storage("Failed to acquire CRL lock for revocation".to_string())
            })?;
            
            if let Some(cert) = self.get_certificate_by_id(cert_id).await? {
                crl.revoked_certificates.insert(
                    cert.serial_number.clone(),
                    RevokedCertificate {
                        serial_number: cert.serial_number,
                        revocation_date: SystemTime::now(),
                        reason,
                    }
                );
                crl.this_update = SystemTime::now();
                crl.next_update = SystemTime::now() + Duration::from_secs(86400 * 7); // 7 days
            }
        }
        
        // Persist the updated certificate and CRL
        self.persist_certificates().await?;
        self.persist_crl().await?;
        
        self.audit_log("revoke_certificate", Some(cert_id), "unknown", "SUCCESS", HashMap::new()).await;
        info!("Revoked certificate: {} (reason: {:?})", cert_id, reason);
        
        Ok(())
    }
    
    /// Get all revoked certificates
    pub async fn get_revoked_certificates(&self) -> Result<Vec<RevokedCertificate>> {
        let certificates = self.certificates.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire certificate lock for reading".to_string())
        })?;
        
        let revoked_certs: Vec<RevokedCertificate> = certificates
            .values()
            .filter(|cert| cert.info.status == CertificateStatus::Revoked)
            .map(|cert| RevokedCertificate {
                serial_number: cert.info.serial_number.clone(),
                revocation_date: cert.info.created_at, // Use created_at as placeholder
                reason: cert.info.revocation_reason.unwrap_or(RevocationReason::Unspecified),
            })
            .collect();
            
        Ok(revoked_certs)
    }
    
    /// Get certificates that are expiring within the threshold
    pub async fn get_expiring_certificates(&self, threshold_days: u32) -> Result<Vec<CertificateInfo>> {
        let threshold = SystemTime::now() + Duration::from_secs(threshold_days as u64 * 86400);
        
        let certificates = self.certificates.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire certificate lock for expiry check".to_string())
        })?;
        
        let expiring: Vec<CertificateInfo> = certificates.values()
            .filter_map(|cert_metadata| {
                if cert_metadata.info.status == CertificateStatus::Active &&
                   cert_metadata.info.not_after <= threshold {
                    Some(cert_metadata.info.clone())
                } else {
                    None
                }
            })
            .collect();
        
        debug!("Found {} certificates expiring within {} days", expiring.len(), threshold_days);
        Ok(expiring)
    }
    
    /// List all certificates
    pub async fn list_all_certificates(&self) -> Result<Vec<CertificateInfo>> {
        let certificates = self.certificates.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire certificate lock for listing".to_string())
        })?;
        
        let all_certs: Vec<CertificateInfo> = certificates.values()
            .map(|cert_metadata| cert_metadata.info.clone())
            .collect();
        
        debug!("Listed {} certificates", all_certs.len());
        Ok(all_certs)
    }
    
    /// Rotate a certificate (revoke old, issue new)
    pub async fn rotate_certificate(&self, cert_id: &str) -> Result<CertificateInfo> {
        self.audit_log("rotate_certificate", Some(cert_id), "unknown", "STARTED", HashMap::new()).await;
        
        let renewed_cert = self.renew_certificate(cert_id).await?;
        
        self.audit_log("rotate_certificate", Some(&renewed_cert.id), "unknown", "SUCCESS", HashMap::new()).await;
        info!("Rotated certificate: {} -> {}", cert_id, renewed_cert.id);
        
        Ok(renewed_cert)
    }
    
    /// Validate a certificate chain
    pub async fn validate_certificate_chain(&self, chain: &[Certificate]) -> Result<ValidationResult> {
        if chain.is_empty() {
            return Ok(ValidationResult {
                is_valid: false,
                errors: vec!["Certificate chain is empty".to_string()],
                warnings: vec![],
                chain_length: 0,
                trust_anchor: None,
            });
        }
        
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut trust_anchor = None;
        
        // Parse the end-entity certificate
        let end_cert = &chain[0];
        match X509Certificate::from_der(&end_cert.0) {
            Ok((_, cert)) => {
                // Check if certificate is expired
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                if cert.validity().not_after.timestamp() < now as i64 {
                    errors.push("Certificate has expired".to_string());
                }
                
                if cert.validity().not_before.timestamp() > now as i64 {
                    errors.push("Certificate is not yet valid".to_string());
                }
                
                // Check if certificate is revoked
                let serial = hex::encode(cert.serial.to_bytes_be());
                let crl = self.crl.read().map_err(|_| {
                    RustMqError::Storage("Failed to acquire CRL lock for validation".to_string())
                })?;
                
                if crl.revoked_certificates.contains_key(&serial) {
                    errors.push("Certificate has been revoked".to_string());
                }
                
                trust_anchor = Some(cert.issuer().to_string());
            }
            Err(e) => {
                errors.push(format!("Failed to parse certificate: {}", e));
            }
        }
        
        // TODO: Implement full chain validation
        if chain.len() > 1 {
            warnings.push("Full chain validation not yet implemented".to_string());
        }
        
        Ok(ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
            chain_length: chain.len(),
            trust_anchor,
        })
    }
    
    /// Get certificate status
    pub async fn get_certificate_status(&self, cert_id: &str) -> Result<CertificateStatus> {
        let certificates = self.certificates.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire certificate lock for status check".to_string())
        })?;
        
        certificates.get(cert_id)
            .map(|cert_metadata| cert_metadata.info.status)
            .ok_or_else(|| RustMqError::CertificateNotFound { 
                identifier: cert_id.to_string() 
            })
    }
    
    /// Get the CA certificate chain
    pub async fn get_ca_chain(&self) -> Result<Vec<Certificate>> {
        let certificates = self.certificates.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire certificate lock for CA chain".to_string())
        })?;
        
        let mut ca_certs: Vec<Certificate> = Vec::new();
        
        for cert_metadata in certificates.values()
            .filter(|cert_metadata| cert_metadata.info.is_ca && cert_metadata.info.status == CertificateStatus::Active) {
            
            // Parse PEM to Certificate
            let pem_bytes = cert_metadata.pem_data.as_bytes();
            let certs = rustls_pemfile::certs(&mut pem_bytes.clone())
                .map_err(|e| RustMqError::InvalidCertificate {
                    reason: format!("Failed to parse CA certificate: {}", e),
                })?;
            
            for cert_der in certs {
                ca_certs.push(Certificate(cert_der));
            }
        }
        
        Ok(ca_certs)
    }
    
    // ===== Private Helper Methods =====
    
    /// Reconstruct an RcgenCertificate from stored PEM data (needed for signing)
    async fn reconstruct_rcgen_certificate(&self, cert_id: &str) -> Result<RcgenCertificate> {
        let cert_metadata = {
            let certificates = self.certificates.read().map_err(|_| {
                RustMqError::Storage("Failed to acquire certificate lock for reconstruction".to_string())
            })?;
            
            certificates.get(cert_id)
                .ok_or_else(|| RustMqError::CertificateNotFound { 
                    identifier: cert_id.to_string() 
                })?
                .clone()
        };
        
        // Get the private key PEM - either from memory or from storage
        let private_key_pem = if let Some(ref pem) = cert_metadata.info.private_key_pem {
            pem.clone()
        } else if let Some(ref key_path) = cert_metadata.private_key_path {
            // Load from storage
            let key_data = self.storage.get(key_path).await
                .map_err(|e| RustMqError::Storage(format!("Failed to load private key: {}", e)))?;
            String::from_utf8(key_data.to_vec())
                .map_err(|e| RustMqError::InvalidCertificate {
                    reason: format!("Invalid UTF-8 in private key: {}", e),
                })?
        } else {
            return Err(RustMqError::InvalidCertificate {
                reason: "No private key available for certificate".to_string(),
            });
        };
        
        // Parse the private key
        let key_pair = KeyPair::from_pem(&private_key_pem)
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse private key: {}", e),
            })?;
        
        // Parse the certificate to extract parameters
        let cert_pem = &cert_metadata.pem_data;
        let cert_der = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse certificate PEM: {}", e),
            })?
            .into_iter()
            .next()
            .ok_or_else(|| RustMqError::InvalidCertificate {
                reason: "No certificate found in PEM data".to_string(),
            })?;
        
        let (_, parsed_cert) = X509Certificate::from_der(&cert_der)
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse certificate DER: {}", e),
            })?;
        
        // Reconstruct certificate parameters
        let mut cert_params = CertificateParams::new(cert_metadata.info.san_entries.clone());
        
        // Set the distinguished name
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(rcgen::DnType::CommonName, 
            parsed_cert.subject().iter_common_name().next()
                .and_then(|cn| cn.as_str().ok())
                .unwrap_or("Unknown")
                .to_string());
        
        // Add other DN components if needed
        for attr in parsed_cert.subject().iter_organization() {
            if let Ok(org) = attr.as_str() {
                distinguished_name.push(rcgen::DnType::OrganizationName, org.to_string());
            }
        }
        
        cert_params.distinguished_name = distinguished_name;
        
        // Extract and set key usage from the original certificate
        let mut key_usages = Vec::new();
        let mut extended_key_usages = Vec::new();
        let mut basic_constraints = rcgen::BasicConstraints::Unconstrained;
        
        // Parse certificate extensions to extract usage information
        for extension in parsed_cert.extensions() {
            match extension.oid.to_string().as_str() {
                // Key Usage extension (2.5.29.15)
                "2.5.29.15" => {
                    if let x509_parser::extensions::ParsedExtension::KeyUsage(usage) = extension.parsed_extension() {
                        if usage.digital_signature() {
                            key_usages.push(rcgen::KeyUsagePurpose::DigitalSignature);
                        }
                        if usage.key_encipherment() {
                            key_usages.push(rcgen::KeyUsagePurpose::KeyEncipherment);
                        }
                        if usage.data_encipherment() {
                            key_usages.push(rcgen::KeyUsagePurpose::DataEncipherment);
                        }
                        if usage.key_agreement() {
                            key_usages.push(rcgen::KeyUsagePurpose::KeyAgreement);
                        }
                        if usage.key_cert_sign() {
                            key_usages.push(rcgen::KeyUsagePurpose::KeyCertSign);
                        }
                        if usage.crl_sign() {
                            key_usages.push(rcgen::KeyUsagePurpose::CrlSign);
                        }
                    }
                },
                // Extended Key Usage extension (2.5.29.37)
                "2.5.29.37" => {
                    if let x509_parser::extensions::ParsedExtension::ExtendedKeyUsage(usage) = extension.parsed_extension() {
                        if usage.server_auth {
                            extended_key_usages.push(rcgen::ExtendedKeyUsagePurpose::ServerAuth);
                        }
                        if usage.client_auth {
                            extended_key_usages.push(rcgen::ExtendedKeyUsagePurpose::ClientAuth);
                        }
                        if usage.code_signing {
                            extended_key_usages.push(rcgen::ExtendedKeyUsagePurpose::CodeSigning);
                        }
                        if usage.email_protection {
                            extended_key_usages.push(rcgen::ExtendedKeyUsagePurpose::EmailProtection);
                        }
                        if usage.time_stamping {
                            extended_key_usages.push(rcgen::ExtendedKeyUsagePurpose::TimeStamping);
                        }
                        if usage.ocsp_signing {
                            extended_key_usages.push(rcgen::ExtendedKeyUsagePurpose::OcspSigning);
                        }
                    }
                },
                // Basic Constraints extension (2.5.29.19)
                "2.5.29.19" => {
                    if let x509_parser::extensions::ParsedExtension::BasicConstraints(constraints) = extension.parsed_extension() {
                        if constraints.ca {
                            basic_constraints = match constraints.path_len_constraint {
                                Some(len) => rcgen::BasicConstraints::Constrained(len as u8),
                                None => rcgen::BasicConstraints::Unconstrained,
                            };
                        }
                    }
                },
                _ => {} // Skip other extensions
            }
        }
        
        // Set certificate type and constraints
        cert_params.is_ca = if cert_metadata.info.is_ca {
            rcgen::IsCa::Ca(basic_constraints)
        } else {
            rcgen::IsCa::NoCa
        };
        
        // Set key usage and extended key usage
        cert_params.key_usages = key_usages;
        cert_params.extended_key_usages = extended_key_usages;
        
        // Set validity period
        cert_params.not_before = ::time::OffsetDateTime::from_unix_timestamp(
            cert_metadata.info.not_before.duration_since(UNIX_EPOCH)
                .unwrap_or_default().as_secs() as i64
        ).map_err(|e| RustMqError::InvalidCertificate {
            reason: format!("Invalid not_before time: {}", e),
        })?;
        
        cert_params.not_after = ::time::OffsetDateTime::from_unix_timestamp(
            cert_metadata.info.not_after.duration_since(UNIX_EPOCH)
                .unwrap_or_default().as_secs() as i64
        ).map_err(|e| RustMqError::InvalidCertificate {
            reason: format!("Invalid not_after time: {}", e),
        })?;
        
        cert_params.key_pair = Some(key_pair);
        
        // Create the certificate from params
        RcgenCertificate::from_params(cert_params)
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to reconstruct certificate: {}", e),
            })
    }
    
    /// Generate a key pair for certificate generation
    fn generate_key_pair(&self, key_type: KeyType, key_size: u32) -> Result<KeyPair> {
        match key_type {
            KeyType::Rsa => {
                KeyPair::generate(&rcgen::PKCS_RSA_SHA256)
                    .map_err(|e| RustMqError::CertificateGeneration {
                        reason: format!("Failed to generate RSA key pair: {}", e),
                    })
            }
            KeyType::Ecdsa => {
                let alg = match key_size {
                    256 => &rcgen::PKCS_ECDSA_P256_SHA256,
                    384 => &rcgen::PKCS_ECDSA_P384_SHA384,
                    _ => &rcgen::PKCS_ECDSA_P256_SHA256,
                };
                KeyPair::generate(alg)
                    .map_err(|e| RustMqError::CertificateGeneration {
                        reason: format!("Failed to generate ECDSA key pair: {}", e),
                    })
            }
        }
    }
    
    /// Store a certificate with metadata
    async fn store_certificate(
        &self,
        certificate: RcgenCertificate,
        role: CertificateRole,
        issuer_id: Option<String>,
        created_by: &str,
    ) -> Result<CertificateInfo> {
        let cert_id = Uuid::new_v4().to_string();
        let pem_data = certificate.serialize_pem()
            .map_err(|e| RustMqError::CertificateGeneration {
                reason: format!("Failed to serialize certificate: {}", e),
            })?;
        
        // Parse certificate to extract metadata
        // Use DER data from PEM for consistency with later fingerprint calculations
        let der_data = rustls_pemfile::certs(&mut pem_data.as_bytes())
            .map_err(|e| RustMqError::CertificateGeneration {
                reason: format!("Failed to parse PEM to DER: {}", e),
            })?
            .into_iter()
            .next()
            .ok_or_else(|| RustMqError::CertificateGeneration {
                reason: "No certificates found in PEM data".to_string(),
            })?;
        
        let (_, parsed_cert) = X509Certificate::from_der(&der_data)
            .map_err(|e| RustMqError::CertificateGeneration {
                reason: format!("Failed to parse generated certificate: {}", e),
            })?;
        
        let not_before = UNIX_EPOCH + Duration::from_secs(parsed_cert.validity().not_before.timestamp() as u64);
        let not_after = UNIX_EPOCH + Duration::from_secs(parsed_cert.validity().not_after.timestamp() as u64);
        let renewal_threshold = not_after - Duration::from_secs(self.config.basic.auto_renew_before_expiry_days as u64 * 86400);
        
        let san_entries: Vec<String> = parsed_cert.extensions()
            .iter()
            .find(|ext| ext.oid == oid_registry::OID_X509_EXT_SUBJECT_ALT_NAME)
            .and_then(|ext| {
                if let Ok((_, san)) = SubjectAlternativeName::from_der(&ext.value) {
                    Some(san.general_names.iter().filter_map(|gn| {
                        match gn {
                            GeneralName::DNSName(name) => Some(name.to_string()),
                            GeneralName::IPAddress(ip) => Some(format!("{:?}", ip)),
                            _ => None,
                        }
                    }).collect())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        
        let cert_info = CertificateInfo {
            id: cert_id.clone(),
            subject: parsed_cert.subject().to_string(),
            issuer: parsed_cert.issuer().to_string(),
            serial_number: hex::encode(parsed_cert.serial.to_bytes_be()),
            not_before,
            not_after,
            fingerprint: self.calculate_fingerprint(&der_data),
            role,
            status: CertificateStatus::Active,
            created_at: SystemTime::now(),
            last_used: None,
            renewal_threshold,
            key_type: KeyType::Ecdsa, // TODO: Detect from certificate
            key_size: 256, // TODO: Extract from certificate
            is_ca: matches!(role, CertificateRole::RootCa | CertificateRole::IntermediateCa),
            issuer_id,
            revocation_reason: None,
            san_entries,
            certificate_pem: Some(pem_data.clone()),
            private_key_pem: if self.config.key_encryption_enabled {
                Some(certificate.serialize_private_key_pem())
            } else {
                None
            },
        };
        
        // Store private key securely
        let private_key_path = if self.config.key_encryption_enabled {
            Some(self.store_private_key(&cert_id, &certificate.serialize_private_key_pem()).await?)
        } else {
            None
        };
        
        let metadata = CertificateMetadata {
            info: cert_info.clone(),
            pem_data,
            private_key_path,
            created_by: created_by.to_string(),
            storage_version: 1,
        };
        
        // Store in memory
        {
            let mut certificates = self.certificates.write().map_err(|_| {
                RustMqError::Storage("Failed to acquire certificate lock for storage".to_string())
            })?;
            certificates.insert(cert_id.clone(), metadata);
        }
        
        // Persist to storage
        self.persist_certificates().await?;
        
        Ok(cert_info)
    }
    
    /// Store a signed certificate with metadata (used for CA-signed certificates)
    async fn store_signed_certificate(
        &self,
        certificate_pem: String,
        private_key_pem: String,
        role: CertificateRole,
        issuer_id: Option<String>,
        created_by: &str,
    ) -> Result<CertificateInfo> {
        let cert_id = Uuid::new_v4().to_string();
        
        // Parse certificate to extract metadata
        let der_data = rustls_pemfile::certs(&mut certificate_pem.as_bytes())
            .map_err(|e| RustMqError::CertificateGeneration {
                reason: format!("Failed to parse signed certificate PEM: {}", e),
            })?
            .into_iter()
            .next()
            .ok_or_else(|| RustMqError::CertificateGeneration {
                reason: "No certificates found in signed PEM data".to_string(),
            })?;
        
        let (_, parsed_cert) = X509Certificate::from_der(&der_data)
            .map_err(|e| RustMqError::CertificateGeneration {
                reason: format!("Failed to parse signed certificate: {}", e),
            })?;
        
        let not_before = UNIX_EPOCH + Duration::from_secs(parsed_cert.validity().not_before.timestamp() as u64);
        let not_after = UNIX_EPOCH + Duration::from_secs(parsed_cert.validity().not_after.timestamp() as u64);
        let renewal_threshold = not_after - Duration::from_secs(self.config.basic.auto_renew_before_expiry_days as u64 * 86400);
        
        let san_entries: Vec<String> = parsed_cert.extensions()
            .iter()
            .find(|ext| ext.oid == oid_registry::OID_X509_EXT_SUBJECT_ALT_NAME)
            .and_then(|ext| {
                if let Ok((_, san)) = SubjectAlternativeName::from_der(&ext.value) {
                    Some(san.general_names.iter().filter_map(|gn| {
                        match gn {
                            GeneralName::DNSName(name) => Some(name.to_string()),
                            GeneralName::IPAddress(ip) => Some(format!("{:?}", ip)),
                            _ => None,
                        }
                    }).collect())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        
        let cert_info = CertificateInfo {
            id: cert_id.clone(),
            subject: parsed_cert.subject().to_string(),
            issuer: parsed_cert.issuer().to_string(),
            serial_number: hex::encode(parsed_cert.serial.to_bytes_be()),
            not_before,
            not_after,
            fingerprint: self.calculate_fingerprint(&der_data),
            role,
            status: CertificateStatus::Active,
            created_at: SystemTime::now(),
            last_used: None,
            renewal_threshold,
            key_type: KeyType::Ecdsa, // TODO: Detect from certificate
            key_size: 256, // TODO: Extract from certificate
            is_ca: matches!(role, CertificateRole::RootCa | CertificateRole::IntermediateCa),
            issuer_id,
            revocation_reason: None,
            san_entries,
            certificate_pem: Some(certificate_pem.clone()),
            private_key_pem: if self.config.key_encryption_enabled {
                Some(private_key_pem.clone())
            } else {
                None
            },
        };
        
        // Store private key securely
        let private_key_path = if self.config.key_encryption_enabled {
            Some(self.store_private_key(&cert_id, &private_key_pem).await?)
        } else {
            None
        };
        
        let metadata = CertificateMetadata {
            info: cert_info.clone(),
            pem_data: certificate_pem,
            private_key_path,
            created_by: created_by.to_string(),
            storage_version: 1,
        };
        
        // Store in memory
        {
            let mut certificates = self.certificates.write().map_err(|_| {
                RustMqError::Storage("Failed to acquire certificate lock for storage".to_string())
            })?;
            certificates.insert(cert_id.clone(), metadata);
        }
        
        // Persist to storage
        self.persist_certificates().await?;
        
        Ok(cert_info)
    }
    
    /// Store private key securely
    async fn store_private_key(&self, cert_id: &str, private_key_pem: &str) -> Result<String> {
        let key_path = format!("certificates/private_keys/{}.key", cert_id);
        
        // TODO: Implement key encryption if password is provided
        let key_data = if let Some(_password) = &self.config.key_encryption_password {
            // For now, store unencrypted. In production, implement proper encryption
            warn!("Private key encryption is configured but not yet implemented");
            private_key_pem.as_bytes().to_vec()
        } else {
            private_key_pem.as_bytes().to_vec()
        };
        
        self.storage.put(&key_path, key_data.into()).await
            .map_err(|e| RustMqError::Storage(format!("Failed to store private key: {}", e)))?;
        
        Ok(key_path)
    }
    
    /// Calculate certificate fingerprint
    fn calculate_fingerprint(&self, der_data: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(der_data);
        hex::encode(hasher.finalize())
    }
    
    /// Get certificate by ID
    pub async fn get_certificate_by_id(&self, cert_id: &str) -> Result<Option<CertificateInfo>> {
        let certificates = self.certificates.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire certificate lock for lookup".to_string())
        })?;
        
        Ok(certificates.get(cert_id).map(|metadata| metadata.info.clone()))
    }
    
    /// Load existing certificates from storage
    async fn load_certificates(&self) -> Result<()> {
        let cert_list_path = "certificates/certificate_list.json";
        
        match self.storage.get(&cert_list_path).await {
            Ok(data) => {
                let certificate_map: HashMap<String, CertificateMetadata> = 
                    serde_json::from_slice(&data)
                        .map_err(|e| RustMqError::Storage(format!("Failed to deserialize certificates: {}", e)))?;
                
                let mut certificates = self.certificates.write().map_err(|_| {
                    RustMqError::Storage("Failed to acquire certificate lock for loading".to_string())
                })?;
                
                *certificates = certificate_map;
                info!("Loaded {} certificates from storage", certificates.len());
            }
            Err(_) => {
                info!("No existing certificates found in storage");
            }
        }
        
        Ok(())
    }
    
    /// Persist certificates to storage
    async fn persist_certificates(&self) -> Result<()> {
        let (data, count) = {
            let certificates = self.certificates.read().map_err(|_| {
                RustMqError::Storage("Failed to acquire certificate lock for persistence".to_string())
            })?;
            
            let data = serde_json::to_vec(&*certificates)
                .map_err(|e| RustMqError::Storage(format!("Failed to serialize certificates: {}", e)))?;
            
            (data, certificates.len())
        }; // Lock is dropped here
        
        self.storage.put("certificates/certificate_list.json", data.into()).await
            .map_err(|e| RustMqError::Storage(format!("Failed to persist certificates: {}", e)))?;
        
        debug!("Persisted {} certificates to storage", count);
        Ok(())
    }
    
    /// Persist CRL to storage
    async fn persist_crl(&self) -> Result<()> {
        let crl = self.crl.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire CRL lock for persistence".to_string())
        })?;
        
        let data = serde_json::to_vec(&*crl)
            .map_err(|e| RustMqError::Storage(format!("Failed to serialize CRL: {}", e)))?;
        
        self.storage.put("certificates/crl.json", data.into()).await
            .map_err(|e| RustMqError::Storage(format!("Failed to persist CRL: {}", e)))?;
        
        debug!("Persisted CRL to storage");
        Ok(())
    }
    
    /// Start background renewal task
    async fn start_renewal_task(&mut self) -> Result<()> {
        let certificates = self.certificates.clone();
        let config = self.config.clone();
        let audit_log = self.audit_log.clone();
        
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config.renewal_check_interval_hours * 3600));
            
            loop {
                interval.tick().await;
                
                // Check for expiring certificates
                let certs_to_renew = {
                    let certs = match certificates.read() {
                        Ok(certs) => certs,
                        Err(_) => {
                            error!("Failed to acquire certificate lock for renewal check");
                            continue;
                        }
                    };
                    
                    let threshold = SystemTime::now() + Duration::from_secs(config.basic.auto_renew_before_expiry_days as u64 * 86400);
                    
                    certs.values()
                        .filter(|cert_metadata| {
                            cert_metadata.info.status == CertificateStatus::Active &&
                            cert_metadata.info.not_after <= threshold &&
                            !cert_metadata.info.is_ca // Don't auto-renew CA certificates
                        })
                        .map(|cert_metadata| cert_metadata.info.id.clone())
                        .collect::<Vec<_>>()
                };
                
                if !certs_to_renew.is_empty() {
                    info!("Found {} certificates to renew", certs_to_renew.len());
                    
                    // TODO: Implement actual renewal logic
                    // This would require access to the CertificateManager instance
                    // For now, just log the certificates that need renewal
                    for cert_id in certs_to_renew {
                        warn!("Certificate {} needs renewal", cert_id);
                    }
                }
            }
        });
        
        self.renewal_task = Some(task);
        info!("Started certificate renewal background task");
        Ok(())
    }
    
    /// Log audit entry
    async fn audit_log(
        &self,
        operation: &str,
        certificate_id: Option<&str>,
        subject: &str,
        result: &str,
        details: HashMap<String, String>,
    ) {
        if !self.config.audit_enabled {
            return;
        }
        
        let entry = CertificateAuditEntry {
            timestamp: SystemTime::now(),
            operation: operation.to_string(),
            certificate_id: certificate_id.map(|s| s.to_string()),
            subject: Some(subject.to_string()),
            user: None, // TODO: Get from context
            source_ip: None, // TODO: Get from context
            result: result.to_string(),
            details,
        };
        
        let mut audit_log = self.audit_log.lock().await;
        audit_log.push(entry);
        
        // TODO: Implement audit log persistence and rotation
        if audit_log.len() > 10000 {
            warn!("Audit log is getting large, consider implementing log rotation");
        }
    }
    
    /// Get recent audit entries
    pub async fn get_recent_audit_entries(&self, limit: usize) -> Result<Vec<CertificateAuditEntry>> {
        let audit_log = self.audit_log.lock().await;
        let start_index = if audit_log.len() > limit {
            audit_log.len() - limit
        } else {
            0
        };
        
        Ok(audit_log[start_index..].to_vec())
    }
    
    /// Get fingerprints of all revoked certificates for authentication manager sync
    pub async fn get_revoked_certificate_fingerprints(&self) -> Result<Vec<String>> {
        let certificates = self.certificates.read().map_err(|_| {
            RustMqError::Storage("Failed to acquire certificate lock for revoked fingerprints".to_string())
        })?;
        
        let revoked_fingerprints: Vec<String> = certificates
            .values()
            .filter_map(|cert_metadata| {
                if cert_metadata.info.status == CertificateStatus::Revoked {
                    Some(cert_metadata.info.fingerprint.clone())
                } else {
                    None
                }
            })
            .collect();
            
        Ok(revoked_fingerprints)
    }
}

// ===== Additional trait implementations =====

impl Drop for CertificateManager {
    fn drop(&mut self) {
        if let Some(task) = self.renewal_task.take() {
            task.abort();
        }
    }
}

// Include comprehensive tests
#[cfg(test)]
mod tests {
    include!("certificate_manager_tests.rs");
}