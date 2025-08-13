#!/bin/bash
# Certificate Renewal Script for RustMQ - v2.0
# Compatible with RustMQ 1.0.0+ Certificate Infrastructure
# Updated: August 2025 - Enhanced for production certificate chains
# Automated certificate renewal and rotation

set -euo pipefail

# Configuration
CERT_DIR="${CERT_DIR:-/etc/rustmq/certs}"
CA_DIR="${CA_DIR:-/etc/rustmq/ca}"
BACKUP_DIR="${BACKUP_DIR:-/backup/rustmq/certificates}"
RENEWAL_THRESHOLD_DAYS="${RENEWAL_THRESHOLD_DAYS:-30}"
EMAIL_ALERTS="${EMAIL_ALERTS:-security@company.com}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() {
    echo -e "${GREEN}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Send alert email
send_alert() {
    local subject="$1"
    local message="$2"
    
    if command -v mail &> /dev/null; then
        echo "$message" | mail -s "$subject" "$EMAIL_ALERTS"
    else
        warn "Mail command not available. Alert not sent: $subject"
    fi
}

# Check certificate expiry
check_certificate_expiry() {
    local cert_file="$1"
    local cert_name="$2"
    
    if [[ ! -f "$cert_file" ]]; then
        error "Certificate file not found: $cert_file"
        return 1
    fi
    
    # Get certificate expiry date
    local expiry_date=$(openssl x509 -in "$cert_file" -noout -enddate | cut -d= -f2)
    local expiry_epoch=$(date -d "$expiry_date" +%s)
    local current_epoch=$(date +%s)
    local days_until_expiry=$(( (expiry_epoch - current_epoch) / 86400 ))
    
    info "$cert_name expires in $days_until_expiry days ($expiry_date)"
    
    # RustMQ 1.0.0+ Certificate validation
    if openssl x509 -in "$cert_file" -noout -issuer 2>/dev/null | grep -q "CN=.*Root CA"; then
        info "$cert_name has proper CA-signed certificate chain (RustMQ 1.0.0+ compatible)"
    else
        warn "$cert_name may be self-signed or have invalid chain"
    fi
    
    if [[ $days_until_expiry -le $RENEWAL_THRESHOLD_DAYS ]]; then
        warn "$cert_name expires in $days_until_expiry days - renewal required"
        return 0  # Needs renewal
    else
        log "$cert_name is valid for $days_until_expiry days"
        return 1  # No renewal needed
    fi
}

# Backup existing certificate
backup_certificate() {
    local cert_file="$1"
    local cert_name="$2"
    
    local backup_timestamp=$(date '+%Y%m%d_%H%M%S')
    local backup_file="$BACKUP_DIR/${cert_name}_${backup_timestamp}.pem"
    
    mkdir -p "$BACKUP_DIR"
    cp "$cert_file" "$backup_file"
    
    log "Certificate backed up: $backup_file"
}

