#!/bin/bash
# Start RustMQ Controller in Development Mode
# Updated with platform-aware feature detection for optimal performance

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Platform detection for optimal features
UNAME_S=$(uname -s)
KERNEL_VERSION=$(uname -r | cut -d. -f1-2)

# Feature flags based on platform (matches Makefile logic)
if [[ "$UNAME_S" == "Linux" ]]; then
    KERNEL_MAJOR=$(echo $KERNEL_VERSION | cut -d. -f1)
    KERNEL_MINOR=$(echo $KERNEL_VERSION | cut -d. -f2)
    if [[ $KERNEL_MAJOR -gt 5 ]] || ([[ $KERNEL_MAJOR -eq 5 ]] && [[ $KERNEL_MINOR -ge 1 ]]); then
        FEATURES="--features io-uring,wasm,moka-cache"
        PLATFORM_INFO="Linux with io-uring support"
    else
        FEATURES="--features wasm,moka-cache"
        PLATFORM_INFO="Linux (no io-uring support)"
    fi
else
    FEATURES="--features wasm,moka-cache"
    PLATFORM_INFO="$UNAME_S (no io-uring support)"
fi

echo "🚀 Starting RustMQ Controller (Development)"
echo "📁 Config: config/controller-dev.toml"
echo "🔐 Certs: certs/"
echo "🖥️  Platform: $PLATFORM_INFO"
echo "⚡ Features: $FEATURES"
echo ""

cargo run --bin rustmq-controller $FEATURES -- --config config/controller-dev.toml
