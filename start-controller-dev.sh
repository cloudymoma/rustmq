#!/bin/bash
# Start RustMQ Controller in Development Mode
# Updated with platform-aware feature detection for optimal performance

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Feature flags (wasm + moka-cache are the default set)
UNAME_S=$(uname -s)
FEATURES="--features wasm,moka-cache"
PLATFORM_INFO="$UNAME_S"

echo "🚀 Starting RustMQ Controller (Development)"
echo "📁 Config: config/controller-dev.toml"
echo "🔐 Certs: certs/"
echo "🖥️  Platform: $PLATFORM_INFO"
echo "⚡ Features: $FEATURES"
echo ""

cargo run --bin rustmq-controller $FEATURES -- --config config/controller-dev.toml
