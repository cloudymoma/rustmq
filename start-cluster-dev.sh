#!/bin/bash
# Start Complete RustMQ Development Cluster
# Updated with platform-aware feature detection and enhanced monitoring

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Platform info
UNAME_S=$(uname -s)
PLATFORM_INFO="$UNAME_S"

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
