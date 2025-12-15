#!/bin/bash
# RustMQ Environment Variable Validation Script
# Validates required environment variables before starting RustMQ services

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track validation status
FAILED=0

echo "RustMQ Environment Validation"
echo "=============================="
echo ""

# =========================================================================
# 1. Check RUSTMQ_KEY_ENCRYPTION_PASSWORD (CRITICAL - required for all components)
# =========================================================================

if [ -z "${RUSTMQ_KEY_ENCRYPTION_PASSWORD:-}" ]; then
    echo -e "${RED}❌ ERROR: RUSTMQ_KEY_ENCRYPTION_PASSWORD not set${NC}"
    echo ""
    echo "This is REQUIRED for brokers and controllers to start."
    echo "Certificate private keys cannot be encrypted without it."
    echo ""
    echo "Set it with:"
    echo "  export RUSTMQ_KEY_ENCRYPTION_PASSWORD=\"\$(openssl rand -base64 32)\""
    echo ""
    FAILED=1
else
    PASSWORD_LENGTH=${#RUSTMQ_KEY_ENCRYPTION_PASSWORD}

    if [ "$PASSWORD_LENGTH" -lt 16 ]; then
        echo -e "${RED}❌ ERROR: Encryption password too short ($PASSWORD_LENGTH characters)${NC}"
        echo "  Minimum: 16 characters"
        echo "  Recommended: 32+ characters"
        echo ""
        echo "Generate a stronger password:"
        echo "  export RUSTMQ_KEY_ENCRYPTION_PASSWORD=\"\$(openssl rand -base64 32)\""
        echo ""
        FAILED=1
    elif [ "$PASSWORD_LENGTH" -lt 24 ]; then
        echo -e "${YELLOW}⚠️  WARNING: Encryption password is short ($PASSWORD_LENGTH characters)${NC}"
        echo "  Recommended: 32+ characters for production"
        echo ""
    else
        echo -e "${GREEN}✅ RUSTMQ_KEY_ENCRYPTION_PASSWORD configured ($PASSWORD_LENGTH characters)${NC}"
    fi
fi

# =========================================================================
# 2. Check MinIO credentials (required for docker-compose)
# =========================================================================

if [ -f "docker/docker-compose.yml" ]; then
    echo ""
    echo "Docker Compose Configuration"
    echo "-----------------------------"

    if [ -z "${MINIO_ACCESS_KEY:-}" ] || [ -z "${MINIO_SECRET_KEY:-}" ]; then
        echo -e "${YELLOW}⚠️  WARNING: MINIO_ACCESS_KEY or MINIO_SECRET_KEY not set${NC}"
        echo "  Required for docker-compose deployments"
        echo ""
        echo "Set with:"
        echo "  export MINIO_ACCESS_KEY=\"\$(openssl rand -base64 32)\""
        echo "  export MINIO_SECRET_KEY=\"\$(openssl rand -base64 32)\""
        echo ""
        echo "Or create docker/.env file:"
        echo "  cd docker && cp .env.example .env"
        echo "  # Edit .env with generated values"
        echo ""
    else
        echo -e "${GREEN}✅ MinIO credentials configured${NC}"
        echo "  Access key: ${#MINIO_ACCESS_KEY} characters"
        echo "  Secret key: ${#MINIO_SECRET_KEY} characters"
    fi
fi

# =========================================================================
# 3. Check optional GCP credentials (for BigQuery subscriber)
# =========================================================================

if [ -n "${GCP_PROJECT_ID:-}" ] || [ -n "${BIGQUERY_DATASET:-}" ]; then
    echo ""
    echo "Google Cloud Configuration (Optional)"
    echo "------------------------------------"

    if [ -z "${GCP_PROJECT_ID:-}" ]; then
        echo -e "${YELLOW}⚠️  WARNING: GCP_PROJECT_ID not set${NC}"
        echo "  Required for BigQuery subscriber"
    else
        echo -e "${GREEN}✅ GCP_PROJECT_ID: $GCP_PROJECT_ID${NC}"
    fi

    if [ -z "${BIGQUERY_DATASET:-}" ]; then
        echo -e "${YELLOW}⚠️  WARNING: BIGQUERY_DATASET not set${NC}"
    else
        echo -e "${GREEN}✅ BIGQUERY_DATASET: $BIGQUERY_DATASET${NC}"
    fi

    if [ -z "${BIGQUERY_TABLE:-}" ]; then
        echo -e "${YELLOW}⚠️  WARNING: BIGQUERY_TABLE not set${NC}"
    else
        echo -e "${GREEN}✅ BIGQUERY_TABLE: $BIGQUERY_TABLE${NC}"
    fi

    if [ -z "${GOOGLE_APPLICATION_CREDENTIALS:-}" ]; then
        echo -e "${YELLOW}⚠️  WARNING: GOOGLE_APPLICATION_CREDENTIALS not set${NC}"
        echo "  Using Workload Identity or default credentials"
    else
        if [ -f "$GOOGLE_APPLICATION_CREDENTIALS" ]; then
            echo -e "${GREEN}✅ GOOGLE_APPLICATION_CREDENTIALS: $GOOGLE_APPLICATION_CREDENTIALS${NC}"
        else
            echo -e "${RED}❌ ERROR: Credentials file not found: $GOOGLE_APPLICATION_CREDENTIALS${NC}"
            FAILED=1
        fi
    fi
fi

# =========================================================================
# Final Summary
# =========================================================================

echo ""
echo "=============================="

if [ $FAILED -eq 1 ]; then
    echo -e "${RED}❌ Validation FAILED${NC}"
    echo ""
    echo "Fix the errors above before starting RustMQ"
    echo ""
    echo "For help, see:"
    echo "  - README.md (Security Configuration section)"
    echo "  - docker/.env.example (template with all variables)"
    echo "  - SECURITY_AUDIT_SUMMARY.md (full security guide)"
    echo ""
    exit 1
else
    echo -e "${GREEN}✅ All required environment variables validated${NC}"
    echo ""
    echo "You can now start RustMQ services:"
    echo "  - Rust binaries: cargo run --bin rustmq-broker -- --config config/broker.toml"
    echo "  - Docker Compose: cd docker && docker-compose up -d"
    echo "  - Kubernetes: kubectl apply -k gke/manifests/overlays/dev"
    echo ""
fi

exit 0
