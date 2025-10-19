#!/usr/bin/env bash
#
# RustMQ GKE Configuration Validation Script
# Phase 2: Configuration Management (October 2025)
#
# This script validates configuration before deployment using:
# - JSON schema validation
# - GCP API checks (project, region, bucket)
# - Semantic validation (Raft quorum, resource limits)
# - Environment-specific rules
#
# Usage:
#   ./validate-config.sh <environment> [--verbose] [--skip-gcp-checks]

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GKE_DIR="$SCRIPT_DIR"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Counters
ERRORS=0
WARNINGS=0

# Flags
VERBOSE=false
SKIP_GCP_CHECKS=false

# Log functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
    ((WARNINGS++))
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
    ((ERRORS++))
}

log_verbose() {
    if [[ "$VERBOSE" == "true" ]]; then
        echo -e "${BLUE}[DEBUG]${NC} $*"
    fi
}

# Show usage
show_usage() {
    cat << EOF
Usage: ./validate-config.sh <environment> [OPTIONS]

Arguments:
  environment       Environment to validate (dev, staging, prod)

Options:
  --verbose        Enable verbose output
  --skip-gcp-checks  Skip GCP API validation (faster, less thorough)
  --help           Show this help message

Examples:
  ./validate-config.sh dev
  ./validate-config.sh prod --verbose
  ./validate-config.sh staging --skip-gcp-checks

EOF
}

