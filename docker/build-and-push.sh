#!/bin/bash
set -euo pipefail

# =============================================================================
# RustMQ Docker Build and Push Script
# =============================================================================
# This script builds RustMQ Docker images and pushes them to Google Container Registry (GCR)
# or other container registries for use with GKE deployments.
#
# Usage:
#   ./build-and-push.sh build [component]     # Build images locally
#   ./build-and-push.sh push [component]      # Push images to registry
#   ./build-and-push.sh all [component]       # Build and push
#   ./build-and-push.sh list                  # List available components
#   ./build-and-push.sh clean                 # Clean local images
#
# Examples:
#   ./build-and-push.sh build                 # Build all components
#   ./build-and-push.sh build controller      # Build only controller
#   ./build-and-push.sh push broker           # Push only broker
#   ./build-and-push.sh all                   # Build and push all
# =============================================================================

# =============================================================================
# CONFIGURABLE DEPLOYMENT PARAMETERS
# =============================================================================

# Google Cloud Project Configuration
PROJECT_ID="${PROJECT_ID:-rustmq-production}"
REGION="${REGION:-us-central1}"
ZONE="${ZONE:-us-central1-a}"

# Container Registry Configuration
REGISTRY_HOST="${REGISTRY_HOST:-gcr.io}"
REGISTRY_PROJECT="${REGISTRY_PROJECT:-${PROJECT_ID}}"
REGISTRY_PREFIX="${REGISTRY_PREFIX:-${REGISTRY_HOST}/${REGISTRY_PROJECT}}"

# Image Configuration
IMAGE_TAG="${IMAGE_TAG:-latest}"
BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"
GIT_COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
VERSION="${VERSION:-v1.0.0}"

# Build Configuration
DOCKERFILE_DIR="$(dirname "$0")"
PROJECT_ROOT="$(cd "${DOCKERFILE_DIR}/.." && pwd)"
BUILD_CONTEXT="${BUILD_CONTEXT:-${PROJECT_ROOT}}"
DOCKER_BUILDKIT="${DOCKER_BUILDKIT:-1}"
PROGRESS="${PROGRESS:-auto}"

# Rust Build Configuration
RUST_TARGET="${RUST_TARGET:-x86_64-unknown-linux-gnu}"
CARGO_BUILD_PROFILE="${CARGO_BUILD_PROFILE:-release}"

# Advanced Configuration
PARALLEL_BUILDS="${PARALLEL_BUILDS:-4}"
PUSH_LATEST="${PUSH_LATEST:-true}"
SCAN_IMAGES="${SCAN_IMAGES:-false}"
MULTI_ARCH="${MULTI_ARCH:-false}"
PLATFORMS="${PLATFORMS:-linux/amd64}"

# =============================================================================
# COMPONENT DEFINITIONS
# =============================================================================

# Define all available components with their configurations
declare -A COMPONENTS=(
    ["controller"]="rustmq-controller:Dockerfile.controller:RustMQ Controller with OpenRaft consensus"
    ["broker"]="rustmq-broker:Dockerfile.broker:RustMQ Broker with QUIC and tiered storage"
    ["admin"]="rustmq-admin:Dockerfile.admin:RustMQ Admin CLI tool"
    ["admin-server"]="rustmq-admin-server:Dockerfile.admin-server:RustMQ Admin REST API server"
    ["bigquery-subscriber"]="rustmq-bigquery-subscriber:Dockerfile.bigquery-subscriber:BigQuery subscriber for real-time streaming"
)

# Core components required for GKE deployment
CORE_COMPONENTS=("controller" "broker")

# Optional components for extended functionality
OPTIONAL_COMPONENTS=("admin" "admin-server" "bigquery-subscriber")

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

