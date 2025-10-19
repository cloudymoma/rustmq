#!/usr/bin/env bash
#
# RustMQ Multi-Platform Docker Build Script with Build Cache Optimization
# Phase 1: Docker Build Optimization (October 2025)
#
# Features:
# - Multi-platform builds (AMD64 and ARM64)
# - Registry-based build cache for faster CI/CD
# - Parallel builds for multiple services
# - Automated tagging and versioning
# - Build time benchmarking
#
# Usage:
#   ./build-multiplatform.sh [OPTIONS]
#
# Options:
#   --registry REGISTRY     Container registry (default: gcr.io/PROJECT_ID)
#   --platforms PLATFORMS   Comma-separated platforms (default: linux/amd64,linux/arm64)
#   --tag TAG              Image tag (default: latest)
#   --cache-from          Enable cache-from (default: true)
#   --cache-to            Enable cache-to (default: true)
#   --push                Push to registry (default: false)
#   --load                Load to local docker (default: false, incompatible with multi-platform)
#   --service SERVICE     Build specific service (broker|controller|admin|admin-server|bigquery-subscriber)
#   --benchmark           Run build time benchmark (default: false)
#   --help                Show this help message

set -euo pipefail

# Default configuration
REGISTRY="${REGISTRY:-gcr.io/$(gcloud config get-value project 2>/dev/null || echo 'rustmq-project')}"
PLATFORMS="${PLATFORMS:-linux/amd64,linux/arm64}"
TAG="${TAG:-latest}"
CACHE_FROM="${CACHE_FROM:-true}"
CACHE_TO="${CACHE_TO:-true}"
PUSH="${PUSH:-false}"
LOAD="${LOAD:-false}"
SERVICE="${SERVICE:-all}"
BENCHMARK="${BENCHMARK:-false}"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Service definitions
declare -A SERVICES=(
    ["broker"]="rustmq-broker"
    ["controller"]="rustmq-controller"
    ["admin"]="rustmq-admin"
    ["admin-server"]="rustmq-admin-server"
    ["bigquery-subscriber"]="rustmq-bigquery-subscriber"
)

# Print colored message
log() {
    local color=$1
    shift
    echo -e "${color}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $*"
}

# Show help
show_help() {
    grep '^#' "$0" | grep -v '#!/usr/bin/env' | sed 's/^# //' | sed 's/^#//'
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --registry)
                REGISTRY="$2"
                shift 2
                ;;
            --platforms)
                PLATFORMS="$2"
                shift 2
                ;;
            --tag)
                TAG="$2"
                shift 2
                ;;
            --cache-from)
                CACHE_FROM="$2"
                shift 2
                ;;
            --cache-to)
                CACHE_TO="$2"
                shift 2
                ;;
            --push)
                PUSH="true"
                shift
                ;;
            --load)
                LOAD="true"
                shift
                ;;
            --service)
                SERVICE="$2"
                shift 2
                ;;
            --benchmark)
                BENCHMARK="true"
                shift
                ;;
            --help)
                show_help
                exit 0
                ;;
            *)
                log "$RED" "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
}

# Validate configuration
validate_config() {
    if [[ "$LOAD" == "true" && "$PLATFORMS" == *","* ]]; then
        log "$RED" "Error: --load is incompatible with multi-platform builds"
        log "$YELLOW" "Use --platforms linux/amd64 for single platform or remove --load"
        exit 1
    fi

    if [[ "$SERVICE" != "all" && ! -v "SERVICES[$SERVICE]" ]]; then
        log "$RED" "Error: Unknown service '$SERVICE'"
        log "$YELLOW" "Available services: ${!SERVICES[*]}"
        exit 1
    fi
}

# Setup buildx builder
setup_buildx() {
    log "$BLUE" "Setting up Docker Buildx..."

    # Create builder if it doesn't exist
    if ! docker buildx inspect rustmq-builder > /dev/null 2>&1; then
        log "$BLUE" "Creating new buildx builder 'rustmq-builder'..."
        docker buildx create \
            --name rustmq-builder \
            --driver docker-container \
            --bootstrap \
            --use
    else
        log "$GREEN" "Using existing buildx builder 'rustmq-builder'"
        docker buildx use rustmq-builder
    fi

    # Bootstrap the builder
    docker buildx inspect --bootstrap
}

