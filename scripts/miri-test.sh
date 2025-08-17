#!/bin/bash
# Miri Memory Safety Test Runner for RustMQ
# 
# This script runs comprehensive Miri tests for memory safety validation
# across core libraries and SDK components.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SDK_ROOT="$PROJECT_ROOT/sdk/rust"

# Test categories
RUN_CORE_TESTS=${RUN_CORE_TESTS:-true}
RUN_SDK_TESTS=${RUN_SDK_TESTS:-true}
RUN_PROPTEST=${RUN_PROPTEST:-true}
VERBOSE=${VERBOSE:-false}

# Miri configuration
export MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation -Zmiri-ignore-leaks -Zmiri-backtrace=full}"
export PROPTEST_CASES="${PROPTEST_CASES:-10}"
export RUST_TEST_TIMEOUT="${RUST_TEST_TIMEOUT:-300}"

log() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

log_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

log_error() {
    echo -e "${RED}✗ $1${NC}"
}

check_prerequisites() {
    log "Checking prerequisites..."
    
    # Check if nightly toolchain is available
    if ! rustup toolchain list | grep -q nightly; then
        log_error "Nightly Rust toolchain not found. Installing..."
        rustup toolchain install nightly
    fi
    
    # Check if Miri is installed
    if ! rustup +nightly component list --installed | grep -q miri; then
        log_error "Miri not found. Installing..."
        rustup +nightly component add miri
        cargo +nightly miri setup
    fi
    
    log_success "Prerequisites checked"
}

run_core_tests() {
    if [[ "$RUN_CORE_TESTS" != "true" ]]; then
        log_warning "Skipping core tests (RUN_CORE_TESTS=false)"
        return 0
    fi
    
    log "Running core library Miri tests..."
    cd "$PROJECT_ROOT"
    
    local test_args=("--features" "miri-safe")
    if [[ "$VERBOSE" == "true" ]]; then
        test_args+=("--" "--nocapture")
    fi
    
    # Storage tests
    log "Testing storage components..."
    if cargo +nightly miri test "${test_args[@]}" miri::storage_tests; then
        log_success "Storage tests passed"
    else
        log_error "Storage tests failed"
        return 1
    fi
    
    # Security tests  
    log "Testing security components..."
    if cargo +nightly miri test "${test_args[@]}" miri::security_tests; then
        log_success "Security tests passed"
    else
        log_error "Security tests failed"
        return 1
    fi
    
    # Cache tests
    log "Testing cache components..."
    if cargo +nightly miri test "${test_args[@]}" miri::cache_tests; then
        log_success "Cache tests passed"
    else
        log_error "Cache tests failed"
        return 1
    fi
    
    log_success "All core tests passed"
}

run_sdk_tests() {
    if [[ "$RUN_SDK_TESTS" != "true" ]]; then
        log_warning "Skipping SDK tests (RUN_SDK_TESTS=false)"
        return 0
    fi
    
    log "Running SDK Miri tests..."
    cd "$SDK_ROOT"
    
    local test_args=("--features" "miri-safe")
    if [[ "$VERBOSE" == "true" ]]; then
        test_args+=("--" "--nocapture")
    fi
    
    # Client tests
    log "Testing SDK client components..."
    if cargo +nightly miri test "${test_args[@]}" miri::client_tests; then
        log_success "SDK client tests passed"
    else
        log_error "SDK client tests failed"
        return 1
    fi
    
    log_success "All SDK tests passed"
    cd "$PROJECT_ROOT"
}

run_proptest_integration() {
    if [[ "$RUN_PROPTEST" != "true" ]]; then
        log_warning "Skipping proptest integration (RUN_PROPTEST=false)"
        return 0
    fi
    
    log "Running proptest integration with Miri..."
    cd "$PROJECT_ROOT"
    
    local test_args=("--features" "miri-safe")
    if [[ "$VERBOSE" == "true" ]]; then
        test_args+=("--" "--nocapture")
    fi
    
    log "Testing property-based tests under Miri..."
    if cargo +nightly miri test "${test_args[@]}" miri::proptest_integration; then
        log_success "Proptest integration passed"
    else
        log_error "Proptest integration failed"
        return 1
    fi
}

run_subset_tests() {
    log "Running fast subset of Miri tests for development..."
    cd "$PROJECT_ROOT"
    
    # Quick smoke tests
    local quick_tests=(
        "miri::storage_tests::test_wal_memory_safety"
        "miri::security_tests::test_certificate_validation_memory_safety"
        "miri::cache_tests::test_lru_cache_memory_safety"
        "miri::proptest_integration::test_miri_basic_functionality"
    )
    
    for test in "${quick_tests[@]}"; do
        log "Running quick test: $test"
        if cargo +nightly miri test --features miri-safe "$test"; then
            log_success "✓ $test"
        else
            log_error "✗ $test"
            return 1
        fi
    done
    
    log_success "Quick Miri subset completed"
}

show_usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Run Miri memory safety tests for RustMQ

OPTIONS:
    --core-only         Run only core library tests
    --sdk-only          Run only SDK tests  
    --proptest-only     Run only proptest integration
    --quick             Run quick subset for development
    --verbose           Enable verbose output
    --help              Show this help message

ENVIRONMENT VARIABLES:
    RUN_CORE_TESTS      Enable/disable core tests (default: true)
    RUN_SDK_TESTS       Enable/disable SDK tests (default: true)
    RUN_PROPTEST        Enable/disable proptest (default: true)
    VERBOSE             Enable verbose output (default: false)
    MIRIFLAGS           Additional Miri flags
    PROPTEST_CASES      Number of proptest cases (default: 10)
    RUST_TEST_TIMEOUT   Test timeout in seconds (default: 300)

EXAMPLES:
    # Run all tests
    $0
    
    # Run only core tests with verbose output
    $0 --core-only --verbose
    
    # Quick development check
    $0 --quick
    
    # Run with custom Miri flags
    MIRIFLAGS="-Zmiri-ignore-leaks" $0
    
    # Run with more proptest cases (slower but more thorough)
    PROPTEST_CASES=50 $0 --proptest-only
EOF
}

main() {
    local quick_mode=false
    
    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --core-only)
                RUN_CORE_TESTS=true
                RUN_SDK_TESTS=false
                RUN_PROPTEST=false
                shift
                ;;
            --sdk-only)
                RUN_CORE_TESTS=false
                RUN_SDK_TESTS=true
                RUN_PROPTEST=false
                shift
                ;;
            --proptest-only)
                RUN_CORE_TESTS=false
                RUN_SDK_TESTS=false
                RUN_PROPTEST=true
                shift
                ;;
            --quick)
                quick_mode=true
                shift
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            --help)
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
    
    log "Starting RustMQ Miri memory safety tests..."
    log "Configuration: CORE=$RUN_CORE_TESTS SDK=$RUN_SDK_TESTS PROPTEST=$RUN_PROPTEST"
    log "Miri flags: $MIRIFLAGS"
    
    check_prerequisites
    
    if [[ "$quick_mode" == "true" ]]; then
        run_subset_tests
    else
        local failed=false
        
        if ! run_core_tests; then
            failed=true
        fi
        
        if ! run_sdk_tests; then
            failed=true
        fi
        
        if ! run_proptest_integration; then
            failed=true
        fi
        
        if [[ "$failed" == "true" ]]; then
            log_error "Some Miri tests failed"
            exit 1
        fi
    fi
    
    log_success "All Miri tests completed successfully! 🎉"
    log "Memory safety validation: ✅ PASSED"
}

# Run main function
main "$@"