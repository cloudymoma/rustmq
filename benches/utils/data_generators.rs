//! Data generation utilities for security performance benchmarks

use rustmq::security::auth::{AclKey, Principal, Permission};
use rustmq::types::TopicName;
use std::sync::Arc;
use rand::{Rng, SeedableRng, distributions::Alphanumeric};
use rand::rngs::StdRng;

/// Generate a single ACL key for testing
pub fn generate_acl_key(principal: &str, topic: &str, permission: Permission) -> AclKey {
    AclKey {
        principal: Arc::from(principal),
        resource: Arc::from(format!("topic:{}", topic)),
        operation: permission_to_operation(permission),
    }
}

/// Generate a random ACL key
pub fn generate_random_acl_key() -> AclKey {
    let mut rng = rand::thread_rng();
    let principal = format!("user_{}", rng.gen::<u32>());
    let topic = format!("topic_{}", rng.gen::<u32>());
    let permission = random_permission(&mut rng);
    
    generate_acl_key(&principal, &topic, permission)
}

/// Generate a batch of ACL keys for testing
pub fn generate_acl_key_batch(count: usize) -> Vec<AclKey> {
    let mut rng = StdRng::seed_from_u64(12345); // Deterministic for benchmarks
    let mut keys = Vec::with_capacity(count);
    
    for i in 0..count {
        let principal = format!("user_{}", i % 100); // Reuse principals for realistic patterns
        let topic = format!("topic_{}", i % 50); // Reuse topics
        let permission = match i % 5 {
            0 => Permission::Read,
            1 => Permission::Write,
            2 => Permission::Admin,
            3 => Permission::Create,
            _ => Permission::Delete,
        };
        
        keys.push(generate_acl_key(&principal, &topic, permission));
    }
    
    keys
}

/// Generate test principals
pub fn generate_principals(count: usize) -> Vec<Principal> {
    let mut principals = Vec::with_capacity(count);
    
    for i in 0..count {
        let principal = Principal {
            name: Arc::from(format!("user_{}", i)),
            principal_type: if i % 10 == 0 { 
                PrincipalType::Service 
            } else { 
                PrincipalType::User 
            },
            groups: if i % 5 == 0 {
                vec![Arc::from("admin"), Arc::from("operators")]
            } else {
                vec![Arc::from("users")]
            },
            attributes: Default::default(),
        };
        principals.push(principal);
    }
    
    principals
}

/// Generate test topics
pub fn generate_topics(count: usize) -> Vec<String> {
    let mut topics = Vec::with_capacity(count);
    let prefixes = ["events", "logs", "metrics", "commands", "data"];
    
    for i in 0..count {
        let prefix = prefixes[i % prefixes.len()];
        topics.push(format!("{}.{}.{}", prefix, i / 10, i));
    }
    
    topics
}

/// Generate ACL rules for testing
pub fn generate_acl_rules(count: usize) -> Vec<AclRule> {
    let mut rules = Vec::with_capacity(count);
    let mut rng = StdRng::seed_from_u64(54321);
    
    for i in 0..count {
        let rule = AclRule {
            id: format!("rule_{}", i),
            principal_pattern: if i % 10 == 0 {
                "*".to_string() // Wildcard
            } else {
                format!("user_{}", i % 100)
            },
            resource_pattern: if i % 20 == 0 {
                "topic:*".to_string() // Wildcard
            } else {
                format!("topic:topic_{}", i % 50)
            },
            operation: match i % 5 {
                0 => "READ",
                1 => "WRITE",
                2 => "CREATE",
                3 => "DELETE",
                _ => "ADMIN",
            }.to_string(),
            permission_type: if rng.gen_bool(0.9) { "ALLOW" } else { "DENY" }.to_string(),
            created_at: chrono::Utc::now(),
            created_by: format!("admin_{}", i % 10),
        };
        rules.push(rule);
    }
    
    rules
}

/// Generate test certificate data
pub fn generate_test_certificate() -> Vec<u8> {
    // Generate a dummy certificate for benchmarking
    // In real benchmarks, this would use rcgen to create actual certificates
    let mut cert = Vec::with_capacity(2048);
    cert.extend_from_slice(b"-----BEGIN CERTIFICATE-----\n");
    
    // Add dummy base64 data
    for _ in 0..20 {
        let line: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(64)
            .map(char::from)
            .collect();
        cert.extend_from_slice(line.as_bytes());
        cert.push(b'\n');
    }
    
    cert.extend_from_slice(b"-----END CERTIFICATE-----\n");
    cert
}

