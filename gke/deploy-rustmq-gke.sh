#!/bin/bash
set -euo pipefail

# =============================================================================
# RustMQ GKE Deployment Script
# =============================================================================
# Complete deployment automation for RustMQ on Google Kubernetes Engine
# Handles infrastructure setup, cluster creation, and application deployment
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# =============================================================================
# CONFIGURABLE DEPLOYMENT PARAMETERS
# =============================================================================

# Google Cloud Project Configuration
PROJECT_ID="${PROJECT_ID:-rustmq-production}"
SERVICE_ACCOUNT="${SERVICE_ACCOUNT:-rustmq-sa@${PROJECT_ID}.iam.gserviceaccount.com}"
REGION="${REGION:-us-central1}"
ZONE="${ZONE:-us-central1-a}"

# GKE Cluster Configuration
CLUSTER_NAME="${CLUSTER_NAME:-rustmq-cluster}"
NETWORK_NAME="${NETWORK_NAME:-rustmq-network}"
SUBNET_NAME="${SUBNET_NAME:-rustmq-subnet}"

# Controller Configuration (1-3 instances max)
CONTROLLER_COUNT="${CONTROLLER_COUNT:-3}"
CONTROLLER_MACHINE_TYPE="${CONTROLLER_MACHINE_TYPE:-n2-standard-4}"
CONTROLLER_CPU_REQUEST="${CONTROLLER_CPU_REQUEST:-2}"
CONTROLLER_CPU_LIMIT="${CONTROLLER_CPU_LIMIT:-4}"
CONTROLLER_MEMORY_REQUEST="${CONTROLLER_MEMORY_REQUEST:-4Gi}"
CONTROLLER_MEMORY_LIMIT="${CONTROLLER_MEMORY_LIMIT:-8Gi}"
CONTROLLER_DISK_SIZE="${CONTROLLER_DISK_SIZE:-50Gi}"

# Broker Configuration (3+ instances)
BROKER_MIN_COUNT="${BROKER_MIN_COUNT:-3}"
BROKER_MAX_COUNT="${BROKER_MAX_COUNT:-20}"
BROKER_MACHINE_TYPE="${BROKER_MACHINE_TYPE:-n2-standard-8}"
BROKER_CPU_REQUEST="${BROKER_CPU_REQUEST:-4}"
BROKER_CPU_LIMIT="${BROKER_CPU_LIMIT:-8}"
BROKER_MEMORY_REQUEST="${BROKER_MEMORY_REQUEST:-8Gi}"
BROKER_MEMORY_LIMIT="${BROKER_MEMORY_LIMIT:-16Gi}"
BROKER_LOCAL_SSD_COUNT="${BROKER_LOCAL_SSD_COUNT:-1}"

# Storage Configuration
STORAGE_CLASS_SSD="${STORAGE_CLASS_SSD:-pd-ssd}"
GCS_BUCKET="${GCS_BUCKET:-${PROJECT_ID}-rustmq-storage}"
GCS_REGION="${GCS_REGION:-us-central1}"

# Security Configuration
ENABLE_MTLS="${ENABLE_MTLS:-true}"
CERT_DOMAIN="${CERT_DOMAIN:-rustmq.internal}"
STATIC_IP_NAME="${STATIC_IP_NAME:-rustmq-static-ip}"

# Image Configuration
REGISTRY_HOST="${REGISTRY_HOST:-gcr.io}"
IMAGE_TAG="${IMAGE_TAG:-latest}"

# Deployment Configuration
ENVIRONMENT="${ENVIRONMENT:-production}"
DRY_RUN="${DRY_RUN:-false}"
SKIP_INFRASTRUCTURE="${SKIP_INFRASTRUCTURE:-false}"
SKIP_IMAGES="${SKIP_IMAGES:-false}"

# =============================================================================
# UTILITY FUNCTIONS
# =============================================================================

log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" >&2
}

