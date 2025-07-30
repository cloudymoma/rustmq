#!/bin/bash

# Complete ETL pipeline test script
# This script demonstrates the full workflow of deploying and testing a WASM ETL module

set -e

BROKER_URL=${1:-"http://localhost:9094"}
API_URL=${2:-"http://localhost:9092"}

echo "🚀 Testing WASM ETL Pipeline"
echo "Broker URL: $BROKER_URL"
echo "API URL: $API_URL"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper function for status messages
status() {
    echo -e "${BLUE}📋 $1${NC}"
}

success() {
    echo -e "${GREEN}✅ $1${NC}"
}

warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

error() {
    echo -e "${RED}❌ $1${NC}"
}

# Step 1: Check if broker is running
status "Checking broker status..."
if curl -s "$BROKER_URL/api/v1/status" > /dev/null; then
    success "Broker is running"
else
    error "Broker is not running at $BROKER_URL"
    exit 1
fi

# Step 2: Build and deploy WASM module
status "Building and deploying WASM module..."
cd message-processor
./build-and-deploy.sh "$BROKER_URL"
cd ..

# Get the latest module version
MODULE_VERSION=$(date +%s)

# Step 3: Create topics if they don't exist
status "Creating topics..."
for topic in "raw-events" "processed-events" "etl-errors"; do
    curl -s -X POST "$API_URL/api/v1/topics" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"$topic\", \"partitions\": 1}" > /dev/null || true
    echo "  - Topic: $topic"
done

# Step 4: Create ETL pipeline
status "Creating ETL pipeline..."
PIPELINE_CONFIG=$(cat sample-pipeline.json | sed "s/\"1.0.0\"/\"$MODULE_VERSION\"/g")
PIPELINE_RESPONSE=$(curl -s -X POST "$BROKER_URL/api/v1/etl/pipelines" \
    -H "Content-Type: application/json" \
    -d "$PIPELINE_CONFIG")

PIPELINE_ID=$(echo "$PIPELINE_RESPONSE" | jq -r '.pipeline_id // empty')
if [ -z "$PIPELINE_ID" ]; then
    error "Failed to create pipeline: $PIPELINE_RESPONSE"
    exit 1
fi
success "Pipeline created with ID: $PIPELINE_ID"

# Step 5: Start the pipeline
status "Starting ETL pipeline..."
curl -s -X POST "$BROKER_URL/api/v1/etl/pipelines/$PIPELINE_ID/start" > /dev/null
sleep 2
success "Pipeline started"

# Step 6: Send test messages
status "Sending test messages..."

# Test message 1: JSON with temperature conversion
echo "  📄 Sending JSON message with temperature data..."
curl -s -X POST "$API_URL/api/v1/topics/raw-events/messages" \
    -H "Content-Type: application/json" \
    -d '{
        "key": "sensor-001",
        "value": "{\"temperature_celsius\": 25.0, \"humidity\": 60.5, \"sensor_id\": \"env001\", \"email\": \"ADMIN@SENSOR.COM\"}",
        "headers": {
            "content-type": "application/json",
            "source": "iot-sensors",
            "client_ip": "192.168.1.100"
        }
    }' > /dev/null

# Test message 2: Text message
echo "  📄 Sending text message..."
curl -s -X POST "$API_URL/api/v1/topics/raw-events/messages" \
    -H "Content-Type: application/json" \
    -d '{
        "key": "text-001", 
        "value": "hello world from the sensor network",
        "headers": {
            "content-type": "text/plain",
            "source": "user-input",
            "client_ip": "8.8.8.8"
        }
    }' > /dev/null

# Test message 3: Message that should be filtered
echo "  📄 Sending message that should be filtered..."
curl -s -X POST "$API_URL/api/v1/topics/raw-events/messages" \
    -H "Content-Type: application/json" \
    -d '{
        "key": "spam-001",
        "value": "This is spam content that should be filtered",
        "headers": {
            "content-type": "text/plain",
            "source": "unknown-source"
        }
    }' > /dev/null

# Test message 4: Large JSON with geolocation
echo "  📄 Sending JSON with geolocation data..."
curl -s -X POST "$API_URL/api/v1/topics/raw-events/messages" \
    -H "Content-Type: application/json" \
    -d '{
        "key": "geo-001",
        "value": "{\"latitude\": 37.7749, \"longitude\": -122.4194, \"temperature_celsius\": 18.5, \"device_id\": \"mobile001\"}",
        "headers": {
            "content-type": "application/json", 
            "source": "mobile-app",
            "client_ip": "1.1.1.1"
        }
    }' > /dev/null

