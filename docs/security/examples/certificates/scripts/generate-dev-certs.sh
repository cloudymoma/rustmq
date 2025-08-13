#!/bin/bash
# RustMQ Development Certificate Generation Script v2.0
# Compatible with RustMQ 1.0.0+ Certificate Signing Implementation
# This script generates a complete set of properly signed certificates for development/testing

set -euo pipefail

# Configuration
COMPANY_NAME="${COMPANY_NAME:-YourCompany}"
DOMAIN="${DOMAIN:-company.com}"
ENVIRONMENT="${ENVIRONMENT:-dev}"
CERT_DIR="${CERT_DIR:-/etc/rustmq/certs}"
CA_DIR="${CA_DIR:-/etc/rustmq/ca}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
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

# Check if running as root
check_permissions() {
    if [[ $EUID -eq 0 ]]; then
        warn "Running as root. Consider using a dedicated user for certificate operations."
    fi
}

# Create directory structure
setup_directories() {
    log "Setting up certificate directory structure"
    
    mkdir -p "$CERT_DIR"
    mkdir -p "$CA_DIR"
    mkdir -p "$CA_DIR/private"
    mkdir -p "$CA_DIR/certs"
    mkdir -p "$CA_DIR/newcerts"
    mkdir -p "$CA_DIR/crl"
    
    # Set proper permissions
    chmod 755 "$CERT_DIR"
    chmod 700 "$CA_DIR/private"
    chmod 755 "$CA_DIR/certs"
    
    # Initialize CA database
    touch "$CA_DIR/index.txt"
    echo 1000 > "$CA_DIR/serial"
    echo 1000 > "$CA_DIR/crlnumber"
}

# Generate CA configuration
generate_ca_config() {
    log "Generating CA configuration"
    
    cat > "$CA_DIR/openssl.cnf" << EOF
[ ca ]
default_ca = CA_default

[ CA_default ]
dir = $CA_DIR
certs = \$dir/certs
crl_dir = \$dir/crl
database = \$dir/index.txt
new_certs_dir = \$dir/newcerts
certificate = \$dir/certs/ca.pem
serial = \$dir/serial
crlnumber = \$dir/crlnumber
crl = \$dir/crl/crl.pem
private_key = \$dir/private/ca.key
RANDFILE = \$dir/private/.rand

x509_extensions = usr_cert
name_opt = ca_default
cert_opt = ca_default
default_days = 365
default_crl_days = 30
default_md = sha256
preserve = no
policy = policy_match

[ policy_match ]
countryName = match
stateOrProvinceName = match
organizationName = match
organizationalUnitName = optional
commonName = supplied
emailAddress = optional

[ req ]
default_bits = 2048
default_md = sha256
default_keyfile = privkey.pem
distinguished_name = req_distinguished_name
attributes = req_attributes
x509_extensions = v3_ca

[ req_distinguished_name ]
countryName = Country Name (2 letter code)
countryName_default = US
countryName_min = 2
countryName_max = 2
stateOrProvinceName = State or Province Name (full name)
stateOrProvinceName_default = California
localityName = Locality Name (eg, city)
localityName_default = San Francisco
0.organizationName = Organization Name (eg, company)
0.organizationName_default = $COMPANY_NAME
organizationalUnitName = Organizational Unit Name (eg, section)
organizationalUnitName_default = IT Security
commonName = Common Name (eg, your name or your server hostname)
commonName_max = 64
emailAddress = Email Address
emailAddress_max = 64

[ req_attributes ]

[ usr_cert ]
basicConstraints = CA:FALSE
nsComment = "OpenSSL Generated Certificate"
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer

[ v3_req ]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment

[ v3_ca ]
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer
basicConstraints = critical,CA:true
keyUsage = critical, digitalSignature, cRLSign, keyCertSign

[ server_cert ]
basicConstraints = CA:FALSE
nsCertType = server
nsComment = "OpenSSL Generated Server Certificate"
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer:always
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth

[ client_cert ]
basicConstraints = CA:FALSE
nsCertType = client, email
nsComment = "OpenSSL Generated Client Certificate"
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer
keyUsage = critical, digitalSignature
extendedKeyUsage = clientAuth, emailProtection
EOF
}

