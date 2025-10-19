#!/usr/bin/env bash
#
# RustMQ GKE Deployment Orchestration
# Phase 3: GKE Infrastructure Optimization (October 2025)
#
# Master deployment script that orchestrates the entire RustMQ deployment.
#
# Usage:
#   ./deploy.sh <environment> [--skip-cluster] [--skip-secrets]
#
# Examples:
#   ./deploy.sh dev
#   ./deploy.sh prod --skip-cluster
#   ./deploy.sh staging --skip-secrets

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}\")\" && pwd)"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SKIP_CLUSTER=false
SKIP_SECRETS=false

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[✓]${NC} $*"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_step() { echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; echo -e "${BLUE}[STEP]${NC} $*"; echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"; }

show_usage() {
    cat << EOF
Usage: ./deploy.sh <environment> [OPTIONS]

Arguments:
  environment     Environment to deploy (dev, staging, prod)

Options:
  --skip-cluster  Skip cluster creation (cluster already exists)
  --skip-secrets  Skip secrets setup (secrets already configured)
  --help, -h      Show this help

Examples:
  ./deploy.sh dev
  ./deploy.sh prod --skip-cluster
  ./deploy.sh staging --skip-secrets

Deployment Steps:
  1. Pre-flight checks (prerequisites, configuration validation)
  2. Cluster creation (create GKE cluster)
  3. Secrets setup (External Secrets Operator + Google Secret Manager)
  4. Storage configuration (StorageClass, PVCs)
  5. RustMQ deployment (Kustomize manifests)
  6. Network configuration (Ingress, load balancers)
  7. Post-deployment verification

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
            --skip-cluster) SKIP_CLUSTER=true; shift ;;
            --skip-secrets) SKIP_SECRETS=true; shift ;;
            --help|-h) show_usage; exit 0 ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
    done
}

