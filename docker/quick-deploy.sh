#!/bin/bash
set -euo pipefail

# =============================================================================
# RustMQ Image Build Script
# =============================================================================
# Convenience wrapper for common RustMQ Docker image operations
# Focused on building and pushing container images
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_SCRIPT="${SCRIPT_DIR}/build-and-push.sh"

# Default configurations for different environments
declare -A ENV_CONFIGS=(
    ["dev"]="PROJECT_ID=rustmq-dev IMAGE_TAG=dev"
    ["staging"]="PROJECT_ID=rustmq-staging IMAGE_TAG=staging"
    ["prod"]="PROJECT_ID=rustmq-production IMAGE_TAG=v1.0.0"
)

log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" >&2
}

error() {
    log "ERROR: $*" >&2
    exit 1
}

success() {
    log "SUCCESS: $*"
}

# Check if build script exists
if [ ! -x "$BUILD_SCRIPT" ]; then
    error "Build script not found or not executable: $BUILD_SCRIPT"
fi

usage() {
    cat << EOF
RustMQ Image Build Script

USAGE:
    $0 <scenario> [environment] [components...]

SCENARIOS:
    build-core          Build core components (controller, broker) for GKE
    build-all           Build all available components
    push-core           Push core components to registry
    push-all            Push all components to registry
    dev-build           Build images for local development (no push)
    staging-images      Build and push for staging environment
    production-images   Build and push for production environment
    hotfix-images       Quick build and push with git commit tag
    cleanup             Clean all local images and prune system
    status              Show current images and configuration

ENVIRONMENTS:
    dev                 Development environment (rustmq-dev, tag: dev)
    staging             Staging environment (rustmq-staging, tag: staging)  
    prod                Production environment (rustmq-production, tag: v1.0.0)

EXAMPLES:
    # Build core components for GKE deployment
    $0 build-core prod

    # Build all components for development
    $0 build-all dev

    # Push core components to production registry
    $0 push-core prod

    # Build and push for staging
    $0 staging-images

    # Emergency hotfix build
    $0 hotfix-images prod controller

    # Check current status
    $0 status

NOTE: For GKE deployment, use the scripts in ../gke/ folder after building images.

EOF
}

# Apply environment configuration
apply_env_config() {
    local env=${1:-prod}
    
    if [[ ${ENV_CONFIGS[$env]+_} ]]; then
        log "Applying environment configuration: $env"
        eval "export ${ENV_CONFIGS[$env]}"
        log "PROJECT_ID=$PROJECT_ID, IMAGE_TAG=$IMAGE_TAG"
    else
        error "Unknown environment: $env. Available: ${!ENV_CONFIGS[*]}"
    fi
}

# Execute build script with environment
execute_build() {
    local command=$1
    shift
    local components=("$@")
    
    log "Executing: $BUILD_SCRIPT $command ${components[*]}"
    "$BUILD_SCRIPT" "$command" "${components[@]}"
}

# Scenario: Build core components for GKE
scenario_build_core() {
    local env=${1:-prod}
    apply_env_config "$env"
    
    log "Building core components for GKE deployment in $env environment..."
    execute_build "all" "controller" "broker"
    
    success "Core components built and pushed for $env environment"
    log "Next steps:"
    log "  1. Deploy to GKE: cd ../gke && ./deploy-rustmq-gke.sh deploy --environment $env"
    log "  2. Or use Kustomize: kubectl apply -k ../gke/overlays/$env"
}

# Scenario: Build all components
scenario_build_all() {
    local env=${1:-prod}
    apply_env_config "$env"
    
    log "Building all components for $env environment..."
    execute_build "all"
    
    success "All components built and pushed for $env environment"
}

# Scenario: Push core components only
scenario_push_core() {
    local env=${1:-prod}
    apply_env_config "$env"
    
    log "Pushing core components for $env environment..."
    execute_build "push" "controller" "broker"
    
    success "Core components pushed to registry for $env environment"
}

# Scenario: Push all components
scenario_push_all() {
    local env=${1:-prod}
    apply_env_config "$env"
    
    log "Pushing all components for $env environment..."
    execute_build "push"
    
    success "All components pushed to registry for $env environment"
}