/// Generate CA certificate chain
pub fn generate_ca_chain() -> Vec<Vec<u8>> {
    vec![
        generate_test_certificate(), // Leaf certificate
        generate_test_certificate(), // Intermediate CA
        generate_test_certificate(), // Root CA
    ]
}

/// Generate test CRL (Certificate Revocation List)
pub fn generate_test_crl(revoked_count: usize) -> CertificateRevocationList {
    let mut revoked = Vec::with_capacity(revoked_count);
    
    for i in 0..revoked_count {
        revoked.push(RevokedCertificate {
            serial_number: format!("{:016x}", i),
            revocation_date: chrono::Utc::now() - chrono::Duration::days(i as i64 % 30),
            reason: match i % 5 {
                0 => RevocationReason::KeyCompromise,
                1 => RevocationReason::CaCompromise,
                2 => RevocationReason::AffiliationChanged,
                3 => RevocationReason::Superseded,
                _ => RevocationReason::Unspecified,
            },
        });
    }
    
    CertificateRevocationList {
        issuer: "CN=Test CA".to_string(),
        this_update: chrono::Utc::now(),
        next_update: chrono::Utc::now() + chrono::Duration::hours(24),
        revoked_certificates: revoked,
    }
}

/// Generate authenticated request for benchmarking
pub fn generate_authenticated_request() -> AuthenticatedRequest {
    AuthenticatedRequest {
        principal: Arc::from("test-user"),
        resource: Arc::from("topic:test-topic"),
        operation: "READ".to_string(),
        certificate: generate_test_certificate(),
        signature: vec![0u8; 256], // Dummy signature
        timestamp: chrono::Utc::now(),
    }
}

/// Generate security metadata for transmission benchmarks
pub fn generate_security_metadata() -> SecurityMetadata {
    SecurityMetadata {
        principal: Arc::from("test-service"),
        groups: vec![Arc::from("admin"), Arc::from("operators")],
        permissions: vec!["READ", "WRITE", "ADMIN"].iter().map(|s| s.to_string()).collect(),
        certificate_chain: generate_ca_chain(),
        session_id: uuid::Uuid::new_v4().to_string(),
        expiry: chrono::Utc::now() + chrono::Duration::hours(1),
    }
}

// Helper structures for benchmarking

#[derive(Clone)]
pub struct AclRule {
    pub id: String,
    pub principal_pattern: String,
    pub resource_pattern: String,
    pub operation: String,
    pub permission_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
}

#[derive(Clone)]
pub enum PrincipalType {
    User,
    Service,
}

pub struct CertificateRevocationList {
    pub issuer: String,
    pub this_update: chrono::DateTime<chrono::Utc>,
    pub next_update: chrono::DateTime<chrono::Utc>,
    pub revoked_certificates: Vec<RevokedCertificate>,
}

pub struct RevokedCertificate {
    pub serial_number: String,
    pub revocation_date: chrono::DateTime<chrono::Utc>,
    pub reason: RevocationReason,
}

#[derive(Clone)]
pub enum RevocationReason {
    Unspecified,
    KeyCompromise,
    CaCompromise,
    AffiliationChanged,
    Superseded,
}

pub struct AuthenticatedRequest {
    pub principal: Arc<str>,
    pub resource: Arc<str>,
    pub operation: String,
    pub certificate: Vec<u8>,
    pub signature: Vec<u8>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct SecurityMetadata {
    pub principal: Arc<str>,
    pub groups: Vec<Arc<str>>,
    pub permissions: Vec<String>,
    pub certificate_chain: Vec<Vec<u8>>,
    pub session_id: String,
    pub expiry: chrono::DateTime<chrono::Utc>,
}

// Helper functions

fn permission_to_operation(permission: Permission) -> Arc<str> {
    Arc::from(match permission {
        Permission::Read => "READ",
        Permission::Write => "WRITE",
        Permission::Admin => "ADMIN",
        Permission::Create => "CREATE",
        Permission::Delete => "DELETE",
        _ => "UNKNOWN",
    })
}

fn random_permission(rng: &mut impl Rng) -> Permission {
    match rng.gen_range(0..5) {
        0 => Permission::Read,
        1 => Permission::Write,
        2 => Permission::Admin,
        3 => Permission::Create,
        _ => Permission::Delete,
    }
}