# Generate Root CA
generate_root_ca() {
    log "Generating Root CA certificate and private key"
    
    # Generate CA private key
    openssl genrsa -out "$CA_DIR/private/ca.key" 4096
    chmod 600 "$CA_DIR/private/ca.key"
    
    # Generate CA certificate
    openssl req -config "$CA_DIR/openssl.cnf" \
        -key "$CA_DIR/private/ca.key" \
        -new -x509 -days 7300 -sha256 -extensions v3_ca \
        -out "$CA_DIR/certs/ca.pem" \
        -subj "/C=US/ST=California/L=San Francisco/O=$COMPANY_NAME/OU=IT Security/CN=$COMPANY_NAME RustMQ Development Root CA"
    
    chmod 644 "$CA_DIR/certs/ca.pem"
    
    # Copy CA cert to main cert directory for easy access
    cp "$CA_DIR/certs/ca.pem" "$CERT_DIR/ca.pem"
    
    log "Root CA generated successfully"
    log "CA Certificate: $CA_DIR/certs/ca.pem"
    log "CA Private Key: $CA_DIR/private/ca.key"
}

# Generate server certificate
generate_server_cert() {
    local hostname="${1:-localhost}"
    local cert_name="${2:-server}"
    
    log "Generating server certificate for $hostname"
    
    # Generate private key
    openssl genrsa -out "$CERT_DIR/${cert_name}.key" 2048
    chmod 600 "$CERT_DIR/${cert_name}.key"
    
    # Create certificate signing request
    openssl req -config "$CA_DIR/openssl.cnf" \
        -key "$CERT_DIR/${cert_name}.key" \
        -new -sha256 \
        -out "$CERT_DIR/${cert_name}.csr" \
        -subj "/C=US/ST=California/L=San Francisco/O=$COMPANY_NAME/OU=$ENVIRONMENT Brokers/CN=$hostname"
    
    # Create certificate extensions file
    cat > "$CERT_DIR/${cert_name}.ext" << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = $hostname
DNS.2 = $hostname.$ENVIRONMENT.$DOMAIN
DNS.3 = $hostname.internal.$DOMAIN
DNS.4 = localhost
IP.1 = 127.0.0.1
IP.2 = ::1
EOF
    
    # Sign the certificate
    openssl ca -config "$CA_DIR/openssl.cnf" \
        -extensions server_cert -days 365 -notext -md sha256 \
        -in "$CERT_DIR/${cert_name}.csr" \
        -out "$CERT_DIR/${cert_name}.pem" \
        -batch \
        -extfile "$CERT_DIR/${cert_name}.ext"
    
    chmod 644 "$CERT_DIR/${cert_name}.pem"
    
    # Clean up
    rm "$CERT_DIR/${cert_name}.csr" "$CERT_DIR/${cert_name}.ext"
    
    log "Server certificate generated: $CERT_DIR/${cert_name}.pem"
    log "Server private key: $CERT_DIR/${cert_name}.key"
}

# Generate client certificate
generate_client_cert() {
    local client_name="${1:-client}"
    local principal="${2:-$client_name@$DOMAIN}"
    
    log "Generating client certificate for $principal"
    
    # Generate private key
    openssl genrsa -out "$CERT_DIR/${client_name}.key" 2048
    chmod 600 "$CERT_DIR/${client_name}.key"
    
    # Create certificate signing request
    openssl req -config "$CA_DIR/openssl.cnf" \
        -key "$CERT_DIR/${client_name}.key" \
        -new -sha256 \
        -out "$CERT_DIR/${client_name}.csr" \
        -subj "/C=US/ST=California/L=San Francisco/O=$COMPANY_NAME/OU=$ENVIRONMENT Clients/CN=$principal"
    
    # Sign the certificate
    openssl ca -config "$CA_DIR/openssl.cnf" \
        -extensions client_cert -days 90 -notext -md sha256 \
        -in "$CERT_DIR/${client_name}.csr" \
        -out "$CERT_DIR/${client_name}.pem" \
        -batch
    
    chmod 644 "$CERT_DIR/${client_name}.pem"
    
    # Clean up
    rm "$CERT_DIR/${client_name}.csr"
    
    log "Client certificate generated: $CERT_DIR/${client_name}.pem"
    log "Client private key: $CERT_DIR/${client_name}.key"
}

