#!/bin/bash
# RustMQ Production Environment Setup Script
# This script guides through production setup and delegates to appropriate tools

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

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

# Check prerequisites for production setup
check_production_prerequisites() {
    log "Checking production prerequisites..."
    
    # Check if RustMQ binaries are built
    if [[ ! -f "$PROJECT_ROOT/target/release/rustmq-admin" ]]; then
        error "RustMQ admin binary not found. Build with: cargo build --release"
    fi
    
    if [[ ! -f "$PROJECT_ROOT/target/release/rustmq-broker" ]]; then
        error "RustMQ broker binary not found. Build with: cargo build --release"
    fi
    
    if [[ ! -f "$PROJECT_ROOT/target/release/rustmq-controller" ]]; then
        error "RustMQ controller binary not found. Build with: cargo build --release"
    fi
    
    # Check required commands for production
    local required_commands=("openssl" "systemctl" "firewall-cmd")
    for cmd in "${required_commands[@]}"; do
        if ! command_exists "$cmd"; then
            warn "Command not found (may be needed for production): $cmd"
        fi
    done
    
    log "Production prerequisites check completed ✅"
}

# Show production certificate setup instructions
show_certificate_setup() {
    echo ""
    echo -e "${BLUE}🔐 Production Certificate Setup${NC}"
    echo -e "${BLUE}════════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}Option 1: Use RustMQ Admin CLI (Recommended)${NC}"
    echo ""
    echo "1. Initialize Certificate Authority:"
    echo -e "${YELLOW}   ./target/release/rustmq-admin ca init --cn 'YourCompany RustMQ Root CA' --org 'YourCompany'${NC}"
    echo ""
    echo "2. Generate broker certificate:"
    echo -e "${YELLOW}   ./target/release/rustmq-admin cert create server \\${NC}"
    echo -e "${YELLOW}     --cn broker.yourdomain.com \\${NC}"
    echo -e "${YELLOW}     --san 'broker.yourdomain.com,broker-01.internal,127.0.0.1'${NC}"
    echo ""
    echo "3. Generate client certificates:"
    echo -e "${YELLOW}   ./target/release/rustmq-admin cert create client \\${NC}"
    echo -e "${YELLOW}     --cn client@yourdomain.com \\${NC}"
    echo -e "${YELLOW}     --usage client-auth${NC}"
    echo ""
    echo -e "${GREEN}Option 2: Use External PKI/CA${NC}"
    echo ""
    echo "1. Obtain certificates from your organization's CA"
    echo "2. Ensure certificate chain validation"
    echo "3. Configure paths in production configuration"
    echo ""
    echo -e "${GREEN}Option 3: Use Certificate Manager (Kubernetes)${NC}"
    echo ""
    echo "1. Deploy cert-manager in your cluster"
    echo "2. Create ClusterIssuer or Issuer"
    echo "3. Use Certificate resources for automatic management"
    echo ""
}

# Show production configuration setup
show_configuration_setup() {
    echo ""
    echo -e "${BLUE}⚙️  Production Configuration Setup${NC}"
    echo -e "${BLUE}═══════════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}1. Use Production Configuration Templates${NC}"
    echo ""
    echo "Copy and customize production configurations:"
    echo -e "${YELLOW}   cp docs/security/examples/configurations/production.toml config/production.toml${NC}"
    echo -e "${YELLOW}   cp config/example-production.toml config/broker-prod.toml${NC}"
    echo -e "${YELLOW}   cp config/example-production.toml config/controller-prod.toml${NC}"
    echo ""
    echo -e "${GREEN}2. Key Configuration Areas${NC}"
    echo ""
    echo "• Security settings (TLS, mTLS, authentication)"
    echo "• Storage configuration (object storage, WAL settings)"
    echo "• Cluster configuration (endpoints, Raft settings)"
    echo "• Performance tuning (cache sizes, timeouts)"
    echo "• Monitoring and logging"
    echo ""
    echo -e "${GREEN}3. Environment Variables${NC}"
    echo ""
    echo "Set production environment variables:"
    echo -e "${YELLOW}   export RUSTMQ_ENVIRONMENT=production${NC}"
    echo -e "${YELLOW}   export RUSTMQ_BROKER_ID=broker-\$(hostname)${NC}"
    echo -e "${YELLOW}   export RUSTMQ_CONTROLLER_ID=controller-\$(hostname)${NC}"
    echo ""
}