check_prerequisites() {
    log_step "Step 1: Pre-flight Checks"

    log_info "Checking prerequisites..."

    # Check required tools
    local tools=("gcloud" "kubectl" "kustomize" "helm")
    for tool in "${tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            log_error "$tool not found. Please install $tool."
            exit 1
        fi
        log_success "$tool found"
    done

    # Check gcloud authentication
    if ! gcloud auth list --filter=status:ACTIVE --format="value(account)" &> /dev/null; then
        log_error "Not authenticated with gcloud. Run: gcloud auth login"
        exit 1
    fi
    log_success "gcloud authentication verified"

    # Check kustomize version
    local kustomize_version=$(kustomize version --short 2>/dev/null || echo "unknown")
    log_info "kustomize version: $kustomize_version"

    log_success "Pre-flight checks passed"
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

validate_config() {
    log_info "Validating configuration..."

    # Run validation script from Phase 2
    if ! "${SCRIPT_DIR}/validate-config.sh" "$ENVIRONMENT"; then
        log_error "Configuration validation failed"
        exit 1
    fi

    log_success "Configuration validation passed"
}

create_cluster() {
    if [[ "$SKIP_CLUSTER" == "true" ]]; then
        log_warning "Skipping cluster creation (--skip-cluster)"
        return 0
    fi

    log_step "Step 2: Cluster Creation"

    log_info "Creating GKE cluster..."

    if ! "${SCRIPT_DIR}/create-cluster.sh" "$ENVIRONMENT"; then
        log_error "Cluster creation failed"
        exit 1
    fi

    log_success "Cluster created successfully"
}

setup_secrets() {
    if [[ "$SKIP_SECRETS" == "true" ]]; then
        log_warning "Skipping secrets setup (--skip-secrets)"
        return 0
    fi

    log_step "Step 3: Secrets Setup"

    log_info "Setting up External Secrets Operator..."

    # Check if External Secrets Operator is already installed
    if ! kubectl get namespace external-secrets-system &> /dev/null; then
        log_info "Installing External Secrets Operator..."

        helm repo add external-secrets https://charts.external-secrets.io || true
        helm repo update

        helm install external-secrets \
            external-secrets/external-secrets \
            --namespace external-secrets-system \
            --create-namespace \
            --set installCRDs=true \
            --wait

        log_success "External Secrets Operator installed"
    else
        log_info "External Secrets Operator already installed"
    fi

    # Create secrets in Google Secret Manager
    log_info "Creating secrets in Google Secret Manager..."

    if ! "${SCRIPT_DIR}/secrets/setup-secrets.sh" "$ENVIRONMENT" create; then
        log_warning "Some secrets may already exist"
    fi

    # Grant Workload Identity access
    log_info "Granting Workload Identity access..."
    "${SCRIPT_DIR}/secrets/setup-secrets.sh" "$ENVIRONMENT" grant-access

    log_success "Secrets setup complete"
}

deploy_storage() {
    log_step "Step 4: Storage Configuration"

    log_info "Applying StorageClass manifests..."

    kubectl apply -f "${SCRIPT_DIR}/storage/storageclass.yaml"

    log_success "Storage configuration applied"
}

deploy_rustmq() {
    log_step "Step 5: RustMQ Deployment"

    local overlay_path="${SCRIPT_DIR}/manifests/overlays/${ENVIRONMENT}"

    if [[ ! -d "$overlay_path" ]]; then
        log_error "Overlay not found for environment: $ENVIRONMENT"
        log_error "Expected path: $overlay_path"
        exit 1
    fi

    log_info "Deploying RustMQ from overlay: $overlay_path"

    # Replace PROJECT_ID in all manifests
    log_info "Configuring manifests for project: $GCP_PROJECT_ID"

    # Use kustomize build and sed to replace PROJECT_ID, then apply
    kustomize build "$overlay_path" | \
        sed "s/PROJECT_ID/${GCP_PROJECT_ID}/g" | \
        kubectl apply -f -

    log_success "RustMQ manifests applied"

    # Apply secrets configuration
    log_info "Applying secrets manifests..."

    kubectl apply -f "${SCRIPT_DIR}/secrets/external-secrets/secret-store.yaml" | \
        sed "s/PROJECT_ID/${GCP_PROJECT_ID}/g" | \
        sed "s/\${GCP_PROJECT_ID}/${GCP_PROJECT_ID}/g" | \
        sed "s/\${GCP_REGION}/${GCP_REGION}/g" | \
        sed "s/\${CLUSTER_NAME}/${CLUSTER_NAME}/g" | \
        kubectl apply -f -

    kubectl apply -f "${SCRIPT_DIR}/secrets/external-secrets/external-secrets.yaml" | \
        sed "s/\${ENVIRONMENT}/${ENVIRONMENT}/g" | \
        sed "s/\${TLS_CERT_SECRET_NAME}/rustmq-${ENVIRONMENT}-tls-cert/g" | \
        sed "s/\${TLS_KEY_SECRET_NAME}/rustmq-${ENVIRONMENT}-tls-key/g" | \
        sed "s/\${TLS_CA_SECRET_NAME}/rustmq-${ENVIRONMENT}-ca-cert/g" | \
        sed "s/\${GCS_CREDENTIALS_SECRET_NAME}/rustmq-${ENVIRONMENT}-gcs-credentials/g" | \
        kubectl apply -f -

    log_success "Secrets manifests applied"

    # Wait for secrets to sync
    log_info "Waiting for secrets to sync (up to 2 minutes)..."
    local timeout=120
    local elapsed=0
    while [[ $elapsed -lt $timeout ]]; do
        local ready=$(kubectl get externalsecrets -n rustmq -o json | \
            jq -r '.items[] | select(.status.conditions[]? | select(.type=="Ready" and .status=="True")) | .metadata.name' | \
            wc -l)

        if [[ $ready -ge 2 ]]; then
            log_success "Secrets synced successfully"
            break
        fi

        sleep 5
        elapsed=$((elapsed + 5))
        echo -n "."
    done
    echo ""

    if [[ $elapsed -ge $timeout ]]; then
        log_warning "Secrets sync timeout - continuing anyway"
    fi
}

deploy_network() {
    log_step "Step 6: Network Configuration"

    log_info "Checking if ingress should be deployed..."

    # Only deploy ingress for staging and prod
    if [[ "$ENVIRONMENT" == "prod" ]] || [[ "$ENVIRONMENT" == "staging" ]]; then
        log_info "Deploying Ingress for $ENVIRONMENT..."

        # Note: User needs to configure domain name first
        log_warning "Remember to:"
        log_warning "  1. Reserve static IP: gcloud compute addresses create rustmq-admin-ip --global"
        log_warning "  2. Configure DNS to point to the IP address"
        log_warning "  3. Update ingress.yaml with your domain name"

        # Uncomment to deploy ingress (after configuring domain)
        # kubectl apply -f "${SCRIPT_DIR}/network/ingress.yaml"
    else
        log_info "Skipping Ingress deployment for $ENVIRONMENT (dev environment)"
    fi

    log_success "Network configuration complete"
}

wait_for_deployment() {
    log_step "Step 7: Post-Deployment Verification"

    log_info "Waiting for controller StatefulSet to be ready..."
    kubectl wait --for=condition=ready pod \
        -l app.kubernetes.io/component=controller \
        -n rustmq \
        --timeout=300s || log_warning "Controller pods not ready yet"

    log_info "Waiting for broker Deployment to be ready..."
    kubectl wait --for=condition=ready pod \
        -l app.kubernetes.io/component=broker \
        -n rustmq \
        --timeout=300s || log_warning "Broker pods not ready yet"

    log_info "Waiting for admin-server Deployment to be ready..."
    kubectl wait --for=condition=ready pod \
        -l app.kubernetes.io/component=admin-server \
        -n rustmq \
        --timeout=180s || log_warning "Admin server pods not ready yet"
}

verify_deployment() {
    log_info "Verifying deployment..."

    # Check pod status
    log_info "Pod status:"
    kubectl get pods -n rustmq

    # Check services
    log_info "Service endpoints:"
    kubectl get svc -n rustmq

    # Check ExternalSecrets
    log_info "External secrets status:"
    kubectl get externalsecrets -n rustmq

    # Check HPA
    log_info "HPA status:"
    kubectl get hpa -n rustmq

    # Run drift detection
    log_info "Running drift detection..."
    if "${SCRIPT_DIR}/drift-detect.sh" "$ENVIRONMENT"; then
        log_success "No configuration drift detected"
    else
        log_warning "Configuration drift detected - review and fix"
    fi
}

show_summary() {
    local cluster_location="$GCP_REGION"
    if [[ "$ENVIRONMENT" == "dev" ]]; then
        cluster_location="${GCP_REGION}-a"
    fi

    echo ""
    log_step "Deployment Complete!"

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_success "RustMQ Deployed Successfully!"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    log_info "Cluster:          $CLUSTER_NAME"
    log_info "Environment:      $ENVIRONMENT"
    log_info "Region:           $cluster_location"
    log_info "Project:          $GCP_PROJECT_ID"
    log_info "Namespace:        rustmq"
    echo ""

    log_info "Access Information:"
    echo ""

    # Get load balancer IPs
    local broker_ip=$(kubectl get svc rustmq-broker -n rustmq -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null || echo "pending")
    local controller_ip=$(kubectl get svc rustmq-controller-lb -n rustmq -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null || echo "pending")

    log_info "  Broker (QUIC):        $broker_ip:9092"
    log_info "  Controller (gRPC):    $controller_ip:9094"
    log_info "  Controller (Admin):   $controller_ip:9095"
    echo ""

    log_info "Useful Commands:"
    echo ""
    log_info "  # View pod status"
    log_info "  kubectl get pods -n rustmq"
    echo ""
    log_info "  # View logs"
    log_info "  kubectl logs -f rustmq-controller-0 -n rustmq"
    log_info "  kubectl logs -f deployment/rustmq-broker -n rustmq"
    echo ""
    log_info "  # Port forward to admin API"
    log_info "  kubectl port-forward svc/rustmq-admin-server 8080:8080 -n rustmq"
    echo ""
    log_info "  # Check drift"
    log_info "  ./gke/drift-detect.sh $ENVIRONMENT"
    echo ""
    log_info "  # Scale brokers"
    log_info "  kubectl scale deployment rustmq-broker -n rustmq --replicas=10"
    echo ""

    log_info "Next Steps:"
    echo ""
    log_info "  1. Test broker connectivity:"
    log_info "     # From within VPC or via kubectl port-forward"
    echo ""
    log_info "  2. Monitor pod health:"
    log_info "     kubectl get pods -n rustmq -w"
    echo ""
    log_info "  3. Check metrics (if monitoring enabled):"
    log_info "     # Access Grafana dashboards"
    echo ""
    log_info "  4. Review logs for any errors:"
    log_info "     kubectl logs -l app.kubernetes.io/name=rustmq -n rustmq --tail=100"
    echo ""

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

main() {
    parse_args "$@"

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "RustMQ GKE Deployment Orchestration"
    log_info "Environment: $ENVIRONMENT"
    if [[ "$SKIP_CLUSTER" == "true" ]]; then
        log_warning "Skip cluster creation: ENABLED"
    fi
    if [[ "$SKIP_SECRETS" == "true" ]]; then
        log_warning "Skip secrets setup: ENABLED"
    fi
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""

    check_prerequisites
    load_config
    validate_config
    create_cluster
    setup_secrets
    deploy_storage
    deploy_rustmq
    deploy_network
    wait_for_deployment
    verify_deployment
    show_summary
}

main "$@"