# Build a single service
build_service() {
    local service_key=$1
    local service_name=${SERVICES[$service_key]}
    local dockerfile="docker/Dockerfile.${service_key//-/.}"
    local image="${REGISTRY}/${service_name}:${TAG}"

    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$BLUE" "Building ${service_name}"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Start timing
    local start_time=$(date +%s)

    # Build cache arguments
    local cache_args=()
    if [[ "$CACHE_FROM" == "true" ]]; then
        cache_args+=(
            "--cache-from=type=registry,ref=${image}-cache"
        )
        log "$YELLOW" "Using cache from: ${image}-cache"
    fi

    if [[ "$CACHE_TO" == "true" ]]; then
        cache_args+=(
            "--cache-to=type=registry,ref=${image}-cache,mode=max"
        )
        log "$YELLOW" "Saving cache to: ${image}-cache"
    fi

    # Build arguments
    local build_args=(
        "--platform=${PLATFORMS}"
        "--file=${dockerfile}"
        "--tag=${image}"
        "${cache_args[@]}"
        "--build-arg=BUILDKIT_INLINE_CACHE=1"
        "--progress=plain"
    )

    # Push or load
    if [[ "$PUSH" == "true" ]]; then
        build_args+=("--push")
        log "$YELLOW" "Will push to registry: ${image}"
    elif [[ "$LOAD" == "true" ]]; then
        build_args+=("--load")
        log "$YELLOW" "Will load to local Docker"
    else
        log "$YELLOW" "Dry run - not pushing or loading"
    fi

    # Execute build
    log "$GREEN" "Executing: docker buildx build ${build_args[*]} ."

    if docker buildx build "${build_args[@]}" .; then
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        log "$GREEN" "✓ Successfully built ${service_name} in ${duration}s"

        # Store benchmark data
        if [[ "$BENCHMARK" == "true" ]]; then
            echo "${service_name},${duration}" >> /tmp/rustmq-build-times.csv
        fi

        return 0
    else
        log "$RED" "✗ Failed to build ${service_name}"
        return 1
    fi
}

# Build all services or specific service
build_services() {
    local failed=0
    local total=0
    local start_time=$(date +%s)

    # Initialize benchmark file
    if [[ "$BENCHMARK" == "true" ]]; then
        echo "service,duration_seconds" > /tmp/rustmq-build-times.csv
    fi

    if [[ "$SERVICE" == "all" ]]; then
        log "$BLUE" "Building all services..."
        for service_key in "${!SERVICES[@]}"; do
            ((total++))
            if ! build_service "$service_key"; then
                ((failed++))
            fi
        done
    else
        ((total++))
        if ! build_service "$SERVICE"; then
            ((failed++))
        fi
    fi

    local end_time=$(date +%s)
    local total_duration=$((end_time - start_time))

    # Summary
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$BLUE" "Build Summary"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$GREEN" "Total services: ${total}"
    log "$GREEN" "Successful: $((total - failed))"
    if [[ $failed -gt 0 ]]; then
        log "$RED" "Failed: ${failed}"
    fi
    log "$BLUE" "Total time: ${total_duration}s"

    # Show benchmark results
    if [[ "$BENCHMARK" == "true" && -f /tmp/rustmq-build-times.csv ]]; then
        log "$BLUE" ""
        log "$BLUE" "Build Time Benchmark Results:"
        log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        column -t -s',' /tmp/rustmq-build-times.csv

        # Calculate average
        local avg=$(awk -F',' 'NR>1 {sum+=$2; count++} END {if(count>0) print sum/count; else print 0}' /tmp/rustmq-build-times.csv)
        log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        log "$GREEN" "Average build time: ${avg}s"
    fi

    return $failed
}

# Main execution
main() {
    parse_args "$@"
    validate_config

    log "$BLUE" "RustMQ Multi-Platform Docker Builder"
    log "$BLUE" "====================================="
    log "$BLUE" "Registry: ${REGISTRY}"
    log "$BLUE" "Platforms: ${PLATFORMS}"
    log "$BLUE" "Tag: ${TAG}"
    log "$BLUE" "Service: ${SERVICE}"
    log "$BLUE" "Cache From: ${CACHE_FROM}"
    log "$BLUE" "Cache To: ${CACHE_TO}"
    log "$BLUE" "Push: ${PUSH}"
    log "$BLUE" "Load: ${LOAD}"
    log "$BLUE" "Benchmark: ${BENCHMARK}"
    log "$BLUE" ""

    setup_buildx
    build_services

    local exit_code=$?
    if [[ $exit_code -eq 0 ]]; then
        log "$GREEN" "All builds completed successfully!"
    else
        log "$RED" "Some builds failed. See errors above."
    fi

    exit $exit_code
}

# Run main function
main "$@"
