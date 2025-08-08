# RustMQ Admin CLI Security Extension

This document describes the comprehensive security command suite added to the RustMQ Admin CLI tool (`rustmq-admin`). The security extension provides complete command-line access to all certificate management, ACL operations, security auditing, and system maintenance functions.

## Overview

The security extension transforms the admin CLI from basic topic management to a comprehensive security operations tool. It provides:

- **Certificate Authority Management**: Create and manage root and intermediate CAs
- **Certificate Lifecycle**: Issue, renew, rotate, revoke, and validate certificates
- **Access Control Lists (ACL)**: Create, manage, and test authorization rules
- **Security Auditing**: View audit logs, real-time events, and operation history
- **Security Operations**: System status, metrics, health checks, and maintenance

## Architecture

The security CLI extension follows a modular design:

```
src/bin/admin/
├── api_client.rs        # HTTP client for REST API communication
├── formatters.rs        # Output formatting (table, JSON, YAML, CSV)
├── security_commands.rs # Security command implementations
├── tests.rs            # Comprehensive unit tests
└── mod.rs              # Module declarations and exports
```

### Key Design Principles

1. **REST API Integration**: All commands communicate with the Admin REST API, not direct module calls
2. **Multiple Output Formats**: Support for table, JSON, YAML, and CSV output formats
3. **User Experience**: Rich formatting, progress indicators, and confirmation prompts
4. **Error Handling**: Comprehensive error handling with helpful user feedback
5. **Extensibility**: Modular design allows easy addition of new security commands

## Command Structure

The CLI uses a hierarchical command structure with clap derive:

```bash
rustmq-admin [OPTIONS] <COMMAND>

Commands:
  topic     Topic management commands
  ca        Certificate Authority management commands
  certs     Certificate lifecycle management commands
  acl       ACL management commands
  audit     Security audit commands
  security  General security commands
  cluster-health  Check cluster health
  serve-api       Start REST API server
```

### Global Options

