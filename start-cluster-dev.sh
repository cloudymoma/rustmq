#!/bin/bash
# Start Complete RustMQ Development Cluster
# Updated with platform-aware feature detection and enhanced monitoring

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
        PLATFORM_INFO="Linux with io-uring support"
    else
        PLATFORM_INFO="Linux (no io-uring support)"
    fi
else
    PLATFORM_INFO="$UNAME_S (no io-uring support)"
fi

echo "🚀 Starting RustMQ Development Cluster"
echo "📁 Project: $(pwd)"
echo "🖥️  Platform: $PLATFORM_INFO"
echo "🔍 Cache: High-performance Moka cache (TinyLFU algorithm)"
echo "🔐 Security: WebPKI integration with fallback support"
echo ""

# Function to cleanup on exit
cleanup() {
    echo "🛑 Shutting down RustMQ cluster..."
    pkill -f "rustmq-controller" || true
    pkill -f "rustmq-broker" || true
    exit 0
}

trap cleanup SIGINT SIGTERM

# Start controller first
echo "🎛️  Starting Controller..."
./start-controller-dev.sh &
CONTROLLER_PID=$!

# Wait a bit for controller to start
sleep 3

# Start broker
echo "🖥️  Starting Broker..."
./start-broker-dev.sh &
BROKER_PID=$!

echo ""
echo "✅ RustMQ Development Cluster Started!"
echo "   Controller HTTP: https://127.0.0.1:9642"
echo "   Controller Raft: 127.0.0.1:9095"
echo "   Broker QUIC:     https://127.0.0.1:9092"
echo "   Broker gRPC:     https://127.0.0.1:9093"
echo "   Metrics:         http://127.0.0.1:8080/metrics"
echo ""
echo "📋 Admin Commands:"
echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml cluster status"
echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml topic create test-topic"
echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml acl list"
echo ""
echo "🧪 Test Examples:"
echo "   cargo run --example secure_producer"
echo "   cargo run --example secure_consumer"
echo ""
echo "📈 Monitoring:"
echo "   curl http://127.0.0.1:8080/metrics    # Prometheus metrics"
echo "   tail -f logs/broker.log              # Broker logs"
echo "   tail -f logs/controller.log          # Controller logs"
echo ""
echo "🔴 Press Ctrl+C to stop the cluster"

# Wait for processes
wait $CONTROLLER_PID $BROKER_PID
