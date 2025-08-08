# RustMQ Production Security Guide

This comprehensive guide covers security considerations, hardening procedures, and operational best practices for deploying RustMQ in production environments with enterprise-grade security requirements.

## Table of Contents

- [Production Security Overview](#production-security-overview)
- [Pre-Deployment Security Checklist](#pre-deployment-security-checklist)
- [Infrastructure Security](#infrastructure-security)
- [Certificate Management in Production](#certificate-management-in-production)
- [Access Control and Authorization](#access-control-and-authorization)
- [Network Security](#network-security)
- [Monitoring and Incident Response](#monitoring-and-incident-response)
- [Compliance and Auditing](#compliance-and-auditing)
- [Backup and Recovery](#backup-and-recovery)
- [Security Operations](#security-operations)
- [Incident Response Procedures](#incident-response-procedures)

## Production Security Overview

RustMQ's production security model implements a defense-in-depth strategy with multiple layers of security controls designed to protect against various threat vectors while maintaining high performance and availability.

### Security Pillars

1. **Identity and Access Management**: mTLS authentication with fine-grained authorization
2. **Data Protection**: Encryption in transit and at rest with proper key management
3. **Network Security**: Zero trust networking with microsegmentation
4. **Operational Security**: Comprehensive monitoring, auditing, and incident response
5. **Compliance**: Meeting regulatory requirements with proper controls and documentation

### Threat Model

#### Primary Threats
- **Unauthorized Access**: Preventing unauthorized access to message data and cluster resources
- **Data Interception**: Protecting message data in transit and at rest
- **Privilege Escalation**: Preventing elevation of access beyond authorized levels
- **Denial of Service**: Protecting against resource exhaustion and availability attacks
- **Insider Threats**: Mitigating risks from privileged users and compromised accounts

#### Security Controls
- **Preventive**: mTLS, ACLs, network policies, input validation
- **Detective**: Audit logging, monitoring, anomaly detection
- **Corrective**: Automated response, incident procedures, access revocation

## Pre-Deployment Security Checklist

### Infrastructure Requirements

#### Network Security
```bash
# ✅ Verify network segmentation
- [ ] Dedicated VPC/subnet for RustMQ cluster
- [ ] Network policies configured for microsegmentation  
- [ ] Firewall rules restricting access to required ports only
- [ ] Load balancer configured with SSL termination
- [ ] WAF configured for admin API protection

# ✅ DNS and certificate infrastructure
- [ ] Internal DNS resolution for cluster communication
- [ ] External DNS for client access with proper TTL
- [ ] Certificate authority established and secured
- [ ] Certificate distribution mechanism configured
```

#### Storage Security
```bash
# ✅ Persistent storage configuration
- [ ] Encrypted storage volumes for WAL data
- [ ] Object storage bucket with encryption at rest
- [ ] Access controls on storage resources
- [ ] Backup and retention policies defined
- [ ] Storage monitoring and alerting configured
```

#### Compute Security
```bash
# ✅ Host and container security
- [ ] Host OS hardened according to security baseline
- [ ] Container images scanned for vulnerabilities
- [ ] Resource limits and quotas configured
- [ ] Security contexts and policies applied
- [ ] Privileged access controls implemented
```

### Security Configuration

#### Authentication and Authorization
```bash
# ✅ Certificate management
- [ ] Root CA properly secured (HSM or offline storage)
- [ ] Intermediate CAs configured for different environments
- [ ] Certificate templates defined for different roles
- [ ] Automated certificate renewal configured
- [ ] Certificate revocation procedures established

# ✅ Access control
- [ ] ACL rules defined following least privilege principle
- [ ] Service accounts created with minimal permissions
- [ ] User access controls integrated with identity provider
- [ ] Regular access reviews scheduled
- [ ] Emergency access procedures documented
```

#### Monitoring and Auditing
```bash
# ✅ Security monitoring
- [ ] Audit logging enabled for all security events
- [ ] Security metrics collection configured
- [ ] Alerting rules defined for security incidents
- [ ] Log aggregation and retention configured
- [ ] SIEM integration established

# ✅ Compliance monitoring
- [ ] Compliance framework requirements mapped
- [ ] Automated compliance scanning configured
- [ ] Evidence collection procedures established
- [ ] Regular compliance assessments scheduled
```

## Infrastructure Security

### Network Architecture

#### Production Network Topology
```
                    Internet
                       │
                   ┌───▼───┐
                   │  WAF  │
                   └───┬───┘
                       │
               ┌───────▼────────┐
               │  Load Balancer │ ← SSL Termination
               │   (Layer 7)    │
               └───────┬────────┘
                       │
          ┌────────────▼─────────────┐
          │      DMZ Network         │
          │  ┌─────────────────────┐ │
          │  │   Admin API         │ │ ← Management Access
          │  │   (Rate Limited)    │ │
          │  └─────────────────────┘ │
          └────────────┬─────────────┘
                       │
          ┌────────────▼─────────────┐
          │   Application Network    │
          │  ┌─────────────────────┐ │
          │  │  RustMQ Brokers     │ │ ← Client Connections
          │  │  (mTLS Required)    │ │
          │  └─────────────────────┘ │
          └────────────┬─────────────┘
                       │
          ┌────────────▼─────────────┐
          │    Backend Network       │
          │  ┌─────────────────────┐ │
          │  │  RustMQ Controllers │ │ ← Internal Coordination
          │  │  (Raft Consensus)   │ │
          │  └─────────────────────┘ │
          │  ┌─────────────────────┐ │
          │  │  Object Storage     │ │ ← Data Persistence
          │  │  (Encrypted)        │ │
          │  └─────────────────────┘ │
          └─────────────────────────┘
```

#### Network Security Controls

##### Firewall Rules (AWS Security Groups Example)
```bash
# Public Load Balancer Security Group
aws ec2 create-security-group \
  --group-name rustmq-public-lb \
  --description "RustMQ Public Load Balancer"

# Allow HTTPS from internet
aws ec2 authorize-security-group-ingress \
  --group-id sg-12345678 \
  --protocol tcp \
  --port 443 \
  --cidr 0.0.0.0/0

# Admin API Security Group (restricted access)
aws ec2 create-security-group \
  --group-name rustmq-admin-api \
  --description "RustMQ Admin API"

# Allow HTTPS from management networks only
aws ec2 authorize-security-group-ingress \
  --group-id sg-23456789 \
  --protocol tcp \
  --port 8080 \
  --cidr 10.0.100.0/24  # Management subnet

# Broker Security Group
aws ec2 create-security-group \
  --group-name rustmq-brokers \
  --description "RustMQ Brokers"

# Allow QUIC from application networks
aws ec2 authorize-security-group-ingress \
  --group-id sg-34567890 \
  --protocol tcp \
  --port 9092 \
  --cidr 10.0.0.0/16

# Allow gRPC from other brokers and controllers
aws ec2 authorize-security-group-ingress \
  --group-id sg-34567890 \
  --protocol tcp \
  --port 9093 \
  --source-group sg-34567890

aws ec2 authorize-security-group-ingress \
  --group-id sg-34567890 \
  --protocol tcp \
  --port 9093 \
  --source-group sg-45678901  # Controller SG
```

##### VPC Configuration
```bash
# Create dedicated VPC for RustMQ
aws ec2 create-vpc \
  --cidr-block 10.0.0.0/16 \
  --tag-specifications 'ResourceType=vpc,Tags=[{Key=Name,Value=rustmq-production}]'

# Create subnets for different tiers
# Public subnet (load balancers)
aws ec2 create-subnet \
  --vpc-id vpc-12345678 \
  --cidr-block 10.0.1.0/24 \
  --availability-zone us-west-2a \
  --tag-specifications 'ResourceType=subnet,Tags=[{Key=Name,Value=rustmq-public-2a}]'

# Private subnet (brokers)
aws ec2 create-subnet \
  --vpc-id vpc-12345678 \
  --cidr-block 10.0.10.0/24 \
  --availability-zone us-west-2a \
  --tag-specifications 'ResourceType=subnet,Tags=[{Key=Name,Value=rustmq-private-2a}]'

# Database subnet (controllers and storage)
aws ec2 create-subnet \
  --vpc-id vpc-12345678 \
  --cidr-block 10.0.20.0/24 \
  --availability-zone us-west-2a \
  --tag-specifications 'ResourceType=subnet,Tags=[{Key=Name,Value=rustmq-data-2a}]'
```

### Compute Security

#### Host Hardening
```bash
# Security baseline for RustMQ hosts
cat > /etc/security/rustmq-baseline.sh << 'EOF'
#!/bin/bash
# RustMQ Host Security Baseline

# Disable unnecessary services
systemctl disable bluetooth
systemctl disable cups
systemctl disable avahi-daemon

# Configure SSH hardening
sed -i 's/#PermitRootLogin yes/PermitRootLogin no/' /etc/ssh/sshd_config
sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config
echo "AllowUsers rustmq-admin" >> /etc/ssh/sshd_config
systemctl restart sshd

# Configure firewall
ufw default deny incoming
ufw default allow outgoing
ufw allow from 10.0.100.0/24 to any port 22
ufw allow from 10.0.0.0/16 to any port 9092
ufw allow from 10.0.0.0/16 to any port 9093
ufw --force enable

# Configure auditd
cat > /etc/audit/rules.d/rustmq.rules << 'AUDIT_EOF'
# Monitor file access in RustMQ directories
-w /etc/rustmq/ -p rwxa -k rustmq_config
-w /var/lib/rustmq/ -p rwxa -k rustmq_data
-w /var/log/rustmq/ -p rwxa -k rustmq_logs

# Monitor process execution
-a always,exit -F arch=b64 -S execve -F path=/usr/local/bin/rustmq-broker -k rustmq_exec
-a always,exit -F arch=b64 -S execve -F path=/usr/local/bin/rustmq-controller -k rustmq_exec
AUDIT_EOF

systemctl restart auditd

# Configure log rotation
cat > /etc/logrotate.d/rustmq << 'LOGROTATE_EOF'
/var/log/rustmq/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 644 rustmq rustmq
    postrotate
        systemctl reload rustmq-broker || true
        systemctl reload rustmq-controller || true
    endscript
}
LOGROTATE_EOF

echo "RustMQ security baseline applied successfully"
EOF

chmod +x /etc/security/rustmq-baseline.sh
```

#### Container Security
```yaml
# Security-hardened container configuration
apiVersion: v1
kind: Pod
spec:
  securityContext:
    # Run as non-root user
    runAsUser: 1000
    runAsGroup: 1000
    fsGroup: 1000
    runAsNonRoot: true
    
    # Use secure defaults
    seccompProfile:
      type: RuntimeDefault
    
    # Set sysctls for security
    sysctls:
    - name: net.core.somaxconn
      value: "1024"
    - name: vm.mmap_min_addr
      value: "65536"
      
  containers:
  - name: rustmq-broker
    securityContext:
      # Prevent privilege escalation
      allowPrivilegeEscalation: false
      
      # Drop all capabilities and add only required ones
      capabilities:
        drop:
        - ALL
        add:
        - NET_BIND_SERVICE  # Only if binding to privileged ports
        
      # Read-only root filesystem
      readOnlyRootFilesystem: true
      
      # Explicit user settings
      runAsUser: 1000
      runAsGroup: 1000
      runAsNonRoot: true
```

## Certificate Management in Production

### Certificate Authority Infrastructure

#### Root CA Security
```bash
# Production Root CA setup with HSM
cat > /etc/rustmq/ca/root-ca-config.yaml << 'EOF'
root_ca:
  # Use Hardware Security Module for production
  hsm:
    enabled: true
    type: "pkcs11"
    library: "/usr/lib/softhsm2/libsofthsm2.so"
    slot_id: 0
    pin_file: "/etc/rustmq/secrets/hsm-pin"
    
  # Root CA certificate parameters
  subject:
    common_name: "RustMQ Production Root CA"
    organization: "Your Organization"
    country: "US"
    
  # Security parameters
  key_algorithm: "RSA"
  key_size: 4096
  signature_algorithm: "SHA256WithRSA"
  validity_years: 20
  
  # Certificate policies
  policies:
    - oid: "1.3.6.1.4.1.99999.1.1"
      name: "RustMQ Certificate Policy"
      cps_uri: "https://pki.company.com/cps"
      
  # Key usage restrictions
  key_usage:
    - "cert_sign"
    - "crl_sign"
    
  # Basic constraints
  basic_constraints:
    ca: true
    path_length: 2
EOF

# Initialize root CA with HSM
rustmq-admin ca init \
  --config /etc/rustmq/ca/root-ca-config.yaml \
  --use-hsm \
  --multi-person-auth \
  --audit-log /var/log/rustmq/ca-audit.log
```

#### Intermediate CA Hierarchy
```bash
# Production Intermediate CA
rustmq-admin ca intermediate \
  --parent-ca root_ca_1 \
  --cn "RustMQ Production Intermediate CA" \
  --org "Your Organization" \
  --ou "Production Infrastructure" \
  --validity-years 10 \
  --path-length 1 \
  --key-size 3072 \
  --policy "1.3.6.1.4.1.99999.1.2" \
  --audit-required

# Staging Intermediate CA
rustmq-admin ca intermediate \
  --parent-ca root_ca_1 \
  --cn "RustMQ Staging Intermediate CA" \
  --org "Your Organization" \
  --ou "Staging Infrastructure" \
  --validity-years 5 \
  --path-length 0 \
  --key-size 2048

# Development Intermediate CA  
rustmq-admin ca intermediate \
  --parent-ca root_ca_1 \
  --cn "RustMQ Development Intermediate CA" \
  --org "Your Organization" \
  --ou "Development Infrastructure" \
  --validity-years 2 \
  --path-length 0 \
  --key-size 2048
```

### Certificate Lifecycle Automation

#### Production Certificate Templates
```toml
# /etc/rustmq/ca/production-templates.toml

[templates.production_broker]
subject_template = "CN={{hostname}}.prod.rustmq.company.com,O=Your Organization,OU=Production Brokers"
validity_days = 365
key_type = "RSA"
key_size = 3072
signature_algorithm = "SHA256WithRSA"

# Key usage for broker certificates
key_usage = ["digital_signature", "key_encipherment", "key_agreement"]
extended_key_usage = ["server_auth", "client_auth"]

# Subject Alternative Names
san_dns = ["{{hostname}}.prod.rustmq.company.com", "{{hostname}}.internal"]
san_ip = ["{{pod_ip}}", "{{node_ip}}"]

# Security requirements
require_san = true
max_path_length = 0
require_key_backup = false

# Monitoring and alerting
renewal_before_expiry_days = 30
renewal_retry_attempts = 3
alert_on_renewal_failure = true
alert_webhook = "https://alerts.company.com/webhook/cert-renewal"

[templates.production_client]
subject_template = "CN={{principal}},O=Your Organization,OU=Production Clients"
validity_days = 90
key_type = "RSA"
key_size = 2048
signature_algorithm = "SHA256WithRSA"

key_usage = ["digital_signature"]
extended_key_usage = ["client_auth"]

# Client-specific security
require_pin_verification = false
max_uses_per_certificate = -1  # Unlimited
require_hardware_key = false

# Short validity for security
renewal_before_expiry_days = 14
auto_renewal_enabled = true

[templates.production_admin]
subject_template = "CN={{principal}},O=Your Organization,OU=Production Administrators"
validity_days = 7
key_type = "ECDSA"
curve = "P-384"
signature_algorithm = "SHA384WithECDSA"

key_usage = ["digital_signature"]
extended_key_usage = ["client_auth", "code_signing"]

# Enhanced security for admin certificates
require_pin_verification = true
require_hardware_key = true
max_uses_per_day = 50
session_timeout_minutes = 60

# Very short validity for administrative access
renewal_before_expiry_days = 2
auto_renewal_enabled = false  # Manual renewal required
require_approval = true
```

#### Automated Certificate Operations
```bash
# Production certificate renewal automation
cat > /usr/local/bin/rustmq-cert-renewal.sh << 'EOF'
#!/bin/bash
# Production Certificate Renewal Automation

set -euo pipefail

# Configuration
RENEWAL_THRESHOLD_DAYS=30
LOG_FILE="/var/log/rustmq/cert-renewal.log"
ALERT_WEBHOOK="https://alerts.company.com/webhook/cert-renewal"
BACKUP_DIR="/backup/rustmq/certificates"

# Logging function
log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

# Alert function
alert() {
    local severity="$1"
    local message="$2"
    
    curl -X POST "$ALERT_WEBHOOK" \
        -H "Content-Type: application/json" \
        -d "{
            \"severity\": \"$severity\",
            \"message\": \"$message\",
            \"timestamp\": \"$(date -Iseconds)\",
            \"source\": \"rustmq-cert-renewal\"
        }" || log "ERROR: Failed to send alert"
}

# Backup function
backup_certificate() {
    local cert_id="$1"
    local backup_path="$BACKUP_DIR/$(date +%Y%m%d)/$cert_id"
    
    mkdir -p "$backup_path"
    
    rustmq-admin certs export \
        --cert-id "$cert_id" \
        --include-private-key \
        --output-dir "$backup_path" \
        --encrypt \
        --password-file "/etc/rustmq/secrets/backup-password"
}

# Main renewal process
main() {
    log "Starting certificate renewal process"
    
    # Get certificates expiring soon
    local expiring_certs
    expiring_certs=$(rustmq-admin certs expiring \
        --days "$RENEWAL_THRESHOLD_DAYS" \
        --format json \
        --filter "auto_renewal_enabled=true")
    
    local cert_count
    cert_count=$(echo "$expiring_certs" | jq '.total_expiring')
    
    if [[ "$cert_count" -eq 0 ]]; then
        log "No certificates require renewal"
        return 0
    fi
    
    log "Found $cert_count certificates requiring renewal"
    
    # Process each certificate
    echo "$expiring_certs" | jq -r '.expiring_certificates[].certificate_id' | while read -r cert_id; do
        log "Processing certificate: $cert_id"
        
        # Backup current certificate
        if backup_certificate "$cert_id"; then
            log "Certificate backup completed: $cert_id"
        else
            log "WARNING: Certificate backup failed: $cert_id"
        fi
        
        # Attempt renewal
        if rustmq-admin certs renew "$cert_id" \
            --validity-days 365 \
            --preserve-key \
            --audit-comment "Automated renewal"; then
            log "Successfully renewed certificate: $cert_id"
            alert "info" "Certificate renewed successfully: $cert_id"
        else
            log "ERROR: Failed to renew certificate: $cert_id"
            alert "critical" "Certificate renewal failed: $cert_id"
            
            # Continue with other certificates
            continue
        fi
        
        # Verify renewed certificate
        if rustmq-admin certs validate --cert-id "$cert_id"; then
            log "Certificate validation passed: $cert_id"
        else
            log "ERROR: Certificate validation failed after renewal: $cert_id"
            alert "critical" "Certificate validation failed after renewal: $cert_id"
        fi
    done
    
    # Generate renewal report
    rustmq-admin certs renewal-report \
        --output-file "/var/log/rustmq/renewal-report-$(date +%Y%m%d).json" \
        --include-metrics
    
    log "Certificate renewal process completed"
}

# Cleanup function
cleanup() {
    # Remove old backups (keep 90 days)
    find "$BACKUP_DIR" -type d -mtime +90 -exec rm -rf {} \; 2>/dev/null || true
    
    # Compress old logs
    find /var/log/rustmq -name "*.log" -mtime +7 -exec gzip {} \; 2>/dev/null || true
}

# Run main process
main "$@"
cleanup

exit 0
EOF

chmod +x /usr/local/bin/rustmq-cert-renewal.sh

# Add to cron for daily execution
(crontab -l 2>/dev/null; echo "0 2 * * * /usr/local/bin/rustmq-cert-renewal.sh") | crontab -
```

## Access Control and Authorization

### Production ACL Strategy

#### Role-Based Access Control
```bash
# Define standard roles with specific permissions

# Broker Role
rustmq-admin acl create \
  --name "broker_internal_access" \
  --principal "*.broker.prod.rustmq.company.com" \
  --resource "admin.cluster.*" \
  --permissions "read,write" \
  --effect "allow" \
  --priority 100 \
  --description "Broker access to cluster management functions"

# Application Service Role
rustmq-admin acl create \
  --name "app_service_data_access" \
  --principal "*@app.company.com" \
  --resource "topic.app.*" \
  --permissions "read,write" \
  --effect "allow" \
  --conditions "source_ip=10.0.0.0/16" \
  --priority 200 \
  --description "Application services data access"

# Analytics Service Role
rustmq-admin acl create \
  --name "analytics_read_only" \
  --principal "analytics@company.com" \
  --resource "topic.*" \
  --permissions "read" \
  --effect "allow" \
  --conditions "time_range=00:00-23:59" \
  --priority 300 \
  --description "Analytics service read-only access"

# Administrative Role
rustmq-admin acl create \
  --name "admin_full_access" \
  --principal "*@admin.company.com" \
  --resource "*" \
  --permissions "all" \
  --effect "allow" \
  --conditions "source_ip=10.0.100.0/24,time_range=06:00-20:00" \
  --priority 400 \
  --description "Administrative access during business hours"
```

#### Security Hardening Rules
```bash
# Deny external access to internal topics
rustmq-admin acl create \
  --name "deny_external_internal_access" \
  --principal "*@external.com" \
  --resource "topic.internal.*" \
  --permissions "all" \
  --effect "deny" \
  --priority 1000 \
  --description "Block external access to internal topics"

# Deny delete operations on production topics
rustmq-admin acl create \
  --name "protect_production_topics" \
  --principal "*" \
  --resource "topic.prod.*" \
  --permissions "delete" \
  --effect "deny" \
  --priority 900 \
  --description "Prevent deletion of production topics"

# Limit high-privilege operations to specific time windows
rustmq-admin acl create \
  --name "limit_admin_operations" \
  --principal "*" \
  --resource "admin.cluster.config" \
  --permissions "write,delete" \
  --effect "deny" \
  --conditions "time_range=!09:00-17:00" \
  --priority 800 \
  --description "Restrict cluster configuration changes to business hours"

# Rate limiting for bulk operations
rustmq-admin acl create \
  --name "rate_limit_bulk_operations" \
  --principal "*" \
  --resource "admin.bulk.*" \
  --permissions "write" \
  --effect "allow" \
  --conditions "max_requests_per_minute=10" \
  --priority 700 \
  --description "Rate limit bulk administrative operations"
```

#### Environment Separation
```bash
# Production environment access controls
rustmq-admin acl create \
  --name "production_environment_isolation" \
  --principal "*@staging.company.com,*@dev.company.com" \
  --resource "topic.prod.*" \
  --permissions "all" \
  --effect "deny" \
  --priority 1500 \
  --description "Isolate production from non-production principals"

# Cross-environment data access (read-only)
rustmq-admin acl create \
  --name "staging_read_prod_data" \
  --principal "staging-analytics@company.com" \
  --resource "topic.prod.public.*" \
  --permissions "read" \
  --effect "allow" \
  --conditions "source_ip=10.1.0.0/16" \
  --priority 600 \
  --description "Staging analytics read access to public prod data"
```

### Identity Integration

#### LDAP/Active Directory Integration
```toml
# /etc/rustmq/config/ldap-integration.toml

[security.ldap]
enabled = true
server_url = "ldaps://ldap.company.com:636"
bind_dn = "cn=rustmq,ou=service-accounts,dc=company,dc=com"
bind_password_file = "/etc/rustmq/secrets/ldap-password"

# Search configuration
search_base = "dc=company,dc=com"
user_search_filter = "(&(objectClass=person)(sAMAccountName={0}))"
group_search_filter = "(&(objectClass=group)(member={0}))"

# Attribute mapping
username_attribute = "sAMAccountName"
email_attribute = "mail"
display_name_attribute = "displayName"
group_attribute = "memberOf"

# TLS configuration
tls_ca_cert_path = "/etc/rustmq/certs/ldap-ca.pem"
tls_verify_certificate = true
tls_minimum_version = "1.2"

# Group to role mapping
[security.ldap.group_mappings]
"CN=RustMQ-Admins,OU=Groups,DC=company,DC=com" = "admin"
"CN=RustMQ-Operators,OU=Groups,DC=company,DC=com" = "operator"
"CN=RustMQ-Developers,OU=Groups,DC=company,DC=com" = "developer"
"CN=RustMQ-ReadOnly,OU=Groups,DC=company,DC=com" = "readonly"

# Cache configuration
[security.ldap.cache]
enabled = true
cache_ttl_seconds = 300
max_cache_entries = 10000
negative_cache_ttl_seconds = 60
```

#### OAuth 2.0 / OIDC Integration
```toml
# /etc/rustmq/config/oauth-integration.toml

[security.oauth]
enabled = true
provider_url = "https://auth.company.com"
client_id = "rustmq-production"
client_secret_file = "/etc/rustmq/secrets/oauth-client-secret"

# Discovery and validation
discovery_url = "https://auth.company.com/.well-known/openid_configuration"
issuer = "https://auth.company.com"
audience = "rustmq-api"

# JWT validation
jwt_signing_algorithm = "RS256"
jwt_public_key_url = "https://auth.company.com/.well-known/jwks.json"
jwt_validation_leeway_seconds = 30

# Claim mapping
username_claim = "preferred_username"
email_claim = "email"
groups_claim = "groups"
roles_claim = "realm_access.roles"

# Token configuration
access_token_ttl_seconds = 3600
refresh_token_ttl_seconds = 86400
token_validation_cache_ttl = 300

# Role mapping
[security.oauth.role_mappings]
"rustmq-admin" = "admin"
"rustmq-operator" = "operator"
"rustmq-developer" = "developer"
"rustmq-readonly" = "readonly"
```

## Network Security

### Production Network Configuration

#### Load Balancer Security
```yaml
# AWS Application Load Balancer configuration
Resources:
  RustMQLoadBalancer:
    Type: AWS::ElasticLoadBalancingV2::LoadBalancer
    Properties:
      Name: rustmq-production-alb
      Type: application
      Scheme: internet-facing
      SecurityGroups:
        - !Ref RustMQALBSecurityGroup
      Subnets:
        - !Ref PublicSubnet1
        - !Ref PublicSubnet2
      LoadBalancerAttributes:
        - Key: idle_timeout.timeout_seconds
          Value: "60"
        - Key: routing.http.drop_invalid_header_fields.enabled
          Value: "true"
        - Key: routing.http.preserve_host_header.enabled
          Value: "true"
        - Key: routing.http.x_amzn_trace_id.enabled
          Value: "true"
        - Key: load_balancing.cross_zone.enabled
          Value: "true"

  # Target Group for RustMQ brokers
  RustMQTargetGroup:
    Type: AWS::ElasticLoadBalancingV2::TargetGroup
    Properties:
      Name: rustmq-brokers
      Protocol: HTTPS
      Port: 9092
      VpcId: !Ref VPC
      TargetType: ip
      HealthCheckProtocol: HTTPS
      HealthCheckPath: /health
      HealthCheckPort: 9642
      HealthCheckIntervalSeconds: 30
      HealthCheckTimeoutSeconds: 10
      HealthyThresholdCount: 2
      UnhealthyThresholdCount: 3
      TargetGroupAttributes:
        - Key: deregistration_delay.timeout_seconds
          Value: "30"
        - Key: stickiness.enabled
          Value: "false"
        - Key: load_balancing.algorithm.type
          Value: "round_robin"

  # Listener with SSL termination
  RustMQALBListener:
    Type: AWS::ElasticLoadBalancingV2::Listener
    Properties:
      LoadBalancerArn: !Ref RustMQLoadBalancer
      Protocol: HTTPS
      Port: 443
      SslPolicy: ELBSecurityPolicy-TLS-1-2-2017-01
      Certificates:
        - CertificateArn: !Ref SSLCertificate
      DefaultActions:
        - Type: forward
          TargetGroupArn: !Ref RustMQTargetGroup
```

#### WAF Configuration
```yaml
# AWS WAF for RustMQ Admin API protection
Resources:
  RustMQWebACL:
    Type: AWS::WAFv2::WebACL
    Properties:
      Name: rustmq-production-waf
      Scope: REGIONAL
      DefaultAction:
        Allow: {}
      Rules:
        # Rate limiting rule
        - Name: RateLimitRule
          Priority: 1
          Statement:
            RateBasedStatement:
              Limit: 1000
              AggregateKeyType: IP
          Action:
            Block: {}
          VisibilityConfig:
            SampledRequestsEnabled: true
            CloudWatchMetricsEnabled: true
            MetricName: RustMQRateLimit

        # Geographic restriction
        - Name: GeoRestrictionRule
          Priority: 2
          Statement:
            GeoMatchStatement:
              CountryCodes:
                - CN
                - RU
                - KP
          Action:
            Block: {}
          VisibilityConfig:
            SampledRequestsEnabled: true
            CloudWatchMetricsEnabled: true
            MetricName: RustMQGeoBlock

        # SQL injection protection
        - Name: SQLInjectionRule
          Priority: 3
          Statement:
            SqliMatchStatement:
              FieldToMatch:
                Body: {}
              TextTransformations:
                - Priority: 0
                  Type: URL_DECODE
                - Priority: 1
                  Type: HTML_ENTITY_DECODE
          Action:
            Block: {}
          VisibilityConfig:
            SampledRequestsEnabled: true
            CloudWatchMetricsEnabled: true
            MetricName: RustMQSQLInjection

        # Known bad inputs
        - Name: KnownBadInputsRule
          Priority: 4
          Statement:
            ManagedRuleGroupStatement:
              VendorName: AWS
              Name: AWSManagedRulesKnownBadInputsRuleSet
          OverrideAction:
            None: {}
          VisibilityConfig:
            SampledRequestsEnabled: true
            CloudWatchMetricsEnabled: true
            MetricName: RustMQKnownBadInputs

      VisibilityConfig:
        SampledRequestsEnabled: true
        CloudWatchMetricsEnabled: true
        MetricName: RustMQWebACL
```

#### Network Monitoring
```bash
# VPC Flow Logs configuration
aws ec2 create-flow-logs \
  --resource-type VPC \
  --resource-ids vpc-12345678 \
  --traffic-type ALL \
  --log-destination-type cloud-watch-logs \
  --log-group-name /aws/vpc/flowlogs \
  --deliver-logs-permission-arn arn:aws:iam::123456789012:role/flowlogsRole

# CloudTrail for API monitoring
aws cloudtrail create-trail \
  --name rustmq-production-trail \
  --s3-bucket-name rustmq-audit-logs \
  --s3-key-prefix cloudtrail/ \
  --include-global-service-events \
  --is-multi-region-trail \
  --enable-log-file-validation

# GuardDuty for threat detection
aws guardduty create-detector \
  --enable
```

## Monitoring and Incident Response

### Security Monitoring

#### Security Metrics Collection
```yaml
# Prometheus configuration for security metrics
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - "rustmq-security-rules.yml"

scrape_configs:
  # RustMQ security metrics
  - job_name: 'rustmq-security'
    static_configs:
      - targets: ['rustmq-broker-1:9642', 'rustmq-broker-2:9642', 'rustmq-broker-3:9642']
    metrics_path: /metrics
    scheme: https
    tls_config:
      cert_file: /etc/prometheus/certs/client.pem
      key_file: /etc/prometheus/certs/client.key
      ca_file: /etc/prometheus/certs/ca.pem
    metric_relabel_configs:
      # Keep only security-related metrics
      - source_labels: [__name__]
        regex: 'rustmq_(auth|tls|cert|acl)_.*'
        action: keep

  # System security metrics
  - job_name: 'node-security'
    static_configs:
      - targets: ['node-1:9100', 'node-2:9100', 'node-3:9100']
    metric_relabel_configs:
      - source_labels: [__name__]
        regex: 'node_(security|audit|firewall)_.*'
        action: keep

alerting:
  alertmanagers:
    - static_configs:
        - targets:
          - alertmanager:9093
```

#### Security Alerting Rules
```yaml
# /etc/prometheus/rustmq-security-rules.yml
groups:
- name: rustmq.security
  rules:
  # Authentication failures
  - alert: RustMQHighAuthFailureRate
    expr: rate(rustmq_authentication_failures_total[5m]) > 0.1
    for: 2m
    labels:
      severity: warning
      category: security
    annotations:
      summary: "High authentication failure rate detected"
      description: "RustMQ authentication failure rate is {{ $value }}/sec for the last 5 minutes"
      runbook_url: "https://runbooks.company.com/rustmq/auth-failures"

  # Authorization denials
  - alert: RustMQUnauthorizedAccessAttempts
    expr: rate(rustmq_authorization_denied_total[5m]) > 0.05
    for: 5m
    labels:
      severity: critical
      category: security
    annotations:
      summary: "High authorization denial rate"
      description: "RustMQ authorization denial rate is {{ $value }}/sec, indicating potential unauthorized access attempts"
      runbook_url: "https://runbooks.company.com/rustmq/unauthorized-access"

  # Certificate expiry
  - alert: RustMQCertificateExpiryWarning
    expr: rustmq_certificate_expiry_days < 30
    for: 1h
    labels:
      severity: warning
      category: security
    annotations:
      summary: "Certificate expiring soon"
      description: "Certificate {{ $labels.certificate_id }} expires in {{ $value }} days"
      runbook_url: "https://runbooks.company.com/rustmq/cert-expiry"

  - alert: RustMQCertificateExpiryCritical
    expr: rustmq_certificate_expiry_days < 7
    for: 10m
    labels:
      severity: critical
      category: security
    annotations:
      summary: "Certificate expiring very soon"
      description: "Certificate {{ $labels.certificate_id }} expires in {{ $value }} days - immediate action required"
      runbook_url: "https://runbooks.company.com/rustmq/cert-expiry"

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
      runbook_url: "https://runbooks.company.com/rustmq/tls-failures"

  # ACL cache performance
  - alert: RustMQACLCacheHitRateLow
    expr: rustmq_acl_cache_hit_rate < 0.8
    for: 10m
    labels:
      severity: warning
      category: performance
    annotations:
      summary: "Low ACL cache hit rate"
      description: "ACL cache hit rate is {{ $value }}, which may impact authorization performance"
      runbook_url: "https://runbooks.company.com/rustmq/acl-performance"

  # Security audit log issues
  - alert: RustMQAuditLogFailures
    expr: rate(rustmq_audit_log_failures_total[5m]) > 0
    for: 1m
    labels:
      severity: critical
      category: security
    annotations:
      summary: "Audit log failures detected"
      description: "Audit logging is failing at {{ $value }}/sec rate"
      runbook_url: "https://runbooks.company.com/rustmq/audit-failures"
```

### SIEM Integration

#### Log Forwarding Configuration
```yaml
# Fluent Bit configuration for security log forwarding
apiVersion: v1
kind: ConfigMap
metadata:
  name: fluent-bit-config
  namespace: logging
data:
  fluent-bit.conf: |
    [SERVICE]
        Flush         5
        Log_Level     info
        Daemon        off
        Parsers_File  parsers.conf
        
    [INPUT]
        Name              tail
        Path              /var/log/rustmq/security-audit.log
        Parser            json
        Tag               rustmq.security.audit
        Refresh_Interval  5
        Mem_Buf_Limit     50MB
        
    [INPUT]
        Name              tail
        Path              /var/log/rustmq/authentication.log
        Parser            json
        Tag               rustmq.security.auth
        Refresh_Interval  5
        Mem_Buf_Limit     50MB
        
    [FILTER]
        Name              record_modifier
        Match             rustmq.security.*
        Record            environment production
        Record            application rustmq
        Record            datacenter us-west-2
        
    [FILTER]
        Name              grep
        Match             rustmq.security.*
        Regex             severity (ERROR|WARN|CRITICAL)
        
    [OUTPUT]
        Name              forward
        Match             rustmq.security.*
        Host              splunk-forwarder.company.com
        Port              24224
        tls               on
        tls.verify        on
        tls.ca_file       /etc/ssl/certs/splunk-ca.pem
        tls.crt_file      /etc/ssl/certs/client.pem
        tls.key_file      /etc/ssl/private/client.key
        
  parsers.conf: |
    [PARSER]
        Name              json
        Format            json
        Time_Key          timestamp
        Time_Format       %Y-%m-%dT%H:%M:%S.%L%z
        Time_Keep         On
```

#### Splunk Integration
```bash
# Splunk Universal Forwarder configuration
cat > /opt/splunkforwarder/etc/apps/rustmq/local/inputs.conf << 'EOF'
[monitor:///var/log/rustmq/security-audit.log]
disabled = false
index = security
sourcetype = rustmq:security:audit
source = rustmq-audit

[monitor:///var/log/rustmq/authentication.log]
disabled = false
index = security
sourcetype = rustmq:security:auth
source = rustmq-auth

[monitor:///var/log/rustmq/authorization.log]
disabled = false
index = security
sourcetype = rustmq:security:authz
source = rustmq-authz

[monitor:///var/log/audit/audit.log]
disabled = false
index = security
sourcetype = linux:audit
source = system-audit
EOF

# Splunk search queries for security monitoring
cat > /opt/splunk/etc/apps/rustmq_security/local/savedsearches.conf << 'EOF'
[RustMQ Authentication Failures]
search = index=security sourcetype="rustmq:security:auth" event_type="authentication_failure" | timechart span=5m count by source_ip
dispatch.earliest_time = -1h
dispatch.latest_time = now
cron_schedule = */5 * * * *
is_scheduled = 1
alert.track = 1
alert.digest_mode = 1
action.email = 1
action.email.to = security@company.com
action.email.subject = RustMQ Authentication Failure Alert

[RustMQ Unauthorized Access Attempts]
search = index=security sourcetype="rustmq:security:authz" result="denied" | stats count by principal, resource, source_ip | where count > 10
dispatch.earliest_time = -15m
dispatch.latest_time = now
cron_schedule = */15 * * * *
is_scheduled = 1
alert.track = 1
action.email = 1
action.email.to = security@company.com,soc@company.com
action.email.subject = RustMQ Unauthorized Access Alert
EOF
```

### Incident Response Procedures

#### Security Incident Response Plan
```bash
# Security incident response automation
cat > /usr/local/bin/rustmq-security-incident-response.sh << 'EOF'
#!/bin/bash
# RustMQ Security Incident Response Script

set -euo pipefail

INCIDENT_ID="$1"
INCIDENT_TYPE="$2"
AFFECTED_PRINCIPAL="${3:-}"
SEVERITY="${4:-medium}"

# Configuration
INCIDENT_LOG="/var/log/rustmq/incidents/incident-${INCIDENT_ID}.log"
ALERT_WEBHOOK="https://alerts.company.com/webhook/security-incident"
SOC_EMAIL="soc@company.com"
SECURITY_EMAIL="security@company.com"

# Logging
exec > >(tee -a "$INCIDENT_LOG") 2>&1

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') [${INCIDENT_ID}] $1"
}

alert() {
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
    
    # Email notification for critical incidents
    if [[ "$SEVERITY" == "critical" || "$SEVERITY" == "high" ]]; then
        echo "$message" | mail -s "RustMQ Security Incident: $INCIDENT_ID" "$SOC_EMAIL,$SECURITY_EMAIL"
    fi
}

# Incident response functions
handle_unauthorized_access() {
    log "Handling unauthorized access incident"
    
    if [[ -n "$AFFECTED_PRINCIPAL" ]]; then
        # Temporarily disable the principal
        log "Disabling access for principal: $AFFECTED_PRINCIPAL"
        rustmq-admin acl emergency-disable \
            --principal "$AFFECTED_PRINCIPAL" \
            --reason "Security incident: $INCIDENT_ID" \
            --audit-comment "Automated response to unauthorized access"
        
        # Revoke active sessions if supported
        rustmq-admin auth revoke-sessions \
            --principal "$AFFECTED_PRINCIPAL" \
            --reason "Security incident: $INCIDENT_ID"
    fi
    
    # Increase monitoring for the affected resource
    log "Increasing monitoring sensitivity"
    rustmq-admin security monitor \
        --increase-sensitivity \
        --duration "1h" \
        --incident-id "$INCIDENT_ID"
}

handle_certificate_compromise() {
    log "Handling certificate compromise incident"
    
    if [[ -n "$AFFECTED_PRINCIPAL" ]]; then
        # Find and revoke all certificates for the principal
        local cert_ids
        cert_ids=$(rustmq-admin certs list \
            --principal "$AFFECTED_PRINCIPAL" \
            --status active \
            --format json | jq -r '.[].certificate_id')
        
        for cert_id in $cert_ids; do
            log "Revoking certificate: $cert_id"
            rustmq-admin certs emergency-revoke "$cert_id" \
                --reason "key-compromise" \
                --incident-id "$INCIDENT_ID" \
                --effective-immediately
        done
        
        # Force CRL update
        log "Updating certificate revocation list"
        rustmq-admin ca generate-crl --force-update
    fi
}

handle_authentication_anomaly() {
    log "Handling authentication anomaly"
    
    # Collect authentication logs for analysis
    log "Collecting authentication logs for analysis"
    rustmq-admin audit investigate \
        --incident-id "$INCIDENT_ID" \
        --event-type "authentication" \
        --time-range "last-1-hour" \
        --output-file "/var/log/rustmq/incidents/auth-analysis-${INCIDENT_ID}.json"
    
    # Increase authentication logging
    log "Enabling detailed authentication logging"
    rustmq-admin config set security.audit.log_authentication_events true
    rustmq-admin config set security.audit.log_authorization_events true
}

generate_incident_report() {
    log "Generating incident report"
    
    local report_file="/var/log/rustmq/incidents/report-${INCIDENT_ID}.json"
    
    cat > "$report_file" << REPORT_EOF
{
    "incident_id": "$INCIDENT_ID",
    "incident_type": "$INCIDENT_TYPE",
    "severity": "$SEVERITY",
    "affected_principal": "$AFFECTED_PRINCIPAL",
    "start_time": "$(date -Iseconds)",
    "actions_taken": [
        $(grep "^\[" "$INCIDENT_LOG" | jq -R . | paste -sd, -)
    ],
    "system_state": {
        "security_status": $(rustmq-admin security status --format json),
        "active_certificates": $(rustmq-admin certs list --status active --format json | jq length),
        "acl_rules": $(rustmq-admin acl list --format json | jq length)
    }
}
REPORT_EOF
    
    log "Incident report generated: $report_file"
}

# Main incident response logic
main() {
    log "Starting incident response for: $INCIDENT_TYPE (Severity: $SEVERITY)"
    
    alert "Security incident response initiated: $INCIDENT_TYPE"
    
    case "$INCIDENT_TYPE" in
        "unauthorized_access")
            handle_unauthorized_access
            ;;
        "certificate_compromise")
            handle_certificate_compromise
            ;;
        "authentication_anomaly")
            handle_authentication_anomaly
            ;;
        *)
            log "Unknown incident type: $INCIDENT_TYPE"
            alert "Unknown incident type encountered: $INCIDENT_TYPE"
            ;;
    esac
    
    generate_incident_report
    
    alert "Security incident response completed for: $INCIDENT_TYPE"
    log "Incident response completed"
}

# Create incident directory
mkdir -p "/var/log/rustmq/incidents"

# Run main function
main "$@"
EOF

chmod +x /usr/local/bin/rustmq-security-incident-response.sh
```

## Compliance and Auditing

### Compliance Framework Implementation

#### SOX Compliance Configuration
```toml
# /etc/rustmq/compliance/sox-config.toml

[compliance.sox]
enabled = true
framework = "SOX"
control_period = "quarterly"

# Required controls
[compliance.sox.controls]
# Access controls
AC_1 = {
    description = "Segregation of duties for administrative access"
    implementation = "Multi-person authorization for sensitive operations"
    testing_frequency = "monthly"
    evidence_collection = "automatic"
}

AC_2 = {
    description = "Periodic access reviews"
    implementation = "Automated quarterly access review reports"
    testing_frequency = "quarterly"
    evidence_collection = "automatic"
}

# Audit controls
AU_1 = {
    description = "Comprehensive audit logging"
    implementation = "All security events logged with immutable storage"
    testing_frequency = "continuous"
    evidence_collection = "automatic"
}

AU_2 = {
    description = "Audit log integrity protection"
    implementation = "Cryptographic signatures and centralized storage"
    testing_frequency = "daily"
    evidence_collection = "automatic"
}

# Change management controls
CM_1 = {
    description = "Configuration change approval process"
    implementation = "Approval workflow for security configuration changes"
    testing_frequency = "continuous"
    evidence_collection = "automatic"
}

CM_2 = {
    description = "Emergency change procedures"
    implementation = "Documented emergency procedures with post-change review"
    testing_frequency = "annually"
    evidence_collection = "manual"
}

[compliance.sox.evidence]
# Audit evidence collection
audit_log_retention_years = 7
evidence_encryption = true
evidence_backup_frequency = "daily"
evidence_access_logging = true

# Automated evidence collection
collect_access_logs = true
collect_configuration_changes = true
collect_certificate_operations = true
collect_emergency_actions = true
```

#### PCI DSS Compliance
```toml
# /etc/rustmq/compliance/pci-config.toml

[compliance.pci_dss]
enabled = true
framework = "PCI-DSS"
version = "4.0"
merchant_level = 1

# PCI DSS Requirements mapping
[compliance.pci_dss.requirements]
# Requirement 2: Strong cryptography and security protocols
req_2_1 = {
    description = "Strong cryptography for authentication"
    implementation = "mTLS with minimum 2048-bit RSA or 256-bit ECC"
    testing = "Quarterly vulnerability scanning"
}

req_2_2 = {
    description = "Disable insecure services and protocols"
    implementation = "TLS 1.3 only, deprecated protocols disabled"
    testing = "Configuration review"
}

# Requirement 8: Identify users and authenticate access
req_8_1 = {
    description = "Unique user identification"
    implementation = "Certificate-based unique identification"
    testing = "Access review"
}

req_8_2 = {
    description = "Strong authentication controls"
    implementation = "Multi-factor authentication with certificates"
    testing = "Authentication testing"
}

# Requirement 10: Log and monitor all access
req_10_1 = {
    description = "Audit trail for access to cardholder data"
    implementation = "Comprehensive audit logging with correlation"
    testing = "Log review"
}

req_10_2 = {
    description = "Automated audit trail review"
    implementation = "SIEM integration with automated analysis"
    testing = "Automated testing"
}

[compliance.pci_dss.data_classification]
# Cardholder data protection
cardholder_data_topics = ["payment.*", "card.*", "transaction.*"]
encryption_required = true
access_logging_required = true
retention_period_months = 12
```

### Automated Compliance Reporting

#### Compliance Report Generation
```bash
# Automated compliance report generation
cat > /usr/local/bin/rustmq-compliance-report.sh << 'EOF'
#!/bin/bash
# RustMQ Compliance Report Generator

set -euo pipefail

FRAMEWORK="$1"
PERIOD="$2"
OUTPUT_DIR="/var/log/rustmq/compliance"
REPORT_DATE=$(date +%Y%m%d)

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1"
}

generate_sox_report() {
    log "Generating SOX compliance report"
    
    local report_file="$OUTPUT_DIR/sox-compliance-report-$REPORT_DATE.json"
    
    # Collect evidence
    local access_review
    access_review=$(rustmq-admin acl access-review --format json --period "$PERIOD")
    
    local audit_summary
    audit_summary=$(rustmq-admin audit summary --period "$PERIOD" --format json)
    
    local certificate_audit
    certificate_audit=$(rustmq-admin certs audit --period "$PERIOD" --format json)
    
    local configuration_changes
    configuration_changes=$(rustmq-admin audit logs \
        --event-type "configuration_changed" \
        --period "$PERIOD" \
        --format json)
    
    # Generate compliance report
    cat > "$report_file" << REPORT_EOF
{
    "framework": "SOX",
    "report_period": "$PERIOD",
    "generation_date": "$(date -Iseconds)",
    "compliance_status": "compliant",
    "controls": {
        "access_controls": {
            "status": "compliant",
            "evidence": $access_review,
            "findings": []
        },
        "audit_controls": {
            "status": "compliant", 
            "evidence": $audit_summary,
            "findings": []
        },
        "change_management": {
            "status": "compliant",
            "evidence": $configuration_changes,
            "findings": []
        }
    },
    "certificate_compliance": $certificate_audit,
    "recommendations": [
        "Continue quarterly access reviews",
        "Maintain audit log retention policy",
        "Review emergency procedures annually"
    ]
}
REPORT_EOF
    
    log "SOX compliance report generated: $report_file"
}

generate_pci_report() {
    log "Generating PCI DSS compliance report"
    
    local report_file="$OUTPUT_DIR/pci-compliance-report-$REPORT_DATE.json"
    
    # PCI-specific compliance checks
    local encryption_status
    encryption_status=$(rustmq-admin security encryption-status --format json)
    
    local access_controls
    access_controls=$(rustmq-admin acl compliance-check --framework PCI-DSS --format json)
    
    local vulnerability_scan
    vulnerability_scan=$(rustmq-admin security vulnerability-scan --format json)
    
    cat > "$report_file" << REPORT_EOF
{
    "framework": "PCI-DSS",
    "version": "4.0",
    "report_period": "$PERIOD",
    "generation_date": "$(date -Iseconds)",
    "compliance_status": "compliant",
    "requirements": {
        "requirement_2": {
            "description": "Strong cryptography and security protocols",
            "status": "compliant",
            "evidence": $encryption_status
        },
        "requirement_8": {
            "description": "Identify users and authenticate access",
            "status": "compliant",
            "evidence": $access_controls
        },
        "requirement_10": {
            "description": "Log and monitor all access",
            "status": "compliant",
            "evidence": {
                "audit_logs_enabled": true,
                "log_retention_days": 2555,
                "automated_monitoring": true
            }
        }
    },
    "vulnerability_assessment": $vulnerability_scan,
    "action_items": []
}
REPORT_EOF
    
    log "PCI DSS compliance report generated: $report_file"
}

# Main function
main() {
    log "Starting compliance report generation for $FRAMEWORK"
    
    mkdir -p "$OUTPUT_DIR"
    
    case "$FRAMEWORK" in
        "SOX"|"sox")
            generate_sox_report
            ;;
        "PCI"|"pci"|"PCI-DSS")
            generate_pci_report
            ;;
        *)
            log "Unknown compliance framework: $FRAMEWORK"
            exit 1
            ;;
    esac
    
    log "Compliance report generation completed"
}

main "$@"
EOF

chmod +x /usr/local/bin/rustmq-compliance-report.sh

# Schedule compliance reports
(crontab -l 2>/dev/null; echo "0 1 1 1,4,7,10 * /usr/local/bin/rustmq-compliance-report.sh SOX quarterly") | crontab -
(crontab -l 2>/dev/null; echo "0 2 1 * * /usr/local/bin/rustmq-compliance-report.sh PCI monthly") | crontab -
```

## Backup and Recovery

### Security Backup Strategy

#### Backup Configuration
```bash
# Security backup script
cat > /usr/local/bin/rustmq-security-backup.sh << 'EOF'
#!/bin/bash
# RustMQ Security Backup Script

set -euo pipefail

BACKUP_DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="/backup/rustmq/security"
S3_BUCKET="rustmq-security-backups"
ENCRYPTION_KEY_FILE="/etc/rustmq/secrets/backup-encryption-key"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1"
}

backup_certificates() {
    log "Backing up certificate store"
    
    local cert_backup_dir="$BACKUP_DIR/certificates/$BACKUP_DATE"
    mkdir -p "$cert_backup_dir"
    
    # Export all certificates and metadata
    rustmq-admin certs export-all \
        --output-dir "$cert_backup_dir" \
        --include-private-keys \
        --include-metadata \
        --encrypt \
        --encryption-key-file "$ENCRYPTION_KEY_FILE"
    
    # Backup CA certificates separately
    rustmq-admin ca export-all \
        --output-dir "$cert_backup_dir/ca" \
        --include-private-keys \
        --encrypt \
        --encryption-key-file "$ENCRYPTION_KEY_FILE"
    
    log "Certificate backup completed: $cert_backup_dir"
}

backup_acl_rules() {
    log "Backing up ACL rules"
    
    local acl_backup_file="$BACKUP_DIR/acl/acl-rules-$BACKUP_DATE.json.enc"
    mkdir -p "$(dirname "$acl_backup_file")"
    
    # Export ACL rules with encryption
    rustmq-admin acl export \
        --output-file "${acl_backup_file%.enc}" \
        --include-metadata \
        --include-usage-statistics \
        --format json
    
    # Encrypt the backup file
    openssl enc -aes-256-cbc -salt \
        -in "${acl_backup_file%.enc}" \
        -out "$acl_backup_file" \
        -pass file:"$ENCRYPTION_KEY_FILE"
    
    rm "${acl_backup_file%.enc}"
    
    log "ACL backup completed: $acl_backup_file"
}

backup_audit_logs() {
    log "Backing up audit logs"
    
    local audit_backup_dir="$BACKUP_DIR/audit/$BACKUP_DATE"
    mkdir -p "$audit_backup_dir"
    
    # Archive and compress audit logs
    tar -czf "$audit_backup_dir/audit-logs.tar.gz" \
        /var/log/rustmq/security-audit.log* \
        /var/log/rustmq/authentication.log* \
        /var/log/rustmq/authorization.log*
    
    # Encrypt the archive
    openssl enc -aes-256-cbc -salt \
        -in "$audit_backup_dir/audit-logs.tar.gz" \
        -out "$audit_backup_dir/audit-logs.tar.gz.enc" \
        -pass file:"$ENCRYPTION_KEY_FILE"
    
    rm "$audit_backup_dir/audit-logs.tar.gz"
    
    log "Audit log backup completed: $audit_backup_dir"
}

backup_configuration() {
    log "Backing up security configuration"
    
    local config_backup_file="$BACKUP_DIR/config/security-config-$BACKUP_DATE.tar.gz.enc"
    mkdir -p "$(dirname "$config_backup_file")"
    
    # Archive security configuration files
    tar -czf "${config_backup_file%.enc}" \
        /etc/rustmq/config/security.toml \
        /etc/rustmq/config/acl-config.toml \
        /etc/rustmq/config/audit-config.toml \
        /etc/rustmq/compliance/
    
    # Encrypt the configuration backup
    openssl enc -aes-256-cbc -salt \
        -in "${config_backup_file%.enc}" \
        -out "$config_backup_file" \
        -pass file:"$ENCRYPTION_KEY_FILE"
    
    rm "${config_backup_file%.enc}"
    
    log "Configuration backup completed: $config_backup_file"
}

upload_to_s3() {
    log "Uploading backups to S3"
    
    aws s3 sync "$BACKUP_DIR" "s3://$S3_BUCKET/$(date +%Y/%m/%d)/" \
        --encryption AES256 \
        --storage-class STANDARD_IA
    
    log "S3 upload completed"
}

verify_backups() {
    log "Verifying backup integrity"
    
    # Verify certificate backups
    local latest_cert_backup
    latest_cert_backup=$(find "$BACKUP_DIR/certificates" -name "*$BACKUP_DATE*" -type d | head -1)
    
    if [[ -n "$latest_cert_backup" ]]; then
        rustmq-admin certs verify-backup \
            --backup-dir "$latest_cert_backup" \
            --encryption-key-file "$ENCRYPTION_KEY_FILE"
    fi
    
    # Verify ACL backups
    local latest_acl_backup
    latest_acl_backup=$(find "$BACKUP_DIR/acl" -name "*$BACKUP_DATE*.enc" | head -1)
    
    if [[ -n "$latest_acl_backup" ]]; then
        # Decrypt and validate JSON
        openssl enc -d -aes-256-cbc \
            -in "$latest_acl_backup" \
            -pass file:"$ENCRYPTION_KEY_FILE" | \
            jq '.' > /dev/null && log "ACL backup verification passed"
    fi
    
    log "Backup verification completed"
}

cleanup_old_backups() {
    log "Cleaning up old backups"
    
    # Keep local backups for 30 days
    find "$BACKUP_DIR" -type f -mtime +30 -delete
    find "$BACKUP_DIR" -type d -empty -delete
    
    # Keep S3 backups based on lifecycle policy
    log "Local backup cleanup completed"
}

main() {
    log "Starting security backup process"
    
    mkdir -p "$BACKUP_DIR"
    
    backup_certificates
    backup_acl_rules
    backup_audit_logs
    backup_configuration
    
    verify_backups
    upload_to_s3
    cleanup_old_backups
    
    log "Security backup process completed successfully"
}

main "$@"
EOF

chmod +x /usr/local/bin/rustmq-security-backup.sh

# Schedule daily backups
(crontab -l 2>/dev/null; echo "0 3 * * * /usr/local/bin/rustmq-security-backup.sh") | crontab -
```

### Disaster Recovery Procedures

#### Security Recovery Plan
```bash
# Security disaster recovery script
cat > /usr/local/bin/rustmq-security-recovery.sh << 'EOF'
#!/bin/bash
# RustMQ Security Disaster Recovery Script

set -euo pipefail

RECOVERY_DATE="$1"
BACKUP_SOURCE="${2:-s3}"
S3_BUCKET="rustmq-security-backups"
ENCRYPTION_KEY_FILE="/etc/rustmq/secrets/backup-encryption-key"
RECOVERY_DIR="/tmp/rustmq-recovery"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1"
}

download_backups() {
    log "Downloading security backups for $RECOVERY_DATE"
    
    mkdir -p "$RECOVERY_DIR"
    
    if [[ "$BACKUP_SOURCE" == "s3" ]]; then
        aws s3 sync "s3://$S3_BUCKET/$RECOVERY_DATE/" "$RECOVERY_DIR/" \
            --exclude "*" \
            --include "certificates/*" \
            --include "acl/*" \
            --include "config/*"
    else
        cp -r "/backup/rustmq/security" "$RECOVERY_DIR/"
    fi
    
    log "Backup download completed"
}

recover_certificates() {
    log "Recovering certificate store"
    
    local cert_backup_dir
    cert_backup_dir=$(find "$RECOVERY_DIR" -name "certificates" -type d | head -1)
    
    if [[ -n "$cert_backup_dir" ]]; then
        # Stop RustMQ services
        systemctl stop rustmq-broker rustmq-controller || true
        
        # Backup current certificates
        mv /etc/rustmq/certs /etc/rustmq/certs.backup.$(date +%s) || true
        
        # Restore certificates
        rustmq-admin certs restore \
            --backup-dir "$cert_backup_dir" \
            --encryption-key-file "$ENCRYPTION_KEY_FILE" \
            --target-dir "/etc/rustmq/certs" \
            --verify-integrity
        
        log "Certificate recovery completed"
    else
        log "ERROR: No certificate backup found"
        return 1
    fi
}

recover_acl_rules() {
    log "Recovering ACL rules"
    
    local acl_backup_file
    acl_backup_file=$(find "$RECOVERY_DIR" -name "acl-rules-*.json.enc" | head -1)
    
    if [[ -n "$acl_backup_file" ]]; then
        # Decrypt ACL backup
        local decrypted_file="/tmp/acl-rules-recovery.json"
        openssl enc -d -aes-256-cbc \
            -in "$acl_backup_file" \
            -out "$decrypted_file" \
            -pass file:"$ENCRYPTION_KEY_FILE"
        
        # Restore ACL rules
        rustmq-admin acl import \
            --input-file "$decrypted_file" \
            --merge-strategy "replace" \
            --verify-before-import
        
        rm "$decrypted_file"
        
        log "ACL recovery completed"
    else
        log "ERROR: No ACL backup found"
        return 1
    fi
}

recover_configuration() {
    log "Recovering security configuration"
    
    local config_backup_file
    config_backup_file=$(find "$RECOVERY_DIR" -name "security-config-*.tar.gz.enc" | head -1)
    
    if [[ -n "$config_backup_file" ]]; then
        # Backup current configuration
        tar -czf "/etc/rustmq/config.backup.$(date +%s).tar.gz" /etc/rustmq/config/ || true
        
        # Decrypt and restore configuration
        openssl enc -d -aes-256-cbc \
            -in "$config_backup_file" \
            -pass file:"$ENCRYPTION_KEY_FILE" | \
            tar -xzf - -C /
        
        log "Configuration recovery completed"
    else
        log "ERROR: No configuration backup found"
        return 1
    fi
}

verify_recovery() {
    log "Verifying security recovery"
    
    # Start RustMQ services
    systemctl start rustmq-controller
    sleep 10
    systemctl start rustmq-broker
    sleep 10
    
    # Verify certificates
    if rustmq-admin certs health-check --all; then
        log "Certificate verification passed"
    else
        log "ERROR: Certificate verification failed"
        return 1
    fi
    
    # Verify ACL system
    if rustmq-admin acl health-check; then
        log "ACL verification passed"
    else
        log "ERROR: ACL verification failed"
        return 1
    fi
    
    # Verify security status
    if rustmq-admin security status | grep -q "Overall Status: HEALTHY"; then
        log "Security system verification passed"
    else
        log "ERROR: Security system verification failed"
        return 1
    fi
    
    log "Recovery verification completed successfully"
}

cleanup_recovery() {
    log "Cleaning up recovery artifacts"
    
    rm -rf "$RECOVERY_DIR"
    
    log "Recovery cleanup completed"
}

main() {
    log "Starting security disaster recovery for $RECOVERY_DATE"
    
    download_backups
    recover_certificates
    recover_acl_rules
    recover_configuration
    verify_recovery
    cleanup_recovery
    
    log "Security disaster recovery completed successfully"
    log "IMPORTANT: Review all recovered configurations and certificates"
    log "IMPORTANT: Update monitoring and alerting systems if needed"
}

main "$@"
EOF

chmod +x /usr/local/bin/rustmq-security-recovery.sh
```

This production security guide provides comprehensive coverage of all security aspects needed for deploying RustMQ in enterprise production environments, ensuring the highest levels of security while maintaining operational efficiency.