- `--api-url <URL>`: Admin API base URL (default: http://127.0.0.1:8080)
- `--format <FORMAT>`: Output format (table, json, yaml, csv)
- `--no-color`: Disable colored output
- `--verbose`: Enable verbose output

## Certificate Authority Commands

### Initialize Root CA

Create a new root Certificate Authority:

```bash
rustmq-admin ca init \
  --cn "RustMQ Root CA" \
  --org "RustMQ Corp" \
  --country US \
  --validity-years 10 \
  --key-size 4096
```

### List Certificate Authorities

```bash
# List all CAs
rustmq-admin ca list

# Filter by status
rustmq-admin ca list --status active

# JSON output
rustmq-admin ca list --format json
```

### Get CA Information

```bash
rustmq-admin ca info <ca_id>
```

### Create Intermediate CA

```bash
rustmq-admin ca intermediate \
  --parent-ca root_ca_1 \
  --cn "RustMQ Intermediate CA" \
  --org "RustMQ Corp" \
  --validity-days 1825
```

## Certificate Management Commands

### Issue Certificate

Issue a new certificate for a principal:

```bash
rustmq-admin certs issue \
  --principal "broker-01.rustmq.com" \
  --role broker \
  --ca-id root_ca_1 \
  --san "broker-01" \
  --san "192.168.1.100" \
  --org "RustMQ Corp" \
  --validity-days 365
```

### List Certificates

```bash
# List all certificates
rustmq-admin certs list

# Filter by status
rustmq-admin certs list --filter active

# Filter by role
rustmq-admin certs list --role broker

# Filter by principal
rustmq-admin certs list --principal "broker-01.rustmq.com"

# Table format with specific columns
rustmq-admin certs list --format table
```

### Certificate Information

```bash
rustmq-admin certs info cert_12345
```

### Certificate Operations

```bash
# Renew certificate
rustmq-admin certs renew cert_12345

# Rotate certificate (generate new key pair)
rustmq-admin certs rotate cert_12345

# Revoke certificate
rustmq-admin certs revoke cert_12345 \
  --reason "key-compromise" \
  --reason-code 1

# Force revoke without confirmation
rustmq-admin certs revoke cert_12345 \
  --reason "superseded" \
  --force
```

### Certificate Status and Validation

```bash
# Check certificate status
rustmq-admin certs status cert_12345

# List expiring certificates
rustmq-admin certs expiring --days 30

# Validate certificate from file
rustmq-admin certs validate \
  --cert-file /path/to/certificate.pem \
  --check-revocation

# Export certificate
rustmq-admin certs export cert_12345 \
  --format pem \
  --output /path/to/exported.pem
```

## Access Control List (ACL) Commands

### Create ACL Rule

```bash
rustmq-admin acl create \
  --principal "user@domain.com" \
  --resource "topic.users.*" \
  --resource-type topic \
  --permissions "read,write" \
  --effect allow \
  --conditions "source_ip=192.168.1.0/24"
```

### List ACL Rules

```bash
# List all rules
rustmq-admin acl list

# Filter by principal
rustmq-admin acl list --principal "user@domain.com"

# Filter by resource
rustmq-admin acl list --resource "topic.users.*"

# Filter by effect
rustmq-admin acl list --effect allow
```

### ACL Rule Management

```bash
# Get rule information
rustmq-admin acl info rule_12345

# Update rule
rustmq-admin acl update rule_12345 \
  --permissions "read" \
  --effect allow

# Delete rule
rustmq-admin acl delete rule_12345

# Force delete without confirmation
rustmq-admin acl delete rule_12345 --force
```

### ACL Evaluation and Testing

```bash
# Test single ACL evaluation
rustmq-admin acl test \
  --principal "user@domain.com" \
  --resource "topic.users.data" \
  --operation read

# Get principal permissions
rustmq-admin acl permissions user@domain.com

# Get rules for resource
rustmq-admin acl rules "topic.users.*"

# Bulk test from file
rustmq-admin acl bulk-test --input-file test_cases.json
```

Example `test_cases.json`:
```json
{
  "evaluations": [
    {
      "principal": "user1@domain.com",
      "resource": "topic.users.data",
      "operation": "read"
    },
    {
      "principal": "user2@domain.com",
      "resource": "topic.admin.logs",
      "operation": "write"
    }
  ]
}
```

### ACL Operations

```bash
# Sync ACL rules to brokers
rustmq-admin acl sync

# Force sync without confirmation
rustmq-admin acl sync --force

# Get ACL version
rustmq-admin acl version

# Cache management
rustmq-admin acl cache invalidate --principals "user1,user2"
rustmq-admin acl cache warm --principals "user1,user2,user3"
```

## Security Audit Commands

### Audit Logs

```bash
# View recent audit logs
rustmq-admin audit logs --limit 50

# Filter by time range
rustmq-admin audit logs \
  --since "2024-01-01T00:00:00Z" \
  --until "2024-01-31T23:59:59Z" \
  --limit 100

# Filter by event type
rustmq-admin audit logs \
  --type certificate_issued \
  --limit 25

# Filter by principal
rustmq-admin audit logs \
  --principal "admin@rustmq.com" \
  --limit 20

# Follow real-time logs
rustmq-admin audit logs --follow
```

### Security Events

```bash
# View real-time security events
rustmq-admin audit events --follow

# Filter events
rustmq-admin audit events --filter authentication
```

### Operation-Specific Audit

```bash
# Certificate operation audit
rustmq-admin audit certificates --operation revoke

# ACL change audit
rustmq-admin audit acl --principal "admin@rustmq.com"
rustmq-admin audit acl --operation create
```

## General Security Commands

### Security Status

```bash
# Overall security status
rustmq-admin security status

# Security performance metrics
rustmq-admin security metrics

# Security health checks
rustmq-admin security health

# Security configuration
rustmq-admin security config
```

### Maintenance Operations

```bash
# Clean up expired certificates
rustmq-admin security cleanup \
  --expired-certs \
  --cache-entries

# Dry run cleanup
rustmq-admin security cleanup \
  --expired-certs \
  --dry-run

# Backup security configuration
rustmq-admin security backup \
  --output /path/to/backup.json \
  --include-certs \
  --include-acl

# Restore from backup
rustmq-admin security restore \
  --input /path/to/backup.json

# Force restore without confirmation
rustmq-admin security restore \
  --input /path/to/backup.json \
  --force
```

## Output Formats

### Table Format (Default)

Human-readable table format with headers and aligned columns:

```
CERTIFICATE_ID        COMMON_NAME              STATUS    EXPIRES_IN_DAYS
cert_12345           broker-01.rustmq.com     active    335
cert_67890           client-01.rustmq.com     active    280
```

### JSON Format

Machine-readable JSON format:

```bash
rustmq-admin certs list --format json
```

```json
[
  {
    "certificate_id": "cert_12345",
    "common_name": "broker-01.rustmq.com",
    "status": "active",
    "not_after": "2025-01-01T00:00:00Z"
  }
]
```

### YAML Format

Configuration-friendly YAML format:

```bash
rustmq-admin security status --format yaml
```

```yaml
overall_status: "healthy"
components:
  authentication:
    status: "healthy"
    last_check: "2024-06-15T10:30:00Z"
```

### CSV Format

Spreadsheet-compatible CSV format:

```bash
rustmq-admin acl list --format csv
```

```csv
rule_id,principal,resource_pattern,operation,effect
rule_12345,user@domain.com,topic.users.*,read,allow
rule_67890,admin@domain.com,topic.*,write,allow
```

## User Experience Features

### Progress Indicators

Long-running operations show progress:

```
Generating root CA certificate ... done
Synchronizing ACL rules ... done
Creating security backup ... done
```

### Confirmation Prompts

Destructive operations require confirmation:

```
Are you sure you want to revoke certificate 'cert_12345'? [y/N]: y
```

Use `--force` to skip confirmations in scripts.

### Color Output

Rich color coding (can be disabled with `--no-color`):

- **Success**: Green
- **Warnings**: Yellow  
- **Errors**: Red
- **Info**: Blue

### Error Handling

Comprehensive error messages with context:

```
Error: Certificate 'cert_nonexistent' not found
Try: rustmq-admin certs list to see available certificates
```

## Integration with REST API

All commands use the Admin REST API endpoints:

- **Certificate Authority**: `/api/v1/security/ca/*`
- **Certificates**: `/api/v1/security/certificates/*`
- **ACL**: `/api/v1/security/acl/*`
- **Audit**: `/api/v1/security/audit/*`
- **Security**: `/api/v1/security/*`

### Authentication

Commands authenticate using the API client configured for the admin REST API. Set the API URL using:

```bash
rustmq-admin --api-url https://rustmq-admin.example.com:8443 security status
```

### Rate Limiting

The CLI handles rate limiting gracefully:

```
Error: Rate limit exceeded. Retry after: 60s, Limit: 100, Remaining: 0
```

## Configuration

### Environment Variables

```bash
export RUSTMQ_ADMIN_API_URL="https://rustmq-admin.example.com:8443"
export RUSTMQ_ADMIN_FORMAT="json"
export RUSTMQ_ADMIN_NO_COLOR="true"
```

### Configuration File

Support for configuration files in future versions.

## Examples and Use Cases

### Certificate Rotation Workflow

```bash
# 1. List expiring certificates
rustmq-admin certs expiring --days 30

# 2. Rotate specific certificate
rustmq-admin certs rotate cert_12345

# 3. Verify new certificate
rustmq-admin certs status cert_12345

# 4. Check audit trail
rustmq-admin audit certificates --operation rotate
```

### ACL Policy Deployment

```bash
# 1. Create user permissions
rustmq-admin acl create \
  --principal "app-service@company.com" \
  --resource "topic.app.*" \
  --permissions "read,write" \
  --effect allow

# 2. Test permissions
rustmq-admin acl test \
  --principal "app-service@company.com" \
  --resource "topic.app.events" \
  --operation write

# 3. Sync to brokers
rustmq-admin acl sync

# 4. Verify deployment
rustmq-admin acl version
```

### Security Compliance Audit

```bash
# 1. Generate security status report
rustmq-admin security status --format json > security_status.json

# 2. List all certificates and their status
rustmq-admin certs list --format csv > certificates.csv

# 3. Export audit logs for time period
rustmq-admin audit logs \
  --since "2024-01-01T00:00:00Z" \
  --until "2024-01-31T23:59:59Z" \
  --format json > audit_january.json

# 4. Check for policy violations
rustmq-admin acl list --effect deny --format csv > denied_rules.csv
```

### Emergency Certificate Revocation

```bash
# 1. Revoke compromised certificate immediately
rustmq-admin certs revoke cert_compromised \
  --reason "key-compromise" \
  --force

# 2. Verify revocation
rustmq-admin certs status cert_compromised

# 3. Invalidate related ACL caches
rustmq-admin acl cache invalidate

# 4. Document incident
rustmq-admin audit certificates \
  --operation revoke \
  --format json > revocation_audit.json
```

## Testing

The security CLI extension includes comprehensive unit tests covering:

- Output formatting for all formats (table, JSON, YAML, CSV)
- API request/response serialization
- Error handling and edge cases
- Complex data structure formatting
- URL encoding and query parameter handling

Run tests with:

```bash
cargo test --bin rustmq-admin
```

## Dependencies

The security CLI extension requires:

1. **Functional Admin REST API**: The security endpoints must be operational
2. **Security Infrastructure**: CertificateManager, AclManager, and SecurityManager must be working
3. **Network Connectivity**: HTTP client access to the admin API server

## Future Enhancements

Planned improvements include:

1. **Configuration Files**: Support for CLI configuration files
2. **Shell Completion**: Tab completion for commands and arguments
3. **Interactive Mode**: Interactive wizards for complex operations
4. **Batch Operations**: Bulk certificate and ACL operations from files
5. **Real-time Monitoring**: Live dashboard for security metrics
6. **Integration Testing**: End-to-end tests with running RustMQ cluster

## Troubleshooting

### Common Issues

1. **Connection Refused**: Check admin API server is running and accessible
2. **Authentication Failed**: Verify API credentials and permissions  
3. **Rate Limited**: Reduce request frequency or increase rate limits
4. **Invalid Arguments**: Use `--help` for command-specific usage information

### Debug Mode

Enable verbose logging:

```bash
rustmq-admin --verbose security status
```

### API Connectivity Test

Test basic connectivity:

```bash
rustmq-admin --api-url http://127.0.0.1:8080 security health
```

This comprehensive security CLI extension provides administrators with powerful, user-friendly tools for managing all aspects of RustMQ security operations through a consistent, well-designed command-line interface.