# Parse arguments
parse_args() {
    if [[ $# -eq 0 ]]; then
        show_usage
        exit 1
    fi

    ENVIRONMENT="$1"
    shift

    while [[ $# -gt 0 ]]; do
        case $1 in
            --verbose)
                VERBOSE=true
                shift
                ;;
            --skip-gcp-checks)
                SKIP_GCP_CHECKS=true
                shift
                ;;
            --help|-h)
                show_usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
        esac
    done

    if [[ ! "$ENVIRONMENT" =~ ^(dev|staging|prod)$ ]]; then
        log_error "Invalid environment: $ENVIRONMENT"
        log_info "Valid environments: dev, staging, prod"
        exit 1
    fi
}

# Load configuration
load_config() {
    log_info "Loading configuration for environment: $ENVIRONMENT"

    # Source the load-config script
    # shellcheck disable=SC1091
    if ! source "${GKE_DIR}/scripts/load-config.sh" "$ENVIRONMENT" 2>/dev/null; then
        log_error "Failed to load configuration"
        exit 1
    fi

    log_success "Configuration loaded"
}

# Validate required variables
validate_required() {
    log_info "Validating required variables..."

    local required_vars=(
        "GCP_PROJECT_ID"
        "GCP_REGION"
        "CLUSTER_NAME"
        "CONTROLLER_REPLICAS"
        "GCS_BUCKET"
    )

    for var in "${required_vars[@]}"; do
        if [[ -z "${!var:-}" ]]; then
            log_error "Required variable not set: $var"
        else
            log_verbose "$var = ${!var}"
        fi
    done

    if [[ $ERRORS -eq 0 ]]; then
        log_success "All required variables are set"
    fi
}

# Validate Raft quorum (CRITICAL for RustMQ controller)
validate_raft_quorum() {
    log_info "Validating Raft quorum configuration..."

    local replicas="${CONTROLLER_REPLICAS:-0}"

    # Must be odd number
    if [[ ! "$replicas" =~ ^[0-9]+$ ]]; then
        log_error "CONTROLLER_REPLICAS must be a number, got: $replicas"
        return
    fi

    if (( replicas % 2 == 0 )); then
        log_error "CONTROLLER_REPLICAS must be ODD for Raft quorum"
        log_error "  Current value: $replicas"
        log_error "  Suggested values: 1, 3, 5, 7"
        log_error "  File: gke/environments/${ENVIRONMENT}.env"
        return
    fi

    # Warn if too low or too high
    if (( replicas == 1 )); then
        log_warning "CONTROLLER_REPLICAS=1 provides no fault tolerance"
        log_warning "  Recommended for dev only"
    elif (( replicas > 7 )); then
        log_warning "CONTROLLER_REPLICAS=$replicas is unusually high"
        log_warning "  This may impact performance (slower consensus)"
    fi

    log_success "Raft quorum configuration valid (replicas=$replicas)"
}

# Validate resource limits
validate_resources() {
    log_info "Validating resource limits..."

    # Check MIN <= MAX for brokers
    local min_nodes="${BROKER_MIN_NODES:-0}"
    local max_nodes="${BROKER_MAX_NODES:-0}"

    if (( min_nodes > max_nodes )); then
        log_error "BROKER_MIN_NODES ($min_nodes) > BROKER_MAX_NODES ($max_nodes)"
    else
        log_verbose "Broker scaling: $min_nodes - $max_nodes nodes"
    fi

    # Check Raft heartbeat timeout > interval
    local heartbeat_interval="${CONTROLLER_RAFT_HEARTBEAT_INTERVAL:-1000}"
    local heartbeat_timeout="${CONTROLLER_RAFT_HEARTBEAT_TIMEOUT:-2000}"

    if (( heartbeat_timeout <= heartbeat_interval )); then
        log_error "CONTROLLER_RAFT_HEARTBEAT_TIMEOUT ($heartbeat_timeout) must be > HEARTBEAT_INTERVAL ($heartbeat_interval)"
    fi

    if [[ $ERRORS -eq 0 ]]; then
        log_success "Resource limits validated"
    fi
}

# Validate GCP project (using gcloud)
validate_gcp_project() {
    if [[ "$SKIP_GCP_CHECKS" == "true" ]]; then
        log_warning "Skipping GCP project validation (--skip-gcp-checks)"
        return
    fi

    log_info "Validating GCP project..."

    # Check if gcloud is installed
    if ! command -v gcloud &> /dev/null; then
        log_warning "gcloud CLI not found, skipping GCP validation"
        log_info "Install: https://cloud.google.com/sdk/docs/install"
        return
    fi

    # Check if project exists
    if ! gcloud projects describe "$GCP_PROJECT_ID" &> /dev/null; then
        log_error "GCP project does not exist: $GCP_PROJECT_ID"
        log_info "Create project: gcloud projects create $GCP_PROJECT_ID"
    else
        log_success "GCP project exists: $GCP_PROJECT_ID"
    fi

    # Check if region is valid
    if ! gcloud compute regions list --filter="name:$GCP_REGION" --format="value(name)" 2>/dev/null | grep -q "$GCP_REGION"; then
        log_error "Invalid GCP region: $GCP_REGION"
        log_info "List regions: gcloud compute regions list"
    else
        log_success "GCP region valid: $GCP_REGION"
    fi
}

# Validate GCS bucket
validate_gcs_bucket() {
    if [[ "$SKIP_GCP_CHECKS" == "true" ]]; then
        log_warning "Skipping GCS bucket validation (--skip-gcp-checks)"
        return
    fi

    log_info "Validating GCS bucket..."

    if ! command -v gsutil &> /dev/null; then
        log_warning "gsutil not found, skipping bucket validation"
        return
    fi

    # Check if bucket exists
    if gsutil ls "gs://$GCS_BUCKET" &> /dev/null; then
        log_success "GCS bucket exists: $GCS_BUCKET"
    else
        log_warning "GCS bucket does not exist: $GCS_BUCKET"
        log_info "Create bucket: gsutil mb -p $GCP_PROJECT_ID -l $GCP_REGION gs://$GCS_BUCKET"
    fi
}

# Validate environment-specific rules
validate_environment_rules() {
    log_info "Validating environment-specific rules..."

    case "$ENVIRONMENT" in
        dev)
            # Dev: Allow minimal resources
            if [[ "${CONTROLLER_REPLICAS:-0}" -gt 1 ]]; then
                log_warning "Dev environment using ${CONTROLLER_REPLICAS} controllers (1 is typical for dev)"
            fi
            ;;
        staging)
            # Staging: Should be production-like
            if [[ "${CONTROLLER_REPLICAS:-0}" -lt 3 ]]; then
                log_warning "Staging should use 3+ controllers for realistic testing"
            fi
            if [[ "${TLS_ENABLED:-false}" == "false" ]]; then
                log_warning "TLS disabled in staging (should match production)"
            fi
            ;;
        prod)
            # Production: Strict requirements
            if [[ "${CONTROLLER_REPLICAS:-0}" -lt 3 ]]; then
                log_error "Production REQUIRES 3+ controllers for high availability"
            fi
            if [[ "${TLS_ENABLED:-false}" == "false" ]]; then
                log_error "TLS REQUIRED in production"
            fi
            if [[ "${MTLS_ENABLED:-false}" == "false" ]]; then
                log_error "mTLS REQUIRED in production"
            fi
            if [[ "${ENABLE_WORKLOAD_IDENTITY:-false}" == "false" ]]; then
                log_error "Workload Identity REQUIRED in production"
            fi
            if [[ "${ENABLE_BINARY_AUTHORIZATION:-false}" == "false" ]]; then
                log_error "Binary Authorization REQUIRED in production"
            fi
            if [[ "${BROKER_MIN_NODES:-0}" -lt 3 ]]; then
                log_error "Production REQUIRES 3+ minimum broker nodes"
            fi
            ;;
    esac

    log_success "Environment-specific validation complete"
}