# Show deployment options
show_deployment_options() {
    echo ""
    echo -e "${BLUE}🚀 Production Deployment Options${NC}"
    echo -e "${BLUE}═══════════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}Option 1: Kubernetes Deployment (Recommended)${NC}"
    echo ""
    echo "1. Use provided Kubernetes manifests:"
    echo -e "${YELLOW}   kubectl apply -f docs/security/examples/kubernetes/security-manifests.yaml${NC}"
    echo ""
    echo "2. Deploy using Helm charts (if available):"
    echo -e "${YELLOW}   helm install rustmq ./charts/rustmq -f values-production.yaml${NC}"
    echo ""
    echo -e "${GREEN}Option 2: Docker Deployment${NC}"
    echo ""
    echo "1. Build production images:"
    echo -e "${YELLOW}   docker build -f Dockerfile.broker -t rustmq/broker:latest .${NC}"
    echo -e "${YELLOW}   docker build -f Dockerfile.controller -t rustmq/controller:latest .${NC}"
    echo ""
    echo "2. Use Docker Compose:"
    echo -e "${YELLOW}   docker-compose -f docker-compose.yml up -d${NC}"
    echo ""
    echo -e "${GREEN}Option 3: Systemd Services${NC}"
    echo ""
    echo "1. Install binaries and create systemd services"
    echo "2. Configure service files with proper security settings"
    echo "3. Enable and start services"
    echo ""
}

# Show security hardening checklist
show_security_checklist() {
    echo ""
    echo -e "${BLUE}🔒 Production Security Checklist${NC}"
    echo -e "${BLUE}═══════════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}Certificate Security:${NC}"
    echo "  ☐ Use production CA-signed certificates"
    echo "  ☐ Enable certificate chain validation"
    echo "  ☐ Configure certificate expiry monitoring"
    echo "  ☐ Set up automatic certificate renewal"
    echo "  ☐ Store private keys securely (HSM recommended)"
    echo ""
    echo -e "${GREEN}Network Security:${NC}"
    echo "  ☐ Enable mTLS for all communications"
    echo "  ☐ Configure firewall rules"
    echo "  ☐ Use VPN or private networks"
    echo "  ☐ Enable network policies (Kubernetes)"
    echo "  ☐ Configure rate limiting"
    echo ""
    echo -e "${GREEN}Authentication & Authorization:${NC}"
    echo "  ☐ Enable authentication for all users"
    echo "  ☐ Configure ACL rules with least privilege"
    echo "  ☐ Set up multi-factor authentication"
    echo "  ☐ Regular access review and rotation"
    echo "  ☐ Enable audit logging"
    echo ""
    echo -e "${GREEN}Infrastructure Security:${NC}"
    echo "  ☐ Regular security updates"
    echo "  ☐ Enable SELinux/AppArmor"
    echo "  ☐ Configure proper file permissions"
    echo "  ☐ Use non-root users"
    echo "  ☐ Enable resource limits"
    echo ""
}

# Show monitoring and observability setup
show_monitoring_setup() {
    echo ""
    echo -e "${BLUE}📊 Production Monitoring & Observability${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}Metrics Collection:${NC}"
    echo "  • Prometheus metrics endpoint: /metrics"
    echo "  • Grafana dashboards for visualization"
    echo "  • Custom alerts for critical events"
    echo ""
    echo -e "${GREEN}Logging:${NC}"
    echo "  • Structured JSON logging"
    echo "  • Centralized log aggregation (ELK/Loki)"
    echo "  • Log retention policies"
    echo ""
    echo -e "${GREEN}Tracing:${NC}"
    echo "  • Distributed tracing with Jaeger/Zipkin"
    echo "  • Performance monitoring"
    echo "  • Error tracking"
    echo ""
    echo -e "${GREEN}Health Checks:${NC}"
    echo "  • Kubernetes readiness/liveness probes"
    echo "  • Load balancer health checks"
    echo "  • Automated recovery procedures"
    echo ""
}