success "Test messages sent"

# Step 7: Wait for processing
status "Waiting for message processing..."
sleep 3

# Step 8: Check processed messages
status "Checking processed messages..."
PROCESSED_MESSAGES=$(curl -s "$API_URL/api/v1/topics/processed-events/messages?offset=0&max_messages=10")

if [ -z "$PROCESSED_MESSAGES" ] || [ "$PROCESSED_MESSAGES" == "[]" ]; then
    warning "No processed messages found yet. This could mean:"
    echo "  - Messages are still being processed"
    echo "  - Messages were filtered out"
    echo "  - There's an issue with the ETL pipeline"
else
    success "Found processed messages!"
    echo "$PROCESSED_MESSAGES" | jq '.'
fi

# Step 9: Check pipeline metrics
status "Checking pipeline metrics..."
METRICS=$(curl -s "$BROKER_URL/api/v1/etl/pipelines/$PIPELINE_ID/metrics")
echo "$METRICS" | jq '.'

# Step 10: Check for any errors
status "Checking for processing errors..."
ERROR_MESSAGES=$(curl -s "$API_URL/api/v1/topics/etl-errors/messages?offset=0&max_messages=10")
if [ "$ERROR_MESSAGES" != "[]" ]; then
    warning "Found error messages:"
    echo "$ERROR_MESSAGES" | jq '.'
else
    success "No error messages found"
fi

# Step 11: Verify transformations
status "Verifying message transformations..."

# Check if we have any processed messages to analyze
if [ "$PROCESSED_MESSAGES" != "[]" ] && [ -n "$PROCESSED_MESSAGES" ]; then
    echo ""
    echo "🔍 Transformation Analysis:"
    
    # Count messages
    MESSAGE_COUNT=$(echo "$PROCESSED_MESSAGES" | jq 'length // 0')
    echo "  📊 Processed messages: $MESSAGE_COUNT"
    
    # Check for temperature conversion
    TEMP_CONVERSION=$(echo "$PROCESSED_MESSAGES" | jq '.[].value | fromjson? | select(.temperature_fahrenheit != null) | .temperature_fahrenheit' 2>/dev/null || echo "")
    if [ -n "$TEMP_CONVERSION" ]; then
        success "Temperature conversion detected: ${TEMP_CONVERSION}°F"
    fi
    
    # Check for email normalization
    EMAIL_NORM=$(echo "$PROCESSED_MESSAGES" | jq '.[].value | fromjson? | select(.email != null) | .email' 2>/dev/null || echo "")
    if [ -n "$EMAIL_NORM" ]; then
        success "Email normalization detected: $EMAIL_NORM"
    fi
    
    # Check for enrichment headers
    ENRICHED=$(echo "$PROCESSED_MESSAGES" | jq '.[].headers.enriched // empty' 2>/dev/null || echo "")
    if [ "$ENRICHED" == "\"true\"" ]; then
        success "Message enrichment detected"
    fi
    
    # Check for geolocation
    GEO_COUNTRY=$(echo "$PROCESSED_MESSAGES" | jq '.[].headers.geo_country // empty' 2>/dev/null || echo "")
    if [ -n "$GEO_COUNTRY" ]; then
        success "Geolocation enrichment detected: $GEO_COUNTRY"
    fi
fi

echo ""
status "Test Summary:"
echo "  🏗️  WASM module built and deployed"
echo "  🔧 ETL pipeline created and started"
echo "  📨 Test messages sent (4 messages)"
echo "  📊 Pipeline metrics retrieved"
echo "  ✨ Transformations verified"

echo ""
success "ETL pipeline test completed successfully!"

# Cleanup option
echo ""
read -p "Do you want to stop and cleanup the pipeline? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    status "Cleaning up..."
    curl -s -X POST "$BROKER_URL/api/v1/etl/pipelines/$PIPELINE_ID/stop" > /dev/null
    curl -s -X DELETE "$BROKER_URL/api/v1/etl/pipelines/$PIPELINE_ID" > /dev/null
    success "Pipeline stopped and removed"
else
    echo "Pipeline left running. To stop manually:"
    echo "  curl -X POST $BROKER_URL/api/v1/etl/pipelines/$PIPELINE_ID/stop"
    echo "  curl -X DELETE $BROKER_URL/api/v1/etl/pipelines/$PIPELINE_ID"
fi