# Generate admin certificate
generate_admin_cert() {
    local admin_name="${1:-admin}"
    local principal="${2:-$admin_name@$DOMAIN}"
    
    log "Generating admin certificate for $principal"
    
    # Generate private key (using ECDSA for admin certs)
    openssl ecparam -genkey -name prime256v1 -out "$CERT_DIR/${admin_name}.key"
    chmod 600 "$CERT_DIR/${admin_name}.key"
    
    # Create certificate signing request
    openssl req -config "$CA_DIR/openssl.cnf" \
        -key "$CERT_DIR/${admin_name}.key" \
        -new -sha256 \
        -out "$CERT_DIR/${admin_name}.csr" \
        -subj "/C=US/ST=California/L=San Francisco/O=$COMPANY_NAME/OU=$ENVIRONMENT Administrators/CN=$principal"
    
    # Sign the certificate (short validity for admin certs)
    openssl ca -config "$CA_DIR/openssl.cnf" \
        -extensions client_cert -days 7 -notext -md sha256 \
        -in "$CERT_DIR/${admin_name}.csr" \
        -out "$CERT_DIR/${admin_name}.pem" \
        -batch
    
    chmod 644 "$CERT_DIR/${admin_name}.pem"
    
    # Clean up
    rm "$CERT_DIR/${admin_name}.csr"
    
    log "Admin certificate generated: $CERT_DIR/${admin_name}.pem"
    log "Admin private key: $CERT_DIR/${admin_name}.key"
}

# Generate certificate bundles
generate_bundles() {
    log "Generating certificate bundles"
    
    # Create full chain certificates
    if [[ -f "$CERT_DIR/server.pem" ]]; then
        cat "$CERT_DIR/server.pem" "$CERT_DIR/ca.pem" > "$CERT_DIR/server-chain.pem"
        log "Server certificate chain: $CERT_DIR/server-chain.pem"
    fi
    
    if [[ -f "$CERT_DIR/client.pem" ]]; then
        cat "$CERT_DIR/client.pem" "$CERT_DIR/ca.pem" > "$CERT_DIR/client-chain.pem"
        log "Client certificate chain: $CERT_DIR/client-chain.pem"
    fi
    
    # Create PKCS#12 bundles for easy distribution
    if [[ -f "$CERT_DIR/client.pem" && -f "$CERT_DIR/client.key" ]]; then
        openssl pkcs12 -export -out "$CERT_DIR/client.p12" \
            -inkey "$CERT_DIR/client.key" \
            -in "$CERT_DIR/client.pem" \
            -certfile "$CERT_DIR/ca.pem" \
            -passout pass:changeme
        log "Client PKCS#12 bundle: $CERT_DIR/client.p12 (password: changeme)"
    fi
}

# Verify generated certificates
verify_certificates() {
    log "Verifying generated certificates"
    
    # Verify CA certificate
    if openssl x509 -in "$CERT_DIR/ca.pem" -noout -text >/dev/null 2>&1; then
        log "✓ CA certificate is valid"
    else
        error "✗ CA certificate is invalid"
    fi
    
    # Verify server certificate against CA
    if [[ -f "$CERT_DIR/server.pem" ]]; then
        if openssl verify -CAfile "$CERT_DIR/ca.pem" "$CERT_DIR/server.pem" >/dev/null 2>&1; then
            log "✓ Server certificate is valid"
        else
            error "✗ Server certificate verification failed"
        fi
    fi
    
    # Verify client certificate against CA
    if [[ -f "$CERT_DIR/client.pem" ]]; then
        if openssl verify -CAfile "$CERT_DIR/ca.pem" "$CERT_DIR/client.pem" >/dev/null 2>&1; then
            log "✓ Client certificate is valid"
        else
            error "✗ Client certificate verification failed"
        fi
    fi
    
    # Verify admin certificate against CA
    if [[ -f "$CERT_DIR/admin.pem" ]]; then
        if openssl verify -CAfile "$CERT_DIR/ca.pem" "$CERT_DIR/admin.pem" >/dev/null 2>&1; then
            log "✓ Admin certificate is valid"
        else
            error "✗ Admin certificate verification failed"
        fi
    fi
}

