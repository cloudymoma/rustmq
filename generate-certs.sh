#!/bin/bash

# Quick certificate generation script for RustMQ
# This is a convenience wrapper that calls the main script in scripts/

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "🔐 RustMQ Certificate Generator"
echo "🚀 Generating development certificates..."
echo ""

# Execute the main certificate generation script
exec "$SCRIPT_DIR/scripts/generate-certs.sh" "$@"