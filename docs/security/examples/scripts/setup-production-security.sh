#!/bin/bash
# Production Security Setup Script for RustMQ - v2.0
# Compatible with RustMQ 1.0.0+ Security Infrastructure
# Updated: August 2025 - Enhanced for production certificate chains
# This script sets up complete production security environment

set -euo pipefail

# Configuration
COMPANY_NAME="${COMPANY_NAME:-YourCompany}"
DOMAIN="${DOMAIN:-company.com}"
ENVIRONMENT="${ENVIRONMENT:-production}"
SECURITY_DIR="${SECURITY_DIR:-/etc/rustmq/security}"
CONFIG_DIR="${CONFIG_DIR:-/etc/rustmq/config}"
BACKUP_DIR="${BACKUP_DIR:-/backup/rustmq/security}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        error "This script must be run as root for production security setup"
    fi
}

# Validate environment
validate_environment() {
    log "Validating production environment requirements"
    
    # Check required commands
    local required_commands=("openssl" "curl" "systemctl" "firewall-cmd" "semanage")
    for cmd in "${required_commands[@]}"; do
        if ! command -v "$cmd" &> /dev/null; then
            error "Required command not found: $cmd"
        fi
    done
    
    # Check system requirements
    if [[ ! -f /etc/redhat-release ]] && [[ ! -f /etc/debian_version ]]; then
        warn "Unsupported operating system. This script is tested on RHEL/CentOS and Debian/Ubuntu"
    fi
    
    # Check available disk space
    local available_space=$(df /etc | awk 'NR==2 {print $4}')
    if [[ $available_space -lt 1048576 ]]; then  # 1GB in KB
        error "Insufficient disk space. At least 1GB required in /etc"
    fi
    
    # Validate RustMQ 1.0.0+ compatibility
    info "Checking RustMQ 1.0.0+ compatibility requirements"
    
    # Check for proper certificate infrastructure
    if command -v rustmq-admin &> /dev/null; then
        local rustmq_version=$(rustmq-admin --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
        if [[ -n "$rustmq_version" ]]; then
            info "RustMQ version detected: $rustmq_version"
            if [[ "$rustmq_version" < "1.0.0" ]]; then
                warn "RustMQ version $rustmq_version may not support all 1.0.0+ security features"
            fi
        fi
    fi
    
    log "Environment validation completed"
}

# Setup directory structure
setup_directories() {
    log "Setting up production security directory structure"
    
    # Create main directories
    mkdir -p "$SECURITY_DIR"/{ca,certs,private,acl,audit,backup}
    mkdir -p "$CONFIG_DIR"
    mkdir -p "$BACKUP_DIR"
    mkdir -p /var/log/rustmq/security
    
    # Set proper permissions
    chmod 755 "$SECURITY_DIR"
    chmod 700 "$SECURITY_DIR/private"
    chmod 700 "$SECURITY_DIR/ca"
    chmod 755 "$SECURITY_DIR/certs"
    chmod 750 "$SECURITY_DIR/acl"
    chmod 750 "$SECURITY_DIR/audit"
    chmod 700 "$BACKUP_DIR"
    
    # Set ownership
    chown -R rustmq:rustmq "$SECURITY_DIR" 2>/dev/null || true
    chown -R rustmq:rustmq "$CONFIG_DIR" 2>/dev/null || true
    chown -R rustmq:rustmq /var/log/rustmq 2>/dev/null || true
    
    log "Directory structure created successfully"
}

# Configure system security
configure_system_security() {
    log "Configuring system-level security"
    
    # Configure SELinux (if available)
    if command -v getenforce &> /dev/null; then
        if [[ $(getenforce) != "Enforcing" ]]; then
            warn "SELinux is not in enforcing mode. Consider enabling for production"
        fi
        
        # Set SELinux contexts for RustMQ directories
        semanage fcontext -a -t admin_home_t "$SECURITY_DIR" 2>/dev/null || true
        restorecon -R "$SECURITY_DIR" 2>/dev/null || true
    fi
    
    # Configure firewall
    if command -v firewall-cmd &> /dev/null; then
        info "Configuring firewall rules for RustMQ"
        
        # RustMQ broker ports
        firewall-cmd --permanent --add-port=9092/tcp --zone=internal
        firewall-cmd --permanent --add-port=9093/tcp --zone=internal
        
        # RustMQ controller ports
        firewall-cmd --permanent --add-port=9094/tcp --zone=internal
        firewall-cmd --permanent --add-port=9095/tcp --zone=internal
        firewall-cmd --permanent --add-port=9642/tcp --zone=internal
        
        # Reload firewall
        firewall-cmd --reload
        
        log "Firewall configured for RustMQ"
    fi
    
    # Configure kernel parameters for security
    cat > /etc/sysctl.d/99-rustmq-security.conf << EOF
# RustMQ Security Kernel Parameters
net.ipv4.tcp_syncookies = 1
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1
net.ipv4.conf.all.accept_source_route = 0
net.ipv4.conf.default.accept_source_route = 0
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0
net.ipv4.conf.all.secure_redirects = 0
net.ipv4.conf.default.secure_redirects = 0
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.default.send_redirects = 0
net.ipv4.icmp_echo_ignore_broadcasts = 1
net.ipv4.icmp_ignore_bogus_error_responses = 1
net.ipv4.tcp_max_syn_backlog = 4096
net.core.netdev_max_backlog = 5000
EOF
    
    sysctl -p /etc/sysctl.d/99-rustmq-security.conf
    
    log "System security configuration completed"
}

# Setup HSM (if available)
setup_hsm() {
    log "Setting up Hardware Security Module (HSM) integration"
    
    # Check if SoftHSM is available for development/testing
    if command -v softhsm2-util &> /dev/null; then
        info "Setting up SoftHSM for development/testing"
        
        # Initialize SoftHSM token
        softhsm2-util --init-token --slot 0 --label "rustmq-production" --pin 1234 --so-pin 5678
        
        # Create HSM configuration
        cat > "$SECURITY_DIR/hsm-config.conf" << EOF
# SoftHSM Configuration for RustMQ
directories.tokendir = /var/lib/softhsm/tokens/
objectstore.backend = file
log.level = INFO
slots.removable = false
EOF
        
        warn "SoftHSM is configured for development only. Use hardware HSM in production!"
    else
        warn "No HSM available. Consider setting up hardware HSM for production"
        
        # Create placeholder for HSM configuration
        cat > "$SECURITY_DIR/hsm-config.conf" << EOF
# Hardware HSM Configuration Placeholder
# Configure your production HSM here
# Example for Luna Network HSM:
# library = /usr/lib/libCryptoki2_64.so
# slot = 0
# pin_file = /etc/rustmq/security/private/hsm-pin.enc
EOF
    fi
    
    chmod 600 "$SECURITY_DIR/hsm-config.conf"
    
    log "HSM configuration completed"
}

# Generate production certificates
generate_production_certificates() {
    log "Generating production-grade certificates"
    
    # Check if certificates already exist
    if [[ -f "$SECURITY_DIR/certs/ca.pem" ]]; then
        warn "Certificates already exist. Skipping certificate generation"
        return
    fi
    
    # Generate strong CA private key (4096-bit RSA)
    openssl genrsa -out "$SECURITY_DIR/private/ca.key" 4096
    chmod 600 "$SECURITY_DIR/private/ca.key"
    
    # Generate CA certificate (20-year validity for root CA)
    openssl req -new -x509 -days 7300 -key "$SECURITY_DIR/private/ca.key" \
        -out "$SECURITY_DIR/certs/ca.pem" \
        -subj "/C=US/ST=California/L=San Francisco/O=$COMPANY_NAME/OU=IT Security/CN=$COMPANY_NAME RustMQ Production Root CA"
    
    # Generate intermediate CA key and certificate
    openssl genrsa -out "$SECURITY_DIR/private/intermediate-ca.key" 3072
    chmod 600 "$SECURITY_DIR/private/intermediate-ca.key"
    
    # Create intermediate CA CSR
    openssl req -new -key "$SECURITY_DIR/private/intermediate-ca.key" \
        -out "$SECURITY_DIR/certs/intermediate-ca.csr" \
        -subj "/C=US/ST=California/L=San Francisco/O=$COMPANY_NAME/OU=IT Security/CN=$COMPANY_NAME RustMQ Production Intermediate CA"
    
    # Sign intermediate CA with root CA
    openssl x509 -req -in "$SECURITY_DIR/certs/intermediate-ca.csr" \
        -CA "$SECURITY_DIR/certs/ca.pem" \
        -CAkey "$SECURITY_DIR/private/ca.key" \
        -CAcreateserial -out "$SECURITY_DIR/certs/intermediate-ca.pem" \
        -days 3650 -extensions v3_ca
    
    # Create certificate bundle
    cat "$SECURITY_DIR/certs/intermediate-ca.pem" "$SECURITY_DIR/certs/ca.pem" > "$SECURITY_DIR/certs/ca-bundle.pem"
    
    # Clean up CSR
    rm "$SECURITY_DIR/certs/intermediate-ca.csr"
    
    log "Production certificates generated successfully"
}

# Setup ACL system
setup_acl_system() {
    log "Setting up production ACL system"
    
    # Create ACL database directory
    mkdir -p "$SECURITY_DIR/acl/rules"
    mkdir -p "$SECURITY_DIR/acl/cache"
    
    # Install production ACL rules
    if [[ -f "$(dirname "$0")/../acl-rules/advanced-scenarios/production-acls.json" ]]; then
        cp "$(dirname "$0")/../acl-rules/advanced-scenarios/production-acls.json" \
           "$SECURITY_DIR/acl/rules/production-acls.json"
    else
        # Create basic production ACL rules
        cat > "$SECURITY_DIR/acl/rules/production-acls.json" << EOF
{
  "acl_rules": [
    {
      "name": "default_deny_all",
      "description": "Default deny-all policy",
      "principal": "*",
      "resource": "*",
      "permissions": ["all"],
      "effect": "deny",
      "priority": 9999,
      "enabled": true
    },
    {
      "name": "admin_access",
      "description": "Administrative access",
      "principal": "*@admin.$DOMAIN",
      "resource": "*",
      "permissions": ["all"],
      "effect": "allow",
      "priority": 100,
      "enabled": true
    }
  ]
}
EOF
    fi
    
    # Set proper permissions
    chmod 640 "$SECURITY_DIR/acl/rules/production-acls.json"
    chown rustmq:rustmq "$SECURITY_DIR/acl/rules/production-acls.json" 2>/dev/null || true
    
    log "ACL system configured"
}

# Setup audit logging
setup_audit_logging() {
    log "Setting up production audit logging"
    
    # Create audit log directory
    mkdir -p /var/log/rustmq/security
    chmod 750 /var/log/rustmq/security
    
    # Configure rsyslog for RustMQ security logs
    cat > /etc/rsyslog.d/50-rustmq-security.conf << EOF
# RustMQ Security Audit Logging
\$template RustMQSecurityLogFormat,"%timestamp:::date-rfc3339% %hostname% %syslogtag% %msg%\\n"

# Security audit logs
local0.* /var/log/rustmq/security/audit.log;RustMQSecurityLogFormat
& stop

# Authentication logs  
local1.* /var/log/rustmq/security/auth.log;RustMQSecurityLogFormat
& stop

# Authorization logs
local2.* /var/log/rustmq/security/authz.log;RustMQSecurityLogFormat
& stop
EOF
    
    # Configure logrotate for RustMQ logs
    cat > /etc/logrotate.d/rustmq-security << EOF
/var/log/rustmq/security/*.log {
    daily
    rotate 365
    compress
    delaycompress
    missingok
    notifempty
    create 640 rustmq rustmq
    sharedscripts
    postrotate
        /bin/kill -HUP \$(cat /var/run/rsyslogd.pid 2> /dev/null) 2> /dev/null || true
    endscript
}
EOF
    
    # Restart rsyslog
    systemctl restart rsyslog
    
    log "Audit logging configured"
}

# Setup monitoring
setup_monitoring() {
    log "Setting up security monitoring"
    
    # Create monitoring configuration directory
    mkdir -p "$SECURITY_DIR/monitoring"
    
    # Create security metrics collection script
    cat > "$SECURITY_DIR/monitoring/collect-security-metrics.sh" << 'EOF'
#!/bin/bash
# Security Metrics Collection Script

METRICS_FILE="/var/log/rustmq/security/metrics.log"
DATE=$(date '+%Y-%m-%d %H:%M:%S')

# Collect authentication metrics
AUTH_SUCCESS=$(grep "authentication_success" /var/log/rustmq/security/auth.log | wc -l)
AUTH_FAILED=$(grep "authentication_failed" /var/log/rustmq/security/auth.log | wc -l)

# Collect authorization metrics  
AUTHZ_ALLOWED=$(grep "authorization_allowed" /var/log/rustmq/security/authz.log | wc -l)
AUTHZ_DENIED=$(grep "authorization_denied" /var/log/rustmq/security/authz.log | wc -l)

# Collect certificate metrics
CERT_VALID=$(grep "certificate_valid" /var/log/rustmq/security/audit.log | wc -l)
CERT_INVALID=$(grep "certificate_invalid" /var/log/rustmq/security/audit.log | wc -l)

# Log metrics
echo "$DATE auth_success=$AUTH_SUCCESS auth_failed=$AUTH_FAILED authz_allowed=$AUTHZ_ALLOWED authz_denied=$AUTHZ_DENIED cert_valid=$CERT_VALID cert_invalid=$CERT_INVALID" >> "$METRICS_FILE"
EOF
    
    chmod +x "$SECURITY_DIR/monitoring/collect-security-metrics.sh"
    
    # Setup cron job for metrics collection
    cat > /etc/cron.d/rustmq-security-metrics << EOF
# RustMQ Security Metrics Collection
*/5 * * * * rustmq $SECURITY_DIR/monitoring/collect-security-metrics.sh
EOF
    
    log "Security monitoring configured"
}

# Setup backup system
setup_backup_system() {
    log "Setting up security backup system"
    
    # Create backup script
    cat > "$SECURITY_DIR/backup/backup-security.sh" << EOF
#!/bin/bash
# RustMQ Security Backup Script

BACKUP_DATE=\$(date '+%Y%m%d_%H%M%S')
BACKUP_FILE="$BACKUP_DIR/rustmq-security-backup-\$BACKUP_DATE.tar.gz"

# Create backup
tar -czf "\$BACKUP_FILE" \\
    --exclude="$SECURITY_DIR/private/*" \\
    "$SECURITY_DIR" \\
    "$CONFIG_DIR" \\
    /var/log/rustmq/security

# Encrypt backup
gpg --cipher-algo AES256 --compress-algo 1 --symmetric \\
    --output "\$BACKUP_FILE.gpg" "\$BACKUP_FILE"

# Remove unencrypted backup
rm "\$BACKUP_FILE"

# Keep only last 30 backups
find "$BACKUP_DIR" -name "rustmq-security-backup-*.tar.gz.gpg" -mtime +30 -delete

echo "Security backup completed: \$BACKUP_FILE.gpg"
EOF
    
    chmod +x "$SECURITY_DIR/backup/backup-security.sh"
    
    # Setup daily backup cron job
    cat > /etc/cron.d/rustmq-security-backup << EOF
# RustMQ Security Daily Backup
0 2 * * * rustmq $SECURITY_DIR/backup/backup-security.sh
EOF
    
    log "Backup system configured"
}

# Generate configuration files
generate_configuration() {
    log "Generating production security configuration"
    
    # Check if configuration template exists
    local config_template="$(dirname "$0")/../configurations/production.toml"
    if [[ -f "$config_template" ]]; then
        # Use template and substitute variables
        sed -e "s/{{DOMAIN}}/$DOMAIN/g" \
            -e "s/{{ENVIRONMENT}}/$ENVIRONMENT/g" \
            -e "s/{{COMPANY_NAME}}/$COMPANY_NAME/g" \
            "$config_template" > "$CONFIG_DIR/security.toml"
    else
        # Generate basic configuration
        cat > "$CONFIG_DIR/security.toml" << EOF
[security]
enabled = true
security_level = "strict"
environment = "$ENVIRONMENT"

[security.tls]
enabled = true
server_cert_path = "$SECURITY_DIR/certs/server.pem"
server_key_path = "$SECURITY_DIR/private/server.key"
client_ca_cert_path = "$SECURITY_DIR/certs/ca-bundle.pem"
tls_version_min = "TLS1.3"
verify_client_cert = true

[security.mtls]
enabled = true
require_client_cert = true
verify_client_cert = true

[security.auth]
enabled = true
require_authentication = true
default_mechanism = "certificate"

[security.authz]
enabled = true
default_policy = "deny"
acl_enabled = true

[security.acl]
enabled = true
storage_type = "file"
storage_config = { file_path = "$SECURITY_DIR/acl/rules/production-acls.json" }

[security.audit]
enabled = true
audit_level = "detailed"
audit_log_path = "/var/log/rustmq/security/audit.log"
EOF
    fi
    
    chmod 640 "$CONFIG_DIR/security.toml"
    chown rustmq:rustmq "$CONFIG_DIR/security.toml" 2>/dev/null || true
    
    log "Configuration files generated"
}

# Validate security setup
validate_security_setup() {
    log "Validating security setup"
    
    local validation_errors=0
    
    # Check certificate files
    if [[ ! -f "$SECURITY_DIR/certs/ca.pem" ]]; then
        error "CA certificate not found"
        ((validation_errors++))
    fi
    
    # Validate certificate
    if ! openssl x509 -in "$SECURITY_DIR/certs/ca.pem" -noout -text &>/dev/null; then
        error "CA certificate is invalid"
        ((validation_errors++))
    fi
    
    # Check file permissions
    local file_perms=$(stat -c "%a" "$SECURITY_DIR/private" 2>/dev/null || echo "000")
    if [[ "$file_perms" != "700" ]]; then
        error "Private directory has incorrect permissions: $file_perms (should be 700)"
        ((validation_errors++))
    fi
    
    # Check configuration file
    if [[ ! -f "$CONFIG_DIR/security.toml" ]]; then
        error "Security configuration file not found"
        ((validation_errors++))
    fi
    
    # Check services
    if ! systemctl is-enabled rsyslog &>/dev/null; then
        warn "rsyslog is not enabled - audit logging may not work"
    fi
    
    if [[ $validation_errors -eq 0 ]]; then
        log "Security setup validation completed successfully"
    else
        error "Security setup validation failed with $validation_errors errors"
    fi
}

# Generate summary report
generate_summary() {
    log "Generating security setup summary"
    
    cat > "$SECURITY_DIR/security-setup-summary.txt" << EOF
RustMQ Production Security Setup Summary
Generated: $(date)
Company: $COMPANY_NAME
Domain: $DOMAIN
Environment: $ENVIRONMENT

Security Components Installed:
- Certificate Authority (Root + Intermediate)
- Production-grade certificates
- ACL system with default rules
- Comprehensive audit logging
- Security monitoring and metrics
- Automated backup system
- System-level security hardening

Directory Structure:
- Security Directory: $SECURITY_DIR
- Configuration Directory: $CONFIG_DIR
- Backup Directory: $BACKUP_DIR
- Log Directory: /var/log/rustmq/security

Certificate Information:
- CA Certificate: $SECURITY_DIR/certs/ca.pem
- Intermediate CA: $SECURITY_DIR/certs/intermediate-ca.pem
- Certificate Bundle: $SECURITY_DIR/certs/ca-bundle.pem

Configuration Files:
- Main Security Config: $CONFIG_DIR/security.toml
- ACL Rules: $SECURITY_DIR/acl/rules/production-acls.json
- HSM Config: $SECURITY_DIR/hsm-config.conf

Monitoring:
- Security metrics collection: Every 5 minutes
- Daily security backups: 2:00 AM
- Log rotation: Daily with 365-day retention

Next Steps:
1. Review and customize ACL rules for your environment
2. Generate server and client certificates using the CA
3. Configure RustMQ brokers and controllers with security settings
4. Test authentication and authorization
5. Set up external monitoring and alerting
6. Schedule regular security audits

Security Contacts:
- Security Team: security@$DOMAIN
- On-Call: security-oncall@$DOMAIN

IMPORTANT SECURITY NOTES:
- Private keys are stored in $SECURITY_DIR/private (mode 700)
- Backups are encrypted with GPG
- Default deny-all ACL policy is active
- All security events are logged and monitored
- Regular certificate rotation is recommended
EOF
    
    log "Summary report generated: $SECURITY_DIR/security-setup-summary.txt"
}

# Main function
main() {
    log "Starting RustMQ production security setup"
    
    check_root
    validate_environment
    setup_directories
    configure_system_security
    setup_hsm
    generate_production_certificates
    setup_acl_system
    setup_audit_logging
    setup_monitoring
    setup_backup_system
    generate_configuration
    validate_security_setup
    generate_summary
    
    log "RustMQ production security setup completed successfully!"
    info "Review the summary report: $SECURITY_DIR/security-setup-summary.txt"
    warn "Don't forget to:"
    warn "1. Backup the CA private key securely"
    warn "2. Generate server and client certificates"
    warn "3. Configure RustMQ services"
    warn "4. Test the security configuration"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --company)
            COMPANY_NAME="$2"
            shift 2
            ;;
        --domain)
            DOMAIN="$2"
            shift 2
            ;;
        --environment)
            ENVIRONMENT="$2"
            shift 2
            ;;
        --security-dir)
            SECURITY_DIR="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo "Options:"
            echo "  --company NAME      Company name (default: YourCompany)"
            echo "  --domain DOMAIN     Domain name (default: company.com)"
            echo "  --environment ENV   Environment (default: production)"
            echo "  --security-dir DIR  Security directory (default: /etc/rustmq/security)"
            echo "  --help              Show this help message"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Run main function
main