# Renew server certificate
renew_server_certificate() {
    local hostname="$1"
    local cert_name="${2:-$hostname}"
    
    log "Renewing server certificate for $hostname"
    
    # Backup existing certificate
    if [[ -f "$CERT_DIR/${cert_name}.pem" ]]; then
        backup_certificate "$CERT_DIR/${cert_name}.pem" "$cert_name"
    fi
    
    # Generate new private key
    openssl genrsa -out "$CERT_DIR/${cert_name}.key.new" 2048
    chmod 600 "$CERT_DIR/${cert_name}.key.new"
    
    # Create certificate signing request
    openssl req -new -key "$CERT_DIR/${cert_name}.key.new" \
        -out "$CERT_DIR/${cert_name}.csr" \
        -subj "/C=US/ST=California/L=San Francisco/O=YourCompany/OU=Production Servers/CN=$hostname"
    
    # Create certificate extensions
    cat > "$CERT_DIR/${cert_name}.ext" << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = $hostname
DNS.2 = $hostname.company.com
DNS.3 = $hostname.internal.company.com
DNS.4 = localhost
IP.1 = 127.0.0.1
IP.2 = ::1
EOF
    
    # Sign the certificate
    openssl ca -config "$CA_DIR/openssl.cnf" \
        -extensions server_cert -days 365 -notext -md sha256 \
        -in "$CERT_DIR/${cert_name}.csr" \
        -out "$CERT_DIR/${cert_name}.pem.new" \
        -batch \
        -extfile "$CERT_DIR/${cert_name}.ext"
    
    # Verify new certificate
    if openssl verify -CAfile "$CERT_DIR/ca.pem" "$CERT_DIR/${cert_name}.pem.new"; then
        # Replace old certificate and key
        mv "$CERT_DIR/${cert_name}.pem.new" "$CERT_DIR/${cert_name}.pem"
        mv "$CERT_DIR/${cert_name}.key.new" "$CERT_DIR/${cert_name}.key"
        
        # Clean up
        rm "$CERT_DIR/${cert_name}.csr" "$CERT_DIR/${cert_name}.ext"
        
        log "Server certificate renewed successfully: $cert_name"
        
        # Send success notification
        send_alert "RustMQ Certificate Renewed: $cert_name" \
            "Server certificate for $hostname has been successfully renewed and is valid for 365 days."
        
        return 0
    else
        error "Certificate verification failed for $cert_name"
        
        # Clean up failed attempt
        rm -f "$CERT_DIR/${cert_name}.pem.new" "$CERT_DIR/${cert_name}.key.new"
        rm -f "$CERT_DIR/${cert_name}.csr" "$CERT_DIR/${cert_name}.ext"
        
        # Send failure notification
        send_alert "RustMQ Certificate Renewal Failed: $cert_name" \
            "Failed to renew server certificate for $hostname. Manual intervention required."
        
        return 1
    fi
}

# Renew client certificate
renew_client_certificate() {
    local client_name="$1"
    local principal="${2:-$client_name@company.com}"
    
    log "Renewing client certificate for $principal"
    
    # Backup existing certificate
    if [[ -f "$CERT_DIR/${client_name}.pem" ]]; then
        backup_certificate "$CERT_DIR/${client_name}.pem" "$client_name"
    fi
    
    # Generate new private key
    openssl genrsa -out "$CERT_DIR/${client_name}.key.new" 2048
    chmod 600 "$CERT_DIR/${client_name}.key.new"
    
    # Create certificate signing request
    openssl req -new -key "$CERT_DIR/${client_name}.key.new" \
        -out "$CERT_DIR/${client_name}.csr" \
        -subj "/C=US/ST=California/L=San Francisco/O=YourCompany/OU=Production Clients/CN=$principal"
    
    # Sign the certificate
    openssl ca -config "$CA_DIR/openssl.cnf" \
        -extensions client_cert -days 90 -notext -md sha256 \
        -in "$CERT_DIR/${client_name}.csr" \
        -out "$CERT_DIR/${client_name}.pem.new" \
        -batch
    
    # Verify new certificate
    if openssl verify -CAfile "$CERT_DIR/ca.pem" "$CERT_DIR/${client_name}.pem.new"; then
        # Replace old certificate and key
        mv "$CERT_DIR/${client_name}.pem.new" "$CERT_DIR/${client_name}.pem"
        mv "$CERT_DIR/${client_name}.key.new" "$CERT_DIR/${client_name}.key"
        
        # Clean up
        rm "$CERT_DIR/${client_name}.csr"
        
        log "Client certificate renewed successfully: $client_name"
        
        # Send success notification
        send_alert "RustMQ Client Certificate Renewed: $client_name" \
            "Client certificate for $principal has been successfully renewed and is valid for 90 days."
        
        return 0
    else
        error "Certificate verification failed for $client_name"
        
        # Clean up failed attempt
        rm -f "$CERT_DIR/${client_name}.pem.new" "$CERT_DIR/${client_name}.key.new"
        rm -f "$CERT_DIR/${client_name}.csr"
        
        # Send failure notification
        send_alert "RustMQ Client Certificate Renewal Failed: $client_name" \
            "Failed to renew client certificate for $principal. Manual intervention required."
        
        return 1
    fi
}

