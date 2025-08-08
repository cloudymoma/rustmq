# RustMQ CLI Security Guide

This comprehensive guide covers all security operations available through the RustMQ Admin CLI, providing detailed command references, usage examples, and best practices for certificate management, ACL operations, and security monitoring.

## Table of Contents

- [CLI Overview](#cli-overview)
- [Certificate Authority Commands](#certificate-authority-commands)
- [Certificate Management Commands](#certificate-management-commands)
- [ACL Management Commands](#acl-management-commands)
- [Security Audit Commands](#security-audit-commands)
- [Security Operations Commands](#security-operations-commands)
- [Output Formats and Options](#output-formats-and-options)
- [Automation and Scripting](#automation-and-scripting)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)

## CLI Overview

The RustMQ Admin CLI provides 37 security commands across 5 main categories, offering comprehensive security management capabilities through an intuitive command-line interface.

### Command Structure

```bash
rustmq-admin [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS] [ARGUMENTS]

# Global options apply to all commands
Global Options:
  --api-url <URL>       Admin API base URL (default: http://127.0.0.1:8080)
  --format <FORMAT>     Output format: table, json, yaml, csv (default: table)
  --no-color           Disable colored output
  --verbose            Enable verbose output
  --timeout <SECONDS>   Request timeout (default: 30)
  --config <FILE>       Configuration file path
```

### Security Command Categories

```bash
# Certificate Authority management
rustmq-admin ca <SUBCOMMAND>

# Certificate lifecycle management  
rustmq-admin certs <SUBCOMMAND>

# ACL management
rustmq-admin acl <SUBCOMMAND>

# Security audit operations
rustmq-admin audit <SUBCOMMAND>

# General security operations
rustmq-admin security <SUBCOMMAND>
```

### Authentication Setup

```bash
# Set API endpoint
export RUSTMQ_ADMIN_API_URL="https://admin.rustmq.company.com"

# Set API authentication token (if required)
export RUSTMQ_API_TOKEN="your-api-token"

# Or use configuration file
rustmq-admin --config ~/.rustmq/config.toml security status
```

## Certificate Authority Commands

### CA Initialization

#### Initialize Root CA
```bash
# Basic root CA initialization
rustmq-admin ca init \
  --cn "RustMQ Root CA" \
  --org "Your Organization"

# Advanced root CA with full parameters
rustmq-admin ca init \
  --cn "RustMQ Root CA" \
  --org "Your Organization" \
  --country "US" \
  --state "California" \
  --city "San Francisco" \
  --validity-years 10 \
  --key-size 4096 \
  --signature-algorithm "SHA256WithRSA" \
  --output-dir "/etc/rustmq/ca"

# Output:
Root CA initialized successfully
CA ID: root_ca_1
Certificate: /etc/rustmq/ca/root-ca.pem
Private Key: /etc/rustmq/ca/root-ca.key (SECURE!)
Serial Number: 1234567890ABCDEF
Valid Until: 2034-01-15 10:30:00 UTC
```

#### Create Intermediate CA
```bash
# Create intermediate CA for production
rustmq-admin ca intermediate \
  --parent-ca root_ca_1 \
  --cn "RustMQ Production CA" \
  --org "Your Organization" \
  --ou "Production" \
  --validity-years 5 \
  --path-length 0

# Output:
Intermediate CA created successfully
CA ID: production_ca_1  
Parent CA: root_ca_1
Certificate: /etc/rustmq/ca/production/ca.pem
Valid Until: 2029-01-15 10:30:00 UTC
```

### CA Management Operations

#### List Certificate Authorities
```bash
# List all CAs
rustmq-admin ca list

# List with specific status
rustmq-admin ca list --status active

# List with detailed information
rustmq-admin ca list --format json --verbose

# Example output (table format):
CA_ID           COMMON_NAME              STATUS    EXPIRES_IN    CERTS_ISSUED
root_ca_1       RustMQ Root CA           active    3650 days     2
production_ca_1 RustMQ Production CA     active    1825 days     156
development_ca_1 RustMQ Development CA   active    365 days      45
```

#### Get CA Information
```bash
# Get detailed CA information
rustmq-admin ca info --ca-id root_ca_1

# Export CA certificate for distribution
rustmq-admin ca export \
  --ca-id root_ca_1 \
  --output-file company-root-ca.pem

# Get CA statistics
rustmq-admin ca stats --ca-id production_ca_1 --format json
```

#### CA Status and Health
```bash
# Check CA health
rustmq-admin ca health --ca-id production_ca_1

# Check all CAs health
rustmq-admin ca health --all

# Generate CA status report
rustmq-admin ca status-report \
  --output-file ca-status-report.json \
  --include-certificates \
  --include-metrics
```

## Certificate Management Commands

### Certificate Issuance

#### Issue Broker Certificates
```bash
# Basic broker certificate
rustmq-admin certs issue \
  --principal "broker-01.internal.company.com" \
  --role broker \
  --ca-id production_ca_1

# Advanced broker certificate with SANs
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
  --output-dir "/etc/rustmq/certs/broker-01"

# Output:
Certificate issued successfully
Certificate ID: cert_12345
Subject: CN=broker-01.internal.company.com,O=Your Organization
Serial Number: ABCDEF1234567890
Valid Until: 2025-01-15 10:30:00 UTC
Files created:
  - /etc/rustmq/certs/broker-01/cert.pem
  - /etc/rustmq/certs/broker-01/key.pem
  - /etc/rustmq/certs/broker-01/chain.pem
```

#### Issue Client Certificates
```bash
# Client certificate for application
rustmq-admin certs issue \
  --principal "analytics-service@company.com" \
  --role client \
  --ca-id production_ca_1 \
  --validity-days 90 \
  --description "Analytics service certificate"

# Client certificate for user
rustmq-admin certs issue \
  --principal "john.doe@company.com" \
  --role client \
  --ca-id production_ca_1 \
  --validity-days 30 \
  --key-size 2048

# Bulk client certificate issuance
rustmq-admin certs batch-issue \
  --template client \
  --input-file users.csv \
  --ca-id production_ca_1 \
  --output-dir "/etc/rustmq/certs/users"
```

#### Issue Admin Certificates
```bash
# Admin certificate with short validity
rustmq-admin certs issue \
  --principal "admin@company.com" \
  --role admin \
  --ca-id production_ca_1 \
  --validity-days 7 \
  --key-type "ECDSA" \
  --curve "P-256"

# Temporary admin access
rustmq-admin certs issue \
  --principal "temp-admin@company.com" \
  --role admin \
  --ca-id production_ca_1 \
  --validity-days 1 \
  --description "Emergency maintenance access"
```

### Certificate Lifecycle Operations

#### List Certificates
```bash
# List all certificates
rustmq-admin certs list

# List certificates by role
rustmq-admin certs list --role broker --status active

# List expiring certificates
rustmq-admin certs expiring --days 30

# List with detailed information
rustmq-admin certs list --format json --include-details

# Example output:
CERTIFICATE_ID    SUBJECT                              STATUS    EXPIRES_IN    USAGE
cert_12345       broker-01.internal.company.com       active    335 days      server
cert_67890       analytics@company.com                 active    45 days       client
cert_11111       admin@company.com                     active    2 days        admin
```

#### Certificate Information
```bash
# Get certificate details
rustmq-admin certs info --cert-id cert_12345

# Validate certificate
rustmq-admin certs validate --cert-id cert_12345

# Check certificate chain
rustmq-admin certs validate-chain --cert-id cert_12345

# Get certificate usage statistics
rustmq-admin certs usage-stats --cert-id cert_12345 --days 30
```

#### Certificate Renewal
```bash
# Renew specific certificate
rustmq-admin certs renew cert_12345 \
  --validity-days 365 \
  --preserve-key

# Renew with new key
rustmq-admin certs renew cert_12345 \
  --validity-days 365 \
  --generate-new-key \
  --key-size 2048

# Batch renewal of expiring certificates
rustmq-admin certs renew-expiring \
  --days 30 \
  --batch-size 10 \
  --delay-seconds 5 \
  --preserve-keys

# Output:
Certificate renewed successfully
Certificate ID: cert_12345
Old Expiry: 2024-02-15 10:30:00 UTC
New Expiry: 2025-01-15 10:30:00 UTC
Key Preserved: Yes
```

#### Certificate Rotation
```bash
# Rotate certificate (new key pair)
rustmq-admin certs rotate cert_12345 \
  --overlap-days 7 \
  --notification-email "ops@company.com"

# Check rotation status
rustmq-admin certs rotation-status cert_12345

# Complete rotation (remove old certificate)
rustmq-admin certs rotation-complete cert_12345

# Batch rotation for security policy
rustmq-admin certs batch-rotate \
  --filter "issued_before=2023-01-01" \
  --overlap-days 14 \
  --max-concurrent 5
```

#### Certificate Revocation
```bash
# Revoke certificate immediately
rustmq-admin certs revoke cert_12345 \
  --reason "key-compromise" \
  --effective-immediately \
  --notify-clients

# Revoke with future effective date
rustmq-admin certs revoke cert_12345 \
  --reason "cessation-of-operation" \
  --effective-date "2024-12-31T23:59:59Z"

# Emergency revocation with incident tracking
rustmq-admin certs emergency-revoke cert_12345 \
  --reason "security-incident" \
  --incident-id "INC-2024-001" \
  --audit-comment "Compromised private key detected"

# Batch revocation
rustmq-admin certs batch-revoke \
  --certificate-ids "cert_123,cert_124,cert_125" \
  --reason "ca-compromise" \
  --effective-immediately

# Available revocation reasons:
# - unspecified
# - key-compromise
# - ca-compromise  
# - affiliation-changed
# - superseded
# - cessation-of-operation
# - certificate-hold
# - privilege-withdrawn
```

### Certificate Monitoring and Maintenance

#### Certificate Expiry Monitoring
```bash
# Check certificates expiring soon
rustmq-admin certs expiring --days 30

# Set up expiry notifications
rustmq-admin certs configure-expiry-alerts \
  --warning-days 30,14,7,1 \
  --email "security@company.com" \
  --webhook "https://alerts.company.com/webhook"

# Generate expiry report
rustmq-admin certs expiry-report \
  --format html \
  --output-file expiry-report.html \
  --include-recommendations
```

#### Certificate Health Checks
```bash
# Health check for specific certificate
rustmq-admin certs health-check --cert-id cert_12345

# Health check for all certificates
rustmq-admin certs health-check --all

# Comprehensive certificate audit
rustmq-admin certs audit \
  --check-expiry \
  --check-revocation \
  --check-chain-validity \
  --check-key-strength \
  --output-file cert-audit.json
```

## ACL Management Commands

### ACL Rule Creation

#### Basic ACL Rules
```bash
# Allow topic read access
rustmq-admin acl create \
  --principal "analytics@company.com" \
  --resource "topic.user-events" \
  --permissions "read" \
  --effect "allow" \
  --description "Analytics service access to user events"

# Allow topic write access with pattern
rustmq-admin acl create \
  --principal "logger@company.com" \
  --resource "topic.logs.*" \
  --permissions "write" \
  --effect "allow"

# Allow consumer group access
rustmq-admin acl create \
  --principal "analytics@company.com" \
  --resource "consumer-group.analytics-*" \
  --permissions "read,create,delete" \
  --effect "allow"

# Output:
ACL rule created successfully
Rule ID: acl_rule_12345
Principal: analytics@company.com
Resource: topic.user-events
Permissions: read
Effect: allow
Status: active
```

#### Advanced ACL Rules with Conditions
```bash
# IP-restricted access
rustmq-admin acl create \
  --principal "external-partner@partner.com" \
  --resource "topic.partner-data" \
  --permissions "read" \
  --effect "allow" \
  --conditions "source_ip=203.0.113.0/24" \
  --description "Partner access from specific IP range"

# Time-restricted access
rustmq-admin acl create \
  --principal "business-user@company.com" \
  --resource "topic.reports.*" \
  --permissions "read" \
  --effect "allow" \
  --conditions "time_range=09:00-17:00,timezone=America/New_York,days=Mon-Fri" \
  --description "Business hours access only"

# Multi-condition rule
rustmq-admin acl create \
  --principal "secure-service@company.com" \
  --resource "topic.confidential.*" \
  --permissions "read" \
  --effect "allow" \
  --conditions "source_ip=10.0.0.0/8,time_range=09:00-17:00,client_version>=2.0.0" \
  --priority 200
```

#### Deny Rules for Security
```bash
# Explicit deny for sensitive topics
rustmq-admin acl create \
  --principal "*@external.com" \
  --resource "topic.internal.*" \
  --permissions "all" \
  --effect "deny" \
  --priority 1000 \
  --description "Block external access to internal topics"

# Protect production topics from deletion
rustmq-admin acl create \
  --principal "*" \
  --resource "topic.prod.*" \
  --permissions "delete" \
  --effect "deny" \
  --priority 500 \
  --description "Prevent deletion of production topics"
```

### ACL Rule Management

#### List and Query ACL Rules
```bash
# List all ACL rules
rustmq-admin acl list

# List rules for specific principal
rustmq-admin acl list --principal "analytics@company.com"

# List rules by effect
rustmq-admin acl list --effect "deny" --format table

# Search rules by resource pattern
rustmq-admin acl search --resource "topic.logs.*"

# Example output:
RULE_ID        PRINCIPAL                RESOURCE           PERMISSIONS  EFFECT  STATUS
acl_rule_123   analytics@company.com    topic.user-events  read         allow   active
acl_rule_124   logger@company.com       topic.logs.*       write        allow   active
acl_rule_125   *@external.com           topic.internal.*   all          deny    active
```

#### Update ACL Rules
```bash
# Update rule permissions
rustmq-admin acl update acl_rule_12345 \
  --permissions "read,write,create" \
  --description "Added create permission"

# Update rule conditions
rustmq-admin acl update acl_rule_12345 \
  --conditions "source_ip=10.0.0.0/8,time_range=00:00-23:59"

# Enable/disable rules
rustmq-admin acl enable acl_rule_12345
rustmq-admin acl disable acl_rule_12345

# Update rule priority
rustmq-admin acl update acl_rule_12345 --priority 150
```

#### Delete ACL Rules
```bash
# Delete specific rule
rustmq-admin acl delete acl_rule_12345 \
  --reason "No longer needed - service decommissioned"

# Bulk delete with confirmation
rustmq-admin acl bulk-delete \
  --filter "created_before=2023-01-01" \
  --confirm-with-phrase "DELETE_OLD_RULES"

# Delete rules for specific principal
rustmq-admin acl delete-for-principal "old-service@company.com" \
  --confirm
```

### ACL Testing and Validation

#### Single Permission Testing
```bash
# Test specific permission
rustmq-admin acl test \
  --principal "analytics@company.com" \
  --resource "topic.user-events" \
  --operation "read" \
  --source-ip "192.168.1.100"

# Test with timestamp
rustmq-admin acl test \
  --principal "business-user@company.com" \
  --resource "topic.reports.daily" \
  --operation "read" \
  --timestamp "2024-01-15T14:30:00Z"

# Example output:
Permission Test Result
Principal: analytics@company.com
Resource: topic.user-events  
Operation: read
Result: ALLOWED
Matched Rule: analytics_user_events_read (acl_rule_12345)
Evaluation Time: 45ns
Cache Level: L1
Conditions Evaluated: source_ip=10.0.0.0/8 (MATCHED)
```

#### Bulk Permission Testing
```bash
# Test multiple permissions from file
rustmq-admin acl bulk-test \
  --input-file test-cases.json \
  --output-file test-results.json \
  --format detailed

# Example test-cases.json:
{
  "test_cases": [
    {
      "name": "analytics_read_access",
      "principal": "analytics@company.com",
      "resource": "topic.user-events",
      "operation": "read",
      "expected_result": "allowed"
    },
    {
      "name": "unauthorized_write_attempt", 
      "principal": "guest@company.com",
      "resource": "topic.sensitive-data",
      "operation": "write",
      "expected_result": "denied"
    }
  ]
}

# Generate test cases from existing access patterns
rustmq-admin acl generate-test-cases \
  --from-audit-logs \
  --days 7 \
  --output-file generated-tests.json
```

#### Permission Discovery
```bash
# Discover permissions for a principal
rustmq-admin acl permissions "analytics@company.com"

# Discover principals with access to resource
rustmq-admin acl principals \
  --resource "topic.sensitive-data" \
  --operation "read"

# Find all resources accessible by principal
rustmq-admin acl resources "analytics@company.com" \
  --operation "read"

# Example output:
Permissions for analytics@company.com:
RESOURCE                 OPERATIONS       EFFECT    CONDITIONS
topic.user-events.*     read,write       allow     source_ip=10.0.0.0/8
topic.metrics.*         read             allow     time_range=00:00-23:59
consumer-group.analytics read,create      allow     none
```

### ACL Policy Management

#### Import/Export ACL Rules
```bash
# Export all ACL rules
rustmq-admin acl export \
  --output-file acl-backup.json \
  --include-metadata \
  --format json

# Export rules for specific principals
rustmq-admin acl export \
  --principals "service1@company.com,service2@company.com" \
  --output-file service-acls.json

# Import ACL rules
rustmq-admin acl import \
  --input-file acl-rules.json \
  --merge-strategy "replace" \
  --validate-before-import \
  --dry-run

# Import with transformation
rustmq-admin acl import \
  --input-file dev-acls.json \
  --transform-principals "s/@dev.company.com/@prod.company.com/" \
  --transform-resources "s/dev-/prod-/"
```

#### ACL Policy Validation
```bash
# Validate ACL rules syntax
rustmq-admin acl validate \
  --input-file acl-rules.json \
  --check-syntax \
  --check-conflicts \
  --check-security-best-practices

# Check for rule conflicts
rustmq-admin acl conflicts \
  --principal "user@company.com" \
  --resource "topic.test"

# Security policy compliance check
rustmq-admin acl compliance-check \
  --policy-framework "SOX" \
  --output-file compliance-report.json
```

### ACL Cache Management

#### Cache Operations
```bash
# Check cache statistics
rustmq-admin acl cache-stats

# Refresh ACL cache
rustmq-admin acl cache-refresh --level "all"

# Warm cache for specific principals
rustmq-admin acl cache-warm \
  --principals "service1@company.com,service2@company.com"

# Clear cache (use with caution)
rustmq-admin acl cache-clear --confirm

# Example cache stats output:
ACL Cache Statistics:
L1 Cache (Connection-Local):
  Total Entries: 1,250
  Hit Rate: 89%
  Average Lookup Time: 12ns
  Evictions/Hour: 15

L2 Cache (Broker-Wide):
  Total Entries: 8,750  
  Hit Rate: 76%
  Average Lookup Time: 52ns
  Shard Distribution: Balanced

L3 Bloom Filter:
  Size: 1,000,000 elements
  False Positive Rate: 0.8%
  Average Lookup Time: 18ns
```

#### Cache Performance Tuning
```bash
# Analyze cache performance
rustmq-admin acl cache-analysis \
  --duration 1h \
  --show-hit-rates \
  --show-eviction-patterns \
  --recommend-optimizations

# Benchmark cache performance
rustmq-admin acl cache-benchmark \
  --duration 60s \
  --concurrency 100 \
  --test-scenarios realistic-workload.json
```

## Security Audit Commands

### Audit Log Operations

#### View Audit Logs
```bash
# View recent audit logs
rustmq-admin audit logs --limit 50

# Filter by event type
rustmq-admin audit logs \
  --event-type "certificate_issued" \
  --since "2024-01-01T00:00:00Z" \
  --limit 100

# Filter by principal
rustmq-admin audit logs \
  --principal "admin@company.com" \
  --include-failed-attempts

# Real-time audit monitoring
rustmq-admin audit events --follow --filter "authentication"

# Example output:
TIMESTAMP              EVENT_TYPE           PRINCIPAL              RESOURCE              RESULT
2024-01-15 14:30:00   certificate_issued   admin@company.com      cert_12345           success
2024-01-15 14:29:45   authentication       broker-01@company.com  -                    success  
2024-01-15 14:29:30   authorization        analytics@company.com  topic.user-events    allowed
2024-01-15 14:29:15   acl_rule_created     admin@company.com      acl_rule_67890       success
```

#### Audit Analysis and Reporting
```bash
# Generate security summary report
rustmq-admin audit summary \
  --period "last-30-days" \
  --format "html" \
  --output-file "security-summary.html"

# Analyze authentication patterns
rustmq-admin audit analyze \
  --type "authentication" \
  --detect-anomalies \
  --baseline-days 30

# Generate compliance report
rustmq-admin audit compliance-report \
  --framework "SOX,PCI-DSS" \
  --period "quarterly" \
  --output-format "pdf"

# Security incident investigation
rustmq-admin audit investigate \
  --incident-id "INC-2024-001" \
  --time-range "2024-01-15T10:00:00Z:2024-01-15T16:00:00Z" \
  --include-related-events
```

### Audit Event Monitoring

#### Real-time Event Streaming
```bash
# Monitor all security events
rustmq-admin audit events --follow

# Monitor specific event types
rustmq-admin audit events \
  --follow \
  --filter "authentication,authorization" \
  --format json

# Monitor failed events only
rustmq-admin audit events \
  --follow \
  --filter "failed" \
  --alert-webhook "https://security.company.com/alerts"
```

#### Security Analytics
```bash
# Analyze access patterns for anomalies
rustmq-admin audit analyze-patterns \
  --principal "user@company.com" \
  --lookback-days 30 \
  --detect-anomalies

# Generate security dashboard metrics
rustmq-admin audit dashboard-metrics \
  --metrics "denied_requests,unusual_access_patterns,certificate_expiry" \
  --format prometheus \
  --output-file security-metrics.prom

# Security trend analysis
rustmq-admin audit trends \
  --metric "authentication_failures" \
  --period "last-7-days" \
  --granularity "hourly"
```

## Security Operations Commands

### Security Status and Health

#### Overall Security Status
```bash
# Get comprehensive security status
rustmq-admin security status

# Detailed security health check
rustmq-admin security health --detailed

# Security metrics overview
rustmq-admin security metrics --duration 24h

# Example output:
RustMQ Security Status
Overall Status: HEALTHY
Security Features:
  ✓ TLS/mTLS: Enabled and functioning
  ✓ Certificate Management: 156 active certificates
  ✓ ACL System: 42 active rules, 87% cache hit rate
  ✓ Audit System: Logging operational

Certificates:
  Active: 156
  Expiring (30 days): 3
  Revoked: 14

ACL System:
  Total Rules: 45
  Enabled Rules: 42
  Cache Hit Rate: 87%
  Average Authorization Time: 47ns

Recent Activity:
  Authentication Attempts (24h): 15,750 (99.7% success)
  Authorization Requests (24h): 125,600 (99.4% allowed)
  Certificate Operations (24h): 12 issued, 2 renewed
```

#### Component Health Checks
```bash
# Check certificate manager health
rustmq-admin security health --component certificate-manager

# Check ACL system health
rustmq-admin security health --component acl-system

# Check audit system health
rustmq-admin security health --component audit-system

# Run all health checks
rustmq-admin security health --all-components --format json
```

### Security Maintenance Operations

#### Security Cleanup
```bash
# Clean up expired certificates
rustmq-admin security cleanup \
  --expired-certs \
  --dry-run

# Clean up unused ACL rules
rustmq-admin security cleanup \
  --unused-acl-rules \
  --days-without-use 90 \
  --dry-run

# Clean up old audit logs
rustmq-admin security cleanup \
  --audit-logs \
  --older-than "1-year" \
  --compress-before-delete

# Comprehensive cleanup
rustmq-admin security cleanup \
  --expired-certs \
  --unused-acl-rules \
  --old-audit-logs \
  --optimize-caches \
  --force
```

#### Security Backup and Restore
```bash
# Create security backup
rustmq-admin security backup \
  --output-file "security-backup-$(date +%Y%m%d).tar.gz" \
  --include-certs \
  --include-acl-rules \
  --include-audit-logs \
  --encrypt \
  --password-file "/etc/rustmq/secrets/backup-password"

# Restore from backup
rustmq-admin security restore \
  --backup-file "security-backup-20240115.tar.gz" \
  --password-file "/etc/rustmq/secrets/backup-password" \
  --verify-before-restore \
  --dry-run

# Verify backup integrity
rustmq-admin security verify-backup \
  --backup-file "security-backup-20240115.tar.gz" \
  --password-file "/etc/rustmq/secrets/backup-password"
```

### Security Configuration Management

#### View and Update Security Configuration
```bash
# View current security configuration
rustmq-admin security config --show-current

# Update security settings
rustmq-admin security config \
  --set "acl.cache_size_mb=100" \
  --set "certificate_management.auto_renewal_enabled=true"

# Export security configuration
rustmq-admin security config \
  --export \
  --output-file "security-config.toml"

# Validate security configuration
rustmq-admin security config \
  --validate \
  --config-file "security-config.toml"
```

## Output Formats and Options

### Available Output Formats

#### Table Format (Default)
```bash
# Human-readable table output
rustmq-admin certs list --format table

# Example output:
CERTIFICATE_ID    COMMON_NAME                      STATUS    EXPIRES_IN
cert_12345       broker-01.internal.company.com   active    335 days
cert_67890       analytics@company.com             active    45 days
```

#### JSON Format
```bash
# Machine-readable JSON output
rustmq-admin certs list --format json

# Pipe to jq for processing
rustmq-admin certs list --format json | jq '.[] | {id: .certificate_id, cn: .common_name}'

# Save JSON output to file
rustmq-admin acl list --format json > acl-rules.json
```

#### YAML Format
```bash
# YAML output for configuration files
rustmq-admin security config --format yaml

# Example output:
security:
  tls:
    enabled: true
    protocol_version: "1.3"
  acl:
    enabled: true
    cache_size_mb: 50
```

#### CSV Format
```bash
# CSV output for spreadsheet import
rustmq-admin certs list --format csv > certificates.csv

# ACL rules for analysis
rustmq-admin acl list --format csv | sort -t, -k3
```

### Global Options

#### Color and Formatting
```bash
# Disable colored output for scripts
rustmq-admin certs list --no-color

# Verbose output with additional details
rustmq-admin security status --verbose

# Quiet output (errors only)
rustmq-admin acl test --principal "user@company.com" --resource "topic.test" --operation "read" --quiet
```

#### API Configuration
```bash
# Custom API endpoint
rustmq-admin --api-url "https://admin-prod.rustmq.company.com" security status

# Custom timeout
rustmq-admin --timeout 60 certs list

# Configuration file
rustmq-admin --config ~/.rustmq/config.toml security status
```

## Automation and Scripting

### Bash Scripting Examples

#### Certificate Renewal Script
```bash
#!/bin/bash
# automated-cert-renewal.sh

set -euo pipefail

# Configuration
API_URL="https://admin.rustmq.company.com"
NOTIFICATION_EMAIL="security@company.com"
LOG_FILE="/var/log/rustmq/cert-renewal.log"

# Function to log messages
log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

# Function to send notification
notify() {
    local subject="$1"
    local message="$2"
    echo "$message" | mail -s "$subject" "$NOTIFICATION_EMAIL"
}

# Main renewal process
main() {
    log "Starting automated certificate renewal process"
    
    # Get certificates expiring in 30 days
    local expiring_certs
    expiring_certs=$(rustmq-admin --api-url "$API_URL" certs expiring --days 30 --format json)
    
    if [[ $(echo "$expiring_certs" | jq '.total_expiring') -eq 0 ]]; then
        log "No certificates require renewal"
        return 0
    fi
    
    # Process each expiring certificate
    echo "$expiring_certs" | jq -r '.expiring_certificates[].certificate_id' | while read -r cert_id; do
        log "Renewing certificate: $cert_id"
        
        if rustmq-admin --api-url "$API_URL" certs renew "$cert_id" --validity-days 365 --preserve-key; then
            log "Successfully renewed certificate: $cert_id"
        else
            log "ERROR: Failed to renew certificate: $cert_id"
            notify "Certificate Renewal Failed" "Failed to renew certificate: $cert_id"
        fi
    done
    
    log "Certificate renewal process completed"
}

# Run main function
main "$@"
```

#### ACL Validation Script
```bash
#!/bin/bash
# validate-acl-changes.sh

set -euo pipefail

ACL_FILE="$1"
TEST_CASES_FILE="$2"

# Validate ACL syntax
echo "Validating ACL syntax..."
if ! rustmq-admin acl validate --input-file "$ACL_FILE" --check-syntax --check-conflicts; then
    echo "ERROR: ACL validation failed"
    exit 1
fi

# Test ACL rules against known test cases
echo "Testing ACL rules..."
if ! rustmq-admin acl bulk-test --input-file "$TEST_CASES_FILE" --format json > test-results.json; then
    echo "ERROR: ACL testing failed"
    exit 1
fi

# Check if all tests passed
local passed_tests
passed_tests=$(jq '.summary.passed' test-results.json)
local total_tests
total_tests=$(jq '.summary.total_tests' test-results.json)

if [[ "$passed_tests" -eq "$total_tests" ]]; then
    echo "SUCCESS: All $total_tests ACL tests passed"
    exit 0
else
    echo "ERROR: Only $passed_tests out of $total_tests tests passed"
    jq '.results[] | select(.passed == false)' test-results.json
    exit 1
fi
```

### Python Automation

#### Certificate Monitoring Script
```python
#!/usr/bin/env python3
"""
Certificate expiry monitoring and alerting
"""

import subprocess
import json
import smtplib
from email.mime.text import MimeText
from datetime import datetime, timedelta
import sys

def run_command(cmd):
    """Run rustmq-admin command and return JSON output"""
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, check=True)
        return json.loads(result.stdout)
    except (subprocess.CalledProcessError, json.JSONDecodeError) as e:
        print(f"Error running command '{cmd}': {e}")
        return None

def send_alert(subject, message, recipients):
    """Send email alert"""
    msg = MimeText(message)
    msg['Subject'] = subject
    msg['From'] = 'rustmq-monitor@company.com'
    msg['To'] = ', '.join(recipients)
    
    try:
        with smtplib.SMTP('localhost') as smtp:
            smtp.send_message(msg)
        print(f"Alert sent: {subject}")
    except Exception as e:
        print(f"Failed to send alert: {e}")

def check_certificate_expiry():
    """Check for expiring certificates and send alerts"""
    # Check certificates expiring in 30, 7, and 1 days
    warning_periods = [30, 7, 1]
    
    for days in warning_periods:
        cmd = f"rustmq-admin certs expiring --days {days} --format json"
        data = run_command(cmd)
        
        if not data or data.get('total_expiring', 0) == 0:
            continue
            
        expiring_certs = data['expiring_certificates']
        
        if expiring_certs:
            message = f"The following certificates expire in {days} days:\n\n"
            for cert in expiring_certs:
                message += f"- {cert['subject']} (ID: {cert['certificate_id']})\n"
                message += f"  Expires: {cert['expires_at']}\n"
                message += f"  Auto-renewal: {'Yes' if cert.get('auto_renewal_enabled') else 'No'}\n\n"
            
            subject = f"RustMQ Certificates Expiring in {days} Days"
            recipients = ['security@company.com', 'ops@company.com']
            send_alert(subject, message, recipients)

def main():
    """Main monitoring function"""
    print(f"Starting certificate expiry check at {datetime.now()}")
    check_certificate_expiry()
    print("Certificate expiry check completed")

if __name__ == "__main__":
    main()
```

### PowerShell Automation (Windows)

#### Security Status Dashboard
```powershell
# security-dashboard.ps1

param(
    [string]$ApiUrl = "https://admin.rustmq.company.com",
    [string]$OutputPath = "security-dashboard.html"
)

# Function to run rustmq-admin command
function Invoke-RustMQCommand {
    param([string]$Command)
    
    try {
        $result = & rustmq-admin --api-url $ApiUrl --format json $Command.Split(' ')
        return $result | ConvertFrom-Json
    }
    catch {
        Write-Error "Failed to run command: $Command"
        return $null
    }
}

# Get security status
$securityStatus = Invoke-RustMQCommand "security status"
$certificateList = Invoke-RustMQCommand "certs list"
$aclStats = Invoke-RustMQCommand "acl cache-stats"

# Generate HTML dashboard
$html = @"
<!DOCTYPE html>
<html>
<head>
    <title>RustMQ Security Dashboard</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        .status-good { color: green; }
        .status-warning { color: orange; }
        .status-error { color: red; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #f2f2f2; }
    </style>
</head>
<body>
    <h1>RustMQ Security Dashboard</h1>
    <p>Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')</p>
    
    <h2>Security Status</h2>
    <p>Overall Status: <span class="status-good">$($securityStatus.security_enabled)</span></p>
    
    <h2>Certificate Summary</h2>
    <table>
        <tr><th>Metric</th><th>Value</th></tr>
        <tr><td>Active Certificates</td><td>$($securityStatus.certificate_authority.total_certificates)</td></tr>
        <tr><td>Expiring (30 days)</td><td>$($securityStatus.certificate_authority.certificates_expiring_30_days)</td></tr>
    </table>
    
    <h2>ACL System</h2>
    <table>
        <tr><th>Metric</th><th>Value</th></tr>
        <tr><td>Total Rules</td><td>$($securityStatus.acl_system.total_rules)</td></tr>
        <tr><td>Cache Hit Rate</td><td>$($securityStatus.acl_system.cache_hit_rate)%</td></tr>
    </table>
</body>
</html>
"@

# Save dashboard
$html | Out-File -FilePath $OutputPath -Encoding UTF8
Write-Host "Security dashboard saved to: $OutputPath"
```

## Best Practices

### Command Usage Best Practices

#### 1. Use Appropriate Output Formats
```bash
# Human consumption - use table format
rustmq-admin certs list --format table

# Script consumption - use JSON format
rustmq-admin certs list --format json | jq '.[] | select(.expires_in_days < 30)'

# Spreadsheet analysis - use CSV format
rustmq-admin acl list --format csv > acl-analysis.csv
```

#### 2. Implement Proper Error Handling
```bash
# Check command exit status
if rustmq-admin certs renew cert_12345; then
    echo "Certificate renewed successfully"
else
    echo "Certificate renewal failed"
    exit 1
fi

# Use dry-run for destructive operations
rustmq-admin acl bulk-delete --filter "unused=true" --dry-run
```

#### 3. Use Configuration Files
```bash
# Create configuration file for consistent settings
cat > ~/.rustmq/config.toml << EOF
[api]
url = "https://admin.rustmq.company.com"
timeout = 30
format = "table"

[auth]
token_file = "/etc/rustmq/secrets/api-token"
EOF

# Use configuration file
rustmq-admin --config ~/.rustmq/config.toml security status
```

### Security Best Practices

#### 1. Principle of Least Privilege
```bash
# Good: Specific permissions
rustmq-admin acl create \
  --principal "analytics@company.com" \
  --resource "topic.user-events" \
  --permissions "read"

# Avoid: Overly broad permissions
# Don't use: --permissions "all" unless absolutely necessary
```

#### 2. Regular Security Audits
```bash
# Weekly certificate health check
rustmq-admin certs health-check --all --output-file weekly-cert-audit.json

# Monthly ACL review
rustmq-admin acl access-review --output-file monthly-acl-review.json

# Quarterly security compliance check
rustmq-admin audit compliance-report --framework "SOX" --period "quarterly"
```

#### 3. Proper Secret Management
```bash
# Store API tokens securely
export RUSTMQ_API_TOKEN_FILE="/etc/rustmq/secrets/api-token"

# Use proper file permissions
chmod 600 /etc/rustmq/secrets/api-token
chown rustmq:rustmq /etc/rustmq/secrets/api-token

# Don't log sensitive commands
rustmq-admin certs issue --principal "secret-service@company.com" 2>/dev/null
```

### Operational Best Practices

#### 1. Automation and Monitoring
```bash
# Set up automated certificate renewal
(crontab -l 2>/dev/null; echo "0 2 * * * /usr/local/bin/automated-cert-renewal.sh") | crontab -

# Monitor security metrics
rustmq-admin security metrics --duration 24h --format prometheus >> /var/lib/prometheus/rustmq-security.prom
```

#### 2. Documentation and Change Management
```bash
# Document all security changes
rustmq-admin acl create \
  --principal "new-service@company.com" \
  --resource "topic.data.*" \
  --permissions "read,write" \
  --description "Ticket #SEC-2024-001: New data processing service"

# Version control ACL rules
rustmq-admin acl export --output-file "acl-rules-$(date +%Y%m%d).json"
git add acl-rules-*.json && git commit -m "ACL rules backup - $(date)"
```

#### 3. Incident Response Preparation
```bash
# Prepare emergency procedures
alias emergency-revoke='rustmq-admin certs emergency-revoke'
alias emergency-disable='rustmq-admin acl emergency-disable'

# Create incident response playbook
cat > incident-response.sh << 'EOF'
#!/bin/bash
# Emergency security incident response

INCIDENT_ID="$1"
AFFECTED_PRINCIPAL="$2"

# Disable all access for compromised principal
rustmq-admin acl emergency-disable \
  --principal "$AFFECTED_PRINCIPAL" \
  --reason "Security incident #$INCIDENT_ID"

# Generate incident report
rustmq-admin audit investigate \
  --incident-id "$INCIDENT_ID" \
  --time-range "last-24-hours" \
  --output-file "incident-$INCIDENT_ID-report.json"
EOF
```

## Troubleshooting

### Common CLI Issues

#### 1. API Connection Issues
```bash
# Problem: Connection refused
# Error: Failed to connect to admin API

# Check API endpoint
curl -f "${RUSTMQ_ADMIN_API_URL}/health" || echo "API not reachable"

# Check network connectivity
ping admin.rustmq.company.com

# Verify TLS certificate
openssl s_client -connect admin.rustmq.company.com:443 -servername admin.rustmq.company.com
```

#### 2. Authentication Issues
```bash
# Problem: Authentication failed
# Error: HTTP 401 Unauthorized

# Check API token
echo "Token: ${RUSTMQ_API_TOKEN:0:10}..." # Show first 10 characters

# Test authentication
rustmq-admin --api-url "$RUSTMQ_ADMIN_API_URL" security status

# Generate new token if needed
rustmq-admin auth token-generate --scope "security:read,security:write"
```

#### 3. Permission Issues
```bash
# Problem: Permission denied
# Error: HTTP 403 Forbidden

# Check current user permissions
rustmq-admin auth whoami

# Check required permissions for operation
rustmq-admin auth required-permissions --operation "certificates:issue"

# Request additional permissions
rustmq-admin auth request-permissions --permissions "certificates:issue"
```

#### 4. Command Syntax Issues
```bash
# Problem: Invalid command syntax
# Use help for correct syntax
rustmq-admin certs --help
rustmq-admin certs issue --help

# Check available options
rustmq-admin --help

# Validate JSON input files
cat acl-rules.json | jq '.' > /dev/null && echo "Valid JSON" || echo "Invalid JSON"
```

### Debugging Commands

#### Enable Verbose Output
```bash
# Verbose mode for detailed output
rustmq-admin --verbose security status

# Debug API requests
export RUSTMQ_DEBUG=true
rustmq-admin certs list

# Trace network requests
export RUSTMQ_TRACE=true
rustmq-admin acl test --principal "user@company.com" --resource "topic.test" --operation "read"
```

#### Validate Configuration
```bash
# Check CLI configuration
rustmq-admin config validate

# Test API connectivity
rustmq-admin config test-connection

# Show current configuration
rustmq-admin config show --include-secrets=false
```

### Performance Troubleshooting

#### Slow Command Execution
```bash
# Measure command execution time
time rustmq-admin acl list

# Check API response times
rustmq-admin --verbose acl test --principal "user@company.com" --resource "topic.test" --operation "read"

# Monitor cache performance
rustmq-admin acl cache-stats --show-slow-queries
```

This CLI Security Guide provides comprehensive coverage of all security commands available in the RustMQ Admin CLI, with practical examples and best practices for production use.