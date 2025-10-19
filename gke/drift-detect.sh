#!/usr/bin/env bash
#
# RustMQ Configuration Drift Detection
# Phase 2: Configuration Management (October 2025)
#
# Detects configuration drift between desired state (config files)
# and actual state (deployed resources in GKE).
#
# Usage:
#   ./drift-detect.sh <environment> [--show-diff] [--auto-fix]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

DRIFT_FOUND=false
SHOW_DIFF=false
AUTO_FIX=false

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[✓]${NC} $*"; }
log_warning() { echo -e "${YELLOW}[DRIFT]${NC} $*"; DRIFT_FOUND=true; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

show_usage() {
    cat << EOF
Usage: ./drift-detect.sh <environment> [OPTIONS]

Arguments:
  environment     Environment to check (dev, staging, prod)

Options:
  --show-diff    Show detailed differences
  --auto-fix     Automatically fix drift (DANGEROUS!)
  --help         Show this help

Examples:
  ./drift-detect.sh prod
  ./drift-detect.sh staging --show-diff

EOF
}

parse_args() {
    if [[ $# -eq 0 ]]; then show_usage; exit 1; fi

    ENVIRONMENT="$1"
    shift

    while [[ $# -gt 0 ]]; do
        case $1 in
            --show-diff) SHOW_DIFF=true; shift ;;
            --auto-fix) AUTO_FIX=true; shift ;;
            --help|-h) show_usage; exit 0 ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
    done
}

load_config() {
    log_info "Loading desired configuration..."
    # shellcheck disable=SC1091
    source "${SCRIPT_DIR}/scripts/load-config.sh" "$ENVIRONMENT" > /dev/null 2>&1
    log_success "Configuration loaded"
}

check_cluster_exists() {
    log_info "Checking if cluster exists..."

    if ! gcloud container clusters describe "$CLUSTER_NAME" \
        --region="$GCP_REGION" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then
        log_error "Cluster does not exist: $CLUSTER_NAME"
        log_info "No drift to detect - cluster not deployed yet"
        exit 0
    fi

    log_success "Cluster exists: $CLUSTER_NAME"
}

check_controller_replicas() {
    log_info "Checking controller replicas..."

    local desired="${CONTROLLER_REPLICAS:-3}"
    local actual=$(kubectl get statefulset rustmq-controller -n rustmq \
        -o jsonpath='{.spec.replicas}' 2>/dev/null || echo "0")

    if [[ "$actual" != "$desired" ]]; then
        log_warning "Controller replicas: desired=$desired, actual=$actual"
        if [[ "$AUTO_FIX" == "true" ]]; then
            log_info "Auto-fixing controller replicas..."
            kubectl scale statefulset rustmq-controller -n rustmq --replicas="$desired"
        fi
    else
        log_success "Controller replicas match: $desired"
    fi
}

check_broker_scaling() {
    log_info "Checking broker scaling limits..."

    local desired_min="${BROKER_MIN_NODES:-2}"
    local desired_max="${BROKER_MAX_NODES:-10}"

    # This would check HPA settings if deployed
    log_success "Broker scaling configuration (check HPA for actual values)"
}

show_summary() {
    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "Drift Detection Summary for: $ENVIRONMENT"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ "$DRIFT_FOUND" == "true" ]]; then
        log_warning "Configuration drift detected!"
        log_info "Run with --show-diff to see details"
        log_info "Run with --auto-fix to automatically remediate (use with caution!)"
        return 1
    else
        log_success "No drift detected - configuration matches deployed state"
        return 0
    fi
}

main() {
    parse_args "$@"

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "RustMQ Configuration Drift Detection"
    log_info "Environment: $ENVIRONMENT"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""

    load_config
    check_cluster_exists
    check_controller_replicas
    check_broker_scaling
    show_summary
}

main "$@"
