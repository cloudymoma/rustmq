# RustMQ Security Best Practices Guide

This comprehensive guide consolidates security best practices for deploying, operating, and maintaining RustMQ in production environments. These practices are derived from enterprise deployments and security standards to ensure maximum protection while maintaining operational efficiency.

## Table of Contents

- [Overview](#overview)
- [Certificate Management Best Practices](#certificate-management-best-practices)
- [Access Control Best Practices](#access-control-best-practices)
- [Network Security Best Practices](#network-security-best-practices)
- [Operational Security Best Practices](#operational-security-best-practices)
- [Monitoring and Incident Response](#monitoring-and-incident-response)
- [Compliance and Governance](#compliance-and-governance)
- [Development Security Practices](#development-security-practices)
- [Deployment Security Patterns](#deployment-security-patterns)
- [Performance vs Security Trade-offs](#performance-vs-security-trade-offs)

## Overview

RustMQ's security model is built on defense-in-depth principles, providing multiple layers of protection. This guide outlines best practices for each security layer:

### Security Layers
1. **Network Security**: TLS/mTLS, network segmentation, and traffic filtering
2. **Identity and Access**: Certificate-based authentication and fine-grained authorization
3. **Data Protection**: Encryption at rest and in transit, secure key management
4. **Audit and Monitoring**: Comprehensive logging and real-time security monitoring
5. **Operational Security**: Secure deployment, maintenance, and incident response

### Core Security Principles

#### 1. Zero Trust Architecture
```bash
# Every connection requires valid certificates
# No implicit trust based on network location
# Continuous verification of identity and authorization
```

#### 2. Principle of Least Privilege
```bash
# Grant minimum required permissions
# Regular permission reviews and cleanup
# Time-limited access for administrative operations
```

#### 3. Defense in Depth
```bash
# Multiple security controls at each layer
# Redundant protections against common attack vectors
# Fail-secure defaults throughout the system
```

## Certificate Management Best Practices

### Certificate Authority Hierarchy

#### Establish Proper CA Structure
```bash
# Root CA (Offline, Maximum Security)
rustmq-admin ca init \
  --cn "Company Root CA" \
  --validity-years 20 \
  --key-size 4096 \
  --store-offline \
  --require-hardware-security

# Intermediate CAs (Environment Separation)
# Production Intermediate CA
rustmq-admin ca intermediate \
  --parent-ca root_ca \
  --cn "RustMQ Production CA" \
  --validity-years 10 \
  --path-length 1

# Staging Intermediate CA  
rustmq-admin ca intermediate \
  --parent-ca root_ca \
  --cn "RustMQ Staging CA" \
  --validity-years 5 \
  --path-length 0
```

#### Certificate Lifecycle Management
```bash
# Automated Certificate Renewal (30 days before expiry)
0 2 * * * /usr/local/bin/rustmq-cert-renewal.sh --threshold 30

# Certificate rotation policy (Annual for production)
# Broker certificates: 1 year validity, 30-day renewal window
# Client certificates: 90 days validity, 15-day renewal window  
# Admin certificates: 7 days validity, 2-day renewal window

# Certificate validation checks
rustmq-admin certs health-check --all --daily
```

### Certificate Security Standards

#### Key Management Best Practices
```toml
# Certificate security configuration
[security.certificates]
# Minimum key sizes
rsa_min_key_size = 2048
ecdsa_min_key_size = 256

# Approved algorithms
approved_signature_algorithms = ["SHA256WithRSA", "SHA384WithECDSA"]
approved_key_algorithms = ["RSA", "ECDSA"]

# Certificate validation
require_key_usage_validation = true
require_extended_key_usage = true
validate_certificate_chain = true
check_certificate_revocation = true

# Certificate storage
encrypt_private_keys = true
require_pin_for_private_keys = true
backup_certificates = true
backup_encryption_enabled = true
```

#### Certificate Templates and Standards
```bash
# Production broker certificate template
rustmq-admin certs create-template \
  --name "production-broker" \
  --subject-template "CN={{hostname}}.prod.rustmq.company.com,O=Company,OU=Production" \
  --validity-days 365 \
  --key-size 2048 \
  --key-usage "digital_signature,key_encipherment" \
  --extended-key-usage "server_auth,client_auth" \
  --san-dns "{{hostname}}.internal,{{hostname}}.k8s.local" \
  --require-approval false

# Production client certificate template
rustmq-admin certs create-template \
  --name "production-client" \
  --subject-template "CN={{principal}},O=Company,OU=Clients" \
  --validity-days 90 \
  --key-size 2048 \
  --key-usage "digital_signature" \
  --extended-key-usage "client_auth" \
  --require-approval false

# Administrative certificate template (High Security)
rustmq-admin certs create-template \
  --name "admin" \
  --subject-template "CN={{principal}},O=Company,OU=Administrators" \
  --validity-days 7 \
  --key-type "ECDSA" \
  --curve "P-384" \
  --key-usage "digital_signature" \
  --extended-key-usage "client_auth" \
  --require-approval true \
  --require-hardware-key true
```

### Certificate Monitoring and Alerting

#### Proactive Certificate Management
```bash
# Certificate expiry monitoring
cat > /etc/rustmq/monitoring/cert-monitor.sh << 'EOF'
#!/bin/bash
# Certificate expiry alerting

# Alert thresholds (days before expiry)
CRITICAL_THRESHOLD=7
WARNING_THRESHOLD=30
INFO_THRESHOLD=90

# Check for expiring certificates
rustmq-admin certs expiring --days $CRITICAL_THRESHOLD --format json > critical_certs.json
rustmq-admin certs expiring --days $WARNING_THRESHOLD --format json > warning_certs.json

# Send alerts based on findings
if [[ $(jq '.total_expiring' critical_certs.json) -gt 0 ]]; then
    send_alert "CRITICAL" "Certificates expire within $CRITICAL_THRESHOLD days"
fi

if [[ $(jq '.total_expiring' warning_certs.json) -gt 0 ]]; then
    send_alert "WARNING" "Certificates expire within $WARNING_THRESHOLD days"
fi

# Certificate health verification
rustmq-admin certs health-check --all --output-file cert-health.json
if [[ $(jq '.unhealthy_certificates | length' cert-health.json) -gt 0 ]]; then
    send_alert "WARNING" "Unhealthy certificates detected"
fi
EOF

# Schedule monitoring (every 4 hours)
0 */4 * * * /etc/rustmq/monitoring/cert-monitor.sh
```

## Access Control Best Practices

### ACL Design Patterns

#### Hierarchical Permission Model
```bash
# 1. Deny-by-default with explicit allow rules
# Start with restrictive base policy
rustmq-admin acl create \
  --name "default_deny_all" \
  --principal "*" \
  --resource "*" \
  --permissions "all" \
  --effect "deny" \
  --priority 9999 \
  --description "Default deny-all policy"

# 2. Service-specific access patterns
# Application service permissions
rustmq-admin acl create \
  --name "app_service_data_access" \
  --principal "app-service@company.com" \
  --resource "topic.app.data.*" \
  --permissions "read,write" \
  --effect "allow" \
  --priority 100 \
  --conditions "source_ip=10.0.0.0/16"

# 3. Environment isolation
rustmq-admin acl create \
  --name "environment_isolation" \
  --principal "*@dev.company.com,*@staging.company.com" \
  --resource "topic.prod.*" \
  --permissions "all" \
  --effect "deny" \
  --priority 1000 \
  --description "Prevent non-prod access to production topics"
```

#### Role-Based Access Control (RBAC)
```bash
# Define standard roles with specific permissions

# Data Producer Role
rustmq-admin acl create \
  --name "data_producer_role" \
  --principal "*@producers.company.com" \
  --resource "topic.data.*" \
  --permissions "write,create" \
  --effect "allow" \
  --priority 200

# Data Consumer Role  
rustmq-admin acl create \
  --name "data_consumer_role" \
  --principal "*@consumers.company.com" \
  --resource "topic.data.*" \
  --permissions "read" \
  --effect "allow" \
  --priority 200

# Analytics Role (Read-only across multiple topics)
rustmq-admin acl create \
  --name "analytics_role" \
  --principal "analytics@company.com" \
  --resource "topic.*" \
  --permissions "read" \
  --effect "allow" \
  --conditions "time_range=00:00-23:59" \
  --priority 300

# Administrative Role (Full access with restrictions)
rustmq-admin acl create \
  --name "admin_role" \
  --principal "*@admin.company.com" \
  --resource "*" \
  --permissions "all" \
  --effect "allow" \
  --conditions "source_ip=10.0.100.0/24,time_range=06:00-20:00" \
  --priority 400
```

### Advanced ACL Patterns

#### Conditional Access Controls
```bash
# Time-based access restrictions
rustmq-admin acl create \
  --name "business_hours_access" \
  --principal "business-users@company.com" \
  --resource "topic.reports.*" \
  --permissions "read" \
  --effect "allow" \
  --conditions "time_range=09:00-17:00,timezone=UTC,days=Mon-Fri" \
  --priority 500

# IP-based access controls
rustmq-admin acl create \
  --name "secure_network_access" \
  --principal "external-partner@partner.com" \
  --resource "topic.partner-data" \
  --permissions "read" \
  --effect "allow" \
  --conditions "source_ip=203.0.113.0/24" \
  --priority 600

# Client version restrictions
rustmq-admin acl create \
  --name "secure_client_version" \
  --principal "*" \
  --resource "topic.secure.*" \
  --permissions "all" \
  --effect "allow" \
  --conditions "client_version>=2.0.0" \
  --priority 700
```

#### Data Classification and Protection
```bash
# Confidential data protection
rustmq-admin acl create \
  --name "confidential_data_protection" \
  --principal "*" \
  --resource "topic.confidential.*" \
  --permissions "all" \
  --effect "deny" \
  --priority 1500 \
  --description "Block access to confidential data"

# Selective confidential access
rustmq-admin acl create \
  --name "confidential_authorized_access" \
  --principal "security-cleared@company.com" \
  --resource "topic.confidential.*" \
  --permissions "read" \
  --effect "allow" \
  --conditions "source_ip=10.0.200.0/24" \
  --priority 200 \
  --description "Authorized access to confidential data"

# PII data handling
rustmq-admin acl create \
  --name "pii_data_access" \
  --principal "gdpr-compliant@company.com" \
  --resource "topic.pii.*" \
  --permissions "read,write" \
  --effect "allow" \
  --conditions "audit_required=true" \
  --priority 800
```

### ACL Performance Optimization

#### Cache Optimization Strategies
```toml
# ACL cache configuration for optimal performance
[security.acl.cache]
# L1 Cache (Connection-local)
l1_cache_size = 1000
l1_cache_ttl_seconds = 300

# L2 Cache (Broker-wide)  
l2_cache_size = 10000
l2_cache_ttl_seconds = 600
l2_cache_shards = 32

# L3 Bloom Filter
l3_bloom_filter_size = 1000000
l3_bloom_filter_false_positive_rate = 0.01

# Controller synchronization
controller_fetch_batch_size = 50
controller_fetch_interval_seconds = 30
controller_fetch_timeout_seconds = 5

# String interning for memory efficiency
enable_string_interning = true
string_intern_threshold = 10
```

#### ACL Rule Optimization
```bash
# Best practices for ACL rule efficiency

# 1. Use specific resource patterns instead of wildcards
# Good: topic.user-events.login
# Avoid: topic.*

# 2. Order rules by frequency of use (higher priority = more frequent)
# 3. Combine related permissions in single rules
# Good: permissions = ["read", "write", "create"]
# Avoid: Multiple rules with single permissions

# 4. Use conditions efficiently
# Cache-friendly conditions: source_ip, time_range
# Expensive conditions: custom_function, external_lookup

# 5. Regular ACL cleanup
rustmq-admin acl cleanup \
  --unused-rules \
  --days-without-use 90 \
  --dry-run

# 6. ACL performance testing
rustmq-admin acl benchmark \
  --duration 60s \
  --concurrency 100 \
  --realistic-workload
```

## Network Security Best Practices

### TLS/mTLS Configuration

#### TLS Security Standards
```toml
# Production TLS configuration
[security.tls]
enabled = true
protocol_version = "1.3"  # Require TLS 1.3
cipher_suites = [
    "TLS_AES_256_GCM_SHA384",
    "TLS_CHACHA20_POLY1305_SHA256",
    "TLS_AES_128_GCM_SHA256"
]

# Certificate configuration
server_cert_path = "/etc/rustmq/certs/server.pem"
server_key_path = "/etc/rustmq/certs/server.key"
client_ca_cert_path = "/etc/rustmq/certs/ca.pem"

# mTLS requirements
client_cert_required = true
verify_cert_chain = true
verify_cert_hostname = true
verify_cert_not_expired = true
check_cert_revocation = true

# Security headers
enable_hsts = true
hsts_max_age = 31536000
hsts_include_subdomains = true
```

#### Certificate Pinning and Validation
```bash
# Certificate pinning for critical connections
cat > /etc/rustmq/config/cert-pinning.toml << 'EOF'
[security.certificate_pinning]
enabled = true

# Pin specific certificates for broker-to-broker communication
[security.certificate_pinning.broker_to_broker]
pin_type = "certificate"
pinned_certificates = [
    "sha256:1234567890abcdef...",  # Broker 1 cert hash
    "sha256:abcdef1234567890...",  # Broker 2 cert hash
]

# Pin CA for client connections
[security.certificate_pinning.client_connections]
pin_type = "ca_certificate"
pinned_ca_certificates = [
    "sha256:fedcba0987654321..."   # Production CA cert hash
]
EOF
```

### Network Segmentation

#### Production Network Architecture
```bash
# Network segmentation strategy

# DMZ (External Access)
# - Load balancers
# - WAF/Proxy servers
# Network: 10.0.1.0/24

# Application Tier (RustMQ Brokers)
# - Client-facing brokers
# - Public topic access
# Network: 10.0.10.0/24

# Service Tier (Internal Services)
# - Broker-to-broker communication
# - Internal topics
# Network: 10.0.20.0/24

# Data Tier (Controllers and Storage)
# - RustMQ controllers
# - Object storage
# - Monitoring systems
# Network: 10.0.30.0/24

# Management Network
# - Administrative access
# - Monitoring and logging
# Network: 10.0.100.0/24
```

#### Firewall Rules and Network Policies
```bash
# Production firewall configuration

# DMZ to Application Tier
iptables -A FORWARD -s 10.0.1.0/24 -d 10.0.10.0/24 -p tcp --dport 9092 -j ACCEPT

# Application Tier Internal Communication
iptables -A FORWARD -s 10.0.10.0/24 -d 10.0.10.0/24 -p tcp --dport 9093 -j ACCEPT

# Application to Service Tier
iptables -A FORWARD -s 10.0.10.0/24 -d 10.0.20.0/24 -p tcp --dport 9093 -j ACCEPT

# Service to Data Tier
iptables -A FORWARD -s 10.0.20.0/24 -d 10.0.30.0/24 -p tcp --dport 9094 -j ACCEPT

# Management access (restricted source IPs)
iptables -A FORWARD -s 192.168.100.0/24 -d 10.0.100.0/24 -p tcp --dport 22 -j ACCEPT
iptables -A FORWARD -s 192.168.100.0/24 -d 10.0.100.0/24 -p tcp --dport 8080 -j ACCEPT

# Default deny
iptables -P FORWARD DROP
```

### Load Balancer and Proxy Security

#### Secure Load Balancer Configuration
```yaml
# HAProxy configuration for RustMQ
global
    ssl-default-bind-ciphers ECDHE+AESGCM:ECDHE+CHACHA20:DHE+AESGCM:DHE+CHACHA20:!aNULL:!SHA1:!AESCCM
    ssl-default-bind-options ssl-min-ver TLSv1.2 no-tls-tickets
    ssl-default-server-ciphers ECDHE+AESGCM:ECDHE+CHACHA20:DHE+AESGCM:DHE+CHACHA20:!aNULL:!SHA1:!AESCCM
    ssl-default-server-options ssl-min-ver TLSv1.2 no-tls-tickets

defaults
    mode tcp
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms
    option tcplog

frontend rustmq_frontend
    bind *:9092 ssl crt /etc/ssl/certs/rustmq.pem
    default_backend rustmq_brokers
    
    # Security headers
    http-response set-header Strict-Transport-Security max-age=31536000
    http-response set-header X-Frame-Options DENY
    http-response set-header X-Content-Type-Options nosniff

backend rustmq_brokers
    balance roundrobin
    option ssl-hello-chk
    server broker1 10.0.10.10:9092 check ssl verify required ca-file /etc/ssl/certs/ca.pem
    server broker2 10.0.10.11:9092 check ssl verify required ca-file /etc/ssl/certs/ca.pem
    server broker3 10.0.10.12:9092 check ssl verify required ca-file /etc/ssl/certs/ca.pem
```

## Operational Security Best Practices

### Secure Configuration Management

#### Configuration Security Standards
```toml
# Secure configuration practices
[security.configuration]
# Encrypt sensitive configuration values
encrypt_sensitive_values = true
sensitive_value_patterns = [
    "*password*",
    "*key*", 
    "*secret*",
    "*token*"
]

# Configuration validation
validate_on_load = true
require_signature = true
config_schema_validation = true

# Runtime configuration updates
allow_runtime_updates = true
require_authentication_for_updates = true
audit_configuration_changes = true
backup_before_changes = true

# Configuration drift detection
enable_drift_detection = true
baseline_config_path = "/etc/rustmq/baseline/config.toml"
drift_check_interval_minutes = 60
alert_on_drift = true
```

#### Secrets Management Integration
```bash
# Integration with HashiCorp Vault
cat > /etc/rustmq/config/vault-integration.toml << 'EOF'
[security.vault]
enabled = true
vault_addr = "https://vault.company.com"
vault_role = "rustmq-production"
vault_mount_path = "secret/rustmq"

# Authentication
auth_method = "kubernetes"
service_account_token_path = "/var/run/secrets/kubernetes.io/serviceaccount/token"

# Secret rotation
auto_rotate_secrets = true
rotation_interval_hours = 24
rotation_overlap_hours = 1

# Secret caching
cache_secrets = true
cache_ttl_minutes = 60
EOF

# AWS Secrets Manager integration
cat > /etc/rustmq/config/aws-secrets.toml << 'EOF'
[security.aws_secrets]
enabled = true
region = "us-west-2"
secret_prefix = "rustmq/production"

# IAM role for secret access
iam_role = "arn:aws:iam::123456789012:role/RustMQSecretsAccess"

# Automatic secret rotation
enable_rotation = true
rotation_days = 30
EOF
```

### Deployment Security

#### Secure Deployment Pipeline
```yaml
# CI/CD Security Pipeline
name: Secure RustMQ Deployment

on:
  push:
    branches: [main]

jobs:
  security-scan:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Security Vulnerability Scan
      run: |
        cargo audit
        cargo clippy -- -D warnings
        
    - name: Container Security Scan
      uses: aquasecurity/trivy-action@master
      with:
        image-ref: 'rustmq:${{ github.sha }}'
        format: 'sarif'
        output: 'trivy-results.sarif'
        
    - name: Configuration Security Validation
      run: |
        # Validate configuration files
        rustmq-admin config validate --config config/production.toml
        
        # Check for hardcoded secrets
        grep -r "password\|secret\|key" config/ && exit 1 || true

  deploy:
    needs: security-scan
    runs-on: ubuntu-latest
    steps:
    - name: Deploy with Security Verification
      run: |
        # Deploy to staging first
        kubectl apply -f k8s/staging/ --dry-run=server
        
        # Run security tests
        ./scripts/security-tests.sh staging
        
        # Deploy to production if tests pass
        kubectl apply -f k8s/production/
        
        # Verify deployment security
        ./scripts/verify-production-security.sh
```

#### Container Security Hardening
```dockerfile
# Secure Dockerfile for RustMQ
FROM rust:1.70-alpine AS builder

# Use specific user for build
RUN adduser -D -s /bin/sh rustmq-build
USER rustmq-build

WORKDIR /app
COPY --chown=rustmq-build:rustmq-build . .

# Build with security flags
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime image
FROM scratch

# Copy CA certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Create non-root user
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

# Copy application
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rustmq-broker /usr/local/bin/

# Security configurations
USER rustmq-build:rustmq-build
EXPOSE 9092 9093

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/rustmq-broker", "--health-check"]

ENTRYPOINT ["/usr/local/bin/rustmq-broker"]
```

### Security Maintenance

#### Regular Security Operations
```bash
# Daily security maintenance script
cat > /usr/local/bin/rustmq-daily-security.sh << 'EOF'
#!/bin/bash
# Daily RustMQ security maintenance

set -euo pipefail

LOG_FILE="/var/log/rustmq/security-maintenance.log"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

# 1. Certificate health check
log "Running certificate health check"
if ! rustmq-admin certs health-check --all; then
    log "WARNING: Certificate health check failed"
    # Send alert
fi

# 2. ACL performance review
log "Checking ACL performance"
rustmq-admin acl cache-stats | tee -a "$LOG_FILE"

# 3. Security log analysis
log "Analyzing security logs"
rustmq-admin audit analyze \
    --type "security_violations" \
    --period "last-24-hours" \
    --detect-anomalies

# 4. Configuration drift detection
log "Checking configuration drift"
if ! rustmq-admin config drift-check; then
    log "WARNING: Configuration drift detected"
    # Send alert and create remediation ticket
fi

# 5. Security metrics collection
log "Collecting security metrics"
rustmq-admin security metrics \
    --duration 24h \
    --format prometheus >> /var/lib/prometheus/rustmq-security.prom

log "Daily security maintenance completed"
EOF

chmod +x /usr/local/bin/rustmq-daily-security.sh

# Schedule daily execution
(crontab -l 2>/dev/null; echo "0 6 * * * /usr/local/bin/rustmq-daily-security.sh") | crontab -
```

#### Security Update Management
```bash
# Security update procedure
cat > /usr/local/bin/rustmq-security-update.sh << 'EOF'
#!/bin/bash
# RustMQ security update process

set -euo pipefail

UPDATE_TYPE="${1:-patch}"  # patch, minor, major
ENVIRONMENT="${2:-staging}"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1"
}

# Pre-update security backup
log "Creating security backup before update"
rustmq-admin security backup \
    --output-file "security-backup-pre-$(date +%Y%m%d).tar.gz" \
    --include-all \
    --encrypt

# Download and verify update
log "Downloading and verifying security update"
curl -O "https://releases.rustmq.com/v1.x.x/rustmq-v1.x.x.tar.gz"
curl -O "https://releases.rustmq.com/v1.x.x/rustmq-v1.x.x.tar.gz.sig"

# Verify signature
gpg --verify rustmq-v1.x.x.tar.gz.sig rustmq-v1.x.x.tar.gz

# Apply update with rolling deployment
log "Applying security update with rolling deployment"
kubectl patch deployment rustmq-broker -p \
    '{"spec":{"template":{"spec":{"containers":[{"name":"broker","image":"rustmq:v1.x.x"}]}}}}'

# Wait for rollout and verify
kubectl rollout status deployment/rustmq-broker --timeout=600s

# Security verification after update
log "Running post-update security verification"
./scripts/security-verification.sh

# Update security configuration if needed
if [[ -f "config/security-updates.toml" ]]; then
    log "Applying security configuration updates"
    rustmq-admin config update --file config/security-updates.toml
fi

log "Security update completed successfully"
EOF
```

## Monitoring and Incident Response

### Security Monitoring Setup

#### Real-time Security Monitoring
```yaml
# Prometheus security monitoring configuration
groups:
- name: rustmq.security.critical
  interval: 30s
  rules:
  
  # Authentication failures
  - alert: RustMQHighAuthFailureRate
    expr: rate(rustmq_authentication_failures_total[5m]) > 0.1
    for: 2m
    labels:
      severity: critical
      category: security
    annotations:
      summary: "High authentication failure rate detected"
      description: "Authentication failure rate is {{ $value }}/sec"
      runbook_url: "https://runbooks.company.com/rustmq/auth-failures"

  # Authorization violations
  - alert: RustMQUnauthorizedAccessSpike
    expr: rate(rustmq_authorization_denied_total[5m]) > 0.05
    for: 1m
    labels:
      severity: critical
      category: security
    annotations:
      summary: "Spike in unauthorized access attempts"
      description: "Authorization denial rate is {{ $value }}/sec"
      
  # Certificate issues
  - alert: RustMQCertificateValidationFailures
    expr: rate(rustmq_certificate_validation_failures_total[5m]) > 0.01
    for: 1m
    labels:
      severity: warning
      category: security
    annotations:
      summary: "Certificate validation failures detected"
      description: "Certificate validation failing at {{ $value }}/sec"

  # TLS handshake failures
  - alert: RustMQTLSHandshakeFailures
    expr: rate(rustmq_tls_handshake_failures_total[5m]) > 0.01
    for: 1m
    labels:
      severity: warning
      category: security
    annotations:
      summary: "TLS handshake failures detected"
      description: "TLS handshake failure rate is {{ $value }}/sec"
```

#### SIEM Integration
```bash
# Elastic Stack integration for security monitoring
cat > /etc/filebeat/rustmq-security.yml << 'EOF'
filebeat.inputs:
- type: log
  enabled: true
  paths:
    - /var/log/rustmq/security-audit.log
  fields:
    logtype: rustmq-security
  fields_under_root: true
  json.keys_under_root: true
  
- type: log
  enabled: true 
  paths:
    - /var/log/rustmq/authentication.log
  fields:
    logtype: rustmq-auth
  fields_under_root: true
  json.keys_under_root: true

output.elasticsearch:
  hosts: ["elasticsearch-security.company.com:9200"]
  index: "rustmq-security-%{+yyyy.MM.dd}"
  ssl.verification_mode: full
  ssl.certificate_authorities: ["/etc/pki/ca.pem"]

processors:
- add_host_metadata:
    when.not.contains.tags: forwarded
- add_docker_metadata: ~
- add_kubernetes_metadata: ~

# Security event enrichment
- script:
    lang: javascript
    source: >
      function process(event) {
        if (event.Get("event_type") === "authentication_failure") {
          event.Put("security_severity", "high");
        }
        if (event.Get("event_type") === "authorization_denied") {
          event.Put("security_severity", "medium");
        }
      }
EOF
```

### Incident Response Procedures

#### Automated Security Response
```bash
# Security incident response automation
cat > /usr/local/bin/rustmq-security-incident.sh << 'EOF'
#!/bin/bash
# Automated security incident response

set -euo pipefail

INCIDENT_TYPE="$1"  # auth_failure_spike, cert_compromise, unauthorized_access
SEVERITY="${2:-medium}"  # low, medium, high, critical
AFFECTED_PRINCIPAL="${3:-}"

# Configuration
INCIDENT_ID="INC-$(date +%Y%m%d%H%M%S)"
INCIDENT_LOG="/var/log/rustmq/incidents/incident-${INCIDENT_ID}.log"
ALERT_WEBHOOK="https://alerts.company.com/webhook/security"

# Create incident directory
mkdir -p "$(dirname "$INCIDENT_LOG")"

# Logging function
log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') [${INCIDENT_ID}] $1" | tee -a "$INCIDENT_LOG"
}

# Alert function
send_alert() {
    local message="$1"
    curl -X POST "$ALERT_WEBHOOK" \
        -H "Content-Type: application/json" \
        -d "{
            \"incident_id\": \"$INCIDENT_ID\",
            \"incident_type\": \"$INCIDENT_TYPE\",
            \"severity\": \"$SEVERITY\",
            \"message\": \"$message\",
            \"timestamp\": \"$(date -Iseconds)\",
            \"affected_principal\": \"$AFFECTED_PRINCIPAL\"
        }"
}

# Incident-specific response procedures
case "$INCIDENT_TYPE" in
    "auth_failure_spike")
        log "Handling authentication failure spike incident"
        
        # Increase authentication logging detail
        rustmq-admin config set security.audit.auth_detail_level debug
        
        # Enable rate limiting for failed attempts
        rustmq-admin config set security.rate_limiting.auth_failures.enabled true
        rustmq-admin config set security.rate_limiting.auth_failures.limit 5
        
        # Collect forensic data
        rustmq-admin audit logs \
            --event-type authentication_failure \
            --since "1h ago" \
            --output-file "${INCIDENT_LOG%.log}-auth-failures.json"
        
        send_alert "Authentication failure spike detected and mitigated"
        ;;
        
    "cert_compromise")
        log "Handling certificate compromise incident"
        
        if [[ -n "$AFFECTED_PRINCIPAL" ]]; then
            # Emergency certificate revocation
            local cert_ids
            cert_ids=$(rustmq-admin certs list \
                --principal "$AFFECTED_PRINCIPAL" \
                --status active \
                --format json | jq -r '.[].certificate_id')
            
            for cert_id in $cert_ids; do
                log "Emergency revoking certificate: $cert_id"
                rustmq-admin certs emergency-revoke "$cert_id" \
                    --reason "security-incident" \
                    --incident-id "$INCIDENT_ID" \
                    --effective-immediately
            done
            
            # Force CRL update
            rustmq-admin ca generate-crl --force-update
            
            # Disable affected principal temporarily
            rustmq-admin acl emergency-disable \
                --principal "$AFFECTED_PRINCIPAL" \
                --reason "Certificate compromise - $INCIDENT_ID"
        fi
        
        send_alert "Certificate compromise incident response completed"
        ;;
        
    "unauthorized_access")
        log "Handling unauthorized access incident"
        
        # Collect access attempt details
        rustmq-admin audit logs \
            --event-type authorization_denied \
            --since "30m ago" \
            --output-file "${INCIDENT_LOG%.log}-unauthorized-access.json"
        
        # Analyze patterns
        rustmq-admin audit analyze \
            --type unauthorized_access \
            --incident-id "$INCIDENT_ID" \
            --detect-anomalies
        
        # Increase monitoring sensitivity
        rustmq-admin security monitor \
            --increase-sensitivity \
            --duration "2h" \
            --incident-id "$INCIDENT_ID"
        
        send_alert "Unauthorized access incident analyzed and monitoring increased"
        ;;
        
    *)
        log "Unknown incident type: $INCIDENT_TYPE"
        send_alert "Unknown security incident type: $INCIDENT_TYPE"
        exit 1
        ;;
esac

# Generate incident report
rustmq-admin audit incident-report \
    --incident-id "$INCIDENT_ID" \
    --output-file "${INCIDENT_LOG%.log}-report.json" \
    --include-timeline \
    --include-affected-resources

log "Security incident response completed for $INCIDENT_TYPE"
send_alert "Security incident $INCIDENT_ID response completed"
EOF

chmod +x /usr/local/bin/rustmq-security-incident.sh
```

#### Manual Incident Response Procedures

##### Security Incident Classification
```bash
# Incident severity classification

# CRITICAL (Immediate response required)
# - Active security breach
# - Certificate authority compromise  
# - Mass unauthorized access
# - Data exfiltration detected

# HIGH (Response within 2 hours)
# - Individual certificate compromise
# - Privilege escalation attempts
# - Persistent authentication failures
# - Configuration tampering

# MEDIUM (Response within 8 hours)
# - Unusual access patterns
# - Failed authorization attempts
# - Certificate validation issues
# - Performance-based security events

# LOW (Response within 24 hours)
# - Policy violations
# - Audit log anomalies
# - Non-critical configuration drift
# - Expired certificates (non-production)
```

##### Emergency Response Procedures
```bash
# Emergency security procedures

# 1. Immediate threat containment
emergency_lockdown() {
    # Disable all non-essential access
    rustmq-admin acl emergency-disable-all \
        --except-principals "emergency-admin@company.com" \
        --reason "Security emergency lockdown"
    
    # Enable maximum security logging
    rustmq-admin config set security.audit.log_level debug
    rustmq-admin config set security.audit.log_all_events true
    
    # Activate security monitoring mode
    rustmq-admin security enable-emergency-mode
}

# 2. Evidence collection
collect_evidence() {
    local incident_id="$1"
    local evidence_dir="/var/log/rustmq/incidents/evidence-$incident_id"
    
    mkdir -p "$evidence_dir"
    
    # Collect security logs
    rustmq-admin audit export \
        --time-range "last-24-hours" \
        --output-file "$evidence_dir/audit-logs.json"
    
    # Collect system state
    rustmq-admin security status --detailed > "$evidence_dir/security-status.json"
    rustmq-admin certs list --all > "$evidence_dir/certificates.json"
    rustmq-admin acl list --all > "$evidence_dir/acl-rules.json"
    
    # Collect configuration
    cp -r /etc/rustmq/config "$evidence_dir/"
    
    # Create evidence archive
    tar -czf "$evidence_dir.tar.gz" "$evidence_dir"
    chmod 600 "$evidence_dir.tar.gz"
}

# 3. Communication procedures
notify_stakeholders() {
    local incident_id="$1"
    local severity="$2"
    
    # Security team notification
    echo "Security incident $incident_id (Severity: $severity)" | \
        mail -s "RustMQ Security Incident" security@company.com
    
    # Executive notification for critical incidents
    if [[ "$severity" == "critical" ]]; then
        echo "Critical security incident requiring executive attention" | \
            mail -s "CRITICAL: RustMQ Security Incident" ciso@company.com
    fi
    
    # Operations team notification
    curl -X POST "https://slack.company.com/webhook" \
        -d "{\"text\": \"RustMQ Security Incident: $incident_id (Severity: $severity)\"}"
}
```

## Compliance and Governance

### Regulatory Compliance

#### SOX (Sarbanes-Oxley) Compliance
```toml
# SOX compliance configuration
[compliance.sox]
enabled = true
control_framework = "COSO"
reporting_period = "quarterly"

# Required controls
[compliance.sox.controls.access_control]
description = "Segregation of duties and access controls"
requirements = [
    "multi_person_authorization_for_sensitive_operations",
    "periodic_access_reviews", 
    "audit_trail_for_all_access_changes"
]
testing_frequency = "monthly"
evidence_collection = "automatic"

[compliance.sox.controls.audit_logging]
description = "Comprehensive audit logging for financial data access"
requirements = [
    "log_all_financial_data_access",
    "immutable_audit_logs",
    "centralized_log_storage",
    "log_integrity_verification"
]
testing_frequency = "continuous"
evidence_collection = "automatic"

[compliance.sox.controls.change_management]
description = "Controlled change management process"
requirements = [
    "approval_workflow_for_production_changes",
    "emergency_change_procedures",
    "change_impact_assessment",
    "rollback_procedures"
]
testing_frequency = "quarterly"
evidence_collection = "manual"
```

#### PCI DSS Compliance
```toml
# PCI DSS compliance configuration
[compliance.pci_dss]
enabled = true
version = "4.0"
merchant_level = 1
card_data_environment = true

# PCI DSS Requirements implementation
[compliance.pci_dss.requirement_2]
description = "Strong cryptography and security protocols"
implementation = [
    "tls_1_3_minimum",
    "strong_cipher_suites_only",
    "certificate_based_authentication",
    "key_management_procedures"
]

[compliance.pci_dss.requirement_8]
description = "Identify users and authenticate access"
implementation = [
    "unique_user_identification_via_certificates",
    "multi_factor_authentication_required",
    "strong_authentication_controls",
    "regular_authentication_testing"
]

[compliance.pci_dss.requirement_10]
description = "Log and monitor all access to network resources and cardholder data"
implementation = [
    "comprehensive_audit_logging",
    "real_time_monitoring",
    "automated_log_analysis",
    "security_incident_response"
]

# Cardholder data protection
[compliance.pci_dss.data_protection]
cardholder_data_topics = ["payment.*", "card.*", "transaction.*"]
encryption_required = true
access_logging_required = true
retention_period_months = 12
secure_deletion_required = true
```

#### GDPR Compliance
```toml
# GDPR compliance configuration
[compliance.gdpr]
enabled = true
data_controller = "Your Company Ltd"
data_protection_officer = "dpo@company.com"

# Personal data identification
[compliance.gdpr.personal_data]
pii_topics = ["user.*", "customer.*", "employee.*"]
sensitive_data_topics = ["health.*", "biometric.*", "location.*"]

# Data subject rights
[compliance.gdpr.data_subject_rights]
right_to_access = true
right_to_rectification = true  
right_to_erasure = true
right_to_portability = true
right_to_object = true

# Automated data processing
automated_processing_consent_required = true
profiling_consent_required = true

# Data breach notification
[compliance.gdpr.breach_notification]
supervisory_authority_notification_hours = 72
data_subject_notification_required = true
breach_detection_automated = true
```

### Governance Framework

#### Security Policy Management
```bash
# Security policy enforcement automation
cat > /usr/local/bin/rustmq-policy-enforcement.sh << 'EOF'
#!/bin/bash
# Security policy enforcement and compliance checking

set -euo pipefail

POLICY_DIR="/etc/rustmq/policies"
COMPLIANCE_LOG="/var/log/rustmq/compliance.log"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$COMPLIANCE_LOG"
}

# Policy validation and enforcement
enforce_certificate_policy() {
    log "Enforcing certificate policy"
    
    # Check certificate validity periods
    rustmq-admin certs list --format json | \
        jq '.[] | select(.validity_days > 366)' | \
        while read -r cert; do
            cert_id=$(echo "$cert" | jq -r '.certificate_id')
            log "POLICY VIOLATION: Certificate $cert_id exceeds maximum validity period"
            # Create compliance violation ticket
            create_violation_ticket "CERT_VALIDITY" "$cert_id"
        done
    
    # Check for weak key sizes
    rustmq-admin certs list --format json | \
        jq '.[] | select(.key_size < 2048)' | \
        while read -r cert; do
            cert_id=$(echo "$cert" | jq -r '.certificate_id')
            log "POLICY VIOLATION: Certificate $cert_id uses weak key size"
            # Schedule certificate replacement
            schedule_cert_replacement "$cert_id"
        done
}

enforce_access_control_policy() {
    log "Enforcing access control policy"
    
    # Check for overly permissive rules
    rustmq-admin acl list --format json | \
        jq '.[] | select(.permissions | contains(["all"]))' | \
        while read -r rule; do
            rule_id=$(echo "$rule" | jq -r '.rule_id')
            principal=$(echo "$rule" | jq -r '.principal')
            log "POLICY VIOLATION: Overly permissive ACL rule $rule_id for $principal"
            # Flag for review
            flag_for_review "ACL_OVERPERMISSION" "$rule_id"
        done
    
    # Check for rules without conditions on external principals
    rustmq-admin acl list --format json | \
        jq '.[] | select(.principal | contains("@external.")) | select(.conditions == null)' | \
        while read -r rule; do
            rule_id=$(echo "$rule" | jq -r '.rule_id')
            log "POLICY VIOLATION: External principal rule $rule_id without conditions"
            # Require immediate remediation
            require_immediate_remediation "ACL_EXTERNAL_NO_CONDITIONS" "$rule_id"
        done
}

# Compliance reporting
generate_compliance_report() {
    local framework="$1"
    local output_file="/var/log/rustmq/compliance-${framework}-$(date +%Y%m%d).json"
    
    log "Generating $framework compliance report"
    
    case "$framework" in
        "SOX")
            rustmq-admin audit compliance-report \
                --framework SOX \
                --period quarterly \
                --output-file "$output_file"
            ;;
        "PCI-DSS")
            rustmq-admin audit compliance-report \
                --framework PCI-DSS \
                --period monthly \
                --output-file "$output_file"
            ;;
        "GDPR")
            rustmq-admin audit compliance-report \
                --framework GDPR \
                --period monthly \
                --output-file "$output_file" \
                --include-data-subject-requests
            ;;
    esac
    
    log "Compliance report generated: $output_file"
}

# Main execution
main() {
    log "Starting security policy enforcement"
    
    enforce_certificate_policy
    enforce_access_control_policy
    
    # Generate compliance reports
    generate_compliance_report "SOX"
    generate_compliance_report "PCI-DSS"
    generate_compliance_report "GDPR"
    
    log "Security policy enforcement completed"
}

main "$@"
EOF

chmod +x /usr/local/bin/rustmq-policy-enforcement.sh

# Schedule daily policy enforcement
(crontab -l 2>/dev/null; echo "0 1 * * * /usr/local/bin/rustmq-policy-enforcement.sh") | crontab -
```

## Development Security Practices

### Secure Development Lifecycle

#### Security Code Review Guidelines
```bash
# Security-focused code review checklist

# 1. Authentication and Authorization
#    - Verify all endpoints require proper authentication
#    - Check for privilege escalation vulnerabilities
#    - Ensure input validation on all user inputs
#    - Verify secure session management

# 2. Cryptography
#    - Use approved cryptographic algorithms
#    - Proper key management and storage
#    - Secure random number generation
#    - Certificate validation implementation

# 3. Error Handling
#    - No sensitive information in error messages
#    - Proper logging of security events
#    - Graceful failure handling
#    - Rate limiting on error responses

# 4. Dependencies
#    - All dependencies security-scanned
#    - Regular dependency updates
#    - Minimal dependency principle
#    - License compliance verification

# Security review automation
cat > .github/workflows/security-review.yml << 'EOF'
name: Security Review

on:
  pull_request:
    branches: [main]

jobs:
  security-analysis:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Rust Security Audit
      run: |
        cargo install cargo-audit
        cargo audit
        
    - name: Dependency Vulnerability Scan
      run: |
        cargo install cargo-deny
        cargo deny check
        
    - name: Static Analysis
      run: |
        cargo clippy -- -D warnings -W clippy::security
        
    - name: Secret Detection
      uses: trufflesecurity/trufflehog@main
      with:
        path: ./
        base: main
        head: HEAD
EOF
```

#### Security Testing Framework
```rust
// Security testing utilities
#[cfg(test)]
mod security_tests {
    use super::*;
    use rustmq_security::testing::*;

    #[tokio::test]
    async fn test_authentication_bypass_prevention() {
        let test_env = SecurityTestEnvironment::new().await;
        
        // Test 1: Missing certificate
        let result = test_env.authenticate_client(None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), ErrorCode::AuthenticationRequired);
        
        // Test 2: Invalid certificate
        let invalid_cert = test_env.generate_invalid_certificate();
        let result = test_env.authenticate_client(Some(invalid_cert)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), ErrorCode::InvalidCertificate);
        
        // Test 3: Expired certificate
        let expired_cert = test_env.generate_expired_certificate();
        let result = test_env.authenticate_client(Some(expired_cert)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), ErrorCode::CertificateExpired);
    }
    
    #[tokio::test]
    async fn test_authorization_bypass_prevention() {
        let test_env = SecurityTestEnvironment::new().await;
        let client_cert = test_env.generate_valid_client_certificate("test@company.com").await;
        
        // Test 1: Access to unauthorized resource
        let result = test_env.test_access(
            &client_cert,
            "topic.admin-only",
            Operation::Read
        ).await;
        assert_eq!(result.effect, AuthorizationEffect::Deny);
        
        // Test 2: Unauthorized operation
        let result = test_env.test_access(
            &client_cert,
            "topic.public-read",
            Operation::Delete
        ).await;
        assert_eq!(result.effect, AuthorizationEffect::Deny);
        
        // Test 3: Privilege escalation attempt
        let result = test_env.test_access(
            &client_cert,
            "admin.cluster.config",
            Operation::Write
        ).await;
        assert_eq!(result.effect, AuthorizationEffect::Deny);
    }
    
    #[tokio::test]
    async fn test_tls_security_configuration() {
        let test_env = SecurityTestEnvironment::new().await;
        
        // Test 1: TLS version enforcement
        let tls_config = test_env.get_tls_config();
        assert_eq!(tls_config.min_protocol_version(), TlsVersion::TLSv1_3);
        
        // Test 2: Cipher suite restrictions
        let cipher_suites = tls_config.cipher_suites();
        assert!(cipher_suites.contains(&CipherSuite::TLS13_AES_256_GCM_SHA384));
        assert!(!cipher_suites.contains(&CipherSuite::TLS_RSA_WITH_RC4_128_SHA));
        
        // Test 3: Certificate validation
        assert!(tls_config.verify_peer_certificate());
        assert!(tls_config.verify_hostname());
    }
    
    #[tokio::test]
    async fn test_rate_limiting_enforcement() {
        let test_env = SecurityTestEnvironment::new().await;
        let client = test_env.create_test_client().await;
        
        // Test rate limiting by sending requests rapidly
        let mut successful_requests = 0;
        let mut rate_limited_requests = 0;
        
        for _ in 0..100 {
            match client.send_request().await {
                Ok(_) => successful_requests += 1,
                Err(RustMqError::RateLimitExceeded) => rate_limited_requests += 1,
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
        
        assert!(rate_limited_requests > 0, "Rate limiting should be enforced");
        assert!(successful_requests < 100, "Not all requests should succeed");
    }
}
```

### Security Configuration as Code

#### Infrastructure Security Templates
```yaml
# Terraform security configuration for RustMQ
resource "aws_security_group" "rustmq_brokers" {
  name        = "rustmq-brokers"
  description = "Security group for RustMQ brokers"
  vpc_id      = var.vpc_id

  # QUIC client connections
  ingress {
    from_port   = 9092
    to_port     = 9092
    protocol    = "tcp"
    cidr_blocks = var.client_subnets
  }

  # Broker-to-broker communication
  ingress {
    from_port = 9093
    to_port   = 9093
    protocol  = "tcp"
    self      = true
  }

  # Metrics endpoint (restricted)
  ingress {
    from_port   = 9642
    to_port     = 9642
    protocol    = "tcp"
    cidr_blocks = var.monitoring_subnets
  }

  # Outbound to object storage
  egress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name        = "rustmq-brokers"
    Environment = var.environment
    Security    = "high"
  }
}

# KMS key for encryption
resource "aws_kms_key" "rustmq_encryption" {
  description             = "RustMQ encryption key"
  deletion_window_in_days = 7

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "Enable IAM User Permissions"
        Effect = "Allow"
        Principal = {
          AWS = "arn:aws:iam::${data.aws_caller_identity.current.account_id}:root"
        }
        Action   = "kms:*"
        Resource = "*"
      },
      {
        Sid    = "Allow RustMQ Service"
        Effect = "Allow"
        Principal = {
          AWS = aws_iam_role.rustmq_service.arn
        }
        Action = [
          "kms:Decrypt",
          "kms:GenerateDataKey"
        ]
        Resource = "*"
      }
    ]
  })

  tags = {
    Name        = "rustmq-encryption"
    Environment = var.environment
  }
}

# S3 bucket with encryption
resource "aws_s3_bucket" "rustmq_data" {
  bucket = "rustmq-data-${var.environment}-${random_string.bucket_suffix.result}"

  tags = {
    Name        = "rustmq-data"
    Environment = var.environment
    Security    = "high"
  }
}

resource "aws_s3_bucket_encryption_configuration" "rustmq_data" {
  bucket = aws_s3_bucket.rustmq_data.id

  rule {
    apply_server_side_encryption_by_default {
      kms_master_key_id = aws_kms_key.rustmq_encryption.arn
      sse_algorithm     = "aws:kms"
    }
  }
}

resource "aws_s3_bucket_versioning" "rustmq_data" {
  bucket = aws_s3_bucket.rustmq_data.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_logging" "rustmq_data" {
  bucket = aws_s3_bucket.rustmq_data.id

  target_bucket = aws_s3_bucket.access_logs.id
  target_prefix = "rustmq-data/"
}
```

## Performance vs Security Trade-offs

### Performance Optimization Guidelines

#### Security Performance Tuning
```toml
# Performance-optimized security configuration
[security.performance]

# Certificate validation optimization
[security.performance.certificate_validation]
cache_validation_results = true
validation_cache_size = 10000
validation_cache_ttl_seconds = 300
parallel_chain_validation = true
ocsp_response_caching = true

# ACL performance optimization
[security.performance.acl]
# Multi-level caching for sub-100ns authorization
enable_l1_cache = true          # Connection-local cache
l1_cache_size = 1000
l1_cache_ttl_seconds = 300

enable_l2_cache = true          # Broker-wide cache  
l2_cache_size = 10000
l2_cache_shards = 32
l2_cache_ttl_seconds = 600

enable_l3_bloom_filter = true   # Fast negative lookups
l3_bloom_filter_size = 1000000
l3_bloom_filter_false_positive_rate = 0.01

# String interning for memory efficiency
enable_string_interning = true
string_intern_threshold = 10    # Intern strings used 10+ times

# Batch fetching from controller
controller_fetch_batch_size = 50
controller_fetch_parallel_requests = 4
controller_fetch_timeout_ms = 100

# TLS performance optimization
[security.performance.tls]
session_resumption = true
session_cache_size = 10000
enable_tls_false_start = true
enable_early_data = true        # TLS 1.3 0-RTT
cipher_suite_preference = "performance"  # Prefer faster ciphers

# Audit logging performance
[security.performance.audit]
async_logging = true
log_buffer_size = 1000
log_flush_interval_ms = 1000
compress_logs = true
structured_logging = true       # JSON format for efficient parsing
```

#### Security vs Performance Benchmarking
```bash
# Security performance benchmark script
cat > /usr/local/bin/rustmq-security-benchmark.sh << 'EOF'
#!/bin/bash
# Security performance benchmarking

set -euo pipefail

BENCHMARK_LOG="/var/log/rustmq/security-benchmark.log"
RESULTS_DIR="/var/log/rustmq/benchmark-results"

mkdir -p "$RESULTS_DIR"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$BENCHMARK_LOG"
}

# Benchmark ACL performance
benchmark_acl_performance() {
    log "Benchmarking ACL performance"
    
    # Test authorization latency
    rustmq-admin acl benchmark \
        --duration 60s \
        --concurrency 100 \
        --operation authorize \
        --output-file "$RESULTS_DIR/acl-authorize-benchmark.json"
    
    # Test cache performance
    rustmq-admin acl cache-benchmark \
        --duration 60s \
        --hit-ratio-target 0.9 \
        --output-file "$RESULTS_DIR/acl-cache-benchmark.json"
    
    # Analyze results
    local avg_latency
    avg_latency=$(jq '.average_latency_ns' "$RESULTS_DIR/acl-authorize-benchmark.json")
    
    if [[ $avg_latency -gt 100 ]]; then
        log "WARNING: Average ACL authorization latency ($avg_latency ns) exceeds target (100ns)"
    else
        log "ACL performance acceptable: $avg_latency ns average latency"
    fi
}

# Benchmark TLS performance
benchmark_tls_performance() {
    log "Benchmarking TLS performance"
    
    # Test handshake performance
    rustmq-admin security benchmark-tls \
        --connections 1000 \
        --duration 60s \
        --output-file "$RESULTS_DIR/tls-handshake-benchmark.json"
    
    # Test certificate validation performance
    rustmq-admin certs benchmark-validation \
        --certificates 1000 \
        --parallel-validations 10 \
        --output-file "$RESULTS_DIR/cert-validation-benchmark.json"
    
    # Analyze results
    local handshake_time
    handshake_time=$(jq '.average_handshake_time_ms' "$RESULTS_DIR/tls-handshake-benchmark.json")
    
    if [[ $(echo "$handshake_time > 50" | bc -l) == 1 ]]; then
        log "WARNING: TLS handshake time ($handshake_time ms) exceeds target (50ms)"
    else
        log "TLS performance acceptable: $handshake_time ms average handshake time"
    fi
}

# Overall security performance assessment
assess_security_performance() {
    log "Assessing overall security performance impact"
    
    # Baseline performance (security disabled)
    rustmq-admin benchmark \
        --security-disabled \
        --duration 60s \
        --output-file "$RESULTS_DIR/baseline-performance.json"
    
    # Security-enabled performance
    rustmq-admin benchmark \
        --security-enabled \
        --duration 60s \
        --output-file "$RESULTS_DIR/security-enabled-performance.json"
    
    # Calculate performance impact
    local baseline_throughput security_throughput impact
    baseline_throughput=$(jq '.throughput_ops_per_sec' "$RESULTS_DIR/baseline-performance.json")
    security_throughput=$(jq '.throughput_ops_per_sec' "$RESULTS_DIR/security-enabled-performance.json")
    impact=$(echo "scale=2; (1 - $security_throughput / $baseline_throughput) * 100" | bc -l)
    
    log "Security performance impact: ${impact}% throughput reduction"
    
    # Generate recommendations
    if [[ $(echo "$impact > 10" | bc -l) == 1 ]]; then
        log "RECOMMENDATION: Security performance impact ($impact%) is high. Consider:"
        log "- Increasing ACL cache sizes"
        log "- Enabling TLS session resumption"
        log "- Using hardware-accelerated cryptography"
        log "- Optimizing certificate chain length"
    fi
}

# Performance tuning recommendations
generate_tuning_recommendations() {
    log "Generating performance tuning recommendations"
    
    cat > "$RESULTS_DIR/tuning-recommendations.md" << 'RECOMMENDATIONS_EOF'
# RustMQ Security Performance Tuning Recommendations

## ACL Optimization
- Increase L2 cache size if hit rate < 80%
- Use more specific resource patterns to improve cache efficiency
- Consider ACL rule consolidation for frequently accessed resources
- Enable string interning for memory efficiency

## TLS Optimization  
- Enable TLS session resumption for repeat connections
- Use ECDSA certificates for faster operations
- Consider TLS 1.3 with 0-RTT for lowest latency
- Use hardware acceleration if available

## Certificate Management
- Minimize certificate chain length
- Cache certificate validation results
- Use OCSP stapling for revocation checking
- Schedule certificate operations during low-traffic periods

## Monitoring and Alerting
- Set up alerts for authorization latency > 100ns
- Monitor cache hit rates and adjust sizes accordingly
- Track TLS handshake times and certificate validation latency
- Regular performance regression testing

RECOMMENDATIONS_EOF
    
    log "Tuning recommendations saved to: $RESULTS_DIR/tuning-recommendations.md"
}

# Main execution
main() {
    log "Starting security performance benchmark"
    
    benchmark_acl_performance
    benchmark_tls_performance
    assess_security_performance
    generate_tuning_recommendations
    
    log "Security performance benchmark completed"
    log "Results available in: $RESULTS_DIR"
}

main "$@"
EOF

chmod +x /usr/local/bin/rustmq-security-benchmark.sh
```

This comprehensive Security Best Practices guide provides enterprise-grade recommendations for deploying, operating, and maintaining RustMQ with maximum security while ensuring optimal performance and compliance with regulatory requirements.