# Show production troubleshooting guide
show_troubleshooting_guide() {
    echo ""
    echo -e "${BLUE}🔧 Production Troubleshooting${NC}"
    echo -e "${BLUE}═══════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}Common Issues:${NC}"
    echo ""
    echo -e "${YELLOW}Certificate Issues:${NC}"
    echo "  • Check certificate expiry: openssl x509 -in cert.pem -noout -dates"
    echo "  • Validate chain: openssl verify -CAfile ca.pem cert.pem"
    echo "  • Check SAN: openssl x509 -in cert.pem -noout -text | grep -A1 'Subject Alternative Name'"
    echo ""
    echo -e "${YELLOW}Connection Issues:${NC}"
    echo "  • Test connectivity: ./target/release/rustmq-admin cluster status"
    echo "  • Check firewall: telnet <broker-host> 9092"
    echo "  • Verify TLS: openssl s_client -connect <broker-host>:9092"
    echo ""
    echo -e "${YELLOW}Performance Issues:${NC}"
    echo "  • Check metrics: curl <broker-host>:8080/metrics"
    echo "  • Monitor logs: tail -f /var/log/rustmq/broker.log"
    echo "  • Analyze resource usage: top, iostat, netstat"
    echo ""
    echo -e "${GREEN}Emergency Procedures:${NC}"
    echo "  • Rolling restart: kubectl rollout restart statefulset/rustmq-broker"
    echo "  • Scale up: kubectl scale statefulset/rustmq-broker --replicas=5"
    echo "  • Backup data: ./target/release/rustmq-admin backup create"
    echo ""
}

# Show next steps
show_next_steps() {
    echo ""
    echo -e "${BLUE}📋 Next Steps for Production Setup${NC}"
    echo -e "${BLUE}═══════════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}1. Plan Your Architecture${NC}"
    echo "   • Determine cluster size and topology"
    echo "   • Plan storage requirements (WAL + object storage)"
    echo "   • Design network architecture and security zones"
    echo ""
    echo -e "${GREEN}2. Prepare Infrastructure${NC}"
    echo "   • Set up Kubernetes cluster or VM infrastructure"
    echo "   • Configure object storage (S3, GCS, Azure)"
    echo "   • Set up monitoring and logging infrastructure"
    echo ""
    echo -e "${GREEN}3. Security Setup${NC}"
    echo "   • Obtain or generate production certificates"
    echo "   • Configure authentication and authorization"
    echo "   • Set up network security (firewalls, VPNs)"
    echo ""
    echo -e "${GREEN}4. Deploy and Test${NC}"
    echo "   • Deploy RustMQ cluster in staging environment"
    echo "   • Run comprehensive testing (functional, performance, security)"
    echo "   • Conduct disaster recovery testing"
    echo ""
    echo -e "${GREEN}5. Production Rollout${NC}"
    echo "   • Deploy to production with blue-green or canary strategy"
    echo "   • Monitor closely during initial rollout"
    echo "   • Document operational procedures"
    echo ""
}

