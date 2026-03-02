//! Enhanced Certificate Metadata for Advanced WebPKI Implementation
//!
//! This module provides comprehensive certificate information extraction
//! using modern ASN.1 parsing libraries for enhanced security validation.

use crate::error::RustMqError;
use chrono::{DateTime, TimeZone, Utc};
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

// Well-known OID constants for extension parsing
mod oids {
    // Key Usage: 2.5.29.15
    pub const KEY_USAGE: &[u8] = &[85, 29, 15];
    // Extended Key Usage: 2.5.29.37
    pub const EXTENDED_KEY_USAGE: &[u8] = &[85, 29, 37];
    // Subject Alternative Name: 2.5.29.17
    pub const SUBJECT_ALT_NAME: &[u8] = &[85, 29, 17];
    // Certificate Policies: 2.5.29.32
    pub const CERTIFICATE_POLICIES: &[u8] = &[85, 29, 32];
    // Authority Key Identifier: 2.5.29.35
    pub const AUTHORITY_KEY_ID: &[u8] = &[85, 29, 35];
    // Subject Key Identifier: 2.5.29.14
    pub const SUBJECT_KEY_ID: &[u8] = &[85, 29, 14];

    // EKU OIDs
    pub const EKU_SERVER_AUTH: &str = "1.3.6.1.5.5.7.3.1";
    pub const EKU_CLIENT_AUTH: &str = "1.3.6.1.5.5.7.3.2";
    pub const EKU_CODE_SIGNING: &str = "1.3.6.1.5.5.7.3.3";
    pub const EKU_EMAIL_PROTECTION: &str = "1.3.6.1.5.5.7.3.4";
    pub const EKU_TIME_STAMPING: &str = "1.3.6.1.5.5.7.3.8";
    pub const EKU_OCSP_SIGNING: &str = "1.3.6.1.5.5.7.3.9";
    pub const EKU_ANY: &str = "2.5.29.37.0";

    // Algorithm OIDs
    pub const RSA_ENCRYPTION: &str = "1.2.840.113549.1.1.1";
    pub const EC_PUBLIC_KEY: &str = "1.2.840.10045.2.1";

    // EC curve OIDs
    pub const PRIME256V1: &str = "1.2.840.10045.3.1.7";
    pub const SECP384R1: &str = "1.3.132.0.34";
    pub const SECP521R1: &str = "1.3.132.0.35";

    // RDN attribute type OIDs
    pub const AT_COMMON_NAME: &str = "2.5.4.3";
    pub const AT_COUNTRY: &str = "2.5.4.6";
    pub const AT_LOCALITY: &str = "2.5.4.7";
    pub const AT_STATE_OR_PROVINCE: &str = "2.5.4.8";
    pub const AT_ORGANIZATION: &str = "2.5.4.10";
    pub const AT_ORGANIZATIONAL_UNIT: &str = "2.5.4.11";
    pub const AT_EMAIL_ADDRESS: &str = "1.2.840.113549.1.9.1";
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

        // Extract extensions using x509_parser for robust parsing
        let (
            subject_alt_names,
            key_usage,
            extended_key_usage,
            certificate_policies,
            authority_key_id,
            subject_key_id,
        ) = Self::extract_extensions_from_der(cert_der)?;

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