# Progress indicator for long operations
progress() {
    local current=$1
    local total=$2
    local component=$3
    local operation=$4
    
    local percent=$((current * 100 / total))
    local bar_length=20
    local filled_length=$((percent * bar_length / 100))
    
    printf "\r["
    printf "%${filled_length}s" | tr ' ' '='
    printf "%$((bar_length - filled_length))s" | tr ' ' '-'
    printf "] %3d%% %s %s" "$percent" "$operation" "$component"
    
    if [ "$current" -eq "$total" ]; then
        echo ""
    fi
}

# Check if required tools are installed
check_prerequisites() {
    local missing_tools=()
    
    if ! command -v docker &> /dev/null; then
        missing_tools+=("docker")
    fi
    
    if ! command -v gcloud &> /dev/null && [[ "$REGISTRY_HOST" == "gcr.io" ]]; then
        missing_tools+=("gcloud")
    fi
    
    if [ ${#missing_tools[@]} -ne 0 ]; then
        error "Missing required tools: ${missing_tools[*]}"
    fi
    
    # Check Docker daemon
    if ! docker info &> /dev/null; then
        error "Docker daemon is not running"
    fi
}

# Validate project structure
validate_project() {
    if [ ! -f "${PROJECT_ROOT}/Cargo.toml" ]; then
        error "Cargo.toml not found in project root: ${PROJECT_ROOT}"
    fi
    
    if [ ! -d "${PROJECT_ROOT}/src" ]; then
        error "src directory not found in project root: ${PROJECT_ROOT}"
    fi
    
    info "Project validation successful: ${PROJECT_ROOT}"
}

# Setup authentication for container registry
setup_authentication() {
    case "$REGISTRY_HOST" in
        "gcr.io"|"us.gcr.io"|"eu.gcr.io"|"asia.gcr.io")
            info "Setting up GCR authentication..."
            if ! gcloud auth configure-docker --quiet 2>/dev/null; then
                warning "Failed to configure Docker for GCR, attempting manual login..."
                gcloud auth print-access-token | docker login -u oauth2accesstoken --password-stdin https://gcr.io
            fi
            ;;
        "docker.io"|"index.docker.io")
            info "Using Docker Hub - ensure you're logged in with 'docker login'"
            ;;
        *)
            info "Using custom registry: $REGISTRY_HOST"
            ;;
    esac
}

# Get component information
get_component_info() {
    local component=$1
    local field=$2
    
    if [[ ! ${COMPONENTS[$component]+_} ]]; then
        error "Unknown component: $component"
    fi
    
    local info="${COMPONENTS[$component]}"
    case "$field" in
        "image")
            echo "${info}" | cut -d: -f1
            ;;
        "dockerfile")
            echo "${info}" | cut -d: -f2
            ;;
        "description")
            echo "${info}" | cut -d: -f3-
            ;;
        *)
            error "Unknown field: $field"
            ;;
    esac
}

# Generate image tags
generate_tags() {
    local image_name=$1
    local tags=()
    
    # Primary tag
    tags+=("${REGISTRY_PREFIX}/${image_name}:${IMAGE_TAG}")
    
    # Version tag
    if [ "$IMAGE_TAG" != "$VERSION" ]; then
        tags+=("${REGISTRY_PREFIX}/${image_name}:${VERSION}")
    fi
    
    # Latest tag (if enabled)
    if [ "$PUSH_LATEST" = "true" ] && [ "$IMAGE_TAG" != "latest" ]; then
        tags+=("${REGISTRY_PREFIX}/${image_name}:latest")
    fi
    
    # Git commit tag
    if [ "$GIT_COMMIT" != "unknown" ]; then
        tags+=("${REGISTRY_PREFIX}/${image_name}:${GIT_COMMIT}")
    fi
    
    printf '%s\n' "${tags[@]}"
}

# =============================================================================
# BUILD FUNCTIONS
# =============================================================================

