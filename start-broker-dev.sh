#!/bin/bash
# Start RustMQ Broker in Development Mode

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🚀 Starting RustMQ Broker (Development)"
echo "📁 Config: config/broker-dev.toml"
echo "🔐 Certs: certs/"
echo ""

cargo run --bin rustmq-broker -- --config config/broker-dev.toml
