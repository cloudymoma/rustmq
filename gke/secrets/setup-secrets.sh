#!/usr/bin/env bash
#
# RustMQ Google Secret Manager Setup Script
# Phase 2: Configuration Management (October 2025)
#
# This script creates and manages secrets in Google Secret Manager
# for RustMQ GKE deployment.
#
# Google Cloud native services used:
# - Google Secret Manager for secret storage
# - Workload Identity for secure access
# - IAM for permission management
#
# Usage:
#   ./setup-secrets.sh <environment> <action>
#
# Actions:
#   create    - Create all secrets
#   update    - Update existing secrets
#   delete    - Delete all secrets
#   list      - List all secrets
#   grant     - Grant Workload Identity access

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GKE_DIR="$(dirname "$SCRIPT_DIR")"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Log functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

# Show usage
show_usage() {
    cat << EOF
Usage: ./setup-secrets.sh <environment> <action>

Arguments:
  environment    Environment (dev, staging, prod)
  action         Action to perform

Actions:
  create         Create all secrets in Google Secret Manager
  update         Update existing secrets
  delete         Delete all secrets (DANGEROUS!)
  list           List all secrets
  grant-access   Grant Workload Identity access to secrets

Examples:
  ./setup-secrets.sh prod create
  ./setup-secrets.sh dev list
  ./setup-secrets.sh staging grant-access

Secrets managed:
  - TLS certificates (cert, key, CA)
  - GCS credentials
  - Admin API keys
  - Database credentials (if needed)

Note: Actual secret values are NOT stored in git.
      You will be prompted to provide values or paths to files.

EOF
}

