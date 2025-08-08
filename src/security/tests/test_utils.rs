//! Test utilities and fixtures for security component testing
//!
//! Provides helper functions, mock implementations, and test data generators
//! for comprehensive security testing across all components.

use crate::error::RustMqError;
use crate::security::{
    SecurityConfig, SecurityManager, SecurityContext, SecurityMetrics,
    AclConfig, CertificateManagementConfig, AuditConfig,
};
use crate::security::auth::{
    AuthenticationManager, AuthorizationManager, AuthContext, AuthorizedRequest,
    Principal, Permission, AclKey, AclCacheEntry,
};
use crate::security::acl::{
    AclManager, AclRule, AclEntry, AclOperation, PermissionSet,
    ResourcePattern, ResourceType, Effect,
};
use crate::security::tls::{
    TlsConfig, CertificateManager, CertificateInfo, RevocationList,
    CachingClientCertVerifier, EnhancedCertificateManagementConfig, 
    CaSettings, CertificateRole, CertificateTemplate, KeyType, KeyUsage, 
    ExtendedKeyUsage, CertificateStatus, RevocationReason, CertificateRequest,
    CaGenerationParams, ValidationResult, RevokedCertificate, CertificateAuditEntry,
};
use crate::types::TopicName;

use std::sync::Arc;
use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use tempfile::TempDir;
use rcgen::{DistinguishedName, SanType, Certificate as RcgenCertificate, CertificateParams, KeyPair};
use rustls::Certificate;
use tokio::sync::Mutex;

/// Test configuration factory for security components
pub struct SecurityTestConfig;

impl SecurityTestConfig {
    /// Create a test security configuration with sensible defaults
    pub fn create_test_config() -> SecurityConfig {
        SecurityConfig {
            tls: TlsConfig {
                enabled: true,
                mode: crate::security::tls::TlsMode::Mutual,
                server: Some(crate::security::tls::TlsServerConfig {
                    cert_path: "/tmp/test-cert.pem".to_string(),
                    key_path: "/tmp/test-key.pem".to_string(),
                    ca_cert_path: Some("/tmp/test-ca.pem".to_string()),
                    require_client_cert: true,
                    supported_versions: vec!["TLSv1.3".to_string()],
                    cipher_suites: vec!["TLS13_AES_256_GCM_SHA384".to_string()],
                }),
                client: Some(crate::security::tls::TlsClientConfig {
                    cert_path: Some("/tmp/test-client-cert.pem".to_string()),
                    key_path: Some("/tmp/test-client-key.pem".to_string()),
                    ca_cert_path: "/tmp/test-ca.pem".to_string(),
                    verify_server_cert: true,
                    ..Default::default()
                }),
            },
            acl: AclConfig {
                enabled: true,
                cache_size_mb: 10, // Smaller for tests
                cache_ttl_seconds: 60,
                l2_shard_count: 4, // Fewer shards for tests
                bloom_filter_size: 10_000, // Smaller filter
                batch_fetch_size: 10,
                enable_audit_logging: true,
                negative_cache_enabled: true,
            },
            certificate_management: CertificateManagementConfig {
                ca_cert_path: "/tmp/test-ca.pem".to_string(),
                ca_key_path: "/tmp/test-ca-key.pem".to_string(),
                cert_validity_days: 365,
                auto_renew_before_expiry_days: 30,
                crl_check_enabled: true,
                ocsp_check_enabled: false,
            },
            audit: AuditConfig {
                enabled: true,
                log_authentication_events: true,
                log_authorization_events: true,
                log_certificate_events: true,
                log_failed_attempts: true,
                max_log_size_mb: 10,
            },
        }
    }

    /// Create enhanced certificate management config for testing
    pub fn create_test_certificate_config(temp_dir: &TempDir) -> EnhancedCertificateManagementConfig {
        EnhancedCertificateManagementConfig {
            storage_path: temp_dir.path().to_string_lossy().to_string(),
            audit_enabled: true,
            auto_renewal_enabled: true,
            renewal_check_interval_hours: 1, // Faster for tests
            certificate_templates: {
                let mut templates = HashMap::new();
                templates.insert(CertificateRole::Broker, CertificateTemplate::broker_default());
                templates.insert(CertificateRole::Controller, CertificateTemplate::controller_default());
                templates.insert(CertificateRole::Client, CertificateTemplate::client_default());
                templates.insert(CertificateRole::Admin, CertificateTemplate::admin_default());
                templates
            },
            ..Default::default()
        }
    }
}

/// Mock certificate generator for testing
pub struct MockCertificateGenerator;

impl MockCertificateGenerator {
    /// Generate a test CA certificate
    pub fn generate_test_ca() -> Result<Certificate, RustMqError> {
        let mut params = CertificateParams::new(vec!["testca.example.com".to_string()]);
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        
        let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| RustMqError::CertificateGeneration { reason: e.to_string() })?;
        
        params.key_pair = Some(key_pair);
        
        let cert = RcgenCertificate::from_params(params)
            .map_err(|e| RustMqError::CertificateGeneration { reason: e.to_string() })?;
        
        let der = cert.serialize_der()
            .map_err(|e| RustMqError::CertificateGeneration { reason: e.to_string() })?;
        
        Ok(Certificate(der))
    }

    /// Generate a test client certificate signed by the given CA
    pub fn generate_test_client_cert(
        ca_cert: &RcgenCertificate,
        common_name: &str,
        role: CertificateRole,
    ) -> Result<Certificate, RustMqError> {
        let mut params = CertificateParams::new(vec![common_name.to_string()]);
        params.is_ca = rcgen::IsCa::NoCa;
        
        // Set key usage based on role
        match role {
            CertificateRole::Broker | CertificateRole::Controller => {
                params.key_usages = vec![
                    rcgen::KeyUsagePurpose::DigitalSignature,
                    rcgen::KeyUsagePurpose::KeyEncipherment,
                ];
                params.extended_key_usages = vec![
                    rcgen::ExtendedKeyUsagePurpose::ServerAuth,
                    rcgen::ExtendedKeyUsagePurpose::ClientAuth,
                ];
            },
            CertificateRole::Client | CertificateRole::Admin => {
                params.key_usages = vec![
                    rcgen::KeyUsagePurpose::DigitalSignature,
                ];
                params.extended_key_usages = vec![
                    rcgen::ExtendedKeyUsagePurpose::ClientAuth,
                ];
            },
            _ => {
                return Err(RustMqError::CertificateGeneration {
                    reason: format!("Unsupported certificate role: {:?}", role)
                });
            }
        }
        
        let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| RustMqError::CertificateGeneration { reason: e.to_string() })?;
        
        params.key_pair = Some(key_pair);
        
        let cert = RcgenCertificate::from_params(params)
            .map_err(|e| RustMqError::CertificateGeneration { reason: e.to_string() })?;
        
        let der = cert.serialize_der_with_signer(ca_cert)
            .map_err(|e| RustMqError::CertificateGeneration { reason: e.to_string() })?;
        
        Ok(Certificate(der))
    }
}

/// Test principal factory
pub struct TestPrincipalFactory;

impl TestPrincipalFactory {
    /// Create test principals for different roles
    pub fn create_broker_principal() -> Principal {
        "broker-001.cluster.internal".into()
    }

    pub fn create_controller_principal() -> Principal {
        "controller-001.cluster.internal".into()
    }

    pub fn create_client_principal() -> Principal {
        "client-producer-001".into()
    }

    pub fn create_admin_principal() -> Principal {
        "admin-user".into()
    }

    pub fn create_test_principals() -> Vec<Principal> {
        vec![
            Self::create_broker_principal(),
            Self::create_controller_principal(),
            Self::create_client_principal(),
            Self::create_admin_principal(),
        ]
    }
}

/// Test ACL rule factory
pub struct TestAclRuleFactory;

impl TestAclRuleFactory {
    /// Create basic ACL rules for testing
    pub fn create_test_rules() -> Vec<AclRule> {
        vec![
            // Broker rules - full access
            AclRule::new(
                "broker-rule-1".to_string(),
                "brokers".to_string(),
                ResourcePattern::new(ResourceType::Topic, "*".to_string()),
                vec![AclOperation::Read, AclOperation::Write, AclOperation::Admin],
                Effect::Allow,
            ),
            
            // Controller rules - management access
            AclRule::new(
                "controller-rule-1".to_string(),
                "controllers".to_string(),
                ResourcePattern::new(ResourceType::Cluster, "*".to_string()),
                vec![AclOperation::Cluster, AclOperation::Describe],
                Effect::Allow,
            ),
            
            // Producer rules - write access to specific topics
            AclRule::new(
                "producer-rule-1".to_string(),
                "producers".to_string(),
                ResourcePattern::new(ResourceType::Topic, "data.*".to_string()),
                vec![AclOperation::Write, AclOperation::Describe],
                Effect::Allow,
            ),
            
            // Admin rules - full access to everything
            AclRule::new(
                "admin-rule-1".to_string(),
                "admins".to_string(),
                ResourcePattern::new(ResourceType::Topic, "*".to_string()),
                vec![AclOperation::Read, AclOperation::Write, AclOperation::Admin, AclOperation::Cluster],
                Effect::Allow,
            ),
            
            // Deny rule for testing
            AclRule::new(
                "deny-rule-1".to_string(),
                "restricted".to_string(),
                ResourcePattern::new(ResourceType::Topic, "system.*".to_string()),
                vec![AclOperation::Read, AclOperation::Write],
                Effect::Deny,
            ),
        ]
    }

    /// Create performance test rules (larger dataset)
    pub fn create_performance_test_rules(count: usize) -> Vec<AclRule> {
        let mut rules = Vec::with_capacity(count);
        
        for i in 0..count {
            let principal = format!("user-{}", i);
            let topic_pattern = format!("user-data-{}", i % 100); // Create some overlap
            
            rules.push(AclRule::new(
                format!("perf-rule-{}", i),
                principal,
                ResourcePattern::new(ResourceType::Topic, topic_pattern),
                vec![AclOperation::Read, AclOperation::Write],
                Effect::Allow,
            ));
        }
        
        rules
    }
}

/// Mock storage for testing ACL components
pub struct MockAclStorage {
    rules: Arc<Mutex<HashMap<String, Vec<AclRule>>>>,
    operations: Arc<Mutex<Vec<String>>>, // Track operations for verification
}

impl MockAclStorage {
    pub fn new() -> Self {
        Self {
            rules: Arc::new(Mutex::new(HashMap::new())),
            operations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn store_rules(&self, principal: &str, rules: Vec<AclRule>) {
        let mut storage = self.rules.lock().await;
        storage.insert(principal.to_string(), rules);
        
        let mut ops = self.operations.lock().await;
        ops.push(format!("store_rules: {}", principal));
    }

    pub async fn get_rules(&self, principal: &str) -> Option<Vec<AclRule>> {
        let storage = self.rules.lock().await;
        let result = storage.get(principal).cloned();
        
        let mut ops = self.operations.lock().await;
        ops.push(format!("get_rules: {}", principal));
        
        result
    }

    pub async fn delete_rules(&self, principal: &str) -> bool {
        let mut storage = self.rules.lock().await;
        let existed = storage.remove(principal).is_some();
        
        let mut ops = self.operations.lock().await;
        ops.push(format!("delete_rules: {}", principal));
        
        existed
    }

    pub async fn get_operations(&self) -> Vec<String> {
        let ops = self.operations.lock().await;
        ops.clone()
    }

    pub async fn clear_operations(&self) {
        let mut ops = self.operations.lock().await;
        ops.clear();
    }
}

/// Test data generators for performance testing
pub struct TestDataGenerator;

impl TestDataGenerator {
    /// Generate test topics for authorization testing
    pub fn generate_test_topics(count: usize) -> Vec<TopicName> {
        (0..count)
            .map(|i| format!("test-topic-{}", i))
            .collect()
    }

    /// Generate test principals for load testing
    pub fn generate_test_principals(count: usize) -> Vec<String> {
        (0..count)
            .map(|i| format!("test-principal-{}", i))
            .collect()
    }

    /// Generate ACL keys for cache testing
    pub fn generate_acl_keys(principal_count: usize, topic_count: usize) -> Vec<AclKey> {
        let mut keys = Vec::new();
        
        for p in 0..principal_count {
            for t in 0..topic_count {
                let principal = format!("principal-{}", p);
                let topic = format!("topic-{}", t);
                
                for permission in [Permission::Read, Permission::Write] {
                    keys.push(AclKey::new(principal.clone().into(), topic.clone().into(), permission));
                }
            }
        }
        
        keys
    }
}

/// Performance test utilities
pub struct PerformanceTestUtils;

impl PerformanceTestUtils {
    /// Measure average latency over multiple iterations
    pub async fn measure_average_latency<F, Fut>(
        iterations: usize,
        operation: F,
    ) -> Duration
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let start = std::time::Instant::now();
        
        for _ in 0..iterations {
            operation().await;
        }
        
        start.elapsed() / iterations as u32
    }

    /// Measure peak memory usage during operation
    pub async fn measure_memory_usage<F, Fut, T>(operation: F) -> (T, usize)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        // Note: This is a simplified memory measurement
        // In a real implementation, you might use system calls or memory profiling tools
        let initial_memory = get_process_memory_usage();
        let result = operation().await;
        let final_memory = get_process_memory_usage();
        
        (result, final_memory.saturating_sub(initial_memory))
    }

    /// Verify authorization latency meets requirements
    pub fn assert_authorization_latency(latency: Duration, max_expected: Duration) {
        assert!(
            latency <= max_expected,
            "Authorization latency {}μs exceeds maximum {}μs",
            latency.as_micros(),
            max_expected.as_micros()
        );
    }
}

/// Simple memory usage estimation (cross-platform approximation)
fn get_process_memory_usage() -> usize {
    // This is a simplified implementation
    // In production, you would use platform-specific APIs
    0
}

/// Test assertion helpers
pub struct SecurityTestAssertions;

impl SecurityTestAssertions {
    /// Assert that authentication succeeded
    pub fn assert_authentication_success(result: &Result<AuthContext, RustMqError>) {
        assert!(result.is_ok(), "Authentication should succeed: {:?}", result);
    }

    /// Assert that authentication failed with expected error
    pub fn assert_authentication_failure(
        result: &Result<AuthContext, RustMqError>,
        expected_error_pattern: &str,
    ) {
        assert!(result.is_err(), "Authentication should fail");
        let error = result.as_ref().unwrap_err();
        assert!(
            error.to_string().contains(expected_error_pattern),
            "Error should contain '{}', got: {}",
            expected_error_pattern,
            error
        );
    }

    /// Assert that authorization succeeded
    pub fn assert_authorization_success(result: &Result<bool, RustMqError>) {
        assert!(result.is_ok(), "Authorization should succeed: {:?}", result);
        assert!(result.as_ref().unwrap(), "Permission should be granted");
    }

    /// Assert that authorization was denied
    pub fn assert_authorization_denied(result: &Result<bool, RustMqError>) {
        assert!(result.is_ok(), "Authorization check should succeed: {:?}", result);
        assert!(!result.as_ref().unwrap(), "Permission should be denied");
    }

    /// Assert cache performance metrics
    pub fn assert_cache_performance(
        hit_rate: f64,
        min_hit_rate: f64,
        avg_latency: Duration,
        max_latency: Duration,
    ) {
        assert!(
            hit_rate >= min_hit_rate,
            "Cache hit rate {}% below minimum {}%",
            hit_rate * 100.0,
            min_hit_rate * 100.0
        );
        
        assert!(
            avg_latency <= max_latency,
            "Average latency {}μs exceeds maximum {}μs",
            avg_latency.as_micros(),
            max_latency.as_micros()
        );
    }
}

/// Async test utilities
pub struct AsyncTestUtils;

impl AsyncTestUtils {
    /// Run test with timeout
    pub async fn with_timeout<F, Fut, T>(duration: Duration, operation: F) -> Result<T, RustMqError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        tokio::time::timeout(duration, operation())
            .await
            .map_err(|_| RustMqError::AuthenticationFailed("Test timeout".to_string()))
    }

    /// Create test runtime for synchronous tests
    pub fn create_test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create test runtime")
    }
}