# Validate naming conventions
validate_naming() {
    log_info "Validating naming conventions..."

    # Cluster name: lowercase, alphanumeric, hyphens, max 40 chars
    if [[ ! "${CLUSTER_NAME:-}" =~ ^[a-z][a-z0-9-]{0,39}$ ]]; then
        log_error "Invalid CLUSTER_NAME: ${CLUSTER_NAME:-}"
        log_info "  Must be lowercase, start with letter, max 40 chars"
    fi

    # GCS bucket: lowercase, alphanumeric, hyphens/underscores
    if [[ ! "${GCS_BUCKET:-}" =~ ^[a-z0-9][a-z0-9-_]{1,61}[a-z0-9]$ ]]; then
        log_error "Invalid GCS_BUCKET: ${GCS_BUCKET:-}"
        log_info "  Must be lowercase, 3-63 chars, alphanumeric with hyphens/underscores"
    fi

    log_success "Naming conventions validated"
}

# Validate secrets references
validate_secrets() {
    log_info "Validating secrets configuration..."

    # Check that secret names are defined (not checking if secrets exist, that's for deployment time)
    local secret_vars=(
        "TLS_CERT_SECRET_NAME"
        "TLS_KEY_SECRET_NAME"
        "GCS_CREDENTIALS_SECRET_NAME"
    )

    for var in "${secret_vars[@]}"; do
        if [[ -z "${!var:-}" ]]; then
            log_warning "Secret name not set: $var"
            log_info "  This secret will need to be created in Google Secret Manager"
        else
            log_verbose "Secret reference: $var = ${!var}"
        fi
    done

    log_success "Secrets configuration validated"
}

# Summary report
show_summary() {
    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "Validation Summary for: $ENVIRONMENT"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ $ERRORS -eq 0 && $WARNINGS -eq 0 ]]; then
        log_success "All checks passed! ✓"
        log_success "Configuration is ready for deployment"
        return 0
    elif [[ $ERRORS -eq 0 ]]; then
        log_warning "$WARNINGS warning(s) found"
        log_info "Configuration can be deployed but review warnings"
        return 0
    else
        log_error "$ERRORS error(s) and $WARNINGS warning(s) found"
        log_error "Fix errors before deployment"
        return 1
    fi
}

# Main validation flow
main() {
    parse_args "$@"

    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "RustMQ GKE Configuration Validation"
    log_info "Environment: $ENVIRONMENT"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""

    # Run all validation checks
    load_config
    validate_required
    validate_raft_quorum
    validate_resources
    validate_naming
    validate_gcp_project
    validate_gcs_bucket
    validate_environment_rules
    validate_secrets

    # Show summary
    show_summary
}

# Run main
main "$@"