# Restart RustMQ services after certificate renewal
restart_rustmq_services() {
    log "Restarting RustMQ services after certificate renewal"
    
    local services=("rustmq-broker" "rustmq-controller")
    local restart_failures=0
    
    for service in "${services[@]}"; do
        if systemctl is-active "$service" &>/dev/null; then
            if systemctl reload "$service"; then
                log "Reloaded $service successfully"
            else
                warn "Failed to reload $service, attempting restart"
                if systemctl restart "$service"; then
                    log "Restarted $service successfully"
                else
                    error "Failed to restart $service"
                    ((restart_failures++))
                fi
            fi
        else
            info "$service is not running, skipping reload"
        fi
    done
    
    if [[ $restart_failures -eq 0 ]]; then
        log "All RustMQ services reloaded/restarted successfully"
        send_alert "RustMQ Services Reloaded" \
            "RustMQ services have been successfully reloaded after certificate renewal."
    else
        error "$restart_failures services failed to restart"
        send_alert "RustMQ Service Restart Failures" \
            "$restart_failures RustMQ services failed to restart after certificate renewal. Manual intervention required."
    fi
}

# Check and renew all certificates
check_and_renew_all() {
    log "Checking all certificates for renewal"
    
    local renewal_needed=false
    local renewal_count=0
    
    # Check server certificates
    local server_certs=("server" "broker" "controller")
    for cert in "${server_certs[@]}"; do
        if [[ -f "$CERT_DIR/${cert}.pem" ]]; then
            if check_certificate_expiry "$CERT_DIR/${cert}.pem" "$cert"; then
                if renew_server_certificate "$cert"; then
                    renewal_needed=true
                    ((renewal_count++))
                fi
            fi
        fi
    done
    
    # Check client certificates
    local client_certs=("client" "admin" "analytics" "monitoring")
    for cert in "${client_certs[@]}"; do
        if [[ -f "$CERT_DIR/${cert}.pem" ]]; then
            if check_certificate_expiry "$CERT_DIR/${cert}.pem" "$cert"; then
                if renew_client_certificate "$cert"; then
                    renewal_needed=true
                    ((renewal_count++))
                fi
            fi
        fi
    done
    
    # Check CA certificate (warning only, no auto-renewal)
    if [[ -f "$CERT_DIR/ca.pem" ]]; then
        if check_certificate_expiry "$CERT_DIR/ca.pem" "CA Certificate"; then
            warn "CA certificate requires renewal - manual intervention required"
            send_alert "RustMQ CA Certificate Expiring" \
                "The RustMQ CA certificate is expiring within $RENEWAL_THRESHOLD_DAYS days. Manual renewal required."
        fi
    fi
    
    # Restart services if any certificates were renewed
    if [[ "$renewal_needed" == "true" ]]; then
        log "$renewal_count certificates were renewed"
        restart_rustmq_services
    else
        log "No certificate renewals were needed"
    fi
}