# Build a single component
build_component() {
    local component=$1
    local image_name dockerfile
    
    image_name=$(get_component_info "$component" "image")
    dockerfile=$(get_component_info "$component" "dockerfile")
    
    info "Building component '$component' (${image_name})"
    
    local dockerfile_path="${DOCKERFILE_DIR}/${dockerfile}"
    if [ ! -f "$dockerfile_path" ]; then
        error "Dockerfile not found: $dockerfile_path"
    fi
    
    # Generate all tags for this image
    local tags
    readarray -t tags < <(generate_tags "$image_name")
    
    # Build the primary tag first
    local primary_tag="${tags[0]}"
    
    info "Building image: $primary_tag"
    info "Dockerfile: $dockerfile_path"
    info "Build context: $BUILD_CONTEXT"
    
    local build_args=(
        --file "$dockerfile_path"
        --build-arg "BUILD_DATE=$BUILD_DATE"
        --build-arg "GIT_COMMIT=$GIT_COMMIT"
        --build-arg "VERSION=$VERSION"
        --build-arg "RUST_TARGET=$RUST_TARGET"
        --build-arg "CARGO_BUILD_PROFILE=$CARGO_BUILD_PROFILE"
        --label "org.opencontainers.image.created=$BUILD_DATE"
        --label "org.opencontainers.image.revision=$GIT_COMMIT"
        --label "org.opencontainers.image.version=$VERSION"
        --label "org.opencontainers.image.title=RustMQ $component"
        --label "org.opencontainers.image.description=$(get_component_info "$component" "description")"
        --progress "$PROGRESS"
        --tag "$primary_tag"
    )
    
    # Add additional tags
    for tag in "${tags[@]:1}"; do
        build_args+=(--tag "$tag")
    done
    
    # Multi-architecture build if enabled
    if [ "$MULTI_ARCH" = "true" ]; then
        build_args+=(--platform "$PLATFORMS")
    fi
    
    # Execute build
    DOCKER_BUILDKIT="$DOCKER_BUILDKIT" docker build "${build_args[@]}" "$BUILD_CONTEXT"
    
    # Scan image for vulnerabilities if enabled
    if [ "$SCAN_IMAGES" = "true" ]; then
        info "Scanning image for vulnerabilities: $primary_tag"
        if command -v trivy &> /dev/null; then
            trivy image "$primary_tag" || warning "Vulnerability scan failed"
        else
            warning "Trivy not installed, skipping vulnerability scan"
        fi
    fi
    
    success "Built component '$component' with ${#tags[@]} tags"
}

