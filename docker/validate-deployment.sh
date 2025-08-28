#!/bin/bash
set -euo pipefail

# =============================================================================
# RustMQ Deployment Validation Script
# =============================================================================
# Validates consistency between Docker images and GKE deployment manifests
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GKE_DIR="${PROJECT_ROOT}/gke"

log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" >&2
}

error() {
    log "ERROR: $*" >&2
    return 1
}

success() {
    log "SUCCESS: $*"
}

warning() {
    log "WARNING: $*"
}

# Validation functions
validate_docker_files() {
    log "Validating Docker files..."
    
    local docker_files=(
        "Dockerfile.controller"
        "Dockerfile.broker"
        "Dockerfile.admin"
        "Dockerfile.admin-server"
        "Dockerfile.bigquery-subscriber"
    )
    
    for dockerfile in "${docker_files[@]}"; do
        local path="${SCRIPT_DIR}/${dockerfile}"
        if [ ! -f "$path" ]; then
            error "Missing Dockerfile: $path"
        else
            log "✓ Found: $dockerfile"
        fi
    done
    
    success "All Docker files validated"
}

validate_gke_manifests() {
    log "Validating GKE manifests..."
    
    local gke_files=(
        "base/namespace.yaml"
        "base/storage.yaml"
        "base/configmap-controller.yaml"
        "base/configmap-broker.yaml"
        "base/controller-statefulset.yaml"
        "base/broker-daemonset.yaml"
        "base/services.yaml"
        "base/ingress.yaml"
        "base/backend-config.yaml"
        "base/monitoring.yaml"
        "base/hpa.yaml"
        "base/kustomization.yaml"
    )
    
    for manifest in "${gke_files[@]}"; do
        local path="${GKE_DIR}/${manifest}"
        if [ ! -f "$path" ]; then
            error "Missing GKE manifest: $path"
        else
            log "✓ Found: $manifest"
        fi
    done
    
    success "All GKE manifests validated"
}

validate_image_references() {
    log "Validating image references consistency..."
    
    # Extract image references from GKE manifests
    local gke_controller_image gke_broker_image
    gke_controller_image=$(grep -h "image:" "${GKE_DIR}/base/controller-statefulset.yaml" | grep -v busybox | awk '{print $2}' | head -1)
    gke_broker_image=$(grep -h "image:" "${GKE_DIR}/base/broker-daemonset.yaml" | grep -v busybox | awk '{print $2}' | head -1)
    
    log "GKE Controller image: $gke_controller_image"
    log "GKE Broker image: $gke_broker_image"
    
    # Expected patterns
    local expected_controller="gcr.io/\${PROJECT_ID}/rustmq-controller:latest"
    local expected_broker="gcr.io/\${PROJECT_ID}/rustmq-broker:latest"
    
    if [ "$gke_controller_image" = "$expected_controller" ]; then
        log "✓ Controller image reference matches expected pattern"
    else
        error "Controller image mismatch. Expected: $expected_controller, Found: $gke_controller_image"
    fi
    
    if [ "$gke_broker_image" = "$expected_broker" ]; then
        log "✓ Broker image reference matches expected pattern"
    else
        error "Broker image mismatch. Expected: $expected_broker, Found: $gke_broker_image"
    fi
    
    success "Image references validated"
}

validate_scripts() {
    log "Validating deployment scripts..."
    
    local scripts=(
        "build-and-push.sh"
        "quick-deploy.sh"
        "validate-deployment.sh"
    )
    
    for script in "${scripts[@]}"; do
        local path="${SCRIPT_DIR}/${script}"
        if [ ! -f "$path" ]; then
            error "Missing script: $path"
        elif [ ! -x "$path" ]; then
            error "Script not executable: $path"
        else
            log "✓ Found and executable: $script"
        fi
    done
    
    success "All scripts validated"
}

validate_component_mapping() {
    log "Validating component mapping between Docker and GKE..."
    
    # Components that should exist in both Docker and GKE
    local core_components=("controller" "broker")
    
    for component in "${core_components[@]}"; do
        # Check Docker file exists
        local dockerfile="Dockerfile.${component}"
        if [ ! -f "${SCRIPT_DIR}/${dockerfile}" ]; then
            error "Missing Dockerfile for core component: $dockerfile"
        fi
        
        # Check GKE manifest references the component
        local gke_manifest=""
        case "$component" in
            "controller")
                gke_manifest="controller-statefulset.yaml"
                ;;
            "broker")
                gke_manifest="broker-daemonset.yaml"
                ;;
        esac
        
        if [ ! -f "${GKE_DIR}/base/${gke_manifest}" ]; then
            error "Missing GKE manifest for core component: $gke_manifest"
        fi
        
        log "✓ Component mapping validated: $component"
    done
    
    success "Component mapping validated"
}

