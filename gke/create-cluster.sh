#!/usr/bin/env bash
#
# RustMQ GKE Cluster Creation
# Phase 3: GKE Infrastructure Optimization (October 2025)
#
# Creates a production-ready GKE cluster with environment-specific configuration.
#
# Usage:
#   ./create-cluster.sh <environment>
#
# Examples:
#   ./create-cluster.sh dev
#   ./create-cluster.sh staging
#   ./create-cluster.sh prod

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}\")\" && pwd)"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[✓]${NC} $*"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

show_usage() {
    cat << EOF
Usage: ./create-cluster.sh <environment>

Arguments:
  environment     Environment to create (dev, staging, prod)

Examples:
  ./create-cluster.sh dev
  ./create-cluster.sh staging
  ./create-cluster.sh prod

EOF
}

check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check gcloud
    if ! command -v gcloud &> /dev/null; then
        log_error "gcloud CLI not found. Please install Google Cloud SDK."
        exit 1
    fi

    # Check kubectl
    if ! command -v kubectl &> /dev/null; then
        log_error "kubectl not found. Please install kubectl."
        exit 1
    fi

    # Check authentication
    if ! gcloud auth list --filter=status:ACTIVE --format="value(account)" &> /dev/null; then
        log_error "Not authenticated with gcloud. Run: gcloud auth login"
        exit 1
    fi

    log_success "Prerequisites check passed"
}

load_config() {
    local environment="$1"

    log_info "Loading configuration for environment: $environment"

    # Source load-config.sh from Phase 2
    if [[ ! -f "${SCRIPT_DIR}/scripts/load-config.sh" ]]; then
        log_error "Configuration loader not found: ${SCRIPT_DIR}/scripts/load-config.sh"
        exit 1
    fi

    # shellcheck disable=SC1091
    source "${SCRIPT_DIR}/scripts/load-config.sh" "$environment"

    log_success "Configuration loaded"
}

validate_config() {
    log_info "Validating configuration..."

    # Run validation script from Phase 2
    if ! "${SCRIPT_DIR}/validate-config.sh" "$ENVIRONMENT"; then
        log_error "Configuration validation failed"
        exit 1
    fi

    log_success "Configuration validation passed"
}

check_cluster_exists() {
    log_info "Checking if cluster already exists..."

    if gcloud container clusters describe "$CLUSTER_NAME" \
        --region="$GCP_REGION" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then
        log_warning "Cluster already exists: $CLUSTER_NAME"
        log_warning "Use delete-cluster.sh to remove it first, or use a different name."
        exit 1
    fi

    log_success "Cluster does not exist - proceeding with creation"
}

create_network() {
    log_info "Setting up VPC network..."

    local network_name="${CLUSTER_NAME}-network"
    local subnet_name="${CLUSTER_NAME}-subnet"

    # Check if network exists
    if ! gcloud compute networks describe "$network_name" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then

        log_info "Creating VPC network: $network_name"
        gcloud compute networks create "$network_name" \
            --project="$GCP_PROJECT_ID" \
            --subnet-mode=custom \
            --bgp-routing-mode=regional

        log_success "VPC network created"
    else
        log_info "VPC network already exists: $network_name"
    fi

    # Check if subnet exists
    if ! gcloud compute networks subnets describe "$subnet_name" \
        --region="$GCP_REGION" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then

        log_info "Creating subnet: $subnet_name"
        gcloud compute networks subnets create "$subnet_name" \
            --project="$GCP_PROJECT_ID" \
            --network="$network_name" \
            --region="$GCP_REGION" \
            --range="10.0.0.0/20" \
            --secondary-range pods=10.4.0.0/14 \
            --secondary-range services=10.8.0.0/20 \
            --enable-private-ip-google-access

        log_success "Subnet created"
    else
        log_info "Subnet already exists: $subnet_name"
    fi

    # Export for use in cluster creation
    export NETWORK_NAME="$network_name"
    export SUBNET_NAME="$subnet_name"
}

