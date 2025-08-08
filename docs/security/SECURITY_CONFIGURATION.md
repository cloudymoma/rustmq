# RustMQ Security Configuration Guide

This comprehensive guide covers all security configuration options for RustMQ, providing detailed examples and best practices for production deployments.

## Table of Contents

- [Configuration Overview](#configuration-overview)
- [TLS Configuration](#tls-configuration)
- [mTLS Authentication](#mtls-authentication)
- [ACL Configuration](#acl-configuration)
- [Certificate Management](#certificate-management)
- [Audit Configuration](#audit-configuration)
- [Performance Tuning](#performance-tuning)
- [Environment-Specific Configurations](#environment-specific-configurations)
- [Configuration Validation](#configuration-validation)
- [Troubleshooting](#troubleshooting)

## Configuration Overview

RustMQ security is configured through TOML configuration files with comprehensive security settings. The security configuration is organized into several main sections:

```toml
[security]
enabled = true

[security.tls]
# TLS/mTLS configuration

[security.acl]
# Access Control List configuration

[security.certificate_management]
# Certificate lifecycle management

[security.audit]
# Security audit and logging
```

### Configuration File Structure

```
config/
├── security-production.toml     # Production security configuration
├── security-development.toml    # Development security configuration
├── security-testing.toml        # Testing security configuration
└── certs/
    ├── ca.pem                   # Certificate Authority certificate
    ├── ca.key                   # Certificate Authority private key (secure!)
    ├── broker-cert.pem          # Broker certificate
    ├── broker-key.pem           # Broker private key
    ├── client-cert.pem          # Client certificate
    └── client-key.pem           # Client private key
```

## TLS Configuration

### Basic TLS Settings

```toml
[security.tls]
enabled = true
protocol_version = "1.3"          # Use TLS 1.3 for best security
cipher_suites = [
    "TLS_AES_256_GCM_SHA384",
    "TLS_AES_128_GCM_SHA256",
    "TLS_CHACHA20_POLY1305_SHA256"
]
compression_enabled = false        # Disable to prevent CRIME attacks
```

### Certificate Configuration

```toml
[security.tls]
# Server certificate for broker
server_cert_path = "/etc/rustmq/certs/broker-cert.pem"
server_key_path = "/etc/rustmq/certs/broker-key.pem"

# Client certificate validation
client_cert_required = true
client_ca_cert_path = "/etc/rustmq/certs/ca.pem"

# Certificate chain validation
verify_cert_chain = true
verify_hostname = true
allow_invalid_hostnames = false   # Never allow in production

# Certificate revocation checking
crl_check_enabled = true
crl_path = "/etc/rustmq/certs/ca.crl"
ocsp_check_enabled = false        # Enable if OCSP responder available
```

### Advanced TLS Settings

```toml
[security.tls]
# Session configuration
session_timeout_seconds = 3600
session_tickets_enabled = true
session_cache_size = 10000

# Security hardening
renegotiation_allowed = false
legacy_renegotiation = false
insecure_ciphers_allowed = false

# Performance tuning
tcp_nodelay = true
tcp_keepalive = true
tcp_keepalive_time = 600
tcp_keepalive_interval = 60
tcp_keepalive_probes = 3

# Buffer sizes for optimal performance
send_buffer_size = 65536
recv_buffer_size = 65536
```

## mTLS Authentication

### Client Certificate Authentication

```toml
[security.tls.client_auth]
# Require client certificates for all connections
required = true

# Certificate verification settings
verify_chain = true
verify_signature = true
verify_not_before = true
verify_not_after = true

# Certificate authority validation
ca_cert_path = "/etc/rustmq/certs/ca.pem"
ca_cert_bundle_path = "/etc/rustmq/certs/ca-bundle.pem"
intermediate_ca_allowed = true

# Certificate revocation
crl_check_enabled = true
crl_cache_ttl_seconds = 3600
ocsp_check_enabled = false
ocsp_timeout_seconds = 5

# Principal extraction
principal_extraction_mode = "subject_cn"  # Options: subject_cn, subject_email, subject_full_dn
principal_case_sensitive = false
```

### Principal Extraction Configuration

```toml
[security.tls.principal_extraction]
# Extract principal from certificate subject Common Name
mode = "subject_cn"

# Alternative modes:
# mode = "subject_email"        # Extract from email address
# mode = "subject_full_dn"      # Use full distinguished name
# mode = "subject_ou"           # Extract from organizational unit
# mode = "san_email"            # Extract from Subject Alternative Name email
# mode = "san_dns"              # Extract from Subject Alternative Name DNS

# Custom attribute extraction
custom_oid = "1.2.3.4.5.6.7.8"  # Custom OID for principal extraction
attribute_delimiter = "@"        # Delimiter for composite principals

# Principal transformation
lowercase_principal = true       # Convert to lowercase
remove_domain = false           # Remove domain from email principals
domain_suffix = "company.com"   # Add domain suffix if missing
```

### Certificate Template Validation

```toml
[security.tls.certificate_templates]
# Broker certificate requirements
[security.tls.certificate_templates.broker]
key_usage = ["digital_signature", "key_encipherment"]
extended_key_usage = ["server_auth", "client_auth"]
subject_alt_name_required = true
subject_pattern = "CN=*.internal.company.com,O=RustMQ Corp"

# Client certificate requirements
[security.tls.certificate_templates.client]
key_usage = ["digital_signature"]
extended_key_usage = ["client_auth"]
subject_pattern = "CN=*@company.com,O=RustMQ Corp"
validity_days_max = 90

# Admin certificate requirements
[security.tls.certificate_templates.admin]
key_usage = ["digital_signature"]
extended_key_usage = ["client_auth"]
subject_pattern = "CN=*@company.com,O=RustMQ Corp,OU=Administrators"
validity_days_max = 30
```

## ACL Configuration

### Basic ACL Settings

```toml
[security.acl]
enabled = true

# Cache configuration for performance
cache_size_mb = 50              # Total cache size across all levels
cache_ttl_seconds = 300         # Time-to-live for cache entries
l2_shard_count = 32            # Number of L2 cache shards for reduced contention
bloom_filter_size = 1_000_000  # Bloom filter size for negative lookups
batch_fetch_size = 100         # Batch size for controller ACL fetching

# Behavioral settings
negative_cache_enabled = true   # Cache negative authorization results
enable_audit_logging = true    # Log all ACL decisions
default_effect = "deny"        # Default to deny if no rules match
case_sensitive_matching = false # Case sensitivity for pattern matching
```

### String Interning Configuration

```toml
[security.acl.string_interning]
enabled = true                  # Enable string interning for memory efficiency
initial_capacity = 10000       # Initial capacity for string interner
load_factor = 0.75             # Load factor before resizing
max_memory_mb = 100            # Maximum memory for string interning
cleanup_interval_seconds = 3600 # Cleanup unused strings every hour
```

### ACL Source Configuration

```toml
[security.acl.sources]
# Primary ACL source (Raft consensus)
primary_source = "raft"
raft_endpoints = [
    "controller-1:9094",
    "controller-2:9094", 
    "controller-3:9094"
]

# Backup ACL source for failover
backup_source = "file"
backup_file_path = "/etc/rustmq/acl/backup-rules.json"
failover_timeout_seconds = 30

# External ACL source integration
external_source_enabled = false
external_source_url = "https://acl-service.company.com/api/v1/acl"
external_source_auth_token = "${ACL_SERVICE_TOKEN}"
external_source_timeout_seconds = 10
```

### Performance Optimization

```toml
[security.acl.performance]
# L1 Cache (Connection-local)
l1_cache_size = 100            # Entries per connection
l1_cache_ttl_seconds = 60      # Shorter TTL for connection-local cache

# L2 Cache (Broker-wide)
l2_cache_size = 10000          # Total entries across all shards
l2_cache_ttl_seconds = 300     # Standard TTL
l2_eviction_policy = "lru"     # LRU eviction policy

# L3 Bloom Filter
l3_bloom_false_positive_rate = 0.01  # 1% false positive rate
l3_bloom_hash_functions = 3          # Number of hash functions

# Batch processing
batch_processing_enabled = true
max_batch_size = 100
batch_timeout_ms = 10
max_concurrent_batches = 10

# Async processing
async_cache_updates = true
cache_update_queue_size = 1000
cache_update_worker_threads = 4
```

## Certificate Management

### Certificate Authority Configuration

```toml
[security.certificate_management]
# Root CA settings
ca_cert_path = "/etc/rustmq/certs/ca.pem"
ca_key_path = "/etc/rustmq/certs/ca.key"
ca_key_password_file = "/etc/rustmq/secrets/ca-key-password"

# Certificate validity
cert_validity_days = 365
ca_cert_validity_years = 10
auto_renew_before_expiry_days = 30

# Certificate storage
cert_storage_type = "file"     # Options: file, database, vault
cert_storage_path = "/etc/rustmq/certs"
cert_backup_enabled = true
cert_backup_path = "/backup/rustmq/certs"

# Revocation settings
crl_check_enabled = true
crl_update_interval_hours = 24
crl_distribution_url = "https://ca.company.com/crl/rustmq.crl"
ocsp_check_enabled = false
ocsp_responder_url = "https://ocsp.company.com/rustmq"
```

### Automated Certificate Operations

```toml
[security.certificate_management.automation]
# Automatic renewal
auto_renewal_enabled = true
renewal_check_interval_hours = 6
renewal_retry_attempts = 3
renewal_retry_delay_minutes = 30

# Certificate rotation
rotation_enabled = true
rotation_overlap_days = 7      # Keep old cert for 7 days during rotation
rotation_notification_enabled = true
rotation_webhook_url = "https://alerts.company.com/webhook/cert-rotation"

# Certificate monitoring
expiry_monitoring_enabled = true
expiry_warning_days = [30, 7, 1]  # Send warnings at 30, 7, and 1 day before expiry
expiry_notification_email = "security@company.com"
```

### Certificate Templates

```toml
[security.certificate_management.templates]
# Default template settings
default_key_size = 2048
default_signature_algorithm = "SHA256WithRSA"
default_curve = "P-256"        # For ECDSA certificates

# Broker template
[security.certificate_management.templates.broker]
subject_template = "CN={{hostname}}.internal.company.com,O=RustMQ Corp,OU=Brokers"
validity_days = 365
key_type = "RSA"
key_size = 2048
key_usage = ["digital_signature", "key_encipherment"]
extended_key_usage = ["server_auth", "client_auth"]
include_san_dns = true
include_san_ip = true

# Client template
[security.certificate_management.templates.client]
subject_template = "CN={{principal}},O=RustMQ Corp,OU=Clients"
validity_days = 90
key_type = "RSA"
key_size = 2048
key_usage = ["digital_signature"]
extended_key_usage = ["client_auth"]

# Admin template
[security.certificate_management.templates.admin]
subject_template = "CN={{principal}},O=RustMQ Corp,OU=Administrators"
validity_days = 30
key_type = "ECDSA"
curve = "P-256"
key_usage = ["digital_signature"]
extended_key_usage = ["client_auth"]
```

## Audit Configuration

### Audit Logging Settings

```toml
[security.audit]
enabled = true

# What to audit
log_authentication_events = true
log_authorization_events = false   # Too verbose for production
log_certificate_events = true
log_failed_attempts = true
log_admin_operations = true
log_acl_changes = true

# Audit log configuration
log_format = "json"               # Options: json, structured, text
log_level = "info"                # Options: debug, info, warn, error
log_file_path = "/var/log/rustmq/security-audit.log"
log_file_max_size_mb = 100
log_file_max_files = 10
log_file_compress = true

# Real-time audit streaming
audit_streaming_enabled = false
audit_stream_endpoint = "https://siem.company.com/api/v1/events"
audit_stream_auth_token = "${SIEM_AUTH_TOKEN}"
audit_stream_batch_size = 100
audit_stream_flush_interval_seconds = 30
```

### Audit Event Configuration

```toml
[security.audit.events]
# Authentication events
authentication_success = true
authentication_failure = true
certificate_validation_failure = true
principal_extraction_failure = true

# Authorization events
authorization_success = false     # Too verbose for production
authorization_failure = true
acl_cache_miss = false           # Only for debugging
acl_rule_evaluation = false      # Only for debugging

# Certificate management events
certificate_issued = true
certificate_renewed = true
certificate_revoked = true
certificate_expired = true
ca_operations = true

# Administrative events
admin_login = true
admin_logout = true
acl_rule_created = true
acl_rule_modified = true
acl_rule_deleted = true
configuration_changed = true
```

### Audit Data Retention

```toml
[security.audit.retention]
# Local retention
local_retention_days = 90
local_compression_enabled = true
local_encryption_enabled = true
local_encryption_key_file = "/etc/rustmq/secrets/audit-encryption-key"

# Remote archival
remote_archival_enabled = true
remote_storage_type = "s3"       # Options: s3, gcs, azure
remote_bucket = "company-security-audit-logs"
remote_prefix = "rustmq/"
remote_compression = "gzip"
remote_encryption = "AES256"

# Data lifecycle
archive_after_days = 30
delete_after_days = 2555         # 7 years for compliance
compliance_mode = "SEC_RULE_17a4" # Compliance standard
```

## Performance Tuning

### Memory Optimization

```toml
[security.performance.memory]
# String interning
string_interning_enabled = true
string_interner_initial_capacity = 10000
string_interner_load_factor = 0.75
string_interner_max_memory_mb = 100

# Object pooling
object_pooling_enabled = true
certificate_pool_size = 1000
acl_rule_pool_size = 5000
principal_pool_size = 10000

# Garbage collection hints
gc_pressure_threshold = 0.8
gc_cleanup_interval_seconds = 300
memory_pressure_monitoring = true
```

### CPU Optimization

```toml
[security.performance.cpu]
# Threading configuration
auth_worker_threads = 4
acl_worker_threads = 8
cert_validation_threads = 2

# Algorithm selection
hash_algorithm = "SHA256"        # Options: SHA256, SHA3-256, BLAKE2b
signature_verification_algorithm = "RSA_PSS" # Options: RSA_PKCS1, RSA_PSS, ECDSA

# SIMD optimization
simd_enabled = true              # Enable SIMD instructions where available
pattern_matching_algorithm = "aho_corasick" # Fast pattern matching

# Cache optimization
cpu_cache_line_size = 64         # CPU cache line size for alignment
numa_awareness = true            # NUMA-aware thread affinity
```

### Network Optimization

```toml
[security.performance.network]
# Connection pooling
connection_pool_size = 100
connection_pool_idle_timeout_seconds = 300
connection_pool_max_lifetime_seconds = 3600

# TLS optimization
tls_session_cache_size = 10000
tls_session_timeout_seconds = 3600
tls_ticket_rotation_hours = 24

# Batch operations
batch_cert_validation = true
batch_acl_requests = true
max_batch_size = 100
batch_timeout_ms = 10
```

## Environment-Specific Configurations

### Production Configuration

```toml
# config/security-production.toml
[security]
enabled = true

[security.tls]
enabled = true
protocol_version = "1.3"
client_cert_required = true
verify_cert_chain = true
verify_hostname = true
allow_invalid_hostnames = false
crl_check_enabled = true
ocsp_check_enabled = true

[security.acl]
enabled = true
cache_size_mb = 100
cache_ttl_seconds = 300
negative_cache_enabled = true
enable_audit_logging = true
default_effect = "deny"

[security.certificate_management]
auto_renewal_enabled = true
rotation_enabled = true
expiry_monitoring_enabled = true
cert_validity_days = 365
auto_renew_before_expiry_days = 30

[security.audit]
enabled = true
log_authentication_events = true
log_authorization_events = false
log_certificate_events = true
log_failed_attempts = true
log_admin_operations = true
local_retention_days = 365
remote_archival_enabled = true
```

### Development Configuration

```toml
# config/security-development.toml
[security]
enabled = true

[security.tls]
enabled = true
protocol_version = "1.2"         # Allow older TLS for compatibility
client_cert_required = true
verify_cert_chain = true
verify_hostname = false          # Allow self-signed certificates
allow_invalid_hostnames = true   # Allow localhost certificates
crl_check_enabled = false
ocsp_check_enabled = false

[security.acl]
enabled = true
cache_size_mb = 10              # Smaller cache for development
cache_ttl_seconds = 60
negative_cache_enabled = false  # Disable for easier debugging
enable_audit_logging = true
default_effect = "allow"        # More permissive for development

[security.certificate_management]
auto_renewal_enabled = false    # Manual renewal for development
rotation_enabled = false
expiry_monitoring_enabled = false
cert_validity_days = 30        # Shorter validity for testing
auto_renew_before_expiry_days = 7

[security.audit]
enabled = true
log_authentication_events = true
log_authorization_events = true  # More verbose for debugging
log_certificate_events = true
log_failed_attempts = true
log_admin_operations = true
local_retention_days = 7
remote_archival_enabled = false
```

### Testing Configuration

```toml
# config/security-testing.toml
[security]
enabled = true

[security.tls]
enabled = true
protocol_version = "1.2"
client_cert_required = false    # Allow insecure connections for testing
verify_cert_chain = false
verify_hostname = false
allow_invalid_hostnames = true
crl_check_enabled = false
ocsp_check_enabled = false

[security.acl]
enabled = false                 # Disable ACL for integration tests
cache_size_mb = 1
cache_ttl_seconds = 10
negative_cache_enabled = false
enable_audit_logging = false
default_effect = "allow"

[security.certificate_management]
auto_renewal_enabled = false
rotation_enabled = false
expiry_monitoring_enabled = false
cert_validity_days = 1         # Short validity for testing
auto_renew_before_expiry_days = 0

[security.audit]
enabled = false                # Disable audit for testing
local_retention_days = 1
remote_archival_enabled = false
```

## Configuration Validation

### Automatic Validation

RustMQ automatically validates security configuration on startup:

```bash
# Validate configuration
rustmq-admin config validate --config security-production.toml

# Check certificate paths and permissions
rustmq-admin config check-certs --config security-production.toml

# Validate ACL configuration
rustmq-admin config check-acl --config security-production.toml

# Test TLS configuration
rustmq-admin config test-tls --config security-production.toml
```

### Configuration Validation Rules

1. **TLS Configuration**
   - Certificate files must exist and be readable
   - Private keys must have restricted permissions (600)
   - Certificate chain must be valid
   - Certificate must not be expired

2. **ACL Configuration**
   - Cache sizes must be reasonable (> 0, < system memory)
   - TTL values must be positive
   - Controller endpoints must be reachable

3. **Certificate Management**
   - CA certificates must be valid
   - Template configurations must be syntactically correct
   - Validity periods must be reasonable

4. **Audit Configuration**
   - Log paths must be writable
   - Retention periods must be positive
   - Remote storage credentials must be valid

### Manual Validation Commands

```bash
# Check certificate validity
openssl x509 -in /etc/rustmq/certs/broker-cert.pem -text -noout

# Verify certificate chain
openssl verify -CAfile /etc/rustmq/certs/ca.pem /etc/rustmq/certs/broker-cert.pem

# Test TLS connection
openssl s_client -connect broker:9092 \
  -cert /etc/rustmq/certs/client-cert.pem \
  -key /etc/rustmq/certs/client-key.pem \
  -CAfile /etc/rustmq/certs/ca.pem

# Check certificate expiration
openssl x509 -in /etc/rustmq/certs/broker-cert.pem -checkend 2592000  # 30 days
```

## Troubleshooting

### Common Configuration Issues

#### 1. Certificate Path Issues
```bash
# Problem: Certificate file not found
# Error: Failed to load certificate: No such file or directory

# Solution: Check file paths and permissions
ls -la /etc/rustmq/certs/
chmod 644 /etc/rustmq/certs/*.pem
chmod 600 /etc/rustmq/certs/*.key
```

#### 2. Certificate Validation Failures
```bash
# Problem: Certificate chain validation failed
# Error: certificate verify failed: unable to get local issuer certificate

# Solution: Ensure CA certificate is properly configured
rustmq-admin certs validate --cert-file /etc/rustmq/certs/broker-cert.pem
```

#### 3. ACL Cache Performance Issues
```bash
# Problem: High ACL lookup latency
# Error: Authorization taking > 1ms consistently

# Solution: Tune cache configuration
[security.acl]
cache_size_mb = 100  # Increase cache size
l2_shard_count = 64  # Increase sharding for high concurrency
```

#### 4. Audit Log Issues
```bash
# Problem: Audit logs not being written
# Error: Failed to write audit log: Permission denied

# Solution: Check log directory permissions
mkdir -p /var/log/rustmq
chown rustmq:rustmq /var/log/rustmq
chmod 755 /var/log/rustmq
```

### Configuration Testing

```bash
# Test complete security configuration
rustmq-admin security test-config --config security-production.toml

# Test specific components
rustmq-admin security test-tls
rustmq-admin security test-acl
rustmq-admin security test-audit

# Performance testing
rustmq-admin security benchmark-auth --duration 60s --concurrency 100
```

### Environment Variables

Security configuration can be overridden with environment variables:

```bash
# TLS settings
export RUSTMQ_TLS_ENABLED=true
export RUSTMQ_TLS_CERT_PATH=/etc/rustmq/certs/broker-cert.pem
export RUSTMQ_TLS_KEY_PATH=/etc/rustmq/certs/broker-key.pem
export RUSTMQ_TLS_CA_PATH=/etc/rustmq/certs/ca.pem

# ACL settings
export RUSTMQ_ACL_ENABLED=true
export RUSTMQ_ACL_CACHE_SIZE_MB=100
export RUSTMQ_ACL_TTL_SECONDS=300

# Certificate management
export RUSTMQ_CERT_AUTO_RENEWAL=true
export RUSTMQ_CERT_VALIDITY_DAYS=365

# Audit settings
export RUSTMQ_AUDIT_ENABLED=true
export RUSTMQ_AUDIT_LOG_PATH=/var/log/rustmq/security-audit.log
```

This configuration guide provides comprehensive coverage of all security settings in RustMQ. For specific deployment scenarios, refer to the environment-specific configuration examples and adjust settings based on your security requirements and performance needs.