error() {
    log "ERROR: $*" >&2
    exit 1
}

warning() {
    log "WARNING: $*" >&2
}

info() {
    log "INFO: $*"
}

success() {
    log "SUCCESS: $*"
}

# Check if required tools are installed
check_prerequisites() {
    local missing_tools=()
    
    if ! command -v gcloud &> /dev/null; then
        missing_tools+=("gcloud")
    fi
    
    if ! command -v kubectl &> /dev/null; then
        missing_tools+=("kubectl")
    fi
    
    if ! command -v kustomize &> /dev/null; then
        missing_tools+=("kustomize")
    fi
    
    if [ ${#missing_tools[@]} -ne 0 ]; then
        error "Missing required tools: ${missing_tools[*]}"
    fi
    
    # Check gcloud authentication
    if ! gcloud auth list --filter=status:ACTIVE --format="value(account)" | grep -q "@"; then
        error "gcloud is not authenticated. Run 'gcloud auth login'"
    fi
    
    info "Prerequisites check passed"
}

# Validate configuration
validate_config() {
    if [ -z "$PROJECT_ID" ]; then
        error "PROJECT_ID is required"
    fi
    
    if [ "$CONTROLLER_COUNT" -lt 1 ] || [ "$CONTROLLER_COUNT" -gt 3 ]; then
        error "CONTROLLER_COUNT must be between 1 and 3"
    fi
    
    if [ "$BROKER_MIN_COUNT" -lt 1 ]; then
        error "BROKER_MIN_COUNT must be at least 1"
    fi
    
    if [ "$BROKER_MAX_COUNT" -lt "$BROKER_MIN_COUNT" ]; then
        error "BROKER_MAX_COUNT must be >= BROKER_MIN_COUNT"
    fi
    
    info "Configuration validation passed"
}

# Set up Google Cloud project and APIs
setup_infrastructure() {
    if [ "$SKIP_INFRASTRUCTURE" = "true" ]; then
        info "Skipping infrastructure setup"
        return 0
    fi
    
    info "Setting up Google Cloud infrastructure..."
    
    # Set project
    gcloud config set project "$PROJECT_ID"
    
    # Enable required APIs
    info "Enabling required APIs..."
    gcloud services enable container.googleapis.com
    gcloud services enable compute.googleapis.com
    gcloud services enable storage.googleapis.com
    gcloud services enable certificatemanager.googleapis.com
    
    # Create service account if it doesn't exist
    if ! gcloud iam service-accounts describe "$SERVICE_ACCOUNT" &>/dev/null; then
        info "Creating service account: $SERVICE_ACCOUNT"
        gcloud iam service-accounts create "$(echo "$SERVICE_ACCOUNT" | cut -d@ -f1)" \
            --description="RustMQ GKE Service Account" \
            --display-name="RustMQ SA"
    fi
    
    # Grant necessary IAM roles
    info "Setting up IAM permissions..."
    gcloud projects add-iam-policy-binding "$PROJECT_ID" \
        --member="serviceAccount:${SERVICE_ACCOUNT}" \
        --role="roles/container.developer" || true
    
    gcloud projects add-iam-policy-binding "$PROJECT_ID" \
        --member="serviceAccount:${SERVICE_ACCOUNT}" \
        --role="roles/storage.admin" || true
    
    success "Infrastructure setup completed"
}

# Set up VPC network
setup_network() {
    info "Setting up VPC network..."
    
    # Create VPC network if it doesn't exist
    if ! gcloud compute networks describe "$NETWORK_NAME" &>/dev/null; then
        info "Creating VPC network: $NETWORK_NAME"
        gcloud compute networks create "$NETWORK_NAME" \
            --subnet-mode=custom \
            --project="$PROJECT_ID"
    fi
    
    # Create subnet if it doesn't exist
    if ! gcloud compute networks subnets describe "$SUBNET_NAME" --region="$REGION" &>/dev/null; then
        info "Creating subnet: $SUBNET_NAME"
        gcloud compute networks subnets create "$SUBNET_NAME" \
            --network="$NETWORK_NAME" \
            --range=10.0.0.0/16 \
            --secondary-range=pods=10.1.0.0/16,services=10.2.0.0/16 \
            --region="$REGION" \
            --project="$PROJECT_ID"
    fi
    
    # Create firewall rules
    if ! gcloud compute firewall-rules describe rustmq-internal &>/dev/null; then
        info "Creating firewall rules"
        gcloud compute firewall-rules create rustmq-internal \
            --network="$NETWORK_NAME" \
            --allow=tcp:9092-9095,tcp:9642,udp:9092 \
            --source-ranges=10.0.0.0/8 \
            --project="$PROJECT_ID"
    fi
    
    success "Network setup completed"
}

# Set up GCS bucket
setup_storage() {
    info "Setting up GCS bucket: $GCS_BUCKET"
    
    # Create GCS bucket if it doesn't exist
    if ! gsutil ls -b "gs://$GCS_BUCKET" &>/dev/null; then
        info "Creating GCS bucket: $GCS_BUCKET"
        gsutil mb -p "$PROJECT_ID" -c STANDARD -l "$GCS_REGION" "gs://$GCS_BUCKET"
        
        # Set up lifecycle policies
        cat > /tmp/lifecycle.json << EOF
{
  "lifecycle": {
    "rule": [
      {
        "action": {"type": "SetStorageClass", "storageClass": "NEARLINE"},
        "condition": {"age": 30}
      },
      {
        "action": {"type": "SetStorageClass", "storageClass": "COLDLINE"},
        "condition": {"age": 90}
      },
      {
        "action": {"type": "Delete"},
        "condition": {"age": 365}
      }
    ]
  }
}
EOF
        gsutil lifecycle set /tmp/lifecycle.json "gs://$GCS_BUCKET"
        rm /tmp/lifecycle.json
    fi
    
    success "Storage setup completed"
}

# Create GKE cluster
create_cluster() {
    info "Creating GKE cluster: $CLUSTER_NAME"
    
    # Check if cluster already exists
    if gcloud container clusters describe "$CLUSTER_NAME" --region="$REGION" &>/dev/null; then
        info "Cluster $CLUSTER_NAME already exists, skipping creation"
        return 0
    fi
    
    # Create GKE cluster
    info "Creating regional GKE cluster..."
    gcloud container clusters create "$CLUSTER_NAME" \
        --project="$PROJECT_ID" \
        --region="$REGION" \
        --network="$NETWORK_NAME" \
        --subnetwork="$SUBNET_NAME" \
        --cluster-secondary-range-name=pods \
        --services-secondary-range-name=services \
        --enable-cloud-logging \
        --enable-cloud-monitoring \
        --enable-autorepair \
        --enable-autoupgrade \
        --maintenance-window-start="2025-01-01T02:00:00Z" \
        --maintenance-window-end="2025-01-01T06:00:00Z" \
        --maintenance-window-recurrence="FREQ=WEEKLY;BYDAY=SU" \
        --workload-pool="${PROJECT_ID}.svc.id.goog" \
        --enable-shielded-nodes \
        --enable-ip-alias \
        --num-nodes=0 \
        --release-channel=stable
    
    success "GKE cluster created"
}

# Create node pools
create_node_pools() {
    info "Creating node pools..."
    
    # Controller node pool
    if ! gcloud container node-pools describe controller-pool --cluster="$CLUSTER_NAME" --region="$REGION" &>/dev/null; then
        info "Creating controller node pool..."
        gcloud container node-pools create controller-pool \
            --cluster="$CLUSTER_NAME" \
            --project="$PROJECT_ID" \
            --region="$REGION" \
            --machine-type="$CONTROLLER_MACHINE_TYPE" \
            --disk-type=pd-ssd \
            --disk-size=100GB \
            --num-nodes="$CONTROLLER_COUNT" \
            --enable-autorepair \
            --enable-autoupgrade \
            --max-pods-per-node=30 \
            --node-taints=workload=controller:NoSchedule \
            --node-labels=workload=controller,storage=pd-ssd \
            --service-account="$SERVICE_ACCOUNT" \
            --scopes=cloud-platform
    fi
    
    # Broker node pool
    if ! gcloud container node-pools describe broker-pool --cluster="$CLUSTER_NAME" --region="$REGION" &>/dev/null; then
        info "Creating broker node pool..."
        gcloud container node-pools create broker-pool \
            --cluster="$CLUSTER_NAME" \
            --project="$PROJECT_ID" \
            --region="$REGION" \
            --machine-type="$BROKER_MACHINE_TYPE" \
            --local-ssd-count="$BROKER_LOCAL_SSD_COUNT" \
            --disk-type=pd-ssd \
            --disk-size=50GB \
            --min-nodes="$BROKER_MIN_COUNT" \
            --max-nodes="$BROKER_MAX_COUNT" \
            --enable-autoscaling \
            --enable-autorepair \
            --enable-autoupgrade \
            --max-pods-per-node=50 \
            --node-taints=workload=broker:NoSchedule \
            --node-labels=workload=broker,storage=local-ssd \
            --service-account="$SERVICE_ACCOUNT" \
            --scopes=cloud-platform
    fi
    
    success "Node pools created"
}

# Get cluster credentials
get_credentials() {
    info "Getting cluster credentials..."
    gcloud container clusters get-credentials "$CLUSTER_NAME" \
        --region="$REGION" \
        --project="$PROJECT_ID"
    
    success "Cluster credentials configured"
}

# Check if images exist
check_images() {
    if [ "$SKIP_IMAGES" = "true" ]; then
        info "Skipping image validation"
        return 0
    fi
    
    info "Checking if required images exist..."
    
    local images=(
        "${REGISTRY_HOST}/${PROJECT_ID}/rustmq-controller:${IMAGE_TAG}"
        "${REGISTRY_HOST}/${PROJECT_ID}/rustmq-broker:${IMAGE_TAG}"
    )
    
    for image in "${images[@]}"; do
        if [ "$REGISTRY_HOST" = "gcr.io" ]; then
            if ! gcloud container images describe "$image" &>/dev/null; then
                error "Image not found: $image. Please build and push images first using docker/build-and-push.sh"
            fi
        else
            info "Skipping image validation for custom registry: $REGISTRY_HOST"
        fi
    done
    
    success "Required images are available"
}

# Deploy Kubernetes resources
deploy_kubernetes() {
    info "Deploying Kubernetes resources..."
    
    # Create temporary environment file for variable substitution
    local env_file="/tmp/rustmq-env-${ENVIRONMENT}.env"
    cat > "$env_file" << EOF
PROJECT_ID=$PROJECT_ID
SERVICE_ACCOUNT=$SERVICE_ACCOUNT
REGION=$REGION
ZONE=$ZONE
CLUSTER_NAME=$CLUSTER_NAME
NETWORK_NAME=$NETWORK_NAME
SUBNET_NAME=$SUBNET_NAME
CONTROLLER_COUNT=$CONTROLLER_COUNT
CONTROLLER_CPU_REQUEST=$CONTROLLER_CPU_REQUEST
CONTROLLER_CPU_LIMIT=$CONTROLLER_CPU_LIMIT
CONTROLLER_MEMORY_REQUEST=$CONTROLLER_MEMORY_REQUEST
CONTROLLER_MEMORY_LIMIT=$CONTROLLER_MEMORY_LIMIT
CONTROLLER_DISK_SIZE=$CONTROLLER_DISK_SIZE
BROKER_MIN_COUNT=$BROKER_MIN_COUNT
BROKER_MAX_COUNT=$BROKER_MAX_COUNT
BROKER_CPU_REQUEST=$BROKER_CPU_REQUEST
BROKER_CPU_LIMIT=$BROKER_CPU_LIMIT
BROKER_MEMORY_REQUEST=$BROKER_MEMORY_REQUEST
BROKER_MEMORY_LIMIT=$BROKER_MEMORY_LIMIT
GCS_BUCKET=$GCS_BUCKET
GCS_REGION=$GCS_REGION
CERT_DOMAIN=$CERT_DOMAIN
STATIC_IP_NAME=$STATIC_IP_NAME
REGISTRY_HOST=$REGISTRY_HOST
IMAGE_TAG=$IMAGE_TAG
EOF
    
    # Apply environment-specific overlay
    local overlay_path="${SCRIPT_DIR}/overlays/${ENVIRONMENT}"
    if [ ! -d "$overlay_path" ]; then
        error "Environment overlay not found: $overlay_path"
    fi
    
    info "Applying $ENVIRONMENT environment configuration..."
    
    if [ "$DRY_RUN" = "true" ]; then
        info "DRY RUN: Would apply the following resources:"
        kustomize build "$overlay_path" | envsubst < "$env_file"
    else
        # Apply with environment variable substitution
        kustomize build "$overlay_path" | envsubst < "$env_file" | kubectl apply -f -
    fi
    
    # Clean up temporary file
    rm "$env_file"
    
    success "Kubernetes resources deployed"
}

# Wait for deployment to be ready
wait_for_deployment() {
    if [ "$DRY_RUN" = "true" ]; then
        info "DRY RUN: Skipping deployment wait"
        return 0
    fi
    
    info "Waiting for deployment to be ready..."
    
    # Wait for controllers
    info "Waiting for controllers to be ready..."
    kubectl wait --for=condition=ready pod -l app=rustmq-controller -n rustmq --timeout=600s
    
    # Wait for brokers
    info "Waiting for brokers to be ready..."
    kubectl wait --for=condition=ready pod -l app=rustmq-broker -n rustmq --timeout=600s
    
    success "Deployment is ready"
}

# Display deployment information
display_info() {
    info "Deployment completed successfully!"
    echo ""
    echo "=============================================================================="
    echo "RustMQ GKE Deployment Information"
    echo "==============================================================================="
    echo ""
    echo "Project ID: $PROJECT_ID"
    echo "Cluster: $CLUSTER_NAME"
    echo "Region: $REGION"
    echo "Environment: $ENVIRONMENT"
    echo ""
    echo "Controllers: $CONTROLLER_COUNT instances"
    echo "Brokers: $BROKER_MIN_COUNT-$BROKER_MAX_COUNT instances"
    echo ""
    echo "Storage:"
    echo "  - GCS Bucket: gs://$GCS_BUCKET"
    echo "  - Controller Storage: pd-ssd"
    echo "  - Broker Storage: Local SSD + GCS"
    echo ""
    echo "Networking:"
    echo "  - VPC: $NETWORK_NAME"
    echo "  - Subnet: $SUBNET_NAME"
    echo "  - Domain: $CERT_DOMAIN"
    echo ""
    echo "Next Steps:"
    echo "  1. Check deployment status: kubectl get all -n rustmq"
    echo "  2. View controller logs: kubectl logs -f statefulset/rustmq-controller -n rustmq"
    echo "  3. View broker logs: kubectl logs -f daemonset/rustmq-broker -n rustmq"
    echo "  4. Port forward admin API: kubectl port-forward svc/controller-admin 9642:9642 -n rustmq"
    echo "  5. Test the cluster: curl http://localhost:9642/health"
    echo ""
    echo "==============================================================================="
}

# Clean up resources
cleanup() {
    if [ "$DRY_RUN" = "true" ]; then
        info "DRY RUN: Would clean up resources"
        return 0
    fi
    
    warning "This will delete all RustMQ resources in GKE cluster $CLUSTER_NAME"
    read -p "Are you sure? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        info "Cleanup cancelled"
        return 0
    fi
    
    info "Cleaning up Kubernetes resources..."
    kubectl delete namespace rustmq --ignore-not-found=true
    
    info "Cleanup completed"
}

# Display usage information
usage() {
    cat << EOF
RustMQ GKE Deployment Script

USAGE:
    $0 <command> [options]

COMMANDS:
    deploy              Deploy RustMQ to GKE (full deployment)
    deploy-k8s          Deploy only Kubernetes resources (skip infrastructure)
    cleanup             Delete all RustMQ resources from GKE
    status              Show deployment status
    help                Show this help message

OPTIONS:
    --dry-run           Show what would be deployed without applying
    --environment ENV   Deployment environment (production|development)
    --skip-infra        Skip infrastructure setup
    --skip-images       Skip image validation

ENVIRONMENT VARIABLES:
    PROJECT_ID          Google Cloud Project ID
    CLUSTER_NAME        GKE cluster name
    REGION              GCP region
    ENVIRONMENT         Deployment environment (production|development)
    IMAGE_TAG           Docker image tag to deploy

EXAMPLES:
    # Full production deployment
    $0 deploy --environment production

    # Development deployment
    $0 deploy --environment development

    # Deploy only Kubernetes resources
    $0 deploy-k8s --environment production

    # Dry run to see what would be deployed
    $0 deploy --dry-run --environment production

    # Clean up all resources
    $0 cleanup

EOF
}

# Show deployment status
show_status() {
    info "RustMQ GKE Deployment Status"
    echo ""
    
    # Check cluster existence
    if gcloud container clusters describe "$CLUSTER_NAME" --region="$REGION" &>/dev/null; then
        success "✓ Cluster $CLUSTER_NAME exists"
        
        # Get credentials if needed
        get_credentials
        
        # Check namespace
        if kubectl get namespace rustmq &>/dev/null; then
            success "✓ Namespace rustmq exists"
            
            # Check deployments
            echo ""
            echo "Pod Status:"
            kubectl get pods -n rustmq -o wide
            
            echo ""
            echo "Service Status:"
            kubectl get services -n rustmq
            
            echo ""
            echo "Storage Status:"
            kubectl get pvc -n rustmq
            
        else
            warning "✗ Namespace rustmq not found"
        fi
    else
        warning "✗ Cluster $CLUSTER_NAME not found"
    fi
}

# Main execution function
main() {
    local command="${1:-help}"
    shift || true
    
    # Parse options
    while [[ $# -gt 0 ]]; do
        case $1 in
            --dry-run)
                DRY_RUN="true"
                shift
                ;;
            --environment)
                ENVIRONMENT="$2"
                shift 2
                ;;
            --skip-infra)
                SKIP_INFRASTRUCTURE="true"
                shift
                ;;
            --skip-images)
                SKIP_IMAGES="true"
                shift
                ;;
            *)
                warning "Unknown option: $1"
                shift
                ;;
        esac
    done
    
    case "$command" in
        "deploy")
            check_prerequisites
            validate_config
            setup_infrastructure
            setup_network
            setup_storage
            create_cluster
            create_node_pools
            get_credentials
            check_images
            deploy_kubernetes
            wait_for_deployment
            display_info
            ;;
        "deploy-k8s")
            check_prerequisites
            validate_config
            get_credentials
            check_images
            deploy_kubernetes
            wait_for_deployment
            display_info
            ;;
        "cleanup")
            check_prerequisites
            get_credentials
            cleanup
            ;;
        "status")
            check_prerequisites
            show_status
            ;;
        "help"|"-h"|"--help")
            usage
            ;;
        *)
            error "Unknown command: $command. Run '$0 help' for usage information."
            ;;
    esac
}

# Script entry point
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi