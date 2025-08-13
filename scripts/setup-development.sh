#!/bin/bash
# RustMQ Development Environment Setup Script
# This script sets up a complete local development environment with certificates, configs, and services

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CERTS_DIR="$PROJECT_ROOT/certs"
CONFIG_DIR="$PROJECT_ROOT/config"

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
    exit 1
}

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
check_prerequisites() {
    log "Checking development prerequisites..."
    
    # Check required commands
    local required_commands=("openssl" "cargo" "git")
    for cmd in "${required_commands[@]}"; do
        if ! command_exists "$cmd"; then
            error "Required command not found: $cmd"
        fi
    done
    
    # Check if we're in RustMQ project directory
    if [[ ! -f "$PROJECT_ROOT/Cargo.toml" ]]; then
        error "This script must be run from the RustMQ project directory"
    fi
    
    log "Prerequisites check completed ✅"
}

# Generate development certificates
generate_development_certificates() {
    log "Generating development certificates..."
    
    mkdir -p "$CERTS_DIR"
    
    # Certificate configuration for development
    local DEFAULT_COUNTRY="US"
    local DEFAULT_STATE="California"
    local DEFAULT_CITY="San Francisco"
    local DEFAULT_ORG="RustMQ Development"
    local DEFAULT_OU="Engineering"
    local DEFAULT_VALIDITY_DAYS=365  # 1 year for development
    local RSA_KEY_SIZE=2048  # Smaller keys for faster development
    local HASH_ALGORITHM="sha256"
    
    # Generate CA configuration
    cat > "$CERTS_DIR/ca.conf" << EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_ca
prompt = no

[req_distinguished_name]
C = $DEFAULT_COUNTRY
ST = $DEFAULT_STATE
L = $DEFAULT_CITY
O = $DEFAULT_ORG
OU = Certificate Authority
CN = RustMQ Development Root CA

[v3_ca]
basicConstraints = critical,CA:TRUE,pathlen:1
keyUsage = critical,keyCertSign,cRLSign,digitalSignature
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer:always
EOF

    # Generate server configuration for development
    cat > "$CERTS_DIR/server.conf" << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = $DEFAULT_COUNTRY
ST = $DEFAULT_STATE
L = $DEFAULT_CITY
O = $DEFAULT_ORG
OU = Development Brokers
CN = localhost

[v3_req]
basicConstraints = CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment,keyAgreement
extendedKeyUsage = serverAuth,clientAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
DNS.2 = broker.local
DNS.3 = broker-01
DNS.4 = rustmq-broker
DNS.5 = 127.0.0.1.nip.io
IP.1 = 127.0.0.1
IP.2 = ::1
EOF

    # Generate client configuration for development
    cat > "$CERTS_DIR/client.conf" << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = $DEFAULT_COUNTRY
ST = $DEFAULT_STATE
L = $DEFAULT_CITY
O = $DEFAULT_ORG
OU = Development Clients
CN = developer@rustmq.dev

[v3_req]
basicConstraints = CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = clientAuth
EOF

    # Generate admin configuration for development
    cat > "$CERTS_DIR/admin.conf" << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = $DEFAULT_COUNTRY
ST = $DEFAULT_STATE
L = $DEFAULT_CITY
O = $DEFAULT_ORG
OU = Development Administrators
CN = admin@rustmq.dev

[v3_req]
basicConstraints = CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = clientAuth,codeSigning
EOF

    # Generate CA key and certificate
    info "Generating CA private key..."
    openssl genrsa -out "$CERTS_DIR/ca.key" $RSA_KEY_SIZE
    chmod 600 "$CERTS_DIR/ca.key"
    
    info "Generating CA certificate..."
    openssl req -new -x509 \
        -key "$CERTS_DIR/ca.key" \
        -out "$CERTS_DIR/ca.pem" \
        -days $DEFAULT_VALIDITY_DAYS \
        -config "$CERTS_DIR/ca.conf" \
        -${HASH_ALGORITHM}
    chmod 644 "$CERTS_DIR/ca.pem"
    
    # Generate certificates for server, client, and admin
    for cert_type in server client admin; do
        local key_file="$CERTS_DIR/${cert_type}.key"
        local csr_file="$CERTS_DIR/${cert_type}.csr"
        local cert_file="$CERTS_DIR/${cert_type}.pem"
        local config_file="$CERTS_DIR/${cert_type}.conf"
        
        info "Generating ${cert_type} certificate..."
        
        # Generate private key
        openssl genrsa -out "$key_file" $RSA_KEY_SIZE
        chmod 600 "$key_file"
        
        # Generate certificate signing request
        openssl req -new \
            -key "$key_file" \
            -out "$csr_file" \
            -config "$config_file"
        
        # Sign certificate with CA
        openssl x509 -req \
            -in "$csr_file" \
            -CA "$CERTS_DIR/ca.pem" \
            -CAkey "$CERTS_DIR/ca.key" \
            -CAcreateserial \
            -out "$cert_file" \
            -days $DEFAULT_VALIDITY_DAYS \
            -extensions v3_req \
            -extfile "$config_file" \
            -${HASH_ALGORITHM}
        
        chmod 644 "$cert_file"
        rm -f "$csr_file"  # Clean up CSR
    done
    
    # Clean up configuration files
    rm -f "$CERTS_DIR"/*.conf "$CERTS_DIR"/*.srl
    
    log "Development certificates generated successfully ✅"
}

# Create development configuration files
create_development_configs() {
    log "Creating development configuration files..."
    
    mkdir -p "$CONFIG_DIR"
    
    # Create development broker configuration
    cat > "$CONFIG_DIR/broker-dev.toml" << EOF
# RustMQ Broker Development Configuration

[broker]
broker_id = "broker-01"
host = "127.0.0.1"
port = 9092
grpc_port = 9093

[cluster]
controller_endpoints = ["127.0.0.1:9094"]

[storage]
wal_dir = "./data/wal"
segment_size_mb = 64
max_segments = 100

[storage.object_storage]
type = "local"
base_path = "./data/segments"

[storage.cache]
enabled = true
max_size_mb = 256
eviction_policy = "Moka"

[security]
enabled = true
tls_cert_path = "./certs/server.pem"
tls_key_path = "./certs/server.key"
ca_cert_path = "./certs/ca.pem"
require_client_cert = true

[logging]
level = "debug"
file = "./logs/broker.log"

[metrics]
enabled = true
port = 8080
EOF

    # Create development controller configuration
    cat > "$CONFIG_DIR/controller-dev.toml" << EOF
# RustMQ Controller Development Configuration

[controller]
controller_id = "controller-01"
host = "127.0.0.1"
port = 9094
raft_port = 9095
http_port = 9642

[raft]
data_dir = "./data/raft"
heartbeat_interval_ms = 500
election_timeout_ms = 2000
max_append_entries_size = 1000

[cluster]
initial_cluster = ["controller-01=127.0.0.1:9095"]

[security]
enabled = true
tls_cert_path = "./certs/server.pem"
tls_key_path = "./certs/server.key"
ca_cert_path = "./certs/ca.pem"

[logging]
level = "debug"
file = "./logs/controller.log"
EOF

    # Create development admin configuration
    cat > "$CONFIG_DIR/admin-dev.toml" << EOF
# RustMQ Admin Development Configuration

[admin]
controller_endpoint = "https://127.0.0.1:9642"
timeout_seconds = 30

[security]
enabled = true
client_cert_path = "./certs/admin.pem"
client_key_path = "./certs/admin.key"
ca_cert_path = "./certs/ca.pem"
verify_server_cert = true

[output]
format = "json"
pretty = true
EOF

    log "Development configuration files created ✅"
}

# Setup development directories
setup_development_directories() {
    log "Setting up development directories..."
    
    # Create necessary directories
    mkdir -p "$PROJECT_ROOT"/{data/{wal,segments,raft},logs,examples}
    
    # Set appropriate permissions
    chmod 755 "$PROJECT_ROOT/data"
    chmod 755 "$PROJECT_ROOT/logs"
    
    log "Development directories created ✅"
}

# Create development startup scripts
create_startup_scripts() {
    log "Creating development startup scripts..."
    
    # Create broker startup script
    cat > "$PROJECT_ROOT/start-broker-dev.sh" << 'EOF'
#!/bin/bash
# Start RustMQ Broker in Development Mode

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🚀 Starting RustMQ Broker (Development)"
echo "📁 Config: config/broker-dev.toml"
echo "🔐 Certs: certs/"
echo ""

cargo run --bin rustmq-broker -- --config config/broker-dev.toml
EOF

    # Create controller startup script
    cat > "$PROJECT_ROOT/start-controller-dev.sh" << 'EOF'
#!/bin/bash
# Start RustMQ Controller in Development Mode

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🚀 Starting RustMQ Controller (Development)"
echo "📁 Config: config/controller-dev.toml"
echo "🔐 Certs: certs/"
echo ""

cargo run --bin rustmq-controller -- --config config/controller-dev.toml
EOF

    # Create development cluster startup script
    cat > "$PROJECT_ROOT/start-cluster-dev.sh" << 'EOF'
#!/bin/bash
# Start Complete RustMQ Development Cluster

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🚀 Starting RustMQ Development Cluster"
echo "📁 Project: $(pwd)"
echo ""

# Function to cleanup on exit
cleanup() {
    echo "🛑 Shutting down RustMQ cluster..."
    pkill -f "rustmq-controller" || true
    pkill -f "rustmq-broker" || true
    exit 0
}

trap cleanup SIGINT SIGTERM

# Start controller first
echo "🎛️  Starting Controller..."
./start-controller-dev.sh &
CONTROLLER_PID=$!

# Wait a bit for controller to start
sleep 3

# Start broker
echo "🖥️  Starting Broker..."
./start-broker-dev.sh &
BROKER_PID=$!

echo ""
echo "✅ RustMQ Development Cluster Started!"
echo "   Controller: https://127.0.0.1:9642"
echo "   Broker: https://127.0.0.1:9092"
echo ""
echo "📋 Admin Commands:"
echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml cluster status"
echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml topic create test-topic"
echo ""
echo "🧪 Test Examples:"
echo "   cargo run --example secure_producer"
echo "   cargo run --example secure_consumer"
echo ""
echo "Press Ctrl+C to stop the cluster"

# Wait for processes
wait $CONTROLLER_PID $BROKER_PID
EOF

    # Make scripts executable
    chmod +x "$PROJECT_ROOT"/start-*-dev.sh

    log "Startup scripts created ✅"
}

# Create development examples
create_development_examples() {
    log "Creating development examples..."
    
    mkdir -p "$PROJECT_ROOT/examples"
    
    # Create secure producer example
    cat > "$PROJECT_ROOT/examples/secure_producer.rs" << 'EOF'
// RustMQ Secure Producer Example (Development)
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 RustMQ Secure Producer (Development)");
    println!("📡 Connecting to: https://127.0.0.1:9092");
    
    // TODO: Implement secure producer using RustMQ client
    // This is a placeholder for the actual implementation
    
    for i in 1..=10 {
        println!("📤 Sending message {}: Hello from secure producer!", i);
        sleep(Duration::from_secs(1)).await;
    }
    
    println!("✅ Producer finished");
    Ok(())
}
EOF

    # Create secure consumer example
    cat > "$PROJECT_ROOT/examples/secure_consumer.rs" << 'EOF'
// RustMQ Secure Consumer Example (Development)
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 RustMQ Secure Consumer (Development)");
    println!("📡 Connecting to: https://127.0.0.1:9092");
    
    // TODO: Implement secure consumer using RustMQ client
    // This is a placeholder for the actual implementation
    
    println!("📥 Waiting for messages...");
    
    for i in 1..=10 {
        sleep(Duration::from_secs(2)).await;
        println!("📨 Received message {}: Hello from secure consumer!", i);
    }
    
    println!("✅ Consumer finished");
    Ok(())
}
EOF

    log "Development examples created ✅"
}

# Validate development setup
validate_development_setup() {
    log "Validating development setup..."
    
    # Check if certificates exist and are valid
    local certs_to_check=("ca" "server" "client" "admin")
    for cert in "${certs_to_check[@]}"; do
        local cert_file="$CERTS_DIR/${cert}.pem"
        if [[ ! -f "$cert_file" ]]; then
            error "Certificate not found: $cert_file"
        fi
        
        # Validate certificate
        if ! openssl x509 -in "$cert_file" -noout -checkend 86400 > /dev/null 2>&1; then
            warn "Certificate expires within 24 hours: $cert_file"
        fi
    done
    
    # Validate certificate chain
    for cert in server client admin; do
        if ! openssl verify -CAfile "$CERTS_DIR/ca.pem" "$CERTS_DIR/${cert}.pem" > /dev/null 2>&1; then
            error "Certificate chain validation failed for: ${cert}.pem"
        fi
    done
    
    # Check if configuration files exist
    local configs_to_check=("broker-dev.toml" "controller-dev.toml" "admin-dev.toml")
    for config in "${configs_to_check[@]}"; do
        if [[ ! -f "$CONFIG_DIR/$config" ]]; then
            error "Configuration file not found: $config"
        fi
    done
    
    # Check if startup scripts exist and are executable
    local scripts_to_check=("start-broker-dev.sh" "start-controller-dev.sh" "start-cluster-dev.sh")
    for script in "${scripts_to_check[@]}"; do
        if [[ ! -x "$PROJECT_ROOT/$script" ]]; then
            error "Startup script not found or not executable: $script"
        fi
    done
    
    log "Development setup validation completed ✅"
}

# Print development setup summary
print_development_summary() {
    echo ""
    echo -e "${GREEN}🎉 RustMQ Development Environment Setup Complete!${NC}"
    echo ""
    echo -e "${BLUE}📁 Project Structure:${NC}"
    echo "   📂 certs/           - Development certificates"
    echo "   📂 config/          - Development configurations"
    echo "   📂 data/            - Runtime data (WAL, segments, Raft)"
    echo "   📂 logs/            - Application logs"
    echo "   📂 examples/        - Development examples"
    echo ""
    echo -e "${BLUE}🚀 Quick Start:${NC}"
    echo "   1. Start the cluster:     ./start-cluster-dev.sh"
    echo "   2. Or start individually:"
    echo "      - Controller:          ./start-controller-dev.sh"
    echo "      - Broker:              ./start-broker-dev.sh"
    echo ""
    echo -e "${BLUE}📋 Admin Commands:${NC}"
    echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml cluster status"
    echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml topic create test-topic"
    echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml acl list"
    echo ""
    echo -e "${BLUE}🧪 Example Applications:${NC}"
    echo "   cargo run --example secure_producer"
    echo "   cargo run --example secure_consumer"
    echo ""
    echo -e "${BLUE}🔗 Service Endpoints:${NC}"
    echo "   Controller:   https://127.0.0.1:9642"
    echo "   Broker QUIC:  https://127.0.0.1:9092"
    echo "   Broker gRPC:  https://127.0.0.1:9093"
    echo "   Metrics:      http://127.0.0.1:8080/metrics"
    echo ""
    echo -e "${YELLOW}⚠️  Development Notes:${NC}"
    echo "   • Certificates are self-signed for development only"
    echo "   • Data is stored locally in ./data/ directory"
    echo "   • Logs are written to ./logs/ directory"
    echo "   • Use --force flag to regenerate certificates"
    echo ""
    echo -e "${GREEN}✨ Happy developing with RustMQ! 🦀${NC}"
}

# Main setup function
main() {
    echo -e "${BLUE}🦀 RustMQ Development Environment Setup${NC}"
    echo -e "${BLUE}📅 $(date)${NC}"
    echo ""
    
    check_prerequisites
    generate_development_certificates
    create_development_configs
    setup_development_directories
    create_startup_scripts
    create_development_examples
    validate_development_setup
    print_development_summary
}

# Handle command line arguments
FORCE_REGENERATE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --force)
            FORCE_REGENERATE=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --force    Force regeneration of existing certificates and configs"
            echo "  --help     Show this help message"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Handle force regeneration
if [[ "$FORCE_REGENERATE" = true ]]; then
    warn "Force regeneration enabled - removing existing certificates and configs"
    rm -rf "$CERTS_DIR" "$CONFIG_DIR"/.*-dev.toml "$PROJECT_ROOT"/start-*-dev.sh
fi

# Check if setup already exists
if [[ -f "$CERTS_DIR/ca.pem" && "$FORCE_REGENERATE" = false ]]; then
    log "Development environment already exists. Use --force to regenerate."
    print_development_summary
    exit 0
fi

# Run main setup
main