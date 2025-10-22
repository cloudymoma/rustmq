#!/bin/bash
# Entrypoint script for RustMQ Web UI
# Generates runtime configuration and starts Nginx

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Generate runtime configuration file
generate_config() {
    local config_dir="/usr/share/nginx/html/config"
    local config_file="${config_dir}/config.js"

    log_info "Generating runtime configuration..."

    # Ensure config directory exists
    mkdir -p "${config_dir}"

    # Default values
    local api_url="${RUSTMQ_API_URL:-http://rustmq-admin-server:8080}"
    local env="${RUSTMQ_ENV:-production}"
    local refresh_interval="${RUSTMQ_REFRESH_INTERVAL:-5000}"

    # Generate config.js
    cat > "${config_file}" <<EOF
// RustMQ Web UI Runtime Configuration
// Auto-generated at container startup
// DO NOT EDIT - This file is generated from environment variables

window.RUSTMQ_CONFIG = {
  // API endpoint for RustMQ admin server
  apiUrl: '${api_url}',

  // Environment (development, staging, production)
  env: '${env}',

  // Auto-refresh interval for dashboard (milliseconds)
  refreshInterval: ${refresh_interval},

  // Feature flags
  features: {
    autoRefresh: true,
    darkMode: true,
    experimental: ${RUSTMQ_EXPERIMENTAL_FEATURES:-false}
  },

  // Version information
  version: '1.0.0',
  buildTime: '$(date -u +"%Y-%m-%dT%H:%M:%SZ")',

  // Debug mode (enabled in non-production environments)
  debug: ${RUSTMQ_DEBUG:-false}
};

// Log configuration (only in development)
if (window.RUSTMQ_CONFIG.debug) {
  console.log('RustMQ Configuration:', window.RUSTMQ_CONFIG);
}
EOF

    log_info "Configuration generated at ${config_file}"
    log_info "API URL: ${api_url}"
    log_info "Environment: ${env}"
    log_info "Refresh Interval: ${refresh_interval}ms"
}

# Update Nginx configuration with environment variables
update_nginx_config() {
    log_info "Updating Nginx configuration..."

    # Extract host and port from RUSTMQ_API_URL
    local api_host=$(echo "${RUSTMQ_API_URL:-http://rustmq-admin-server:8080}" | sed -E 's|https?://([^:/]+).*|\1|')
    local api_port=$(echo "${RUSTMQ_API_URL:-http://rustmq-admin-server:8080}" | sed -E 's|https?://[^:]+:([0-9]+).*|\1|')

    # Default to 8080 if port not specified
    if [[ ! "${api_port}" =~ ^[0-9]+$ ]]; then
        api_port="8080"
    fi

    export RUSTMQ_API_HOST="${api_host}"
    export RUSTMQ_API_PORT="${api_port}"

    log_info "Upstream API: ${RUSTMQ_API_HOST}:${RUSTMQ_API_PORT}"

    # Replace environment variables in nginx.conf
    envsubst '${NGINX_WORKER_PROCESSES} ${NGINX_WORKER_CONNECTIONS} ${RUSTMQ_API_HOST} ${RUSTMQ_API_PORT}' \
        < /etc/nginx/nginx.conf > /tmp/nginx.conf.tmp

    mv /tmp/nginx.conf.tmp /etc/nginx/nginx.conf

    log_info "Nginx configuration updated"
}

# Validate Nginx configuration
validate_nginx() {
    log_info "Validating Nginx configuration..."

    if nginx -t 2>&1; then
        log_info "Nginx configuration is valid"
        return 0
    else
        log_error "Nginx configuration validation failed"
        return 1
    fi
}

# Health check function
health_check() {
    log_info "Running health check..."

    # Check if required files exist
    if [[ ! -f "/usr/share/nginx/html/index.html" ]]; then
        log_error "index.html not found"
        return 1
    fi

    if [[ ! -f "/usr/share/nginx/html/config/config.js" ]]; then
        log_error "config.js not found"
        return 1
    fi

    log_info "Health check passed"
    return 0
}

# Main entrypoint logic
main() {
    log_info "RustMQ Web UI starting..."
    log_info "Version: 1.0.0"
    log_info "User: $(whoami) (UID: $(id -u))"

    # Generate runtime configuration
    generate_config

    # Update Nginx configuration
    update_nginx_config

    # Validate configuration
    if ! validate_nginx; then
        log_error "Nginx configuration validation failed, exiting"
        exit 1
    fi

    # Run health check
    if ! health_check; then
        log_error "Health check failed, exiting"
        exit 1
    fi

    log_info "Starting Nginx..."

    # Execute the command passed to the entrypoint
    exec "$@"
}

# Run main function
main "$@"
