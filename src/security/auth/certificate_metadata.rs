//! Enhanced Certificate Metadata for Advanced WebPKI Implementation
//!
//! This module provides comprehensive certificate information extraction
//! using modern ASN.1 parsing libraries for enhanced security validation.

use crate::error::RustMqError;
use chrono::{DateTime, Utc};
use der::{Decode, Encode};
use spki::SubjectPublicKeyInfo;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use x509_cert::{
    Certificate as X509Certificate,
    ext::{Extension, Extensions},
    name::Name,
};

/// Enhanced certificate metadata with comprehensive information extraction
#[derive(Debug, Clone)]
pub struct CertificateMetadata {
    /// Certificate serial number
    pub serial_number: Vec<u8>,
    /// Subject Distinguished Name
    pub subject_dn: DistinguishedName,
    /// Issuer Distinguished Name  
    pub issuer_dn: DistinguishedName,
    /// Subject Alternative Names
    pub subject_alt_names: Vec<SubjectAltName>,
    /// Key usage extensions
    pub key_usage: Option<KeyUsage>,
    /// Extended key usage extensions
    pub extended_key_usage: Option<ExtendedKeyUsage>,
    /// Certificate role/purpose
    pub certificate_role: CertificateRole,
    /// Validity period information
    pub validity_period: ValidityPeriod,
    /// Public key information
    pub public_key_info: PublicKeyInfo,
    /// Certificate fingerprint (SHA256)
    pub fingerprint: String,
    /// Certificate policies
    pub certificate_policies: Vec<String>,
    /// Authority Key Identifier
    pub authority_key_id: Option<Vec<u8>>,
    /// Subject Key Identifier
    pub subject_key_id: Option<Vec<u8>>,
    /// Certificate parsing timestamp
    pub parsed_at: DateTime<Utc>,
}

/// Enhanced Distinguished Name representation
#[derive(Debug, Clone)]
pub struct DistinguishedName {
    /// Common Name (CN)
    pub common_name: Option<String>,
    /// Organization (O)
    pub organization: Option<String>,
    /// Organizational Unit (OU)
    pub organizational_unit: Vec<String>,
    /// Country (C)
    pub country: Option<String>,
    /// State/Province (ST)
    pub state_or_province: Option<String>,
    /// Locality (L)
    pub locality: Option<String>,
    /// Email Address
    pub email_address: Option<String>,
    /// Raw DN string representation
    pub raw_dn: String,
}

/// Subject Alternative Name types
#[derive(Debug, Clone)]
pub enum SubjectAltName {
    DnsName(String),
    IpAddress(std::net::IpAddr),
    EmailAddress(String),
    Uri(String),
    DirectoryName(DistinguishedName),
    OtherName { type_id: String, value: Vec<u8> },
}

/// X.509 Key Usage flags
#[derive(Debug, Clone)]
pub struct KeyUsage {
    pub digital_signature: bool,
    pub non_repudiation: bool,
    pub key_encipherment: bool,
    pub data_encipherment: bool,
    pub key_agreement: bool,
    pub key_cert_sign: bool,
    pub crl_sign: bool,
    pub encipher_only: bool,
    pub decipher_only: bool,
}

/// Extended Key Usage purposes
#[derive(Debug, Clone)]
pub struct ExtendedKeyUsage {
    pub server_auth: bool,
    pub client_auth: bool,
    pub code_signing: bool,
    pub email_protection: bool,
    pub time_stamping: bool,
    pub ocsp_signing: bool,
    pub any_extended_key_usage: bool,
    pub other_purposes: Vec<String>,
}

/// Certificate role/purpose classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateRole {
    /// Root Certificate Authority
    RootCa,
    /// Intermediate Certificate Authority
    IntermediateCa,
    /// End-entity server certificate
    Server,
    /// End-entity client certificate
    Client,
    /// Code signing certificate
    CodeSigning,
    /// Email protection certificate
    EmailProtection,
    /// Multi-purpose certificate
    MultiPurpose,
    /// Unknown or custom purpose
    Unknown,
}

/// Certificate validity period information
#[derive(Debug, Clone)]
pub struct ValidityPeriod {
    /// Certificate not valid before this time
    pub not_before: DateTime<Utc>,
    /// Certificate not valid after this time
    pub not_after: DateTime<Utc>,
    /// Duration of validity in seconds
    pub validity_duration_seconds: u64,
    /// Whether certificate is currently valid (time-wise)
    pub is_currently_valid: bool,
    /// Days until expiration (if positive) or days since expiration (if negative)
    pub days_until_expiry: i64,
}