# Generate certificate report
generate_certificate_report() {
    log "Generating certificate status report"
    
    local report_file="$CERT_DIR/certificate-status-report-$(date '+%Y%m%d').txt"
    
    cat > "$report_file" << EOF
RustMQ Certificate Status Report
Generated: $(date)

Certificate Expiry Status:
EOF
    
    # Check all certificate files
    for cert_file in "$CERT_DIR"/*.pem; do
        if [[ -f "$cert_file" ]]; then
            local cert_name=$(basename "$cert_file" .pem)
            local expiry_date=$(openssl x509 -in "$cert_file" -noout -enddate | cut -d= -f2)
            local expiry_epoch=$(date -d "$expiry_date" +%s)
            local current_epoch=$(date +%s)
            local days_until_expiry=$(( (expiry_epoch - current_epoch) / 86400 ))
            
            echo "- $cert_name: $days_until_expiry days ($expiry_date)" >> "$report_file"
        fi
    done
    
    cat >> "$report_file" << EOF

Certificate Renewal Threshold: $RENEWAL_THRESHOLD_DAYS days
Next Scheduled Check: $(date -d '+1 day' '+%Y-%m-%d %H:%M:%S')

Backup Location: $BACKUP_DIR
Alert Email: $EMAIL_ALERTS

Actions Recommended:
EOF
    
    # Add recommendations based on certificate status
    for cert_file in "$CERT_DIR"/*.pem; do
        if [[ -f "$cert_file" ]]; then
            local cert_name=$(basename "$cert_file" .pem)
            local expiry_date=$(openssl x509 -in "$cert_file" -noout -enddate | cut -d= -f2)
            local expiry_epoch=$(date -d "$expiry_date" +%s)
            local current_epoch=$(date +%s)
            local days_until_expiry=$(( (expiry_epoch - current_epoch) / 86400 ))
            
            if [[ $days_until_expiry -le $RENEWAL_THRESHOLD_DAYS ]]; then
                echo "- URGENT: Renew $cert_name certificate (expires in $days_until_expiry days)" >> "$report_file"
            elif [[ $days_until_expiry -le 60 ]]; then
                echo "- PLAN: Schedule renewal for $cert_name certificate (expires in $days_until_expiry days)" >> "$report_file"
            fi
        fi
    done
    
    log "Certificate report generated: $report_file"
    
    # Email the report
    if command -v mail &> /dev/null; then
        mail -s "RustMQ Certificate Status Report" "$EMAIL_ALERTS" < "$report_file"
    fi
}

# Main function
main() {
    log "Starting certificate renewal check"
    
    # Create backup directory
    mkdir -p "$BACKUP_DIR"
    
    # Check if CA directory exists
    if [[ ! -d "$CA_DIR" ]]; then
        error "CA directory not found: $CA_DIR"
        exit 1
    fi
    
    # Check if OpenSSL config exists
    if [[ ! -f "$CA_DIR/openssl.cnf" ]]; then
        error "OpenSSL configuration not found: $CA_DIR/openssl.cnf"
        exit 1
    fi
    
    case "${1:-check}" in
        "check")
            check_and_renew_all
            ;;
        "report")
            generate_certificate_report
            ;;
        "renew-server")
            if [[ -z "${2:-}" ]]; then
                error "Usage: $0 renew-server <hostname>"
                exit 1
            fi
            renew_server_certificate "$2"
            ;;
        "renew-client")
            if [[ -z "${2:-}" ]]; then
                error "Usage: $0 renew-client <client-name> [principal]"
                exit 1
            fi
            renew_client_certificate "$2" "${3:-$2@company.com}"
            ;;
        "restart-services")
            restart_rustmq_services
            ;;
        *)
            echo "Usage: $0 [check|report|renew-server|renew-client|restart-services]"
            echo ""
            echo "Commands:"
            echo "  check                     Check all certificates and renew if needed"
            echo "  report                    Generate certificate status report"
            echo "  renew-server <hostname>   Renew specific server certificate"
            echo "  renew-client <name>       Renew specific client certificate"
            echo "  restart-services          Restart RustMQ services"
            echo ""
            echo "Environment Variables:"
            echo "  CERT_DIR                  Certificate directory (default: /etc/rustmq/certs)"
            echo "  CA_DIR                    CA directory (default: /etc/rustmq/ca)"
            echo "  BACKUP_DIR                Backup directory (default: /backup/rustmq/certificates)"
            echo "  RENEWAL_THRESHOLD_DAYS    Days before expiry to renew (default: 30)"
            echo "  EMAIL_ALERTS              Email for alerts (default: security@company.com)"
            exit 1
            ;;
    esac
    
    log "Certificate renewal check completed"
}

# Run main function
main "$@"