    /// Parse Distinguished Name from x509-cert Name, extracting individual RDN components
    fn parse_distinguished_name(name: &Name) -> Result<DistinguishedName, RustMqError> {
        let mut common_name = None;
        let mut organization = None;
        let mut organizational_unit = Vec::new();
        let mut country = None;
        let mut state_or_province = None;
        let mut locality = None;
        let mut email_address = None;

        // Iterate over RDN sequences and extract attribute values by OID
        for rdn in name.0.iter() {
            for atav in rdn.0.iter() {
                let oid_str = atav.oid.to_string();
                // Extract the UTF-8 string value from the ANY value
                let value = Self::extract_rdn_string_value(&atav.value);

                match oid_str.as_str() {
                    oids::AT_COMMON_NAME => common_name = value,
                    oids::AT_ORGANIZATION => organization = value,
                    oids::AT_ORGANIZATIONAL_UNIT => {
                        if let Some(v) = value {
                            organizational_unit.push(v);
                        }
                    }
                    oids::AT_COUNTRY => country = value,
                    oids::AT_STATE_OR_PROVINCE => state_or_province = value,
                    oids::AT_LOCALITY => locality = value,
                    oids::AT_EMAIL_ADDRESS => email_address = value,
                    _ => {} // Skip unknown attributes
                }
            }
        }

        // Build a human-readable DN string
        let mut parts = Vec::new();
        if let Some(ref cn) = common_name {
            parts.push(format!("CN={}", cn));
        }
        if let Some(ref o) = organization {
            parts.push(format!("O={}", o));
        }
        for ou in &organizational_unit {
            parts.push(format!("OU={}", ou));
        }
        if let Some(ref c) = country {
            parts.push(format!("C={}", c));
        }
        if let Some(ref st) = state_or_province {
            parts.push(format!("ST={}", st));
        }
        if let Some(ref l) = locality {
            parts.push(format!("L={}", l));
        }
        let raw_dn = if parts.is_empty() {
            format!("{:?}", name)
        } else {
            parts.join(", ")
        };

        Ok(DistinguishedName {
            common_name,
            organization,
            organizational_unit,
            country,
            state_or_province,
            locality,
            email_address,
            raw_dn,
        })
    }

    /// Extract a UTF-8 string value from an ASN.1 ANY value (handles UTF8String, PrintableString, IA5String)
    fn extract_rdn_string_value(value: &der::Any) -> Option<String> {
        // Try to decode as UTF8String first, then PrintableString, then raw bytes
        if let Ok(s) = der::asn1::Utf8StringRef::try_from(value) {
            return Some(s.as_str().to_string());
        }
        if let Ok(s) = der::asn1::PrintableStringRef::try_from(value) {
            return Some(s.as_str().to_string());
        }
        if let Ok(s) = der::asn1::Ia5StringRef::try_from(value) {
            return Some(s.as_str().to_string());
        }
        // Fallback: try raw bytes as UTF-8
        std::str::from_utf8(value.value()).ok().map(|s| s.to_string())
    }

    /// Parse validity period from certificate, converting ASN.1 time to DateTime<Utc>
    fn parse_validity_period(
        validity: &x509_cert::time::Validity,
    ) -> Result<ValidityPeriod, RustMqError> {
        let not_before = Self::x509_time_to_datetime(&validity.not_before)?;
        let not_after = Self::x509_time_to_datetime(&validity.not_after)?;

        let now = Utc::now();
        let is_currently_valid = now >= not_before && now <= not_after;
        let days_until_expiry = (not_after - now).num_days();
        let validity_duration_seconds = (not_after - not_before)
            .num_seconds()
            .max(0) as u64;

        Ok(ValidityPeriod {
            not_before,
            not_after,
            validity_duration_seconds,
            is_currently_valid,
            days_until_expiry,
        })
    }

    /// Convert x509-cert Time to chrono DateTime<Utc>
    fn x509_time_to_datetime(
        time: &x509_cert::time::Time,
    ) -> Result<DateTime<Utc>, RustMqError> {
        // x509-cert Time can be encoded to DER and contains the raw time bytes.
        // Convert via SystemTime which x509-cert supports.
        let system_time: SystemTime = (*time).into();
        let duration = system_time
            .duration_since(UNIX_EPOCH)
            .map_err(|e| RustMqError::InvalidCertificate {
                reason: format!("Certificate time before Unix epoch: {}", e),
            })?;
        Utc.timestamp_opt(duration.as_secs() as i64, duration.subsec_nanos())
            .single()
            .ok_or_else(|| RustMqError::InvalidCertificate {
                reason: "Failed to convert certificate time to UTC".to_string(),
            })
    }