# Scenario: Local development build (no push)
scenario_dev_build() {
    local env=${1:-dev}
    apply_env_config "$env"
    
    log "Building images for local development (no push)..."
    execute_build "build"
    
    success "Local development images built"
    log "Start local cluster: docker-compose up -d"
}

# Scenario: Staging images
scenario_staging_images() {
    apply_env_config "staging"
    shift # Remove scenario from args
    local components=("$@")
    
    if [ ${#components[@]} -eq 0 ]; then
        components=("controller" "broker" "admin")
    fi
    
    log "Building and pushing staging images with components: ${components[*]}"
    execute_build "all" "${components[@]}"
    
    success "Staging images completed"
    log "Deploy to staging: cd ../gke && ./deploy-rustmq-gke.sh deploy --environment staging"
}

# Scenario: Production images
scenario_production_images() {
    apply_env_config "prod"
    shift # Remove scenario from args
    local components=("$@")
    
    if [ ${#components[@]} -eq 0 ]; then
        components=("controller" "broker")
    fi
    
    log "Building production images with components: ${components[*]}"
    
    # Extra validation for production
    log "Production build - performing additional validation..."
    
    # Check git status
    if git status --porcelain 2>/dev/null | grep -q .; then
        read -p "Warning: Uncommitted changes detected. Continue? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            error "Production build cancelled"
        fi
    fi
    
    execute_build "all" "${components[@]}"
    
    success "Production images completed"
    log "Deploy to production: cd ../gke && ./deploy-rustmq-gke.sh deploy --environment production"
}

# Scenario: Hotfix images
scenario_hotfix_images() {
    local env=${1:-prod}
    shift
    local components=("$@")
    
    if [ ${#components[@]} -eq 0 ]; then
        error "Hotfix requires specific components. Example: $0 hotfix-images prod controller"
    fi
    
    apply_env_config "$env"
    
    # Use git commit as tag for hotfix
    local git_commit
    git_commit=$(git rev-parse --short HEAD)
    export IMAGE_TAG="hotfix-$git_commit"
    
    log "Creating hotfix images with tag: $IMAGE_TAG"
    execute_build "all" "${components[@]}"
    
    success "Hotfix images completed: $IMAGE_TAG"
    log "Deploy hotfix: cd ../gke && IMAGE_TAG=$IMAGE_TAG ./deploy-rustmq-gke.sh deploy --environment $env"
}

# Scenario: Cleanup
scenario_cleanup() {
    log "Cleaning up RustMQ images and Docker system..."
    
    "$BUILD_SCRIPT" clean
    
    # Additional Docker cleanup
    log "Pruning Docker system..."
    docker system prune -f
    docker volume prune -f
    
    success "Cleanup completed"
}

# Scenario: Status check
scenario_status() {
    log "Current RustMQ deployment status:"
    echo ""
    
    "$BUILD_SCRIPT" list
    echo ""
    
    "$BUILD_SCRIPT" list-local
    echo ""
    
    log "Docker system usage:"
    docker system df
}

# Main execution
main() {
    local scenario=${1:-help}
    
    case "$scenario" in
        "build-core")
            shift
            scenario_build_core "$@"
            ;;
        "build-all")
            shift
            scenario_build_all "$@"
            ;;
        "push-core")
            shift
            scenario_push_core "$@"
            ;;
        "push-all")
            shift
            scenario_push_all "$@"
            ;;
        "dev-build")
            shift
            scenario_dev_build "$@"
            ;;
        "staging-images")
            scenario_staging_images "$@"
            ;;
        "production-images")
            scenario_production_images "$@"
            ;;
        "hotfix-images")
            shift
            scenario_hotfix_images "$@"
            ;;
        "cleanup")
            scenario_cleanup
            ;;
        "status")
            scenario_status
            ;;
        "help"|"-h"|"--help")
            usage
            ;;
        *)
            error "Unknown scenario: $scenario. Run '$0 help' for usage."
            ;;
    esac
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi