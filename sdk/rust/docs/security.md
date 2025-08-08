# RustMQ Rust SDK Security Guide

This guide covers the comprehensive security features available in the RustMQ Rust SDK, including authentication, authorization, and secure communication protocols.

## Table of Contents

- [Overview](#overview)
- [TLS Configuration](#tls-configuration)
- [Authentication Methods](#authentication-methods)
- [Authorization and ACL](#authorization-and-acl)
- [Certificate Management](#certificate-management)
- [Security Best Practices](#security-best-practices)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)

## Overview

The RustMQ Rust SDK provides enterprise-grade security features including:

- **Transport Security**: TLS 1.2/1.3 with QUIC protocol support
- **Authentication**: mTLS, JWT tokens, SASL mechanisms
- **Authorization**: Fine-grained ACL (Access Control List) system
- **Certificate Management**: Automatic certificate validation and renewal
- **Principal Extraction**: Identity extraction from certificates
- **Security Metrics**: Comprehensive monitoring and auditing

## TLS Configuration

### Basic TLS Setup

```rust
use rustmq_client::{ClientConfig, TlsConfig, TlsMode};

// Server authentication only (recommended for development)
let tls_config = TlsConfig {
    mode: TlsMode::ServerAuth,
    ca_cert: Some("/path/to/ca.pem".to_string()),
    server_name: Some("rustmq.example.com".to_string()),
    insecure_skip_verify: false,
    ..Default::default()
};

let client_config = ClientConfig {
    brokers: vec!["rustmq.example.com:9092".to_string()],
    tls_config: Some(tls_config),
    ..Default::default()
};
```

### Mutual TLS (mTLS) Setup

```rust
use rustmq_client::TlsConfig;

// Production-ready mTLS configuration
let tls_config = TlsConfig::secure_config(
    // CA certificate for server verification
    std::fs::read_to_string("/path/to/ca.pem")?,
    
    // Client certificate for authentication
    Some(std::fs::read_to_string("/path/to/client.pem")?),
    
    // Client private key
    Some(std::fs::read_to_string("/path/to/client.key")?),
    
    // Server name for SNI
    "rustmq.example.com".to_string(),
);

// Validate configuration
tls_config.validate()?;
```

### TLS Configuration Options

```rust
use rustmq_client::{TlsConfig, CertificateValidationConfig};

let tls_config = TlsConfig {
    mode: TlsMode::MutualAuth,
    ca_cert: Some("...".to_string()),
    client_cert: Some("...".to_string()),
    client_key: Some("...".to_string()),
    server_name: Some("rustmq.example.com".to_string()),
    
    // Security settings
    insecure_skip_verify: false,
    supported_versions: vec!["TLSv1.3".to_string()],
    alpn_protocols: vec!["h3".to_string()],
    
    // Certificate validation
    validation: CertificateValidationConfig {
        verify_chain: true,
        verify_expiration: true,
        check_revocation: true,
        allow_self_signed: false,
        max_chain_depth: 5,
    },
};
```

## Authentication Methods

### mTLS Authentication

```rust
use rustmq_client::{AuthConfig, AuthMethod};

let auth_config = AuthConfig::mtls_config(
    "/path/to/ca.pem".to_string(),
    "/path/to/client.pem".to_string(),
    "/path/to/client.key".to_string(),
    Some("rustmq.example.com".to_string()),
);

assert_eq!(auth_config.method, AuthMethod::Mtls);
assert!(auth_config.requires_tls());
assert!(auth_config.supports_acl());
```

### JWT Token Authentication

```rust
use rustmq_client::AuthConfig;

let auth_config = AuthConfig::token_config(
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...".to_string()
);

// Configure TLS for token transmission
let tls_config = TlsConfig::secure_config(
    ca_cert,
    None, // No client certificate needed
    None,
    "rustmq.example.com".to_string(),
);
```

### SASL Authentication

```rust
use rustmq_client::AuthConfig;

let auth_config = AuthConfig::sasl_plain_config(
    "username".to_string(),
    "password".to_string(),
);

// SASL requires TLS for security
let tls_config = TlsConfig::secure_config(/* ... */);
```

## Authorization and ACL

### Security Configuration

```rust
use rustmq_client::{
    SecurityConfig, PrincipalExtractionConfig, 
    AclClientConfig, CertificateClientConfig
};

let security_config = SecurityConfig {
    enabled: true,
    
    // Principal extraction from certificates
    principal_extraction: PrincipalExtractionConfig {
        use_common_name: true,
        use_subject_alt_name: false,
        custom_patterns: vec![], 
        normalize: true,
    },
    
    // ACL client configuration
    acl: AclClientConfig {
        enabled: true,
        cache_size: 1000,
        cache_ttl_seconds: 300,
        batch_requests: true,
        fail_open: false, // Fail secure
    },
    
    // Certificate management
    certificate_management: CertificateClientConfig {
        auto_renew: true,
        renew_before_expiry_days: 30,
        cert_storage_path: Some("/etc/rustmq/certs".to_string()),
        cert_template: None,
    },
};

let auth_config = AuthConfig {
    method: AuthMethod::Mtls,
    security: Some(security_config),
    ..Default::default()
};
```

### Working with Permissions

```rust
use rustmq_client::{RustMqClient, PermissionSet};

let client = RustMqClient::new(client_config).await?;

if let Some(connection) = client.get_connection().await {
    if let Some(security_context) = connection.get_security_context().await {
        let permissions = &security_context.permissions;
        
        // Check topic permissions
        if permissions.can_read_topic("logs.application") {
            println!("Can read from logs.application topic");
        }
        
        if permissions.can_write_topic("events.user") {
            println!("Can write to events.user topic");
        }
        
        if permissions.can_admin("cluster.health") {
            println!("Can perform cluster health operations");
        }
    }
}
```

## Certificate Management

### Certificate Information

```rust
use rustmq_client::CertificateInfo;

if let Some(security_context) = connection.get_security_context().await {
    if let Some(cert_info) = &security_context.certificate_info {
        println!("Certificate subject: {}", cert_info.subject);
        println!("Certificate issuer: {}", cert_info.issuer);
        println!("Certificate serial: {}", cert_info.serial_number);
        println!("Valid from: {} to: {}", cert_info.not_before, cert_info.not_after);
        
        // Access certificate attributes
        if let Some(org) = cert_info.attributes.get("O") {
            println!("Organization: {}", org);
        }
        
        if let Some(cn) = cert_info.attributes.get("CN") {
            println!("Common Name: {}", cn);
        }
    }
}
```

### Security Context Management

```rust
// Get current security context
if let Some(context) = connection.get_security_context().await {
    println!("Principal: {}", context.principal);
    println!("Authenticated at: {:?}", context.auth_time);
    
    // Check if context is still valid
    if context.auth_time.elapsed() > Duration::from_hours(1) {
        println!("Security context is getting old, refreshing...");
        connection.refresh_security_context().await?;
    }
}

// Manually refresh security context
connection.refresh_security_context().await?;
```

## Security Best Practices

### 1. Certificate Management

```rust
// Use strong certificate validation in production
let tls_config = TlsConfig {
    validation: CertificateValidationConfig {
        verify_chain: true,
        verify_expiration: true,
        check_revocation: true,
        allow_self_signed: false,
        max_chain_depth: 5,
    },
    supported_versions: vec!["TLSv1.3".to_string()], // Only TLS 1.3
    ..secure_config
};
```

### 2. Principal and Permission Validation

```rust
// Always validate permissions before operations
async fn send_message_securely(
    producer: &Producer,
    topic: &str,
    message: Message,
    connection: &Connection,
) -> Result<()> {
    // Check authorization
    if let Some(context) = connection.get_security_context().await {
        if !context.permissions.can_write_topic(topic) {
            return Err(ClientError::AuthorizationDenied(
                format!("No write permission for topic: {}", topic)
            ));
        }
    }
    
    // Send message
    producer.send(message).await?;
    Ok(())
}
```

### 3. Error Handling

```rust
use rustmq_client::ClientError;

match producer.send(message).await {
    Ok(metadata) => {
        println!("Message sent: {:?}", metadata);
    }
    Err(ClientError::AuthorizationDenied(msg)) => {
        eprintln!("Access denied: {}", msg);
        // Maybe refresh permissions or request new token
    }
    Err(ClientError::InvalidCertificate { reason }) => {
        eprintln!("Certificate error: {}", reason);
        // Maybe renew certificate
    }
    Err(ClientError::Authentication(msg)) => {
        eprintln!("Authentication failed: {}", msg);
        // Maybe re-authenticate
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

### 4. Security Metrics Monitoring

```rust
use rustmq_client::SecurityManager;

let security_manager = SecurityManager::new(security_config)?;
let metrics = security_manager.metrics();

// Monitor authentication attempts
let auth_stats = metrics.get_auth_stats();
for (method, (attempts, successes)) in auth_stats {
    let success_rate = successes as f64 / attempts as f64 * 100.0;
    println!("Auth method {}: {:.1}% success rate", method, success_rate);
}

// Monitor ACL cache efficiency
let (hits, misses) = metrics.get_acl_cache_stats();
let hit_rate = hits as f64 / (hits + misses) as f64 * 100.0;
println!("ACL cache hit rate: {:.1}%", hit_rate);
```

## Examples

### Complete Secure Client Setup

```rust
use rustmq_client::{ClientConfig, TlsConfig, AuthConfig, RustMqClient};

async fn create_secure_client() -> Result<RustMqClient> {
    // Load certificates
    let ca_cert = std::fs::read_to_string("/etc/rustmq/ca.pem")?;
    let client_cert = std::fs::read_to_string("/etc/rustmq/client.pem")?;
    let client_key = std::fs::read_to_string("/etc/rustmq/client.key")?;
    
    // Configure mTLS
    let tls_config = TlsConfig::secure_config(
        ca_cert.clone(),
        Some(client_cert.clone()),
        Some(client_key.clone()),
        "rustmq.example.com".to_string(),
    );
    
    // Configure authentication
    let auth_config = AuthConfig::mtls_config(
        ca_cert,
        client_cert,
        client_key,
        Some("rustmq.example.com".to_string()),
    );
    
    // Create client configuration
    let client_config = ClientConfig {
        brokers: vec!["rustmq.example.com:9092".to_string()],
        client_id: Some("secure-client".to_string()),
        tls_config: Some(tls_config),
        auth: Some(auth_config),
        ..Default::default()
    };
    
    // Create client
    let client = RustMqClient::new(client_config).await?;
    
    // Verify security
    if let Some(connection) = client.get_connection().await {
        assert!(connection.is_security_enabled());
        println!("✓ Secure client created successfully");
    }
    
    Ok(client)
}
```

### Pattern-Based Permissions

```rust
use rustmq_client::PermissionSet;

let mut permissions = PermissionSet::default();

// Allow reading from all log topics
permissions.read_topics.push("logs.*".to_string());

// Allow writing to user events only
permissions.write_topics.push("events.user.*".to_string());

// Allow specific admin operations
permissions.admin_operations.push("health.*".to_string());
permissions.admin_operations.push("metrics.read".to_string());

// Test permissions
assert!(permissions.can_read_topic("logs.application"));
assert!(permissions.can_read_topic("logs.security"));
assert!(!permissions.can_read_topic("secrets.database"));

assert!(permissions.can_write_topic("events.user.login"));
assert!(!permissions.can_write_topic("events.system.alert"));

assert!(permissions.can_admin("health.check"));
assert!(!permissions.can_admin("cluster.shutdown"));
```

## Troubleshooting

### Common Issues

#### 1. Certificate Validation Errors

```rust
// Problem: Certificate verification fails
let error = ClientError::InvalidCertificate { 
    reason: "certificate has expired".to_string() 
};

// Solutions:
// - Check certificate expiration dates
// - Verify CA certificate is correct
// - Ensure certificate chain is complete
// - Check system time synchronization
```

#### 2. Permission Denied Errors

```rust
// Problem: ACL authorization fails
let error = ClientError::AuthorizationDenied(
    "No write permission for topic: sensitive-data".to_string()
);

// Solutions:
// - Verify ACL rules on the server
// - Check principal extraction from certificate
// - Refresh security context
// - Contact administrator for permission updates
```

#### 3. Authentication Failures

```rust
// Problem: mTLS handshake fails
let error = ClientError::Authentication(
    "TLS handshake failed".to_string()
);

// Solutions:
// - Verify client certificate and key match
// - Check certificate is trusted by server CA
// - Ensure TLS versions are compatible
// - Verify ALPN protocol negotiation
```

### Debug Configuration

```rust
use rustmq_client::{TlsConfig, CertificateValidationConfig};

// Development configuration for debugging
let debug_tls_config = TlsConfig {
    mode: TlsMode::ServerAuth,
    ca_cert: Some("...".to_string()),
    insecure_skip_verify: true, // ⚠️ Only for debugging
    validation: CertificateValidationConfig {
        verify_chain: false,
        verify_expiration: false,
        check_revocation: false,
        allow_self_signed: true,
        max_chain_depth: 10,
    },
    ..Default::default()
};

// ⚠️ Never use insecure configuration in production!
```

### Logging and Monitoring

```rust
// Enable debug logging
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

// Monitor security events
if let Some(connection) = client.get_connection().await {
    if let Some(context) = connection.get_security_context().await {
        log::info!("Authenticated as: {}", context.principal);
        log::debug!("Permissions: {:?}", context.permissions);
        
        if let Some(cert_info) = &context.certificate_info {
            log::debug!("Certificate fingerprint: {}", cert_info.fingerprint);
        }
    }
}
```

## Security Considerations

1. **Certificate Security**: Store private keys securely, use proper file permissions (0600)
2. **Token Management**: Rotate JWT tokens regularly, use short expiration times
3. **Network Security**: Use TLS 1.3 when possible, disable weak cipher suites
4. **Access Control**: Follow principle of least privilege, regularly audit permissions
5. **Monitoring**: Log security events, monitor failed authentication attempts
6. **Updates**: Keep certificates up to date, monitor for security patches

For more information on RustMQ security architecture, see the [main security documentation](../../docs/security/).