# Generate certificate information summary
generate_summary() {
    log "Generating certificate summary"
    
    cat > "$CERT_DIR/certificate-summary.txt" << EOF
RustMQ Development Certificates Summary
Generated: $(date)
Environment: $ENVIRONMENT
Company: $COMPANY_NAME
Domain: $DOMAIN

Certificate Authority:
- Location: $CA_DIR/certs/ca.pem
- Subject: $(openssl x509 -in "$CERT_DIR/ca.pem" -noout -subject 2>/dev/null || echo "N/A")
- Valid Until: $(openssl x509 -in "$CERT_DIR/ca.pem" -noout -enddate 2>/dev/null || echo "N/A")

EOF

    if [[ -f "$CERT_DIR/server.pem" ]]; then
        cat >> "$CERT_DIR/certificate-summary.txt" << EOF
Server Certificate:
- Location: $CERT_DIR/server.pem
- Private Key: $CERT_DIR/server.key
- Subject: $(openssl x509 -in "$CERT_DIR/server.pem" -noout -subject 2>/dev/null || echo "N/A")
- Valid Until: $(openssl x509 -in "$CERT_DIR/server.pem" -noout -enddate 2>/dev/null || echo "N/A")

EOF
    fi

    if [[ -f "$CERT_DIR/client.pem" ]]; then
        cat >> "$CERT_DIR/certificate-summary.txt" << EOF
Client Certificate:
- Location: $CERT_DIR/client.pem
- Private Key: $CERT_DIR/client.key
- Subject: $(openssl x509 -in "$CERT_DIR/client.pem" -noout -subject 2>/dev/null || echo "N/A")
- Valid Until: $(openssl x509 -in "$CERT_DIR/client.pem" -noout -enddate 2>/dev/null || echo "N/A")

EOF
    fi

    if [[ -f "$CERT_DIR/admin.pem" ]]; then
        cat >> "$CERT_DIR/certificate-summary.txt" << EOF
Admin Certificate:
- Location: $CERT_DIR/admin.pem
- Private Key: $CERT_DIR/admin.key
- Subject: $(openssl x509 -in "$CERT_DIR/admin.pem" -noout -subject 2>/dev/null || echo "N/A")
- Valid Until: $(openssl x509 -in "$CERT_DIR/admin.pem" -noout -enddate 2>/dev/null || echo "N/A")

EOF
    fi

    cat >> "$CERT_DIR/certificate-summary.txt" << EOF
Usage Instructions:

1. Copy certificates to RustMQ configuration:
   cp $CERT_DIR/ca.pem /etc/rustmq/config/
   cp $CERT_DIR/server.pem $CERT_DIR/server.key /etc/rustmq/config/

2. Configure RustMQ broker with these certificates in broker.toml:
   [security.tls]
   server_cert_path = "/etc/rustmq/config/server.pem"
   server_key_path = "/etc/rustmq/config/server.key"
   client_ca_cert_path = "/etc/rustmq/config/ca.pem"

3. Use client certificate for connections:
   rustmq-client --cert $CERT_DIR/client.pem --key $CERT_DIR/client.key

SECURITY WARNING:
These are development certificates only. Do not use in production!
- Private keys are not password protected
- Certificates have long validity periods
- No hardware security modules used
- Suitable for development and testing only

CERTIFICATE SIGNING STATUS:
✅ Proper certificate chain signing (RustMQ 1.0.0+ compatible)
✅ CA-signed end-entity certificates (no self-signed issues)
✅ Full X.509 certificate validation supported
✅ Compatible with enterprise PKI infrastructure

EOF

    log "Certificate summary saved to: $CERT_DIR/certificate-summary.txt"
}

# Main function
main() {
    log "Starting development certificate generation"
    log "Company: $COMPANY_NAME"
    log "Domain: $DOMAIN"
    log "Environment: $ENVIRONMENT"
    log "Certificate Directory: $CERT_DIR"
    
    warn "This script generates certificates for DEVELOPMENT/TESTING only!"
    warn "DO NOT use these certificates in production environments!"
    
    check_permissions
    setup_directories
    generate_ca_config
    generate_root_ca
    
    # Generate server certificate for localhost and broker
    generate_server_cert "localhost" "server"
    generate_server_cert "broker.$ENVIRONMENT.$DOMAIN" "broker"
    
    # Generate client certificates
    generate_client_cert "client" "client@$DOMAIN"
    generate_client_cert "analytics" "analytics@$DOMAIN"
    generate_client_cert "logger" "logger@$DOMAIN"
    
    # Generate admin certificate
    generate_admin_cert "admin" "admin@$DOMAIN"
    
    generate_bundles
    verify_certificates
    generate_summary
    
    log "Certificate generation completed successfully!"
    log "Summary available at: $CERT_DIR/certificate-summary.txt"
    
    warn "Remember to:"
    warn "1. Set appropriate file permissions"
    warn "2. Backup the CA private key securely"
    warn "3. Replace with production certificates before going live"
    warn "4. Configure certificate renewal processes"
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
        --cert-dir)
            CERT_DIR="$2"
            shift 2
            ;;
        --ca-dir)
            CA_DIR="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo "Options:"
            echo "  --company NAME      Company name (default: YourCompany)"
            echo "  --domain DOMAIN     Domain name (default: company.com)"
            echo "  --environment ENV   Environment (default: dev)"
            echo "  --cert-dir DIR      Certificate directory (default: /etc/rustmq/certs)"
            echo "  --ca-dir DIR        CA directory (default: /etc/rustmq/ca)"
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