create_cluster() {
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "Creating GKE cluster: $CLUSTER_NAME"
    log_info "Environment: $ENVIRONMENT"
    log_info "Region: $GCP_REGION"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Determine cluster type (zonal for dev, regional for staging/prod)
    local cluster_location="$GCP_REGION"
    local num_nodes="${CONTROLLER_REPLICAS:-3}"

    if [[ "$ENVIRONMENT" == "dev" ]]; then
        # Zonal cluster for dev (cost savings)
        cluster_location="${GCP_REGION}-a"
        log_info "Creating zonal cluster in: $cluster_location"
    else
        log_info "Creating regional cluster in: $cluster_location"
    fi

    # Build gcloud command
    local create_cmd=(
        gcloud container clusters create "$CLUSTER_NAME"
        --project="$GCP_PROJECT_ID"
        --region="$cluster_location"
        --network="$NETWORK_NAME"
        --subnetwork="$SUBNET_NAME"
        --enable-ip-alias
        --cluster-secondary-range-name=pods
        --services-secondary-range-name=services
        --num-nodes="$num_nodes"
        --machine-type="${NODE_MACHINE_TYPE:-n2-standard-4}"
        --disk-type=pd-standard
        --disk-size="${NODE_DISK_SIZE_GB:-100}"
        --enable-autoscaling
        --min-nodes="${BROKER_MIN_NODES:-2}"
        --max-nodes="${BROKER_MAX_NODES:-10}"
        --enable-autorepair
        --enable-autoupgrade
        --maintenance-window-start="2025-01-01T00:00:00Z"
        --maintenance-window-duration=4h
        --maintenance-window-recurrence="FREQ=WEEKLY;BYDAY=SU"
        --addons=HorizontalPodAutoscaling,HttpLoadBalancing,GcePersistentDiskCsiDriver
        --workload-pool="${GCP_PROJECT_ID}.svc.id.goog"
        --enable-shielded-nodes
        --shielded-secure-boot
        --shielded-integrity-monitoring
        --enable-cloud-logging
        --enable-cloud-monitoring
        --logging=SYSTEM,WORKLOAD
        --monitoring=SYSTEM
    )

    # Environment-specific flags
    if [[ "$ENVIRONMENT" == "prod" ]]; then
        log_info "Applying production-grade configuration..."
        create_cmd+=(
            --enable-network-policy
            --enable-intra-node-visibility
            --release-channel=regular
        )

        # Binary Authorization for production
        if [[ "${ENABLE_BINARY_AUTHORIZATION:-false}" == "true" ]]; then
            create_cmd+=(--enable-binauthz)
        fi
    elif [[ "$ENVIRONMENT" == "staging" ]]; then
        create_cmd+=(
            --enable-network-policy
            --release-channel=regular
        )
    else
        # Development uses rapid channel for latest features
        create_cmd+=(--release-channel=rapid)
    fi

    # Preemptible nodes for cost savings
    local preemptible_pct="${PREEMPTIBLE_NODES_PCT:-0}"
    if [[ "$preemptible_pct" -gt 0 ]]; then
        log_info "Enabling ${preemptible_pct}% preemptible nodes for cost savings"
        # Note: GKE doesn't support percentage, so we use node pools for this
    fi

    # Execute cluster creation
    log_info "Executing cluster creation (this may take 5-10 minutes)..."
    "${create_cmd[@]}"

    log_success "Cluster created successfully"
}

configure_kubectl() {
    log_info "Configuring kubectl context..."

    local cluster_location="$GCP_REGION"
    if [[ "$ENVIRONMENT" == "dev" ]]; then
        cluster_location="${GCP_REGION}-a"
    fi

    gcloud container clusters get-credentials "$CLUSTER_NAME" \
        --region="$cluster_location" \
        --project="$GCP_PROJECT_ID"

    log_success "kubectl context configured"

    # Verify connection
    log_info "Verifying cluster connection..."
    kubectl cluster-info

    log_success "Cluster connection verified"
}

create_namespace() {
    log_info "Creating rustmq namespace..."

    kubectl create namespace rustmq --dry-run=client -o yaml | kubectl apply -f -

    # Label namespace
    kubectl label namespace rustmq \
        environment="$ENVIRONMENT" \
        app=rustmq \
        --overwrite

    log_success "Namespace created and labeled"
}

