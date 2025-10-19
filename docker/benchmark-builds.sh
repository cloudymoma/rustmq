#!/usr/bin/env bash
#
# RustMQ Docker Build Performance Benchmark Script
# Phase 1: Docker Build Optimization (October 2025)
#
# This script compares build times between optimized and baseline builds
# to validate the performance improvements from Phase 1 optimizations.
#
# Usage:
#   ./benchmark-builds.sh [OPTIONS]
#
# Options:
#   --service SERVICE     Benchmark specific service (default: broker)
#   --iterations N        Number of benchmark iterations (default: 3)
#   --output FILE        Output CSV file (default: benchmark-results.csv)
#   --clean             Clean build cache before each run
#   --help              Show this help message

set -euo pipefail

# Default configuration
SERVICE="${SERVICE:-broker}"
ITERATIONS="${ITERATIONS:-3}"
OUTPUT="${OUTPUT:-benchmark-results.csv}"
CLEAN="${CLEAN:-false}"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Log function
log() {
    local color=$1
    shift
    echo -e "${color}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $*"
}

# Show help
show_help() {
    grep '^#' "$0" | grep -v '#!/usr/bin/env' | sed 's/^# //' | sed 's/^#//'
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --service)
                SERVICE="$2"
                shift 2
                ;;
            --iterations)
                ITERATIONS="$2"
                shift 2
                ;;
            --output)
                OUTPUT="$2"
                shift 2
                ;;
            --clean)
                CLEAN="true"
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

# Clean build cache
clean_cache() {
    log "$YELLOW" "Cleaning build cache..."
    docker buildx prune --all --force > /dev/null 2>&1 || true
    docker system prune --force > /dev/null 2>&1 || true
    log "$GREEN" "Cache cleaned"
}

# Time a build
time_build() {
    local scenario=$1
    local cache_args=$2
    local start_time=$(date +%s%3N)

    log "$BLUE" "Running: $scenario"

    if ./docker/build-multiplatform.sh \
        --service "$SERVICE" \
        --platforms linux/amd64 \
        --tag benchmark-test \
        $cache_args \
        > /tmp/build-output.log 2>&1; then

        local end_time=$(date +%s%3N)
        local duration=$(( (end_time - start_time) / 1000 ))
        log "$GREEN" "✓ Completed in ${duration}s"
        echo "$duration"
        return 0
    else
        log "$RED" "✗ Build failed"
        cat /tmp/build-output.log
        echo "FAILED"
        return 1
    fi
}

# Run benchmark scenarios
run_benchmarks() {
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$BLUE" "RustMQ Docker Build Performance Benchmark"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$BLUE" "Service: $SERVICE"
    log "$BLUE" "Iterations: $ITERATIONS"
    log "$BLUE" "Output: $OUTPUT"
    log "$BLUE" ""

    # Initialize CSV
    echo "scenario,iteration,duration_seconds,cache_hit,timestamp" > "$OUTPUT"

    # Scenario 1: Cold build (no cache)
    log "$BLUE" "Scenario 1: Cold Build (No Cache)"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    for i in $(seq 1 "$ITERATIONS"); do
        [[ "$CLEAN" == "true" ]] && clean_cache
        duration=$(time_build "Cold Build #$i" "--cache-from false --cache-to false")
        echo "cold_build,$i,$duration,false,$(date +%s)" >> "$OUTPUT"
        sleep 2
    done

    # Scenario 2: Warm build (with cache)
    log "$BLUE" ""
    log "$BLUE" "Scenario 2: Warm Build (With Cache)"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # First build to populate cache
    log "$YELLOW" "Populating cache..."
    clean_cache
    time_build "Cache Population" "--cache-from false --cache-to true" > /dev/null

    # Now benchmark with cache
    for i in $(seq 1 "$ITERATIONS"); do
        duration=$(time_build "Warm Build #$i" "--cache-from true --cache-to false")
        echo "warm_build,$i,$duration,true,$(date +%s)" >> "$OUTPUT"
        sleep 2
    done

    # Scenario 3: Source-only change (cache hit on deps)
    log "$BLUE" ""
    log "$BLUE" "Scenario 3: Source-Only Change (Dependency Cache Hit)"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Simulate source change by touching a file
    for i in $(seq 1 "$ITERATIONS"); do
        log "$YELLOW" "Simulating source change..."
        touch src/main.rs  # Force rebuild

        duration=$(time_build "Source Change #$i" "--cache-from true --cache-to true")
        echo "source_change,$i,$duration,partial,$(date +%s)" >> "$OUTPUT"

        # Restore
        git checkout src/main.rs 2>/dev/null || true
        sleep 2
    done

    # Scenario 4: No changes (full cache hit)
    log "$BLUE" ""
    log "$BLUE" "Scenario 4: No Changes (Full Cache Hit)"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    for i in $(seq 1 "$ITERATIONS"); do
        duration=$(time_build "No Changes #$i" "--cache-from true --cache-to false")
        echo "no_changes,$i,$duration,true,$(date +%s)" >> "$OUTPUT"
        sleep 2
    done
}