    /// Parse public key information, detecting algorithm and key size
    fn parse_public_key_info(
        spki: &SubjectPublicKeyInfo<der::Any, der::asn1::BitString>,
    ) -> Result<PublicKeyInfo, RustMqError> {
        let oid_str = spki.algorithm.oid.to_string();
        let public_key_bytes = spki.subject_public_key.raw_bytes().to_vec();
        let key_bit_len = public_key_bytes.len() as u32 * 8;

        let (algorithm, key_size, curve_name, strength) = match oid_str.as_str() {
            oids::RSA_ENCRYPTION => {
                let size = key_bit_len;
                let strength = if size < 2048 {
                    KeyStrength::Weak
                } else if size < 3072 {
                    KeyStrength::Adequate
                } else {
                    KeyStrength::Strong
                };
                ("rsaEncryption".to_string(), Some(size), None, strength)
            }
            oids::EC_PUBLIC_KEY => {
                // For EC keys, check the curve from algorithm parameters
                let (curve, size, strength) =
                    Self::detect_ec_curve(spki, key_bit_len);
                (
                    "id-ecPublicKey".to_string(),
                    Some(size),
                    Some(curve),
                    strength,
                )
            }
            _ => (
                oid_str,
                None,
                None,
                KeyStrength::Unknown,
            ),
        };

        Ok(PublicKeyInfo {
            algorithm,
            key_size,
            curve_name,
            strength,
            public_key_bytes,
        })
    }

    /// Detect EC curve from SPKI parameters
    fn detect_ec_curve(
        spki: &SubjectPublicKeyInfo<der::Any, der::asn1::BitString>,
        key_bit_len: u32,
    ) -> (String, u32, KeyStrength) {
        // Try to read the curve OID from algorithm parameters
        if let Some(params) = &spki.algorithm.parameters {
            if let Ok(oid) = der::asn1::ObjectIdentifier::from_der(params.value()) {
                let oid_str = oid.to_string();
                return match oid_str.as_str() {
                    oids::PRIME256V1 => ("P-256".to_string(), 256, KeyStrength::Strong),
                    oids::SECP384R1 => ("P-384".to_string(), 384, KeyStrength::Strong),
                    oids::SECP521R1 => ("P-521".to_string(), 521, KeyStrength::Strong),
                    _ => (oid_str, key_bit_len, KeyStrength::Unknown),
                };
            }
        }
        // Fallback: infer from key length
        match key_bit_len {
            512..=520 => ("P-256".to_string(), 256, KeyStrength::Strong),
            768..=776 => ("P-384".to_string(), 384, KeyStrength::Strong),
            1040..=1056 => ("P-521".to_string(), 521, KeyStrength::Strong),
            _ => ("unknown".to_string(), key_bit_len, KeyStrength::Unknown),
        }
    }

