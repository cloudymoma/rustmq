#!/bin/bash
# Build all WASM test modules
# Usage: ./build-all.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WASM_BINARIES_DIR="$SCRIPT_DIR/../wasm_binaries"

echo "🔨 Building WASM test modules..."
echo ""

# Create output directory
mkdir -p "$WASM_BINARIES_DIR"

# Build each module
for module in simple_transform infinite_loop fuel_exhaustion; do
    echo "Building $module..."
    cd "$SCRIPT_DIR/$module"
    cargo build --target wasm32-unknown-unknown --release --quiet

    # Copy to binaries directory
    cp "target/wasm32-unknown-unknown/release/${module}.wasm" "$WASM_BINARIES_DIR/"

    # Show file size
    size=$(ls -lh "$WASM_BINARIES_DIR/${module}.wasm" | awk '{print $5}')
    echo "  ✅ ${module}.wasm ($size)"
done

echo ""
echo "✅ All WASM modules built successfully!"
echo "📁 Output: $WASM_BINARIES_DIR"
echo ""
echo "To run integration tests:"
echo "  cargo test --test wasm_integration_simple --features wasm"
