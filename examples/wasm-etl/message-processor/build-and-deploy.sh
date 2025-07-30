#!/bin/bash

# Build and deploy script for WASM ETL module
# Usage: ./build-and-deploy.sh [broker-url]

set -e

BROKER_URL=${1:-"http://localhost:9094"}
MODULE_NAME="message-processor"
# Use git commit hash for reproducible versioning
if git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
    MODULE_VERSION=$(git rev-parse --short HEAD)
else
    # Fallback to timestamp if not in git repo
    MODULE_VERSION=$(date +%s)
fi

echo "🔧 Building WASM module..."
cargo build --target wasm32-unknown-unknown --release

echo "📦 Optimizing WASM module..."
if command -v wasm-opt &> /dev/null; then
    wasm-opt --strip-debug --strip-producers -Oz \
        target/wasm32-unknown-unknown/release/message_processor.wasm \
        -o target/wasm32-unknown-unknown/release/message_processor_optimized.wasm
    WASM_FILE="target/wasm32-unknown-unknown/release/message_processor_optimized.wasm"
else
    echo "⚠️  wasm-opt not found, using unoptimized WASM"
    WASM_FILE="target/wasm32-unknown-unknown/release/message_processor.wasm"
fi

echo "📤 Uploading module to RustMQ broker at $BROKER_URL..."
response=$(curl -s -X POST "$BROKER_URL/api/v1/etl/modules" \
    -H "Content-Type: application/octet-stream" \
    -H "X-Module-Name: $MODULE_NAME" \
    -H "X-Module-Version: $MODULE_VERSION" \
    --data-binary "@$WASM_FILE")

echo "Response: $response"

echo "✅ Module uploaded successfully!"
echo "Module ID: $MODULE_NAME-v$MODULE_VERSION"

# Show file size
echo "📊 WASM module size: $(ls -lh $WASM_FILE | awk '{print $5}')"

echo ""
echo "🚀 Next steps:"
echo "1. Create an ETL pipeline configuration"
echo "2. Deploy the pipeline using the admin API"
echo "3. Start processing messages!"
echo ""
echo "Example pipeline creation:"
echo "curl -X POST $BROKER_URL/api/v1/etl/pipelines \\"
echo "  -H 'Content-Type: application/json' \\"
echo "  -d '{"
echo "    \"pipeline_name\": \"message-transformation\","
echo "    \"source_topic\": \"raw-events\","
echo "    \"destination_topic\": \"processed-events\","
echo "    \"module_name\": \"$MODULE_NAME\","
echo "    \"module_version\": \"$MODULE_VERSION\","
echo "    \"enabled\": true"
echo "  }'"