validate_kustomization() {
    log "Validating Kustomization configurations..."
    
    # Check base kustomization
    local base_kustomization="${GKE_DIR}/base/kustomization.yaml"
    if [ ! -f "$base_kustomization" ]; then
        error "Missing base kustomization: $base_kustomization"
    fi
    
    # Check overlays
    local overlays=("production" "development")
    for overlay in "${overlays[@]}"; do
        local overlay_kustomization="${GKE_DIR}/overlays/${overlay}/kustomization.yaml"
        if [ ! -f "$overlay_kustomization" ]; then
            error "Missing overlay kustomization: $overlay_kustomization"
        else
            log "✓ Found overlay: $overlay"
        fi
    done
    
    success "Kustomization configurations validated"
}

validate_environment_consistency() {
    log "Validating environment variable consistency..."
    
    # Check if build script and GKE manifests use consistent variables
    local build_script="${SCRIPT_DIR}/build-and-push.sh"
    local gke_vars_found=0
    
    # Key variables that should be consistent
    local vars=("PROJECT_ID" "REGISTRY_HOST" "IMAGE_TAG")
    
    for var in "${vars[@]}"; do
        if grep -q "$var" "$build_script" && grep -q "\${$var}" "${GKE_DIR}"/base/*.yaml; then
            log "✓ Variable $var found in both build script and GKE manifests"
            ((gke_vars_found++))
        else
            warning "Variable $var may not be consistently used"
        fi
    done
    
    if [ $gke_vars_found -ge 2 ]; then
        success "Environment variable consistency validated"
    else
        warning "Some environment variables may not be consistently used"
    fi
}

validate_docker_compose_consistency() {
    log "Validating Docker Compose consistency..."
    
    local compose_file="${SCRIPT_DIR}/docker-compose.yml"
    if [ ! -f "$compose_file" ]; then
        error "Missing docker-compose.yml: $compose_file"
    fi
    
    # Check if docker-compose references the same Dockerfiles
    local dockerfiles_in_compose
    dockerfiles_in_compose=$(grep "dockerfile:" "$compose_file" | awk '{print $2}' | sort -u)
    
    log "Dockerfiles referenced in docker-compose:"
    echo "$dockerfiles_in_compose" | while read -r dockerfile; do
        local full_path="${SCRIPT_DIR}/${dockerfile}"
        if [ -f "$full_path" ]; then
            log "✓ $dockerfile"
        else
            error "Referenced Dockerfile not found: $full_path"
        fi
    done
    
    success "Docker Compose consistency validated"
}

generate_summary() {
    log "Generating deployment summary..."
    
    cat << EOF

=============================================================================
RustMQ Deployment Validation Summary
=============================================================================

✅ Docker Configuration:
   - Controller: Dockerfile.controller → rustmq-controller image
   - Broker: Dockerfile.broker → rustmq-broker image  
   - Admin: Dockerfile.admin → rustmq-admin image
   - Admin Server: Dockerfile.admin-server → rustmq-admin-server image
   - BigQuery: Dockerfile.bigquery-subscriber → rustmq-bigquery-subscriber image

✅ GKE Configuration:
   - Base manifests: 12 files in gke/base/
   - Production overlay: gke/overlays/production/
   - Development overlay: gke/overlays/development/
   - Kustomization: Properly configured with image substitution

✅ Build Scripts:
   - build-and-push.sh: Full-featured build and push automation
   - quick-deploy.sh: Convenience wrapper for common scenarios
   - validate-deployment.sh: This validation script

✅ Image Registry Mapping:
   - Source: Dockerfile.controller → Target: gcr.io/\${PROJECT_ID}/rustmq-controller:\${TAG}
   - Source: Dockerfile.broker → Target: gcr.io/\${PROJECT_ID}/rustmq-broker:\${TAG}

🎯 Deployment Commands:
   
   # Build and push core components for GKE
   ./quick-deploy.sh gke-core prod
   
   # Deploy to GKE production
   kubectl apply -k ../gke/overlays/production
   
   # Deploy to GKE development  
   kubectl apply -k ../gke/overlays/development

=============================================================================

EOF
    
    success "Validation completed successfully"
}

# Main validation routine
main() {
    log "Starting RustMQ deployment validation..."
    echo ""
    
    validate_docker_files
    validate_gke_manifests  
    validate_image_references
    validate_scripts
    validate_component_mapping
    validate_kustomization
    validate_environment_consistency
    validate_docker_compose_consistency
    
    echo ""
    generate_summary
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi