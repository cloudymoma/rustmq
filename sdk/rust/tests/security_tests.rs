//! Security integration tests for RustMQ Rust SDK

use rustmq_client::{
    ClientConfig, TlsConfig, TlsMode, AuthConfig, AuthMethod, SecurityConfig,
    SecurityManager, SecurityContext, CertificateValidationConfig,
    PrincipalExtractionConfig, AclClientConfig, CertificateClientConfig,
    ClientError, Connection,
};
use std::collections::HashMap;
use tokio;

#[tokio::test]
async fn test_tls_config_validation() {
    // Test disabled TLS
    let mut config = TlsConfig::default();
    assert_eq!(config.mode, TlsMode::Disabled);
    assert!(config.validate().is_ok());

    // Test server auth TLS
    config.mode = TlsMode::ServerAuth;
    config.ca_cert = Some("test-ca".to_string());
    assert!(config.validate().is_ok());

    // Test mutual TLS without certificates (should fail)
    config.mode = TlsMode::MutualAuth;
    assert!(config.validate().is_err());

    // Test mutual TLS with certificates
    config.client_cert = Some("test-cert".to_string());
    config.client_key = Some("test-key".to_string());
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_auth_config_builders() {
    // Test mTLS authentication config
    let mtls_config = AuthConfig::mtls_config(
        "ca-cert".to_string(),
        "client-cert".to_string(),
        "client-key".to_string(),
        Some("server.example.com".to_string()),
    );
    
    assert_eq!(mtls_config.method, AuthMethod::Mtls);
    assert!(mtls_config.requires_tls());
    assert!(mtls_config.supports_acl());
    assert!(mtls_config.security.is_some());

    // Test token authentication config
    let token_config = AuthConfig::token_config("jwt-token".to_string());
    
    assert_eq!(token_config.method, AuthMethod::Token);
    assert_eq!(token_config.token, Some("jwt-token".to_string()));
    assert!(token_config.requires_tls());

    // Test SASL PLAIN authentication config
    let sasl_config = AuthConfig::sasl_plain_config(
        "username".to_string(),
        "password".to_string(),
    );
    
    assert_eq!(sasl_config.method, AuthMethod::SaslPlain);
    assert_eq!(sasl_config.username, Some("username".to_string()));
    assert_eq!(sasl_config.password, Some("password".to_string()));
    assert!(sasl_config.requires_tls());
}

#[tokio::test]
async fn test_tls_config_builders() {
    // Test secure production config
    let secure_config = TlsConfig::secure_config(
        "ca-cert".to_string(),
        Some("client-cert".to_string()),
        Some("client-key".to_string()),
        "server.example.com".to_string(),
    );
    
    assert_eq!(secure_config.mode, TlsMode::MutualAuth);
    assert!(secure_config.is_enabled());
    assert!(secure_config.requires_client_cert());
    assert!(!secure_config.insecure_skip_verify);
    assert!(secure_config.validation.verify_chain);
    assert!(secure_config.validation.check_revocation);
    assert!(secure_config.validate().is_ok());

    // Test insecure development config
    let insecure_config = TlsConfig::insecure_config();
    
    assert_eq!(insecure_config.mode, TlsMode::ServerAuth);
    assert!(insecure_config.is_enabled());
    assert!(!insecure_config.requires_client_cert());
    assert!(insecure_config.insecure_skip_verify);
    assert!(!insecure_config.validation.verify_chain);
    assert!(insecure_config.validation.allow_self_signed);
}

#[tokio::test]
async fn test_security_manager_creation() {
    let security_config = SecurityConfig {
        enabled: true,
        principal_extraction: PrincipalExtractionConfig {
            use_common_name: true,
            use_subject_alt_name: false,
            custom_patterns: vec![],
            normalize: true,
        },
        acl: AclClientConfig {
            enabled: true,
            cache_size: 1000,
            cache_ttl_seconds: 300,
            batch_requests: true,
            fail_open: false,
        },
        certificate_management: CertificateClientConfig::default(),
    };

    let security_manager = SecurityManager::new(security_config);
    assert!(security_manager.is_ok());

    let manager = security_manager.unwrap();
    let metrics = manager.metrics();
    
    // Check initial metrics
    let auth_stats = metrics.get_auth_stats();
    assert!(auth_stats.is_empty());
    
    let (cache_hits, cache_misses) = metrics.get_acl_cache_stats();
    assert_eq!(cache_hits, 0);
    assert_eq!(cache_misses, 0);
}

#[tokio::test]
async fn test_certificate_info_extraction() {
    // This test would require actual certificate data
    // For now, we'll test the structure
    let cert_info = rustmq_client::CertificateInfo {
        subject: "CN=test-user,O=Example Corp".to_string(),
        issuer: "CN=CA,O=Example Corp".to_string(),
        serial_number: "123456".to_string(),
        not_before: 1640995200, // 2022-01-01
        not_after: 1672531200,  // 2023-01-01
        fingerprint: "abcdef123456".to_string(),
        attributes: {
            let mut attrs = HashMap::new();
            attrs.insert("CN".to_string(), "test-user".to_string());
            attrs.insert("O".to_string(), "Example Corp".to_string());
            attrs
        },
    };

    assert_eq!(cert_info.subject, "CN=test-user,O=Example Corp");
    assert_eq!(cert_info.attributes.get("CN"), Some(&"test-user".to_string()));
    assert_eq!(cert_info.attributes.get("O"), Some(&"Example Corp".to_string()));
}

#[tokio::test]
async fn test_permission_set_operations() {
    let mut permissions = rustmq_client::PermissionSet::default();
    
    // Add some permissions
    permissions.read_topics.push("logs.*".to_string());
    permissions.write_topics.push("events.*".to_string());
    permissions.admin_operations.push("cluster.*".to_string());
    
    // Test read permissions
    assert!(permissions.can_read_topic("logs.info"));
    assert!(permissions.can_read_topic("logs.error"));
    assert!(!permissions.can_read_topic("events.user"));
    
    // Test write permissions
    assert!(permissions.can_write_topic("events.user"));
    assert!(permissions.can_write_topic("events.system"));
    assert!(!permissions.can_write_topic("logs.info"));
    
    // Test admin permissions
    assert!(permissions.can_admin("cluster.health"));
    assert!(permissions.can_admin("cluster.scale"));
    assert!(!permissions.can_admin("user.create"));
}

#[tokio::test]
async fn test_client_config_with_security() {
    let mut client_config = ClientConfig::default();
    
    // Add TLS configuration
    client_config.tls_config = Some(TlsConfig::secure_config(
        "ca-cert".to_string(),
        Some("client-cert".to_string()),
        Some("client-key".to_string()),
        "localhost".to_string(),
    ));
    
    // Add authentication configuration
    client_config.auth = Some(AuthConfig::mtls_config(
        "ca-cert".to_string(),
        "client-cert".to_string(),
        "client-key".to_string(),
        Some("localhost".to_string()),
    ));
    
    // Verify configuration
    assert!(client_config.tls_config.is_some());
    assert!(client_config.auth.is_some());
    
    let tls_config = client_config.tls_config.as_ref().unwrap();
    assert!(tls_config.is_enabled());
    assert!(tls_config.requires_client_cert());
    
    let auth_config = client_config.auth.as_ref().unwrap();
    assert_eq!(auth_config.method, AuthMethod::Mtls);
    assert!(auth_config.requires_tls());
    assert!(auth_config.supports_acl());
}

#[tokio::test]
async fn test_error_categorization() {
    // Test security-related errors
    let auth_error = ClientError::AuthorizationDenied("Access denied".to_string());
    assert_eq!(auth_error.category(), "authorization");
    assert!(!auth_error.is_retryable());
    
    let cert_error = ClientError::InvalidCertificate {
        reason: "Expired certificate".to_string(),
    };
    assert_eq!(cert_error.category(), "certificate");
    assert!(!cert_error.is_retryable());
    
    let principal_error = ClientError::PrincipalExtraction("No CN found".to_string());
    assert_eq!(principal_error.category(), "principal");
    assert!(!principal_error.is_retryable());
    
    let unsupported_auth = ClientError::UnsupportedAuthMethod {
        method: "custom".to_string(),
    };
    assert_eq!(unsupported_auth.category(), "auth_method");
    assert!(!unsupported_auth.is_retryable());
}

#[tokio::test]
async fn test_connection_security_methods() {
    // This test would require a running broker for full integration
    // For now, we'll test the configuration and error handling
    
    let client_config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        tls_config: Some(TlsConfig::default()),
        auth: Some(AuthConfig::default()),
        ..Default::default()
    };
    
    // This would fail in practice without a running broker, but we can test config validation
    match Connection::new(&client_config).await {
        Ok(connection) => {
            // If this succeeds (unlikely without a broker), test security methods
            assert!(!connection.is_security_enabled()); // Default auth is None
            assert!(connection.get_security_context().await.is_none());
        }
        Err(err) => {
            // Expected to fail without a running broker
            assert!(matches!(err, ClientError::Connection(_) | ClientError::NoConnectionsAvailable));
        }
    }
}

#[test]
fn test_pattern_matching() {
    // Test the pattern matching helper function used in permissions
    use rustmq_client::security::matches_pattern;
    
    // Wildcard patterns
    assert!(matches_pattern("any-topic", "*"));
    assert!(matches_pattern("logs.info", "logs.*"));
    assert!(matches_pattern("user.admin", "*.admin"));
    
    // Exact matches
    assert!(matches_pattern("exact-topic", "exact-topic"));
    
    // Non-matches
    assert!(!matches_pattern("logs.info", "events.*"));
    assert!(!matches_pattern("user.admin", "logs.*"));
    assert!(!matches_pattern("wrong", "exact-topic"));
}

#[tokio::test]
async fn test_security_metrics() {
    let metrics = rustmq_client::SecurityMetrics::default();
    
    // Record some authentication attempts
    metrics.record_authentication("mtls", true);
    metrics.record_authentication("mtls", false);
    metrics.record_authentication("token", true);
    
    // Record some cache operations
    metrics.record_acl_cache_hit();
    metrics.record_acl_cache_hit();
    metrics.record_acl_cache_miss();
    
    // Check statistics
    let auth_stats = metrics.get_auth_stats();
    assert_eq!(auth_stats.get("mtls"), Some(&(2, 1))); // 2 attempts, 1 success
    assert_eq!(auth_stats.get("token"), Some(&(1, 1))); // 1 attempt, 1 success
    
    let (cache_hits, cache_misses) = metrics.get_acl_cache_stats();
    assert_eq!(cache_hits, 2);
    assert_eq!(cache_misses, 1);
}

#[tokio::test]
async fn test_configuration_defaults() {
    // Test that default configurations are sensible
    let tls_config = TlsConfig::default();
    assert_eq!(tls_config.mode, TlsMode::Disabled);
    assert!(!tls_config.is_enabled());
    assert!(!tls_config.requires_client_cert());
    
    let auth_config = AuthConfig::default();
    assert_eq!(auth_config.method, AuthMethod::None);
    assert!(!auth_config.requires_tls());
    assert!(!auth_config.supports_acl());
    
    let security_config = SecurityConfig::default();
    assert!(!security_config.enabled);
    assert!(security_config.principal_extraction.use_common_name);
    assert!(security_config.acl.enabled);
    assert!(!security_config.certificate_management.auto_renew);
    
    let client_config = ClientConfig::default();
    assert!(!client_config.enable_tls); // Deprecated field
    assert!(client_config.tls_config.is_some());
    assert!(client_config.auth.is_none());
}