/// Public key information
#[derive(Debug, Clone)]
pub struct PublicKeyInfo {
    /// Algorithm identifier (e.g., "rsaEncryption", "id-ecPublicKey")
    pub algorithm: String,
    /// Key size in bits
    pub key_size: Option<u32>,
    /// Curve name for EC keys
    pub curve_name: Option<String>,
    /// Key strength classification
    pub strength: KeyStrength,
    /// Raw public key bytes
    pub public_key_bytes: Vec<u8>,
}

/// Key strength classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStrength {
    Weak,     // < 2048 RSA, < 224 EC
    Adequate, // 2048 RSA, 224-255 EC
    Strong,   // 3072+ RSA, 256+ EC
    Unknown,
}

impl CertificateMetadata {
    /// Parse certificate metadata from DER-encoded certificate bytes
    pub fn parse_from_der(cert_der: &[u8]) -> Result<Self, RustMqError> {
        let cert =
            X509Certificate::from_der(cert_der).map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Failed to parse certificate: {}", e),
            })?;

        Self::parse_from_x509_cert(&cert, cert_der)
    }

    /// Parse certificate metadata from x509-cert Certificate
    pub fn parse_from_x509_cert(
        cert: &X509Certificate,
        cert_der: &[u8],
    ) -> Result<Self, RustMqError> {
        let fingerprint = Self::calculate_fingerprint(cert_der);
        let parsed_at = Utc::now();

        // Extract basic certificate information
        let serial_number = cert.tbs_certificate.serial_number.as_bytes().to_vec();
        let subject_dn = Self::parse_distinguished_name(&cert.tbs_certificate.subject)?;
        let issuer_dn = Self::parse_distinguished_name(&cert.tbs_certificate.issuer)?;

        // Extract validity period
        let validity_period = Self::parse_validity_period(&cert.tbs_certificate.validity)?;

        // Extract public key information
        let public_key_info =
            Self::parse_public_key_info(&cert.tbs_certificate.subject_public_key_info)?;

        // Extract extensions
        let extensions = cert.tbs_certificate.extensions.as_ref();
        let subject_alt_names = Self::extract_subject_alt_names(extensions)?;
        let key_usage = Self::extract_key_usage(extensions)?;
        let extended_key_usage = Self::extract_extended_key_usage(extensions)?;
        let certificate_policies = Self::extract_certificate_policies(extensions)?;
        let authority_key_id = Self::extract_authority_key_id(extensions)?;
        let subject_key_id = Self::extract_subject_key_id(extensions)?;

        // Determine certificate role
        let certificate_role = Self::determine_certificate_role(
            &key_usage,
            &extended_key_usage,
            &subject_dn,
            &issuer_dn,
        );

        Ok(CertificateMetadata {
            serial_number,
            subject_dn,
            issuer_dn,
            subject_alt_names,
            key_usage,
            extended_key_usage,
            certificate_role,
            validity_period,
            public_key_info,
            fingerprint,
            certificate_policies,
            authority_key_id,
            subject_key_id,
            parsed_at,
        })
    }

    /// Calculate SHA256 fingerprint of certificate
    fn calculate_fingerprint(cert_der: &[u8]) -> String {
        use ring::digest;
        let hash = digest::digest(&digest::SHA256, cert_der);
        hex::encode(hash.as_ref())
    }

    /// Parse Distinguished Name from x509-cert Name
    fn parse_distinguished_name(name: &Name) -> Result<DistinguishedName, RustMqError> {
        // For now, use a simplified implementation
        // In a full implementation, you'd parse each RDN component
        let raw_dn = format!("{:?}", name); // Temporary representation

        Ok(DistinguishedName {
            common_name: None, // TODO: Extract from RDNs
            organization: None,
            organizational_unit: Vec::new(),
            country: None,
            state_or_province: None,
            locality: None,
            email_address: None,
            raw_dn,
        })
    }

    /// Parse validity period from certificate
    fn parse_validity_period(
        validity: &x509_cert::time::Validity,
    ) -> Result<ValidityPeriod, RustMqError> {
        // Convert x509-cert time to DateTime<Utc>
        let not_before = Utc::now(); // TODO: Convert from validity.not_before
        let not_after = Utc::now(); // TODO: Convert from validity.not_after

        let now = Utc::now();
        let is_currently_valid = now >= not_before && now <= not_after;
        let days_until_expiry = (not_after - now).num_days();
        let validity_duration_seconds = (not_after - not_before).num_seconds() as u64;

        Ok(ValidityPeriod {
            not_before,
            not_after,
            validity_duration_seconds,
            is_currently_valid,
            days_until_expiry,
        })
    }

    /// Parse public key information
    fn parse_public_key_info(
        spki: &SubjectPublicKeyInfo<der::Any, der::asn1::BitString>,
    ) -> Result<PublicKeyInfo, RustMqError> {
        let algorithm = format!("{:?}", spki.algorithm.oid); // Simplified
        let public_key_bytes = spki.subject_public_key.raw_bytes().to_vec();

        Ok(PublicKeyInfo {
            algorithm,
            key_size: None, // TODO: Calculate based on key type
            curve_name: None,
            strength: KeyStrength::Unknown,
            public_key_bytes,
        })
    }

    /// Extract Subject Alternative Names from extensions
    fn extract_subject_alt_names(
        extensions: Option<&Extensions>,
    ) -> Result<Vec<SubjectAltName>, RustMqError> {
        // TODO: Implement SAN extraction
        Ok(Vec::new())
    }

    /// Extract Key Usage from extensions
    fn extract_key_usage(extensions: Option<&Extensions>) -> Result<Option<KeyUsage>, RustMqError> {
        // TODO: Implement key usage extraction
        Ok(None)
    }

    /// Extract Extended Key Usage from extensions
    fn extract_extended_key_usage(
        extensions: Option<&Extensions>,
    ) -> Result<Option<ExtendedKeyUsage>, RustMqError> {
        // TODO: Implement extended key usage extraction
        Ok(None)
    }

    /// Extract Certificate Policies from extensions
    fn extract_certificate_policies(
        extensions: Option<&Extensions>,
    ) -> Result<Vec<String>, RustMqError> {
        // TODO: Implement certificate policies extraction
        Ok(Vec::new())
    }

    /// Extract Authority Key Identifier from extensions
    fn extract_authority_key_id(
        extensions: Option<&Extensions>,
    ) -> Result<Option<Vec<u8>>, RustMqError> {
        // TODO: Implement authority key ID extraction
        Ok(None)
    }

    /// Extract Subject Key Identifier from extensions
    fn extract_subject_key_id(
        extensions: Option<&Extensions>,
    ) -> Result<Option<Vec<u8>>, RustMqError> {
        // TODO: Implement subject key ID extraction
        Ok(None)
    }

    /// Determine certificate role based on extensions and DN
    fn determine_certificate_role(
        key_usage: &Option<KeyUsage>,
        extended_key_usage: &Option<ExtendedKeyUsage>,
        subject_dn: &DistinguishedName,
        issuer_dn: &DistinguishedName,
    ) -> CertificateRole {
        // Check if it's a CA certificate
        if let Some(ku) = key_usage {
            if ku.key_cert_sign {
                // Determine if root or intermediate CA
                if subject_dn.raw_dn == issuer_dn.raw_dn {
                    return CertificateRole::RootCa;
                } else {
                    return CertificateRole::IntermediateCa;
                }
            }
        }

        // Check extended key usage for end-entity certificates
        if let Some(eku) = extended_key_usage {
            if eku.server_auth && !eku.client_auth {
                return CertificateRole::Server;
            }
            if eku.client_auth && !eku.server_auth {
                return CertificateRole::Client;
            }
            if eku.code_signing {
                return CertificateRole::CodeSigning;
            }
            if eku.email_protection {
                return CertificateRole::EmailProtection;
            }
            if eku.server_auth && eku.client_auth {
                return CertificateRole::MultiPurpose;
            }
        }

        CertificateRole::Unknown
    }

    /// Get a human-readable principal from the certificate metadata
    pub fn get_principal(&self) -> Arc<str> {
        // Prefer Common Name from subject
        if let Some(ref cn) = self.subject_dn.common_name {
            return Arc::from(cn.as_str());
        }

        // Fall back to first SAN if available
        for san in &self.subject_alt_names {
            match san {
                SubjectAltName::DnsName(dns) => return Arc::from(dns.as_str()),
                SubjectAltName::EmailAddress(email) => return Arc::from(email.as_str()),
                _ => continue,
            }
        }

        // Fall back to raw DN
        Arc::from(self.subject_dn.raw_dn.as_str())
    }

    /// Check if certificate is valid for a specific purpose
    pub fn is_valid_for_purpose(&self, purpose: CertificateRole) -> bool {
        match purpose {
            CertificateRole::Server => {
                if let Some(ref eku) = self.extended_key_usage {
                    return eku.server_auth || eku.any_extended_key_usage;
                }
                false
            }
            CertificateRole::Client => {
                if let Some(ref eku) = self.extended_key_usage {
                    return eku.client_auth || eku.any_extended_key_usage;
                }
                false
            }
            _ => self.certificate_role == purpose,
        }
    }

    /// Check if certificate is currently time-valid
    pub fn is_time_valid(&self) -> bool {
        self.validity_period.is_currently_valid
    }

    /// Get certificate age in days
    pub fn get_age_days(&self) -> i64 {
        (Utc::now() - self.validity_period.not_before).num_days()
    }

    /// Check if certificate is nearing expiration (within specified days)
    pub fn is_nearing_expiration(&self, warning_days: i64) -> bool {
        self.validity_period.days_until_expiry <= warning_days
            && self.validity_period.days_until_expiry > 0
    }
}