# Generate summary report
generate_report() {
    log "$BLUE" ""
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$BLUE" "Benchmark Results Summary"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Calculate statistics
    for scenario in cold_build warm_build source_change no_changes; do
        log "$BLUE" ""
        log "$GREEN" "Scenario: ${scenario//_/ }"

        # Average
        avg=$(awk -F',' -v scenario="$scenario" \
            'NR>1 && $1==scenario {sum+=$3; count++} END {if(count>0) printf "%.2f", sum/count; else print "N/A"}' \
            "$OUTPUT")

        # Min
        min=$(awk -F',' -v scenario="$scenario" \
            'NR>1 && $1==scenario {if(min=="" || $3<min) min=$3} END {if(min!="") print min; else print "N/A"}' \
            "$OUTPUT")

        # Max
        max=$(awk -F',' -v scenario="$scenario" \
            'NR>1 && $1==scenario {if(max=="" || $3>max) max=$3} END {if(max!="") print max; else print "N/A"}' \
            "$OUTPUT")

        log "$YELLOW" "  Average: ${avg}s"
        log "$YELLOW" "  Min:     ${min}s"
        log "$YELLOW" "  Max:     ${max}s"
    done

    # Improvement calculations
    log "$BLUE" ""
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$BLUE" "Performance Improvements"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    cold_avg=$(awk -F',' 'NR>1 && $1=="cold_build" {sum+=$3; count++} END {print sum/count}' "$OUTPUT")
    warm_avg=$(awk -F',' 'NR>1 && $1=="warm_build" {sum+=$3; count++} END {print sum/count}' "$OUTPUT")
    source_avg=$(awk -F',' 'NR>1 && $1=="source_change" {sum+=$3; count++} END {print sum/count}' "$OUTPUT")
    no_change_avg=$(awk -F',' 'NR>1 && $1=="no_changes" {sum+=$3; count++} END {print sum/count}' "$OUTPUT")

    if [[ -n "$cold_avg" && -n "$warm_avg" ]]; then
        improvement=$(awk "BEGIN {printf \"%.1f\", (1 - $warm_avg/$cold_avg) * 100}")
        log "$GREEN" "Cache improvement: ${improvement}% faster ($cold_avg"s" → $warm_avg"s")"
    fi

    if [[ -n "$cold_avg" && -n "$source_avg" ]]; then
        improvement=$(awk "BEGIN {printf \"%.1f\", (1 - $source_avg/$cold_avg) * 100}")
        log "$GREEN" "Source-only change: ${improvement}% faster ($cold_avg"s" → $source_avg"s")"
    fi

    if [[ -n "$cold_avg" && -n "$no_change_avg" ]]; then
        improvement=$(awk "BEGIN {printf \"%.1f\", (1 - $no_change_avg/$cold_avg) * 100}")
        log "$GREEN" "Full cache hit: ${improvement}% faster ($cold_avg"s" → $no_change_avg"s")"
    fi

    log "$BLUE" ""
    log "$BLUE" "Detailed results saved to: $OUTPUT"
}

# Main execution
main() {
    parse_args "$@"

    # Check dependencies
    if [[ ! -x "./docker/build-multiplatform.sh" ]]; then
        log "$RED" "Error: build-multiplatform.sh not found or not executable"
        exit 1
    fi

    run_benchmarks
    generate_report

    log "$GREEN" ""
    log "$GREEN" "Benchmark completed successfully!"
}

main "$@"