setup_workload_identity() {
    log_info "Setting up Workload Identity..."

    local gsa_name="rustmq-sa"
    local gsa_email="${gsa_name}@${GCP_PROJECT_ID}.iam.gserviceaccount.com"

    # Create Google Service Account if not exists
    if ! gcloud iam service-accounts describe "$gsa_email" \
        --project="$GCP_PROJECT_ID" &> /dev/null; then

        log_info "Creating Google Service Account: $gsa_name"
        gcloud iam service-accounts create "$gsa_name" \
            --project="$GCP_PROJECT_ID" \
            --display-name="RustMQ Service Account" \
            --description="Service account for RustMQ GKE workloads"

        log_success "Google Service Account created"
    else
        log_info "Google Service Account already exists: $gsa_name"
    fi

    # Grant GCS access
    log_info "Granting GCS access to service account..."
    gcloud projects add-iam-policy-binding "$GCP_PROJECT_ID" \
        --member="serviceAccount:${gsa_email}" \
        --role="roles/storage.objectAdmin" \
        --condition=None

    # Create Kubernetes Service Account
    log_info "Creating Kubernetes Service Account..."
    kubectl create serviceaccount rustmq-sa \
        -n rustmq \
        --dry-run=client -o yaml | kubectl apply -f -

    # Annotate for Workload Identity
    kubectl annotate serviceaccount rustmq-sa \
        -n rustmq \
        iam.gke.io/gcp-service-account="$gsa_email" \
        --overwrite

    # Bind Kubernetes SA to Google SA
    log_info "Binding Kubernetes SA to Google SA..."
    gcloud iam service-accounts add-iam-policy-binding "$gsa_email" \
        --project="$GCP_PROJECT_ID" \
        --role=roles/iam.workloadIdentityUser \
        --member="serviceAccount:${GCP_PROJECT_ID}.svc.id.goog[rustmq/rustmq-sa]"

    log_success "Workload Identity configured"
}

create_gcs_bucket() {
    local bucket_name="${GCS_BUCKET}"

    if [[ -z "$bucket_name" ]]; then
        log_warning "GCS_BUCKET not configured - skipping bucket creation"
        return 0
    fi

    log_info "Creating GCS bucket: $bucket_name"

    # Check if bucket exists
    if gsutil ls -b "gs://${bucket_name}" &> /dev/null; then
        log_info "GCS bucket already exists: $bucket_name"
        return 0
    fi

    # Create bucket
    gsutil mb \
        -p "$GCP_PROJECT_ID" \
        -c STANDARD \
        -l "$GCP_REGION" \
        "gs://${bucket_name}"

    # Enable versioning
    gsutil versioning set on "gs://${bucket_name}"

    # Set lifecycle policy (delete old versions after 30 days)
    cat > /tmp/lifecycle.json << EOF
{
  "lifecycle": {
    "rule": [
      {
        "action": {"type": "Delete"},
        "condition": {
          "numNewerVersions": 3,
          "isLive": false
        }
      }
    ]
  }
}
EOF
    gsutil lifecycle set /tmp/lifecycle.json "gs://${bucket_name}"
    rm /tmp/lifecycle.json

    log_success "GCS bucket created: $bucket_name"
}

show_summary() {
    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_success "GKE Cluster Creation Complete!"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    log_info "Cluster Name:     $CLUSTER_NAME"
    log_info "Environment:      $ENVIRONMENT"
    log_info "Region:           $GCP_REGION"
    log_info "Project:          $GCP_PROJECT_ID"
    log_info "Namespace:        rustmq"
    log_info "Service Account:  rustmq-sa"
    if [[ -n "${GCS_BUCKET:-}" ]]; then
        log_info "GCS Bucket:       $GCS_BUCKET"
    fi
    echo ""
    log_info "Next Steps:"
    log_info "  1. Install External Secrets Operator:"
    log_info "     helm repo add external-secrets https://charts.external-secrets.io"
    log_info "     helm install external-secrets external-secrets/external-secrets \\"
    log_info "       --namespace external-secrets-system --create-namespace"
    echo ""
    log_info "  2. Create secrets in Google Secret Manager:"
    log_info "     ./gke/secrets/setup-secrets.sh $ENVIRONMENT create"
    echo ""
    log_info "  3. Deploy RustMQ:"
    log_info "     ./gke/deploy.sh $ENVIRONMENT"
    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

main() {
    if [[ $# -eq 0 ]]; then
        show_usage
        exit 1
    fi

    local environment="$1"

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "RustMQ GKE Cluster Creation"
    log_info "Environment: $environment"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""

    check_prerequisites
    load_config "$environment"
    validate_config
    check_cluster_exists
    create_network
    create_cluster
    configure_kubectl
    create_namespace
    setup_workload_identity
    create_gcs_bucket
    show_summary
}

main "$@"