    /// Find an extension by matching the trailing OID bytes
    fn find_extension<'a>(
        extensions: Option<&'a Extensions>,
        oid_suffix: &[u8],
    ) -> Option<&'a Extension> {
        extensions?.iter().find(|ext| {
            let ext_bytes = ext.extn_id.as_bytes();
            ext_bytes.ends_with(oid_suffix)
        })
    }

    /// Extract extensions using x509_parser from the raw DER certificate bytes.
    /// This is called from parse_from_der with the original cert_der.
    /// For the x509-cert based path, we re-parse using x509_parser to extract extensions
    /// since x509_parser has richer extension parsing support.
    fn extract_extensions_from_der(
        cert_der: &[u8],
    ) -> Result<
        (
            Vec<SubjectAltName>,
            Option<KeyUsage>,
            Option<ExtendedKeyUsage>,
            Vec<String>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
        ),
        RustMqError,
    > {
        let (_, parsed) = x509_parser::parse_x509_certificate(cert_der).map_err(|e| {
            RustMqError::InvalidCertificate {
                reason: format!("x509_parser failed: {}", e),
            }
        })?;

        let mut sans = Vec::new();
        let mut key_usage = None;
        let mut extended_key_usage = None;
        let mut certificate_policies = Vec::new();
        let mut authority_key_id = None;
        let mut subject_key_id = None;

        for ext in parsed.extensions() {
            match ext.parsed_extension() {
                x509_parser::extensions::ParsedExtension::SubjectAlternativeName(san) => {
                    for gn in &san.general_names {
                        match gn {
                            x509_parser::extensions::GeneralName::DNSName(name) => {
                                sans.push(SubjectAltName::DnsName(name.to_string()));
                            }
                            x509_parser::extensions::GeneralName::RFC822Name(email) => {
                                sans.push(SubjectAltName::EmailAddress(email.to_string()));
                            }
                            x509_parser::extensions::GeneralName::URI(uri) => {
                                sans.push(SubjectAltName::Uri(uri.to_string()));
                            }
                            x509_parser::extensions::GeneralName::IPAddress(ip_bytes) => {
                                if let Some(addr) = Self::parse_ip_from_bytes(ip_bytes) {
                                    sans.push(SubjectAltName::IpAddress(addr));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                x509_parser::extensions::ParsedExtension::KeyUsage(ku) => {
                    key_usage = Some(KeyUsage {
                        digital_signature: ku.digital_signature(),
                        non_repudiation: ku.non_repudiation(),
                        key_encipherment: ku.key_encipherment(),
                        data_encipherment: ku.data_encipherment(),
                        key_agreement: ku.key_agreement(),
                        key_cert_sign: ku.key_cert_sign(),
                        crl_sign: ku.crl_sign(),
                        encipher_only: ku.encipher_only(),
                        decipher_only: ku.decipher_only(),
                    });
                }
                x509_parser::extensions::ParsedExtension::ExtendedKeyUsage(eku) => {
                    let mut other_purposes = Vec::new();
                    for oid in &eku.other {
                        other_purposes.push(oid.to_id_string());
                    }
                    extended_key_usage = Some(ExtendedKeyUsage {
                        server_auth: eku.server_auth,
                        client_auth: eku.client_auth,
                        code_signing: eku.code_signing,
                        email_protection: eku.email_protection,
                        time_stamping: eku.time_stamping,
                        ocsp_signing: eku.ocsp_signing,
                        any_extended_key_usage: eku.any,
                        other_purposes,
                    });
                }
                x509_parser::extensions::ParsedExtension::CertificatePolicies(policies) => {
                    for policy in policies.iter() {
                        certificate_policies.push(policy.policy_id.to_id_string());
                    }
                }
                x509_parser::extensions::ParsedExtension::AuthorityKeyIdentifier(aki) => {
                    if let Some(key_id) = &aki.key_identifier {
                        authority_key_id = Some(key_id.0.to_vec());
                    }
                }
                x509_parser::extensions::ParsedExtension::SubjectKeyIdentifier(ski) => {
                    subject_key_id = Some(ski.0.to_vec());
                }
                _ => {}
            }
        }

        Ok((
            sans,
            key_usage,
            extended_key_usage,
            certificate_policies,
            authority_key_id,
            subject_key_id,
        ))
    }

    /// Parse IP address from raw bytes (4 bytes = IPv4, 16 bytes = IPv6)
    fn parse_ip_from_bytes(bytes: &[u8]) -> Option<std::net::IpAddr> {
        match bytes.len() {
            4 => Some(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
                bytes[0], bytes[1], bytes[2], bytes[3],
            ))),
            16 => {
                let mut octets = [0u8; 16];
                octets.copy_from_slice(bytes);
                Some(std::net::IpAddr::V6(std::net::Ipv6Addr::from(octets)))
            }
            _ => None,
        }
    }

    /// Stub methods kept for API compatibility — actual extraction done in extract_extensions_from_der
    fn extract_subject_alt_names(
        _extensions: Option<&Extensions>,
    ) -> Result<Vec<SubjectAltName>, RustMqError> {
        Ok(Vec::new()) // Populated by extract_extensions_from_der
    }

    fn extract_key_usage(
        _extensions: Option<&Extensions>,
    ) -> Result<Option<KeyUsage>, RustMqError> {
        Ok(None) // Populated by extract_extensions_from_der
    }

    fn extract_extended_key_usage(
        _extensions: Option<&Extensions>,
    ) -> Result<Option<ExtendedKeyUsage>, RustMqError> {
        Ok(None) // Populated by extract_extensions_from_der
    }

    fn extract_certificate_policies(
        _extensions: Option<&Extensions>,
    ) -> Result<Vec<String>, RustMqError> {
        Ok(Vec::new()) // Populated by extract_extensions_from_der
    }

    fn extract_authority_key_id(
        _extensions: Option<&Extensions>,
    ) -> Result<Option<Vec<u8>>, RustMqError> {
        Ok(None) // Populated by extract_extensions_from_der
    }

    fn extract_subject_key_id(
        _extensions: Option<&Extensions>,
    ) -> Result<Option<Vec<u8>>, RustMqError> {
        Ok(None) // Populated by extract_extensions_from_der
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
