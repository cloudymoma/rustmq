#!/bin/bash
# Start Complete RustMQ Development Cluster

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🚀 Starting RustMQ Development Cluster"
echo "📁 Project: $(pwd)"
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
echo "   Controller: https://127.0.0.1:9642"
echo "   Broker: https://127.0.0.1:9092"
echo ""
echo "📋 Admin Commands:"
echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml cluster status"
echo "   cargo run --bin rustmq-admin -- --config config/admin-dev.toml topic create test-topic"
echo ""
echo "🧪 Test Examples:"
echo "   cargo run --example secure_producer"
echo "   cargo run --example secure_consumer"
echo ""
echo "Press Ctrl+C to stop the cluster"

# Wait for processes
wait $CONTROLLER_PID $BROKER_PID
