#!/usr/bin/env bash
#
# RustMQ Configuration Loader with Inheritance
# Phase 2: Configuration Management (October 2025)
#
# This script loads environment configuration with proper inheritance:
# prod.env > common.env > Defaults
#
# Usage:
#   source ./scripts/load-config.sh <environment>
#   OR
#   eval "$(./scripts/load-config.sh <environment>)"
#
# Environment: dev, staging, prod

set -euo pipefail

# Configuration directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GKE_DIR="$(dirname "$SCRIPT_DIR")"
ENV_DIR="${GKE_DIR}/environments"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'  # No Color

# Log function
log() {
    local color=$1
    shift
    echo -e "${color}[load-config]${NC} $*" >&2
}

# Show usage
show_usage() {
    cat << EOF
Usage: source ./scripts/load-config.sh <environment>

Environments:
  dev       - Development environment
  staging   - Staging environment
  prod      - Production environment

Examples:
  source ./scripts/load-config.sh dev
  source ./scripts/load-config.sh prod

The script loads configuration with inheritance:
  <environment>.env > common.env > Defaults

All configuration variables are exported to the environment.
EOF
}

# Validate environment argument
validate_environment() {
    local env=$1

    if [[ ! "$env" =~ ^(dev|staging|prod)$ ]]; then
        log "$RED" "ERROR: Invalid environment '$env'"
        log "$YELLOW" "Valid environments: dev, staging, prod"
        return 1
    fi

    return 0
}

# Check if file exists and is readable
check_file() {
    local file=$1
    local name=$2

    if [[ ! -f "$file" ]]; then
        log "$RED" "ERROR: $name not found: $file"
        return 1
    fi

    if [[ ! -r "$file" ]]; then
        log "$RED" "ERROR: $name not readable: $file"
        return 1
    fi

    return 0
}

# Load a single .env file and export variables
load_env_file() {
    local file=$1
    local name=$2

    if ! check_file "$file" "$name"; then
        return 1
    fi

    log "$BLUE" "Loading $name: $(basename "$file")"

    # Use a temporary file to hold export statements
    local temp_exports=$(mktemp)

    # Parse .env file and generate export statements
    # This handles:
    # - Comments (lines starting with #)
    # - Empty lines
    # - Variable expansion (${VAR})
    # - Quoted values
    while IFS= read -r line || [[ -n "$line" ]]; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// /}" ]] && continue

        # Extract variable name and value
        if [[ "$line" =~ ^[[:space:]]*([A-Z_][A-Z0-9_]*)= ]]; then
            local var_name="${BASH_REMATCH[1]}"
            local var_value="${line#*=}"

            # Evaluate the value (allows ${VAR} expansion)
            # Only export if not already set (allows overrides)
            if [[ -z "${!var_name:-}" ]]; then
                echo "export $var_name=$var_value" >> "$temp_exports"
            fi
        fi
    done < "$file"

    # Source the export statements
    # shellcheck disable=SC1090
    source "$temp_exports"
    rm -f "$temp_exports"

    return 0
}

# Load configuration with inheritance
load_configuration() {
    local environment=$1

    log "$GREEN" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$GREEN" "Loading RustMQ Configuration: $environment"
    log "$GREEN" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Step 1: Load common.env (base configuration)
    if ! load_env_file "${ENV_DIR}/common.env" "Common configuration"; then
        log "$RED" "Failed to load common configuration"
        return 1
    fi

    # Step 2: Load environment-specific file (overrides common)
    local env_file="${ENV_DIR}/${environment}.env"
    if ! load_env_file "$env_file" "Environment configuration ($environment)"; then
        log "$RED" "Failed to load $environment configuration"
        return 1
    fi

    # Step 3: Show loaded configuration summary
    log "$GREEN" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$GREEN" "Configuration Summary"
    log "$GREEN" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "$BLUE" "Environment:        $environment"
    log "$BLUE" "GCP Project:        ${GCP_PROJECT_ID:-<not set>}"
    log "$BLUE" "GCP Region:         ${GCP_REGION:-<not set>}"
    log "$BLUE" "Cluster Name:       ${CLUSTER_NAME:-<not set>}"
    log "$BLUE" "Controller Replicas: ${CONTROLLER_REPLICAS:-<not set>}"
    log "$BLUE" "Broker Min/Max:     ${BROKER_MIN_NODES:-?}/${BROKER_MAX_NODES:-?}"
    log "$BLUE" "GCS Bucket:         ${GCS_BUCKET:-<not set>}"
    log "$BLUE" "Image Tag:          ${IMAGE_TAG:-<not set>}"
    log "$BLUE" "TLS Enabled:        ${TLS_ENABLED:-<not set>}"
    log "$GREEN" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Export environment name for use by other scripts
    export RUSTMQ_ENVIRONMENT="$environment"

    log "$GREEN" "✓ Configuration loaded successfully"
    return 0
}

# Show all loaded variables (for debugging)
show_all_variables() {
    log "$BLUE" "All configuration variables:"
    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Get all exported variables that look like configuration
    env | grep -E '^[A-Z_]' | sort | while IFS='=' read -r name value; do
        # Skip common shell variables
        if [[ ! "$name" =~ ^(PATH|HOME|USER|SHELL|TERM|PWD|OLDPWD|SHLVL|_)$ ]]; then
            printf "%-40s = %s\n" "$name" "$value" >&2
        fi
    done

    log "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# Export configuration as JSON (for other tools)
export_as_json() {
    local output_file=${1:-/dev/stdout}

    # Start JSON object
    echo "{" > "$output_file"

    # Export all config variables as JSON
    local first=true
    env | grep -E '^[A-Z_]' | sort | while IFS='=' read -r name value; do
        # Skip common shell variables
        if [[ ! "$name" =~ ^(PATH|HOME|USER|SHELL|TERM|PWD|OLDPWD|SHLVL|_)$ ]]; then
            if [[ "$first" == "true" ]]; then
                first=false
            else
                echo "," >> "$output_file"
            fi
            # Escape quotes in value
            value="${value//\"/\\\"}"
            printf "  \"%s\": \"%s\"" "$name" "$value" >> "$output_file"
        fi
    done

    # End JSON object
    echo "" >> "$output_file"
    echo "}" >> "$output_file"

    if [[ "$output_file" != "/dev/stdout" ]]; then
        log "$GREEN" "Configuration exported to: $output_file"
    fi
}

# Main function
main() {
    local environment="${1:-}"
    local show_vars="${2:-false}"

    # Check for help flag
    if [[ "$environment" == "--help" || "$environment" == "-h" || -z "$environment" ]]; then
        show_usage
        return 1
    fi

    # Validate environment
    if ! validate_environment "$environment"; then
        return 1
    fi

    # Load configuration
    if ! load_configuration "$environment"; then
        return 1
    fi

    # Show all variables if requested
    if [[ "$show_vars" == "--show-all" || "$show_vars" == "-a" ]]; then
        show_all_variables
    fi

    # Export as JSON if requested
    if [[ "$show_vars" == "--json" ]]; then
        export_as_json
    fi

    return 0
}

# Run main function if script is executed (not sourced)
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
else
    # Script is being sourced, load configuration
    if [[ $# -eq 0 ]]; then
        log "$RED" "ERROR: Environment argument required"
        show_usage
        return 1
    fi

    main "$@"
fi