# Generate production setup script
generate_production_setup_script() {
    local setup_script="$PROJECT_ROOT/production-setup-guide.sh"
    
    cat > "$setup_script" << 'EOF'
#!/bin/bash
# RustMQ Production Setup Guide
# This script provides a guided setup process for production deployment

set -euo pipefail

echo "🚀 RustMQ Production Setup Guide"
echo "================================="
echo ""
echo "This guide will help you set up RustMQ for production use."
echo "Please follow the steps carefully and customize for your environment."
echo ""

# Interactive setup questions
echo "📋 Environment Information"
echo "========================="
read -p "Company/Organization name: " COMPANY_NAME
read -p "Domain name: " DOMAIN_NAME
read -p "Environment (production/staging): " ENVIRONMENT
read -p "Deployment platform (kubernetes/docker/systemd): " DEPLOYMENT_PLATFORM
echo ""

echo "🔐 Security Configuration"
echo "========================="
echo "Do you have existing certificates? (y/n)"
read -p "> " HAS_CERTIFICATES

if [[ "$HAS_CERTIFICATES" != "y" ]]; then
    echo ""
    echo "Certificate options:"
    echo "1. Generate with RustMQ Admin CLI"
    echo "2. Use external CA/PKI"
    echo "3. Use cert-manager (Kubernetes)"
    read -p "Choose option (1-3): " CERT_OPTION
fi

echo ""
echo "📊 Monitoring Setup"
echo "=================="
echo "Do you want to enable monitoring? (y/n)"
read -p "> " ENABLE_MONITORING

if [[ "$ENABLE_MONITORING" == "y" ]]; then
    echo "Available monitoring options:"
    echo "1. Prometheus + Grafana"
    echo "2. Custom metrics endpoint"
    echo "3. Cloud provider monitoring"
    read -p "Choose option (1-3): " MONITORING_OPTION
fi

# Generate configuration based on inputs
echo ""
echo "📝 Generating Production Configuration..."
echo "========================================"

# Create production config directory
mkdir -p ./config/production

# Generate environment-specific configuration
cat > "./config/production/environment.conf" << EOF
# RustMQ Production Environment Configuration
# Generated: $(date)

COMPANY_NAME="$COMPANY_NAME"
DOMAIN_NAME="$DOMAIN_NAME"
ENVIRONMENT="$ENVIRONMENT"
DEPLOYMENT_PLATFORM="$DEPLOYMENT_PLATFORM"
EOF

echo "✅ Configuration generated in ./config/production/"
echo ""
echo "📚 Next Steps:"
echo "=============="
echo "1. Review and customize the generated configuration"
echo "2. Set up certificates according to your chosen method"
echo "3. Deploy using your selected platform"
echo "4. Configure monitoring and alerting"
echo "5. Run production validation tests"
echo ""
echo "📖 For detailed instructions, see:"
echo "   docs/deployment/production-guide.md"
echo "   docs/security/production-security.md"
echo ""
EOF

    chmod +x "$setup_script"
    info "Production setup script created: $setup_script"
}

# Main production setup function
main() {
    echo -e "${BLUE}🦀 RustMQ Production Environment Setup Guide${NC}"
    echo -e "${BLUE}📅 $(date)${NC}"
    echo ""
    
    check_production_prerequisites
    
    echo -e "${RED}⚠️  IMPORTANT: Production Security Notice${NC}"
    echo -e "${RED}════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${YELLOW}This script provides guidance for production setup.${NC}"
    echo -e "${YELLOW}DO NOT use development certificates or configurations in production!${NC}"
    echo ""
    echo -e "${GREEN}For production environments, you must:${NC}"
    echo "  • Use CA-signed certificates from a trusted authority"
    echo "  • Configure proper authentication and authorization"
    echo "  • Enable comprehensive monitoring and logging"
    echo "  • Follow security best practices and hardening guides"
    echo "  • Conduct thorough testing before deployment"
    echo ""
    
    show_certificate_setup
    show_configuration_setup
    show_deployment_options
    show_security_checklist
    show_monitoring_setup
    show_troubleshooting_guide
    show_next_steps
    
    echo ""
    echo -e "${GREEN}📚 Additional Resources${NC}"
    echo -e "${GREEN}═══════════════════════${NC}"
    echo ""
    echo "• Production deployment guide: docs/deployment/production-guide.md"
    echo "• Security examples: docs/security/examples/"
    echo "• Kubernetes manifests: docs/security/examples/kubernetes/"
    echo "• Configuration templates: config/example-production.toml"
    echo "• Performance tuning: docs/performance/optimization.md"
    echo ""
    
    generate_production_setup_script
    
    echo -e "${GREEN}✨ Production setup guidance complete!${NC}"
    echo ""
    echo -e "${BLUE}For interactive setup, run:${NC}"
    echo -e "${YELLOW}  ./production-setup-guide.sh${NC}"
    echo ""
}

# Handle command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "This script provides guidance for setting up RustMQ in production."
            echo "It does not automatically configure production systems, but provides"
            echo "instructions and examples for secure production deployment."
            echo ""
            echo "Options:"
            echo "  --help     Show this help message"
            echo ""
            echo "For development setup, use:"
            echo "  ./scripts/setup-development.sh"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Run main setup
main