# Build all or specified components
build_images() {
    local components=("$@")
    
    if [ ${#components[@]} -eq 0 ]; then
        components=("${CORE_COMPONENTS[@]}" "${OPTIONAL_COMPONENTS[@]}")
    fi
    
    info "Building ${#components[@]} components: ${components[*]}"
    
    local current=0
    for component in "${components[@]}"; do
        ((current++))
        progress "$current" "${#components[@]}" "$component" "Building"
        build_component "$component"
    done
    
    success "All builds completed successfully"
}

# =============================================================================
# PUSH FUNCTIONS
# =============================================================================

# Push a single component
push_component() {
    local component=$1
    local image_name
    
    image_name=$(get_component_info "$component" "image")
    
    info "Pushing component '$component' (${image_name})"
    
    # Get all tags for this image
    local tags
    readarray -t tags < <(generate_tags "$image_name")
    
    for tag in "${tags[@]}"; do
        info "Pushing tag: $tag"
        
        # Verify image exists locally before pushing
        if ! docker image inspect "$tag" &> /dev/null; then
            error "Image not found locally: $tag (run build first)"
        fi
        
        docker push "$tag"
    done
    
    success "Pushed component '$component' with ${#tags[@]} tags"
}

# Push all or specified components
push_images() {
    local components=("$@")
    
    if [ ${#components[@]} -eq 0 ]; then
        components=("${CORE_COMPONENTS[@]}" "${OPTIONAL_COMPONENTS[@]}")
    fi
    
    info "Pushing ${#components[@]} components: ${components[*]}"
    
    setup_authentication
    
    local current=0
    for component in "${components[@]}"; do
        ((current++))
        progress "$current" "${#components[@]}" "$component" "Pushing"
        push_component "$component"
    done
    
    success "All pushes completed successfully"
}

# =============================================================================
# MANAGEMENT FUNCTIONS
# =============================================================================

# List all available components
list_components() {
    echo "Available RustMQ components:"
    echo ""
    echo "Core Components (required for GKE):"
    for component in "${CORE_COMPONENTS[@]}"; do
        local image_name description
        image_name=$(get_component_info "$component" "image")
        description=$(get_component_info "$component" "description")
        printf "  %-20s %-30s %s\n" "$component" "$image_name" "$description"
    done
    
    echo ""
    echo "Optional Components:"
    for component in "${OPTIONAL_COMPONENTS[@]}"; do
        local image_name description
        image_name=$(get_component_info "$component" "image")
        description=$(get_component_info "$component" "description")
        printf "  %-20s %-30s %s\n" "$component" "$image_name" "$description"
    done
    
    echo ""
    echo "Configuration:"
    printf "  %-20s %s\n" "Registry:" "$REGISTRY_PREFIX"
    printf "  %-20s %s\n" "Project ID:" "$PROJECT_ID"
    printf "  %-20s %s\n" "Image Tag:" "$IMAGE_TAG"
    printf "  %-20s %s\n" "Version:" "$VERSION"
    printf "  %-20s %s\n" "Git Commit:" "$GIT_COMMIT"
}

# List local images
list_local_images() {
    echo "Local RustMQ images:"
    docker images --filter "reference=${REGISTRY_PREFIX}/rustmq-*" --format "table {{.Repository}}:{{.Tag}}\t{{.CreatedAt}}\t{{.Size}}"
}

# List remote images (GCR only)
list_remote_images() {
    if [[ "$REGISTRY_HOST" == "gcr.io" ]]; then
        echo "Remote RustMQ images in GCR:"
        for component in "${CORE_COMPONENTS[@]}" "${OPTIONAL_COMPONENTS[@]}"; do
            local image_name
            image_name=$(get_component_info "$component" "image")
            echo ""
            echo "Component: $component ($image_name)"
            gcloud container images list-tags "${REGISTRY_PREFIX}/${image_name}" --limit=10 --format="table(tags.list():label=TAGS,timestamp.date():label=CREATED,digest:label=DIGEST)" 2>/dev/null || echo "  No images found"
        done
    else
        warning "Remote image listing only supported for GCR"
    fi
}

# Clean local images
clean_images() {
    local components=("$@")
    
    if [ ${#components[@]} -eq 0 ]; then
        info "Cleaning all RustMQ images..."
        docker images --filter "reference=${REGISTRY_PREFIX}/rustmq-*" --format "{{.Repository}}:{{.Tag}}" | xargs -r docker rmi
    else
        info "Cleaning specified components: ${components[*]}"
        for component in "${components[@]}"; do
            local image_name
            image_name=$(get_component_info "$component" "image")
            docker images --filter "reference=${REGISTRY_PREFIX}/${image_name}" --format "{{.Repository}}:{{.Tag}}" | xargs -r docker rmi
        done
    fi
    
    # Clean dangling images
    docker image prune -f
    
    success "Image cleanup completed"
}

# Generate deployment manifests with correct image references
generate_manifests() {
    local output_dir="${1:-./manifests}"
    
    info "Generating deployment manifests with current image configuration..."
    
    mkdir -p "$output_dir"
    
    # Create environment file for GKE deployment
    cat > "${output_dir}/image-config.env" << EOF
# RustMQ Image Configuration
# Generated at: $BUILD_DATE
# Git commit: $GIT_COMMIT

PROJECT_ID=$PROJECT_ID
REGISTRY_HOST=$REGISTRY_HOST
REGISTRY_PREFIX=$REGISTRY_PREFIX
IMAGE_TAG=$IMAGE_TAG
VERSION=$VERSION

# Core Images
CONTROLLER_IMAGE=${REGISTRY_PREFIX}/rustmq-controller:${IMAGE_TAG}
BROKER_IMAGE=${REGISTRY_PREFIX}/rustmq-broker:${IMAGE_TAG}

# Optional Images
ADMIN_IMAGE=${REGISTRY_PREFIX}/rustmq-admin:${IMAGE_TAG}
ADMIN_SERVER_IMAGE=${REGISTRY_PREFIX}/rustmq-admin-server:${IMAGE_TAG}
BIGQUERY_SUBSCRIBER_IMAGE=${REGISTRY_PREFIX}/rustmq-bigquery-subscriber:${IMAGE_TAG}
EOF

    # Create Kustomization patch for image updates
    cat > "${output_dir}/image-patch.yaml" << EOF
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

images:
- name: gcr.io/PROJECT_ID/rustmq-controller
  newName: ${REGISTRY_PREFIX}/rustmq-controller
  newTag: ${IMAGE_TAG}
- name: gcr.io/PROJECT_ID/rustmq-broker
  newName: ${REGISTRY_PREFIX}/rustmq-broker
  newTag: ${IMAGE_TAG}
EOF

    info "Generated manifests in: $output_dir"
    success "Manifest generation completed"
}

# =============================================================================
# MAIN SCRIPT LOGIC
# =============================================================================

# Display usage information
usage() {
    cat << EOF
RustMQ Docker Build and Push Script

USAGE:
    $0 <command> [component...]

COMMANDS:
    build [component...]     Build Docker images locally
    push [component...]      Push images to container registry
    all [component...]       Build and push images
    list                     List available components and configuration
    list-local              List local Docker images
    list-remote             List remote images (GCR only)
    clean [component...]     Remove local Docker images
    manifests [output-dir]   Generate deployment manifests
    help                     Show this help message

COMPONENTS:
    controller              RustMQ Controller (required for GKE)
    broker                  RustMQ Broker (required for GKE)
    admin                   RustMQ Admin CLI
    admin-server            RustMQ Admin REST API server
    bigquery-subscriber     BigQuery subscriber service
    
    If no components specified, all components will be processed.

ENVIRONMENT VARIABLES:
    PROJECT_ID              Google Cloud Project ID (default: rustmq-production)
    REGISTRY_HOST           Container registry host (default: gcr.io)
    IMAGE_TAG               Docker image tag (default: latest)
    VERSION                 Version label (default: v1.0.0)
    BUILD_CONTEXT           Docker build context (default: project root)
    PARALLEL_BUILDS         Number of parallel builds (default: 4)
    SCAN_IMAGES             Scan images for vulnerabilities (default: false)
    MULTI_ARCH              Enable multi-architecture builds (default: false)

EXAMPLES:
    # Build all images
    $0 build

    # Build only core components for GKE
    $0 build controller broker

    # Build and push with custom tag
    IMAGE_TAG=v1.2.0 $0 all controller broker

    # Push to custom registry
    REGISTRY_HOST=my-registry.com PROJECT_ID=my-project $0 push

    # Generate deployment manifests
    $0 manifests ./deploy

    # Clean up all local images
    $0 clean

EOF
}

# Validate component names
validate_components() {
    local components=("$@")
    
    for component in "${components[@]}"; do
        if [[ ! ${COMPONENTS[$component]+_} ]]; then
            error "Unknown component: $component. Run '$0 list' to see available components."
        fi
    done
}

# Main execution function
main() {
    local command="${1:-help}"
    shift || true
    
    case "$command" in
        "build")
            check_prerequisites
            validate_project
            validate_components "$@"
            build_images "$@"
            ;;
        "push")
            check_prerequisites
            validate_components "$@"
            push_images "$@"
            ;;
        "all")
            check_prerequisites
            validate_project
            validate_components "$@"
            build_images "$@"
            push_images "$@"
            ;;
        "list")
            list_components
            ;;
        "list-local")
            list_local_images
            ;;
        "list-remote")
            list_remote_images
            ;;
        "clean")
            validate_components "$@" 2>/dev/null || true  # Allow cleaning non-existent components
            clean_images "$@"
            ;;
        "manifests")
            generate_manifests "$@"
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