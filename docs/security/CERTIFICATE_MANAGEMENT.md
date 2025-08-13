# RustMQ Certificate Management Guide

This comprehensive guide covers certificate lifecycle management in RustMQ, including Certificate Authority (CA) operations, certificate issuance, renewal, rotation, and revocation procedures.

## Table of Contents

- [Overview](#overview)
- [Certificate Authority Management](#certificate-authority-management)
- [Certificate Lifecycle](#certificate-lifecycle)
- [Certificate Templates](#certificate-templates)
- [Automated Operations](#automated-operations)
- [Certificate Monitoring](#certificate-monitoring)
- [Security Best Practices](#security-best-practices)
- [Troubleshooting](#troubleshooting)
- [Advanced Topics](#advanced-topics)

## Overview

RustMQ provides comprehensive certificate management capabilities designed for enterprise environments. **Following recent security infrastructure improvements**, the certificate management system now features proper certificate signing chains and validated X.509 certificate operations.

### Recent Improvements (August 2025)
✅ **Fixed Certificate Signing**: Resolved critical certificate signing issue where all certificates were being created as self-signed instead of properly signed by their issuing CA
✅ **Production-Ready X.509 Implementation**: Complete certificate chain validation with proper issuer-to-subject relationships
✅ **Security Test Validation**: All 120+ security tests now pass, validating the certificate infrastructure

The system supports:

- **Complete CA Operations**: Root and intermediate CA management with proper certificate signing chains
- **Automated Lifecycle**: Certificate issuance, renewal, and revocation with validated signing
- **Role-Based Templates**: Different certificate types for various use cases
- **Performance Optimization**: Certificate caching and validation optimization  
- **Audit Trail**: Complete audit logging of all certificate operations
- **Proper X.509 Certificate Chains**: End-entity certificates correctly signed by their issuing CA

### Certificate Types

RustMQ uses different certificate types for different components:

1. **Root CA Certificate**: Self-signed certificate for the organization
2. **Intermediate CA Certificates**: Delegated CAs for different environments
3. **Broker Certificates**: Server certificates for RustMQ brokers
4. **Client Certificates**: Client authentication certificates
5. **Admin Certificates**: Administrative access certificates

## Certificate Authority Management

### Initializing a Root CA

The first step in setting up RustMQ security is creating a root Certificate Authority:

```bash
# Initialize a new root CA
rustmq-admin ca init \
  --cn "RustMQ Root CA" \
  --org "Your Organization" \
  --country "US" \
  --state "California" \
  --city "San Francisco" \
  --validity-years 10 \
  --key-size 4096 \
  --output-dir /etc/rustmq/ca

# Verify CA creation
rustmq-admin ca list
```

### CA Configuration

The CA initialization creates several important files:

```
/etc/rustmq/ca/
├── root-ca.pem          # Root CA certificate (public)
├── root-ca.key          # Root CA private key (SECURE!)
├── root-ca.crl          # Certificate Revocation List
├── ca-config.toml       # CA configuration
├── serial.txt           # Serial number tracking
└── index.txt            # Certificate database
```

**Security Note**: The root CA private key (`root-ca.key`) should be stored securely and backed up. Consider using a Hardware Security Module (HSM) for production environments.

### Creating Intermediate CAs

For better security and operational flexibility, create intermediate CAs for different environments:

```bash
# Create intermediate CA for production
rustmq-admin ca intermediate \
  --parent-ca root_ca_1 \
  --cn "RustMQ Production CA" \
  --org "Your Organization" \
  --ou "Production" \
  --validity-years 5 \
  --path-length 0 \
  --output-dir /etc/rustmq/ca/production

# Create intermediate CA for development
rustmq-admin ca intermediate \
  --parent-ca root_ca_1 \
  --cn "RustMQ Development CA" \
  --org "Your Organization" \
  --ou "Development" \
  --validity-years 2 \
  --path-length 0 \
  --output-dir /etc/rustmq/ca/development
```

### CA Management Operations

```bash
# List all CAs
rustmq-admin ca list --format table

# Get CA details
rustmq-admin ca info --ca-id root_ca_1

# Export CA certificate (for client distribution)
rustmq-admin ca export --ca-id root_ca_1 --output ca-cert.pem

# Update CA configuration
rustmq-admin ca update-config --ca-id root_ca_1 --config-file ca-config.toml

# Rotate CA certificate (advanced operation)
rustmq-admin ca rotate --ca-id root_ca_1 --transition-days 90
```

### Certificate Signing Architecture

RustMQ implements proper X.509 certificate signing chains, ensuring that all certificates (except root CAs) are correctly signed by their issuing Certificate Authority.

#### Certificate Chain Structure

```
Root CA Certificate (Self-Signed)
    │
    ├── Intermediate CA Certificate (Signed by Root CA)
    │   │
    │   ├── Broker Certificate (Signed by Intermediate CA)
    │   ├── Client Certificate (Signed by Intermediate CA)
    │   └── Admin Certificate (Signed by Intermediate CA)
    │
    └── Direct End-Entity Certificates (Signed by Root CA)
```

#### Key Implementation Details

**✅ Fixed in August 2025**: The certificate manager now properly implements certificate signing:

1. **Root CA Certificates**: Remain self-signed (correct behavior)
2. **Intermediate CA Certificates**: Now properly signed by their issuing root CA
3. **End-Entity Certificates**: Now properly signed by the appropriate CA (root or intermediate)

**Previous Issue**: All certificates were being created as self-signed using `RcgenCertificate::from_params()`, causing authentication manager certificate validation to fail with "Invalid certificate signature" errors.

**Current Implementation**: 
- Uses `reconstruct_rcgen_certificate()` to rebuild CA certificates from stored PEM data
- Properly signs intermediate CAs with `ca_cert.serialize_der_with_signer(&issuer_cert)`
- Correctly signs end-entity certificates with their designated issuing CA
- Maintains proper certificate metadata and storage

#### Certificate Validation Chain

The authentication manager now successfully validates:

1. **Certificate Chain Verification**: Validates the complete chain from end-entity to root CA
2. **Signature Verification**: Verifies that each certificate is properly signed by its issuer
3. **Issuer-Subject Relationships**: Confirms proper CA hierarchy relationships
4. **Certificate Revocation**: Checks revocation status against CA records

This resolves the previous authentication test failures and ensures production-ready mTLS security.

**For detailed technical implementation details**, see [Certificate Signing Implementation](CERTIFICATE_SIGNING_IMPLEMENTATION.md).

## Certificate Lifecycle

### Certificate Issuance

#### Broker Certificates

Broker certificates are used for server authentication and should include appropriate Subject Alternative Names (SANs):

```bash
# Issue a broker certificate
rustmq-admin certs issue \
  --principal "broker-01.internal.company.com" \
  --role broker \
  --ca-id production_ca_1 \
  --san "broker-01" \
  --san "broker-01.k8s.local" \
  --san "192.168.1.100" \
  --san "10.0.1.100" \
  --validity-days 365 \
  --key-size 2048 \
  --output-dir /etc/rustmq/certs/broker-01

# Verify certificate
rustmq-admin certs validate --cert-file /etc/rustmq/certs/broker-01/cert.pem
```

#### Client Certificates

Client certificates are used for authentication and should follow your organization's naming conventions:

```bash
# Issue a client certificate for an application
rustmq-admin certs issue \
  --principal "analytics-service@company.com" \
  --role client \
  --ca-id production_ca_1 \
  --validity-days 90 \
  --key-size 2048 \
  --output-dir /etc/rustmq/certs/analytics-service

# Issue a client certificate for a user
rustmq-admin certs issue \
  --principal "john.doe@company.com" \
  --role client \
  --ca-id production_ca_1 \
  --validity-days 30 \
  --key-size 2048 \
  --output-dir /etc/rustmq/certs/users/john.doe
```

#### Admin Certificates

Admin certificates have enhanced privileges and should have shorter validity periods:

```bash
# Issue an admin certificate
rustmq-admin certs issue \
  --principal "admin@company.com" \
  --role admin \
  --ca-id production_ca_1 \
  --validity-days 7 \
  --key-size 2048 \
  --enhanced-key-usage "client_auth,code_signing" \
  --output-dir /etc/rustmq/certs/admin
```

### Certificate Renewal

#### Manual Renewal

```bash
# Renew a specific certificate
rustmq-admin certs renew cert_12345 \
  --validity-days 365 \
  --preserve-key  # Keep the same private key

# Renew with new key pair
rustmq-admin certs renew cert_12345 \
  --validity-days 365 \
  --generate-new-key \
  --key-size 2048

# Batch renewal of expiring certificates
rustmq-admin certs renew-expiring \
  --days 30 \
  --batch-size 10 \
  --delay-seconds 5
```

#### Automated Renewal

Configure automatic renewal in the security configuration:

```toml
[security.certificate_management.automation]
auto_renewal_enabled = true
renewal_check_interval_hours = 6
renewal_before_expiry_days = 30
renewal_retry_attempts = 3
renewal_retry_delay_minutes = 30

# Notification settings
renewal_notification_enabled = true
renewal_notification_email = "security@company.com"
renewal_webhook_url = "https://alerts.company.com/webhook/cert-renewal"
```

### Certificate Rotation

Certificate rotation involves creating a new certificate with a new key pair while maintaining service availability:

```bash
# Rotate a certificate (generates new key pair)
rustmq-admin certs rotate cert_12345 \
  --overlap-days 7 \
  --notification-email "ops@company.com"

# Check rotation status
rustmq-admin certs rotation-status cert_12345

# Complete rotation (remove old certificate)
rustmq-admin certs rotation-complete cert_12345
```

#### Rotation Process

1. **Preparation**: Generate new certificate with new key pair
2. **Deployment**: Deploy new certificate alongside existing certificate
3. **Transition**: Update client configurations to use new certificate
4. **Verification**: Verify all clients are using new certificate
5. **Cleanup**: Remove old certificate after overlap period

### Certificate Revocation

When a certificate needs to be revoked due to compromise or other security concerns:

```bash
# Revoke a certificate immediately
rustmq-admin certs revoke cert_12345 \
  --reason "key-compromise" \
  --effective-immediately \
  --notify-clients

# Revoke with future effective date
rustmq-admin certs revoke cert_12345 \
  --reason "cessation-of-operation" \
  --effective-date "2024-12-31T23:59:59Z" \
  --notify-clients

# Revocation reasons:
# - unspecified
# - key-compromise  
# - ca-compromise
# - affiliation-changed
# - superseded
# - cessation-of-operation
# - certificate-hold
# - privilege-withdrawn
```

#### Certificate Revocation List (CRL) Management

```bash
# Generate updated CRL
rustmq-admin ca generate-crl --ca-id production_ca_1

# Publish CRL to distribution point
rustmq-admin ca publish-crl \
  --ca-id production_ca_1 \
  --url "https://ca.company.com/crl/rustmq-prod.crl"

# Check CRL status
rustmq-admin ca crl-status --ca-id production_ca_1
```

## Certificate Templates

RustMQ uses certificate templates to ensure consistent certificate issuance across different roles and environments.

### Broker Certificate Template

```toml
[certificate_templates.broker]
# Subject template with variable substitution
subject_template = "CN={{hostname}}.internal.company.com,O=Your Organization,OU=RustMQ Brokers"

# Certificate properties
validity_days = 365
key_type = "RSA"
key_size = 2048
signature_algorithm = "SHA256WithRSA"

# Key usage
key_usage = ["digital_signature", "key_encipherment", "key_agreement"]
extended_key_usage = ["server_auth", "client_auth"]

# Subject Alternative Names
include_san_dns = true
include_san_ip = true
san_dns_suffixes = [".internal.company.com", ".k8s.local"]

# Additional constraints
path_length_constraint = null
basic_constraints_critical = true
key_usage_critical = true
```

### Client Certificate Template

```toml
[certificate_templates.client]
subject_template = "CN={{principal}},O=Your Organization,OU=RustMQ Clients"

validity_days = 90
key_type = "RSA"
key_size = 2048
signature_algorithm = "SHA256WithRSA"

key_usage = ["digital_signature"]
extended_key_usage = ["client_auth"]

# Client certificates don't need SANs typically
include_san_dns = false
include_san_ip = false

# Shorter validity for better security
auto_renewal_enabled = true
renewal_before_expiry_days = 14
```

### Admin Certificate Template

```toml
[certificate_templates.admin]
subject_template = "CN={{principal}},O=Your Organization,OU=Administrators"

# Shorter validity for administrative access
validity_days = 7
key_type = "ECDSA"
curve = "P-256"
signature_algorithm = "SHA256WithECDSA"

key_usage = ["digital_signature"]
extended_key_usage = ["client_auth", "code_signing"]

# Enhanced security for admin certificates
require_hardware_key = true
require_pin_verification = true
max_uses_per_day = 100

# Automatic renewal not recommended for admin certs
auto_renewal_enabled = false
```

### Custom Template Creation

```bash
# Create a custom template
rustmq-admin certs create-template \
  --name "service-accounts" \
  --subject-template "CN={{service_name}}.svc.company.com,O=Your Organization,OU=Service Accounts" \
  --validity-days 180 \
  --key-type "ECDSA" \
  --curve "P-256" \
  --key-usage "digital_signature" \
  --extended-key-usage "client_auth"

# Use custom template
rustmq-admin certs issue \
  --template "service-accounts" \
  --service-name "kafka-connect" \
  --ca-id production_ca_1
```

## Automated Operations

### Automated Certificate Renewal

RustMQ can automatically renew certificates before they expire:

```bash
# Enable automatic renewal for a certificate
rustmq-admin certs enable-auto-renewal cert_12345 \
  --renewal-days 30 \
  --max-attempts 3

# Configure global auto-renewal settings
rustmq-admin config set security.certificate_management.automation.auto_renewal_enabled true
rustmq-admin config set security.certificate_management.automation.renewal_check_interval_hours 6
```

### Automated Monitoring

Set up automated monitoring for certificate health:

```bash
# Configure expiry monitoring
rustmq-admin certs configure-monitoring \
  --warning-days 30,14,7,1 \
  --email "security@company.com" \
  --webhook "https://alerts.company.com/webhook/cert-expiry"

# Set up health checks
rustmq-admin certs schedule-health-check \
  --interval-hours 6 \
  --check-revocation true \
  --check-chain-validity true
```

### Batch Operations

For managing multiple certificates efficiently:

```bash
# Batch certificate issuance from CSV
rustmq-admin certs batch-issue \
  --input-file certificates.csv \
  --template client \
  --ca-id production_ca_1 \
  --output-dir /etc/rustmq/certs/batch

# Batch renewal of expiring certificates
rustmq-admin certs batch-renew \
  --expiring-within-days 30 \
  --dry-run  # Preview what would be renewed

# Batch revocation (emergency procedure)
rustmq-admin certs batch-revoke \
  --certificate-ids cert_123,cert_124,cert_125 \
  --reason "ca-compromise" \
  --effective-immediately
```

## Certificate Monitoring

### Monitoring Dashboard

Check certificate status across your deployment:

```bash
# Overview of all certificates
rustmq-admin certs status --format table

# Certificates expiring soon
rustmq-admin certs expiring --days 30 --format json

# Certificate health check
rustmq-admin certs health-check --detailed

# Performance metrics
rustmq-admin certs metrics --duration 24h
```

### Monitoring Output Examples

```bash
# Certificate status table
CERTIFICATE_ID    SUBJECT                           STATUS    EXPIRES_IN    USAGE
cert_12345       broker-01.internal.company.com    active    335 days      server
cert_67890       analytics@company.com              active    45 days       client  
cert_11111       admin@company.com                  active    2 days        admin

# Expiring certificates JSON
{
  "certificates": [
    {
      "id": "cert_11111",
      "subject": "CN=admin@company.com,O=Your Organization,OU=Administrators",
      "expires_at": "2024-01-15T10:30:00Z",
      "days_until_expiry": 2,
      "auto_renewal_enabled": false,
      "requires_attention": true
    }
  ],
  "total_expiring": 1,
  "auto_renewable": 0,
  "requires_manual_action": 1
}
```

### Automated Monitoring Setup

```toml
# Configuration for automated monitoring
[security.certificate_management.monitoring]
enabled = true
check_interval_hours = 6

# Expiry warnings
expiry_warning_days = [30, 14, 7, 3, 1]
expiry_notification_email = "security@company.com"
expiry_webhook_url = "https://monitoring.company.com/webhook/cert-expiry"

# Health checks
health_check_enabled = true
health_check_interval_hours = 24
health_check_timeout_seconds = 30

# Performance monitoring
performance_monitoring_enabled = true
performance_metrics_interval_minutes = 5
slow_operation_threshold_ms = 1000
```

## Security Best Practices

### CA Security

1. **Root CA Protection**
   ```bash
   # Store root CA offline when possible
   # Use hardware security modules (HSMs) for production
   # Implement multi-person authorization for CA operations
   
   # Example: Enable HSM for root CA
   rustmq-admin ca configure-hsm \
     --ca-id root_ca_1 \
     --hsm-type "pkcs11" \
     --hsm-library "/usr/lib/opensc-pkcs11.so" \
     --key-id "01:02:03:04"
   ```

2. **Intermediate CA Best Practices**
   ```bash
   # Use separate intermediate CAs for different environments
   # Limit intermediate CA path length
   # Regular intermediate CA rotation
   
   rustmq-admin ca create-intermediate \
     --parent root_ca_1 \
     --path-length 0 \
     --validity-years 2
   ```

### Certificate Security

1. **Private Key Protection**
   ```bash
   # Ensure proper file permissions
   chmod 600 /etc/rustmq/certs/*-key.pem
   chown rustmq:rustmq /etc/rustmq/certs/*-key.pem
   
   # Use strong key sizes
   # RSA: minimum 2048 bits (prefer 3072 or 4096)
   # ECDSA: minimum P-256 (prefer P-384 or P-521)
   ```

2. **Certificate Validity Periods**
   ```bash
   # Use appropriate validity periods by role:
   # - Root CA: 10-20 years
   # - Intermediate CA: 2-5 years  
   # - Broker certificates: 1 year
   # - Client certificates: 90 days
   # - Admin certificates: 7-30 days
   ```

3. **Subject Alternative Names**
   ```bash
   # Include all necessary SANs for brokers
   rustmq-admin certs issue \
     --principal "broker-01.company.com" \
     --role broker \
     --san "broker-01" \
     --san "broker-01.internal" \
     --san "broker-01.k8s.local" \
     --san "192.168.1.100" \
     --san "10.0.1.100"
   ```

### Operational Security

1. **Regular Certificate Audits**
   ```bash
   # Monthly certificate audit
   rustmq-admin audit certificates \
     --output-file monthly-cert-audit.json \
     --include-expired \
     --include-revoked
   ```

2. **Certificate Backup and Recovery**
   ```bash
   # Backup certificate store
   rustmq-admin certs backup \
     --output-file cert-backup-$(date +%Y%m%d).tar.gz \
     --encrypt \
     --password-file /etc/rustmq/secrets/backup-password
   
   # Test backup restoration
   rustmq-admin certs restore \
     --backup-file cert-backup-20240101.tar.gz \
     --test-mode \
     --password-file /etc/rustmq/secrets/backup-password
   ```

3. **Incident Response**
   ```bash
   # Emergency certificate revocation
   rustmq-admin certs emergency-revoke \
     --certificate-ids "cert_123,cert_124" \
     --reason "ca-compromise" \
     --notify-all-clients \
     --generate-incident-report
   ```

## Troubleshooting

### Common Certificate Issues

#### 1. Certificate Validation Failures

```bash
# Problem: Certificate chain validation failed
# Error: certificate verify failed: unable to get local issuer certificate

# Diagnosis
openssl verify -CAfile /etc/rustmq/certs/ca.pem /etc/rustmq/certs/broker-cert.pem

# Solution: Check CA certificate path and chain order
rustmq-admin certs validate-chain \
  --cert-file /etc/rustmq/certs/broker-cert.pem \
  --ca-file /etc/rustmq/certs/ca.pem
```

#### 2. Certificate Expiry Issues

```bash
# Problem: Certificate expired
# Error: certificate has expired

# Check expiry
openssl x509 -in /etc/rustmq/certs/broker-cert.pem -checkend 0

# Emergency renewal
rustmq-admin certs emergency-renew cert_12345 \
  --validity-days 30 \
  --skip-validation
```

#### 3. Private Key Issues

```bash
# Problem: Private key mismatch
# Error: certificate and private key do not match

# Verify key match
openssl x509 -noout -modulus -in cert.pem | openssl md5
openssl rsa -noout -modulus -in key.pem | openssl md5
# MD5 sums should match

# Regenerate certificate for existing key
rustmq-admin certs regenerate cert_12345 \
  --preserve-key \
  --same-subject
```

#### 4. CA Trust Issues

```bash
# Problem: CA not trusted
# Error: certificate verify failed: self signed certificate in certificate chain

# Check CA trust store
rustmq-admin ca trust-store \
  --list \
  --verify-chains

# Add CA to trust store
rustmq-admin ca trust \
  --ca-file /etc/rustmq/certs/intermediate-ca.pem \
  --verify-before-adding
```

### Certificate Performance Issues

#### 1. Slow Certificate Validation

```bash
# Check certificate cache performance
rustmq-admin certs cache-stats

# Tune cache settings
rustmq-admin config set security.tls.cert_cache_size 10000
rustmq-admin config set security.tls.cert_cache_ttl_seconds 3600
```

#### 2. High Certificate Lookup Latency

```bash
# Monitor certificate operations
rustmq-admin certs monitor --duration 60s --show-latency

# Enable certificate pre-loading
rustmq-admin config set security.tls.preload_certificates true
```

### Diagnostic Commands

```bash
# Comprehensive certificate health check
rustmq-admin certs diagnose \
  --check-expiry \
  --check-revocation \
  --check-trust-chain \
  --check-permissions \
  --output-file cert-diagnosis.json

# Certificate performance analysis
rustmq-admin certs analyze-performance \
  --duration 1h \
  --include-cache-stats \
  --include-validation-times

# Generate certificate report
rustmq-admin certs generate-report \
  --format html \
  --output cert-report.html \
  --include-metrics \
  --include-recommendations
```

## Advanced Topics

### Hardware Security Modules (HSM)

For maximum security in production environments:

```bash
# Configure HSM for root CA
rustmq-admin ca configure-hsm \
  --ca-id root_ca_1 \
  --hsm-type "pkcs11" \
  --hsm-library "/usr/lib/softhsm2/libsofthsm2.so" \
  --slot-id 0 \
  --pin-file /etc/rustmq/secrets/hsm-pin

# Generate keys in HSM
rustmq-admin certs issue \
  --principal "broker-01.company.com" \
  --role broker \
  --use-hsm \
  --hsm-key-id "broker-01-key"
```

### Certificate Transparency Integration

```bash
# Submit certificates to CT logs
rustmq-admin certs submit-to-ct \
  --certificate-id cert_12345 \
  --ct-log-urls "https://ct.googleapis.com/rocketeer/" \
  --verify-sct

# Monitor CT logs for unexpected certificates
rustmq-admin certs monitor-ct \
  --domain "*.company.com" \
  --alert-on-unexpected
```

### Cross-Platform Certificate Distribution

```bash
# Export certificates for different platforms
rustmq-admin certs export \
  --certificate-id cert_12345 \
  --format pkcs12 \
  --password-file /etc/rustmq/secrets/export-password \
  --output broker-01.p12

# Generate Kubernetes secrets
rustmq-admin certs export-k8s-secret \
  --certificate-id cert_12345 \
  --secret-name rustmq-broker-cert \
  --namespace rustmq
```

This certificate management guide provides comprehensive coverage of all certificate operations in RustMQ. For production deployments, follow the security best practices and implement proper monitoring and backup procedures.