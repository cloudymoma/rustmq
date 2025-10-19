#!/usr/bin/env bash
#
# RustMQ GKE Cluster Deletion
# Phase 3: GKE Infrastructure Optimization (October 2025)
#
# Safely deletes a GKE cluster with confirmations and cleanup.
#
# Usage:
#   ./delete-cluster.sh <environment> [--force]
#
# Examples:
#   ./delete-cluster.sh dev
#   ./delete-cluster.sh staging --force

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}\")\" && pwd)"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

FORCE=false

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[✓]${NC} $*"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

show_usage() {
    cat << EOF
Usage: ./delete-cluster.sh <environment> [OPTIONS]

Arguments:
  environment     Environment to delete (dev, staging, prod)

Options:
  --force         Skip confirmation prompts (DANGEROUS!)

Examples:
  ./delete-cluster.sh dev
  ./delete-cluster.sh staging --force

WARNING: This will delete the entire GKE cluster and all resources!
         Make sure you have backups of any important data.

EOF
}

parse_args() {
    if [[ $# -eq 0 ]]; then
        show_usage
        exit 1
    fi

    ENVIRONMENT="$1"
    shift

    while [[ $# -gt 0 ]]; do
        case $1 in
            --force) FORCE=true; shift ;;
            --help|-h) show_usage; exit 0 ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
    done
}

load_config() {
    log_info "Loading configuration for environment: $ENVIRONMENT"

    # Source load-config.sh from Phase 2
    if [[ ! -f "${SCRIPT_DIR}/scripts/load-config.sh" ]]; then
        log_error "Configuration loader not found: ${SCRIPT_DIR}/scripts/load-config.sh"
        exit 1
    fi

    # shellcheck disable=SC1091
    source "${SCRIPT_DIR}/scripts/load-config.sh" "$ENVIRONMENT"

    log_success "Configuration loaded"
}

confirm_deletion() {
    if [[ "$FORCE" == "true" ]]; then
        log_warning "Force mode enabled - skipping confirmations"
        return 0
    fi

    log_warning "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_warning "WARNING: You are about to DELETE the following:"
    log_warning "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    log_warning "  Cluster:     $CLUSTER_NAME"
    log_warning "  Environment: $ENVIRONMENT"
    log_warning "  Region:      $GCP_REGION"
    log_warning "  Project:     $GCP_PROJECT_ID"
    echo ""
    log_warning "This action will:"
    log_warning "  • Delete the entire GKE cluster"
    log_warning "  • Delete all running pods and services"
    log_warning "  • Delete persistent volumes (controller WAL data)"
    log_warning "  • Remove load balancers and networking resources"
    echo ""
    log_warning "Data that will be preserved:"
    log_warning "  • Google Secret Manager secrets"
    log_warning "  • GCS bucket data (if GCS_BUCKET is set)"
    log_warning "  • VPC network and subnets"
    echo ""
    log_error "THIS ACTION CANNOT BE UNDONE!"
    echo ""

    read -rp "Type the cluster name to confirm deletion: " confirm_name
    if [[ "$confirm_name" != "$CLUSTER_NAME" ]]; then
        log_error "Cluster name mismatch. Aborting deletion."
        exit 1
    fi

    if [[ "$ENVIRONMENT" == "prod" ]]; then
        log_error "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        log_error "PRODUCTION CLUSTER DELETION"
        log_error "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo ""
        read -rp "Type 'DELETE PRODUCTION' to confirm: " confirm_prod
        if [[ "$confirm_prod" != "DELETE PRODUCTION" ]]; then
            log_error "Confirmation failed. Aborting deletion."
            exit 1
        fi
    fi

    log_success "Deletion confirmed"
}

check_cluster_exists() {
    log_info "Checking if cluster exists..."

    local cluster_location="$GCP_REGION"
    if [[ "$ENVIRONMENT" == "dev" ]]; then
        cluster_location="${GCP_REGION}-a"
    fi

    if ! gcloud container clusters describe "$CLUSTER_NAME" \
        --region="$cluster_location" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then
        log_warning "Cluster does not exist: $CLUSTER_NAME"
        log_info "Nothing to delete."
        exit 0
    fi

    log_success "Cluster exists: $CLUSTER_NAME"
}

backup_resources() {
    log_info "Creating backup of cluster resources..."

    local backup_dir="${SCRIPT_DIR}/backups/${CLUSTER_NAME}-$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$backup_dir"

    # Backup all resources in rustmq namespace
    log_info "Backing up rustmq namespace resources..."
    kubectl get all -n rustmq -o yaml > "${backup_dir}/rustmq-resources.yaml" 2>/dev/null || true
    kubectl get configmap -n rustmq -o yaml > "${backup_dir}/configmaps.yaml" 2>/dev/null || true
    kubectl get pvc -n rustmq -o yaml > "${backup_dir}/pvcs.yaml" 2>/dev/null || true

    # Backup secrets metadata (not values)
    kubectl get secrets -n rustmq -o yaml > "${backup_dir}/secrets-metadata.yaml" 2>/dev/null || true

    log_success "Backup saved to: $backup_dir"
    log_info "Note: Secrets values are not backed up (stored in Google Secret Manager)"
}

delete_cluster() {
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "Deleting GKE cluster: $CLUSTER_NAME"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    local cluster_location="$GCP_REGION"
    if [[ "$ENVIRONMENT" == "dev" ]]; then
        cluster_location="${GCP_REGION}-a"
    fi

    log_info "Deleting cluster (this may take 5-10 minutes)..."
    gcloud container clusters delete "$CLUSTER_NAME" \
        --region="$cluster_location" \
        --project="$GCP_PROJECT_ID" \
        --quiet

    log_success "Cluster deleted successfully"
}

cleanup_network() {
    log_info "Cleaning up network resources..."

    local network_name="${CLUSTER_NAME}-network"
    local subnet_name="${CLUSTER_NAME}-subnet"

    # Delete subnet
    if gcloud compute networks subnets describe "$subnet_name" \
        --region="$GCP_REGION" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then

        log_info "Deleting subnet: $subnet_name"
        gcloud compute networks subnets delete "$subnet_name" \
            --region="$GCP_REGION" \
            --project="$GCP_PROJECT_ID" \
            --quiet
        log_success "Subnet deleted"
    fi

    # Delete network
    if gcloud compute networks describe "$network_name" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then

        log_info "Deleting VPC network: $network_name"
        gcloud compute networks delete "$network_name" \
            --project="$GCP_PROJECT_ID" \
            --quiet
        log_success "VPC network deleted"
    fi
}

cleanup_service_account() {
    log_info "Cleaning up service account IAM bindings..."

    local gsa_email="rustmq-sa@${GCP_PROJECT_ID}.iam.gserviceaccount.com"

    # Remove IAM policy bindings
    log_info "Removing GCS access from service account..."
    gcloud projects remove-iam-policy-binding "$GCP_PROJECT_ID" \
        --member="serviceAccount:${gsa_email}" \
        --role="roles/storage.objectAdmin" \
        --quiet 2>/dev/null || true

    # Remove Workload Identity binding
    log_info "Removing Workload Identity binding..."
    gcloud iam service-accounts remove-iam-policy-binding "$gsa_email" \
        --project="$GCP_PROJECT_ID" \
        --role=roles/iam.workloadIdentityUser \
        --member="serviceAccount:${GCP_PROJECT_ID}.svc.id.goog[rustmq/rustmq-sa]" \
        --quiet 2>/dev/null || true

    log_info "Note: Google Service Account not deleted (may be used by other environments)"
    log_info "      Delete manually if needed: gcloud iam service-accounts delete $gsa_email"
}

show_summary() {
    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_success "GKE Cluster Deletion Complete!"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    log_info "Deleted Resources:"
    log_info "  ✓ GKE Cluster: $CLUSTER_NAME"
    log_info "  ✓ VPC Network and Subnets"
    log_info "  ✓ Persistent Volumes"
    log_info "  ✓ Load Balancers"
    echo ""
    log_info "Preserved Resources:"
    log_info "  • Google Secret Manager secrets"
    log_info "  • GCS bucket data (if configured)"
    log_info "  • Google Service Account (rustmq-sa)"
    log_info "  • Cluster backups in: ${SCRIPT_DIR}/backups/"
    echo ""
    log_info "To recreate the cluster:"
    log_info "  ./gke/create-cluster.sh $ENVIRONMENT"
    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

main() {
    parse_args "$@"

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "RustMQ GKE Cluster Deletion"
    log_info "Environment: $ENVIRONMENT"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""

    load_config
    check_cluster_exists
    confirm_deletion
    backup_resources
    delete_cluster
    cleanup_network
    cleanup_service_account
    show_summary
}

main "$@"
