#!/bin/bash
# Start RustMQ Controller in Development Mode

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🚀 Starting RustMQ Controller (Development)"
echo "📁 Config: config/controller-dev.toml"
echo "🔐 Certs: certs/"
echo ""

cargo run --bin rustmq-controller -- --config config/controller-dev.toml