# Parse arguments
parse_args() {
    if [[ $# -lt 2 ]]; then
        show_usage
        exit 1
    fi

    ENVIRONMENT="$1"
    ACTION="$2"

    if [[ ! "$ENVIRONMENT" =~ ^(dev|staging|prod)$ ]]; then
        log_error "Invalid environment: $ENVIRONMENT"
        exit 1
    fi

    if [[ ! "$ACTION" =~ ^(create|update|delete|list|grant-access)$ ]]; then
        log_error "Invalid action: $ACTION"
        show_usage
        exit 1
    fi
}

# Load configuration
load_config() {
    log_info "Loading configuration for: $ENVIRONMENT"

    # Source the load-config script
    # shellcheck disable=SC1091
    if ! source "${GKE_DIR}/scripts/load-config.sh" "$ENVIRONMENT" > /dev/null 2>&1; then
        log_error "Failed to load configuration"
        exit 1
    fi

    log_success "Configuration loaded"
    log_info "Project: $GCP_PROJECT_ID"
    log_info "Region: $GCP_REGION"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check gcloud
    if ! command -v gcloud &> /dev/null; then
        log_error "gcloud CLI not found"
        log_info "Install: https://cloud.google.com/sdk/docs/install"
        exit 1
    fi

    # Check authentication
    if ! gcloud auth list --filter=status:ACTIVE --format="value(account)" &> /dev/null; then
        log_error "Not authenticated with gcloud"
        log_info "Run: gcloud auth login"
        exit 1
    fi

    # Enable Secret Manager API
    log_info "Ensuring Secret Manager API is enabled..."
    gcloud services enable secretmanager.googleapis.com \
        --project="$GCP_PROJECT_ID" 2>/dev/null || true

    log_success "Prerequisites checked"
}

# Create a secret in Secret Manager
create_secret() {
    local secret_name=$1
    local secret_description=$2
    local secret_file=${3:-}

    log_info "Creating secret: $secret_name"

    # Check if secret already exists
    if gcloud secrets describe "$secret_name" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then
        log_warning "Secret already exists: $secret_name"
        log_info "Use 'update' action to update existing secrets"
        return 0
    fi

    # Create the secret
    if ! gcloud secrets create "$secret_name" \
        --project="$GCP_PROJECT_ID" \
        --replication-policy="user-managed" \
        --locations="$GCP_REGION" \
        --labels="environment=$ENVIRONMENT,managed-by=rustmq" \
        --description="$secret_description"; then
        log_error "Failed to create secret: $secret_name"
        return 1
    fi

    # Add secret version (value)
    if [[ -n "$secret_file" && -f "$secret_file" ]]; then
        # From file
        log_info "Adding secret value from file: $secret_file"
        gcloud secrets versions add "$secret_name" \
            --project="$GCP_PROJECT_ID" \
            --data-file="$secret_file"
    else
        # Generate placeholder or prompt
        log_info "Creating placeholder secret (update with actual value later)"
        echo "PLACEHOLDER_UPDATE_ME" | gcloud secrets versions add "$secret_name" \
            --project="$GCP_PROJECT_ID" \
            --data-file=-
    fi

    log_success "Secret created: $secret_name"
}

# Create all RustMQ secrets
create_all_secrets() {
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "Creating RustMQ Secrets in Google Secret Manager"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # TLS Certificate
    create_secret \
        "${TLS_CERT_SECRET_NAME}" \
        "RustMQ TLS Certificate for $ENVIRONMENT" \
        ""

    # TLS Private Key
    create_secret \
        "${TLS_KEY_SECRET_NAME}" \
        "RustMQ TLS Private Key for $ENVIRONMENT" \
        ""

    # TLS CA Certificate
    create_secret \
        "${TLS_CA_SECRET_NAME:-rustmq-ca-cert}" \
        "RustMQ CA Certificate for $ENVIRONMENT" \
        ""

    # GCS Credentials (Service Account Key)
    create_secret \
        "${GCS_CREDENTIALS_SECRET_NAME}" \
        "RustMQ GCS Credentials for $ENVIRONMENT" \
        ""

    log_success "All secrets created"
    log_warning "IMPORTANT: Update placeholder values with actual secrets!"
    log_info "Use: gcloud secrets versions add SECRET_NAME --data-file=path/to/secret"
}

# List all secrets
list_secrets() {
    log_info "Listing secrets for: $ENVIRONMENT"

    gcloud secrets list \
        --project="$GCP_PROJECT_ID" \
        --filter="labels.environment=$ENVIRONMENT" \
        --format="table(name,createTime,labels.list())"
}

# Delete all secrets
delete_all_secrets() {
    log_warning "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_warning "DANGER: Deleting ALL secrets for $ENVIRONMENT"
    log_warning "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    read -p "Are you sure? Type '$ENVIRONMENT' to confirm: " confirm
    if [[ "$confirm" != "$ENVIRONMENT" ]]; then
        log_info "Aborted"
        exit 0
    fi

    local secrets=(
        "${TLS_CERT_SECRET_NAME}"
        "${TLS_KEY_SECRET_NAME}"
        "${TLS_CA_SECRET_NAME:-rustmq-ca-cert}"
        "${GCS_CREDENTIALS_SECRET_NAME}"
    )

    for secret in "${secrets[@]}"; do
        if gcloud secrets describe "$secret" --project="$GCP_PROJECT_ID" &> /dev/null; then
            log_info "Deleting secret: $secret"
            gcloud secrets delete "$secret" --project="$GCP_PROJECT_ID" --quiet
            log_success "Deleted: $secret"
        fi
    done

    log_success "All secrets deleted"
}

# Grant Workload Identity access to secrets
grant_workload_identity_access() {
    log_info "Granting Workload Identity access to secrets..."

    local ksa_name="rustmq-sa"  # Kubernetes Service Account
    local ksa_namespace="rustmq"
    local gsa_name="rustmq-sa@${GCP_PROJECT_ID}.iam.gserviceaccount.com"

    # Create Google Service Account if it doesn't exist
    if ! gcloud iam service-accounts describe "$gsa_name" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then
        log_info "Creating Google Service Account: $gsa_name"
        gcloud iam service-accounts create "rustmq-sa" \
            --project="$GCP_PROJECT_ID" \
            --display-name="RustMQ Service Account for $ENVIRONMENT" \
            --description="Service account for RustMQ workloads"
    fi

    # Grant Secret Manager access to GSA
    log_info "Granting Secret Manager access to GSA..."
    gcloud projects add-iam-policy-binding "$GCP_PROJECT_ID" \
        --member="serviceAccount:$gsa_name" \
        --role="roles/secretmanager.secretAccessor" \
        --condition=None

    # Bind KSA to GSA for Workload Identity
    log_info "Binding Kubernetes SA to Google SA..."
    gcloud iam service-accounts add-iam-policy-binding "$gsa_name" \
        --project="$GCP_PROJECT_ID" \
        --role="roles/iam.workloadIdentityUser" \
        --member="serviceAccount:${GCP_PROJECT_ID}.svc.id.goog[${ksa_namespace}/${ksa_name}]"

    log_success "Workload Identity access granted"
    log_info "Kubernetes Service Account: $ksa_namespace/$ksa_name"
    log_info "Google Service Account: $gsa_name"
}

# Main function
main() {
    parse_args "$@"
    load_config
    check_prerequisites

    case "$ACTION" in
        create)
            create_all_secrets
            ;;
        list)
            list_secrets
            ;;
        delete)
            delete_all_secrets
            ;;
        grant-access)
            grant_workload_identity_access
            ;;
        update)
            log_info "Update individual secrets using:"
            log_info "  gcloud secrets versions add SECRET_NAME --data-file=path/to/secret"
            ;;
        *)
            log_error "Unknown action: $ACTION"
            show_usage
            exit 1
            ;;
    esac

    log_success "Operation completed"
}

# Run main
main "$@"
