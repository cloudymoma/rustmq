#!/bin/bash
# RustMQ Container Vulnerability Scanner
# Uses trivy to scan Docker images for CVEs
set -euo pipefail

IMAGES=(
    "rustmq-controller:latest"
    "rustmq-broker:latest"
    "rustmq-admin:latest"
    "rustmq-admin-server:latest"
    "rustmq-bigquery-subscriber:latest"
    "rustmq-webui:latest"
)

echo "RustMQ Container Security Scan"
echo "=============================="
echo ""

# Check if trivy is installed
if ! command -v trivy &> /dev/null; then
    echo "Trivy not installed. Install with:"
    echo "  brew install trivy        # macOS"
    echo "  apt install trivy         # Debian/Ubuntu"
    exit 1
fi

FAILED=0
SCANNED=0

for image in "${IMAGES[@]}"; do
    if ! docker image inspect "$image" &>/dev/null; then
        echo "SKIP: $image (not found locally)"
        continue
    fi

    echo "Scanning: $image"
    if trivy image --severity HIGH,CRITICAL --exit-code 1 --quiet "$image"; then
        echo "  PASS"
    else
        echo "  FAIL: HIGH/CRITICAL vulnerabilities found"
        FAILED=1
    fi
    ((SCANNED++))
done

echo ""
echo "=============================="
echo "Scanned: $SCANNED images"

if [ $FAILED -eq 1 ]; then
    echo "Result: VULNERABILITIES FOUND"
    echo ""
    echo "To see details: trivy image <image-name>"
    exit 1
fi

echo "Result: ALL PASSED"
