#!/bin/bash
# RustMQ Secret Validation Script
# Validates required secrets exist and are not placeholders
set -euo pipefail

NAMESPACE="${1:-rustmq}"
echo "RustMQ Secret Validation"
echo "========================"
echo "Namespace: $NAMESPACE"
echo ""

# Check if kubectl is available
if ! command -v kubectl &> /dev/null; then
    echo "ERROR: kubectl not found"
    exit 1
fi

FAILED=0

# Required secrets
SECRETS=(
    "rustmq-tls"
    "rustmq-encryption"
    "rustmq-tls-cert"
)

for secret in "${SECRETS[@]}"; do
    if kubectl get secret "$secret" -n "$NAMESPACE" &>/dev/null; then
        echo "OK: $secret exists"
    else
        echo "MISSING: $secret"
        FAILED=1
    fi
done

echo ""

# Check for placeholder values in secrets
if kubectl get secret rustmq-encryption -n "$NAMESPACE" &>/dev/null; then
    VALUE=$(kubectl get secret rustmq-encryption -n "$NAMESPACE" -o jsonpath='{.data.password}' 2>/dev/null | base64 -d 2>/dev/null || echo "")
    if [[ "$VALUE" == *"PLACEHOLDER"* ]] || [[ "$VALUE" == *"changeme"* ]] || [[ -z "$VALUE" ]]; then
        echo "WARNING: rustmq-encryption contains placeholder/empty value"
        FAILED=1
    else
        echo "OK: rustmq-encryption has real value"
    fi
fi

echo ""

# Verify RUSTMQ_KEY_ENCRYPTION_PASSWORD is set in the encryption secret
if kubectl get secret rustmq-encryption -n "$NAMESPACE" &>/dev/null; then
    # Check that the secret has a 'RUSTMQ_KEY_ENCRYPTION_PASSWORD' key
    HAS_KEY=$(kubectl get secret rustmq-encryption -n "$NAMESPACE" -o jsonpath='{.data.RUSTMQ_KEY_ENCRYPTION_PASSWORD}' 2>/dev/null || echo "")
    if [[ -z "$HAS_KEY" ]]; then
        # Fall back to checking the 'password' key (legacy format)
        HAS_KEY=$(kubectl get secret rustmq-encryption -n "$NAMESPACE" -o jsonpath='{.data.password}' 2>/dev/null || echo "")
    fi
    if [[ -z "$HAS_KEY" ]]; then
        echo "WARNING: rustmq-encryption missing RUSTMQ_KEY_ENCRYPTION_PASSWORD key"
        FAILED=1
    else
        echo "OK: rustmq-encryption has encryption password key"
    fi
fi

# Verify TLS certificate secret is not optional in production
if kubectl get secret rustmq-tls-cert -n "$NAMESPACE" &>/dev/null; then
    echo "OK: rustmq-tls-cert exists (required for production TLS)"
else
    echo "WARNING: rustmq-tls-cert not found - TLS will not be available"
    FAILED=1
fi

echo ""
echo "========================"

if [ $FAILED -eq 1 ]; then
    echo "VALIDATION FAILED"
    echo ""
    echo "Create missing secrets:"
    echo "  kubectl create secret tls rustmq-tls --cert=tls.crt --key=tls.key -n $NAMESPACE"
    echo "  kubectl create secret generic rustmq-encryption \\"
    echo "    --from-literal=RUSTMQ_KEY_ENCRYPTION_PASSWORD=\$(openssl rand -base64 32) -n $NAMESPACE"
    echo "  kubectl create secret tls rustmq-tls-cert --cert=broker.crt --key=broker.key -n $NAMESPACE"
    exit 1
fi

echo "ALL SECRETS VALID"
