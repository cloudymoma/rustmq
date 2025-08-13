#!/bin/bash

# RustMQ Environment Setup Script - Main Entry Point
# This script delegates to appropriate environment setup scripts
# Compatible with RustMQ 1.0.0+ Certificate Signing Implementation

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="${SCRIPT_DIR}/scripts"

# Color output for better UX
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🦀 RustMQ Environment Setup v3.0${NC}"
echo -e "${BLUE}🚀 Complete Environment Configuration${NC}"
echo ""

# Function to show usage
show_usage() {
    echo "Usage: $0 <environment> [OPTIONS]"
    echo ""
    echo "Environments:"
    echo "  develop       Set up complete development environment with certificates, configs, and services"
    echo "  production    Show production setup guidance and best practices"
    echo ""
    echo "Options:"
    echo "  --force       Force regeneration of existing certificates and configs"
    echo "  --help        Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 develop                    # Set up development environment"
    echo "  $0 develop --force            # Force regenerate development setup"
    echo "  $0 production                 # Show production setup guidance"
    echo ""
    echo "What each environment provides:"
    echo ""
    echo -e "${GREEN}Development Environment:${NC}"
    echo "  ✅ Self-signed development certificates"
    echo "  ✅ Development configuration files"
    echo "  ✅ Local data directories and startup scripts"
    echo "  ✅ Example applications and test clients"
    echo "  ✅ Ready-to-run local cluster"
    echo ""
    echo -e "${BLUE}Production Environment:${NC}"
    echo "  📋 Production setup guidance and checklists"
    echo "  📋 Security best practices and hardening"
    echo "  📋 Deployment options (Kubernetes, Docker, systemd)"
    echo "  📋 Monitoring and observability setup"
    echo "  📋 Certificate management with external CA"
    echo ""
}

# Parse command line arguments
ENVIRONMENT=""
FORCE_REGENERATE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        develop|development)
            ENVIRONMENT="development"
            shift
            ;;
        prod|production)
            ENVIRONMENT="production"
            shift
            ;;
        --force)
            FORCE_REGENERATE=true
            shift
            ;;
        --help|-h)
            show_usage
            exit 0
            ;;
        *)
            if [[ -z "$ENVIRONMENT" ]]; then
                echo -e "${RED}❌ Unknown environment: $1${NC}"
            else
                echo -e "${RED}❌ Unknown option: $1${NC}"
            fi
            echo ""
            show_usage
            exit 1
            ;;
    esac
done

# Check if environment is specified
if [[ -z "$ENVIRONMENT" ]]; then
    echo -e "${RED}❌ No environment specified${NC}"
    echo ""
    show_usage
    exit 1
fi

# Check if scripts directory exists
if [[ ! -d "$SCRIPTS_DIR" ]]; then
    echo -e "${RED}❌ Scripts directory not found: $SCRIPTS_DIR${NC}"
    echo -e "${YELLOW}Please ensure you're running this from the RustMQ project root${NC}"
    exit 1
fi

# Delegate to appropriate script based on environment
case "$ENVIRONMENT" in
    "development")
        echo -e "${GREEN}🛠️  Setting up development environment...${NC}"
        echo ""
        
        # Check if development setup script exists
        DEVELOPMENT_SCRIPT="$SCRIPTS_DIR/setup-development.sh"
        if [[ ! -f "$DEVELOPMENT_SCRIPT" ]]; then
            echo -e "${RED}❌ Development setup script not found: $DEVELOPMENT_SCRIPT${NC}"
            exit 1
        fi
        
        # Make script executable
        chmod +x "$DEVELOPMENT_SCRIPT"
        
        # Pass force flag if specified
        if [[ "$FORCE_REGENERATE" = true ]]; then
            echo -e "${YELLOW}🔄 Force regeneration enabled${NC}"
            exec "$DEVELOPMENT_SCRIPT" --force
        else
            exec "$DEVELOPMENT_SCRIPT"
        fi
        ;;
        
    "production")
        echo -e "${BLUE}🏭 Production environment setup guidance...${NC}"
        echo ""
        
        # Check if production setup script exists
        PRODUCTION_SCRIPT="$SCRIPTS_DIR/setup-production.sh"
        if [[ ! -f "$PRODUCTION_SCRIPT" ]]; then
            echo -e "${RED}❌ Production setup script not found: $PRODUCTION_SCRIPT${NC}"
            exit 1
        fi
        
        # Make script executable
        chmod +x "$PRODUCTION_SCRIPT"
        
        # Production script doesn't use force flag, just execute
        exec "$PRODUCTION_SCRIPT"
        ;;
        
    *)
        echo -e "${RED}❌ Unsupported environment: $ENVIRONMENT${NC}"
        echo ""
        show_usage
        exit 1
        ;;
esac