# RustMQ Kubernetes Encryption Setup

⚠️ **CRITICAL SECURITY REQUIREMENT**: All RustMQ private keys MUST be encrypted. This guide shows how to set up the mandatory encryption password for Kubernetes deployments.

## Quick Start

### 1. Generate Encryption Password

```bash
# Generate a strong 32-character password
export RUSTMQ_KEY_ENCRYPTION_PASSWORD="$(openssl rand -base64 32)"
```

### 2. Create Kubernetes Secret

#### Option A: Direct Secret Creation (Development/Testing)

```bash
# Create the secret directly
kubectl create secret generic rustmq-key-encryption \
  --namespace=rustmq \
  --from-literal=password="${RUSTMQ_KEY_ENCRYPTION_PASSWORD}"
```

#### Option B: Using Secret Manifest (Not Recommended for Production)

```bash
# Edit gke/base/secret-key-encryption.yaml
# Replace REPLACE_WITH_GENERATED_PASSWORD with your generated password

# Apply the secret
kubectl apply -f gke/base/secret-key-encryption.yaml
```

### 3. Deploy RustMQ

```bash
# Deploy using kustomize
kubectl apply -k gke/overlays/production
```

## Production Setup with Google Cloud Secret Manager

### Prerequisites

1. Install External Secrets Operator:
```bash
helm repo add external-secrets https://charts.external-secrets.io
helm install external-secrets external-secrets/external-secrets -n external-secrets-system --create-namespace
```

2. Enable Google Cloud Secret Manager API:
```bash
gcloud services enable secretmanager.googleapis.com
```

### Step 1: Create Secret in Google Cloud

```bash
# Generate password
PASSWORD="$(openssl rand -base64 32)"

# Store in Google Cloud Secret Manager
echo -n "$PASSWORD" | gcloud secrets create rustmq-key-encryption-password \
  --replication-policy="automatic" \
  --data-file=-

# Grant access to GKE service account
PROJECT_ID=$(gcloud config get-value project)
SERVICE_ACCOUNT="rustmq-sa@${PROJECT_ID}.iam.gserviceaccount.com"

gcloud secrets add-iam-policy-binding rustmq-key-encryption-password \
  --member="serviceAccount:${SERVICE_ACCOUNT}" \
  --role="roles/secretmanager.secretAccessor"
```

### Step 2: Create SecretStore

Create `gke/base/secret-store.yaml`:

```yaml
apiVersion: external-secrets.io/v1beta1
kind: SecretStore
metadata:
  name: gcpsm-secret-store
  namespace: rustmq
spec:
  provider:
    gcpsm:
      projectID: "YOUR_PROJECT_ID"
      auth:
        workloadIdentity:
          clusterLocation: us-central1
          clusterName: rustmq-cluster
          serviceAccountRef:
            name: rustmq-controller
```

### Step 3: Configure External Secret

Edit `gke/base/secret-key-encryption.yaml` and uncomment the ExternalSecret section:

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: rustmq-key-encryption
  namespace: rustmq
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: gcpsm-secret-store
    kind: SecretStore
  target:
    name: rustmq-key-encryption
    creationPolicy: Owner
  data:
  - secretKey: password
    remoteRef:
      key: rustmq-key-encryption-password
```

### Step 4: Deploy

```bash
# Apply secret store
kubectl apply -f gke/base/secret-store.yaml

# Deploy RustMQ (ExternalSecret will create the secret automatically)
kubectl apply -k gke/overlays/production
```

## Verification

### Check Secret Exists

```bash
kubectl get secret rustmq-key-encryption -n rustmq
```

### Verify Pods Use Secret

```bash
# Check controller pod
kubectl describe pod -n rustmq -l app=rustmq-controller | grep RUSTMQ_KEY_ENCRYPTION_PASSWORD

# Check broker pod
kubectl describe pod -n rustmq -l app=rustmq-broker | grep RUSTMQ_KEY_ENCRYPTION_PASSWORD
```

### Test Encryption

```bash
# Generate a test certificate and verify it's encrypted
kubectl exec -n rustmq rustmq-controller-0 -- \
  rustmq-admin ca init --cn "Test CA"

# Check logs for encryption confirmation
kubectl logs -n rustmq rustmq-controller-0 | grep "Encrypting private key"
```

## Password Rotation

⚠️ **WARNING**: Rotating the encryption password requires re-encrypting all existing private keys.

### Rotation Procedure

1. **Backup Current Certificates**:
```bash
kubectl exec -n rustmq rustmq-controller-0 -- \
  tar czf /tmp/certs-backup.tar.gz /var/lib/rustmq/certificates

kubectl cp rustmq/rustmq-controller-0:/tmp/certs-backup.tar.gz \
  ./certs-backup-$(date +%Y%m%d).tar.gz
```

2. **Generate New Password**:
```bash
NEW_PASSWORD="$(openssl rand -base64 32)"

# Update in Google Cloud Secret Manager
echo -n "$NEW_PASSWORD" | gcloud secrets versions add rustmq-key-encryption-password \
  --data-file=-
```

3. **Re-encrypt Keys** (Future: use admin tool):
```bash
# This will be implemented in rustmq-admin
kubectl exec -n rustmq rustmq-controller-0 -- \
  rustmq-admin certs rotate-encryption-password \
    --old-password-env RUSTMQ_KEY_ENCRYPTION_PASSWORD \
    --new-password-env NEW_RUSTMQ_KEY_ENCRYPTION_PASSWORD
```

4. **Rolling Restart**:
```bash
# Restart controllers
kubectl rollout restart statefulset/rustmq-controller -n rustmq
kubectl rollout status statefulset/rustmq-controller -n rustmq

# Restart brokers
kubectl rollout restart daemonset/rustmq-broker -n rustmq
kubectl rollout status daemonset/rustmq-broker -n rustmq
```

## Troubleshooting

### Error: "key_encryption_password must be configured"

**Cause**: Secret not found or not mounted correctly.

**Solution**:
```bash
# Verify secret exists
kubectl get secret rustmq-key-encryption -n rustmq

# Check secret content (base64 encoded)
kubectl get secret rustmq-key-encryption -n rustmq -o jsonpath='{.data.password}' | base64 -d

# Verify pod has secret mounted
kubectl describe pod -n rustmq rustmq-controller-0 | grep -A 5 "Environment:"
```

### Error: "Decryption failed - wrong password or corrupted data"

**Cause**: Password changed but keys not re-encrypted.

**Solution**:
1. Restore previous password or
2. Re-encrypt all keys with new password (see Rotation Procedure)

### Secret Not Auto-Created by ExternalSecret

**Cause**: External Secrets Operator not installed or SecretStore misconfigured.

**Solution**:
```bash
# Check External Secrets Operator
kubectl get pods -n external-secrets-system

# Check SecretStore status
kubectl describe secretstore gcpsm-secret-store -n rustmq

# Check ExternalSecret status
kubectl describe externalsecret rustmq-key-encryption -n rustmq

# View ExternalSecret events
kubectl get events -n rustmq --field-selector involvedObject.name=rustmq-key-encryption
```

## Security Best Practices

✅ **DO**:
- Use Google Cloud Secret Manager or similar for production
- Rotate passwords regularly (e.g., every 90 days)
- Backup passwords securely and separately from certificates
- Use External Secrets Operator for automatic secret synchronization
- Monitor for failed decryption attempts in logs
- Use Workload Identity for GKE service account authentication

❌ **DON'T**:
- Commit passwords to version control
- Share passwords via insecure channels
- Reuse passwords across environments
- Store passwords in ConfigMaps (use Secrets only)
- Skip password backup (lost password = lost all keys!)

## Compliance

This encryption implementation meets:
- **PCI DSS 3.4**: Private keys encrypted at rest
- **FIPS 140-2**: AES-256-GCM and PBKDF2-HMAC-SHA256 are FIPS-approved
- **SOC 2**: Encryption of sensitive data at rest
- **GDPR**: Technical measures for data protection

## Additional Resources

- Complete encryption documentation: `docs/ENCRYPTION_CONFIGURATION.md`
- Security architecture: `docs/security/SECURITY_ARCHITECTURE.md`
- External Secrets Operator: https://external-secrets.io/
- GCP Secret Manager: https://cloud.google.com/secret-manager/docs

## Support

For questions about encryption configuration:
- Documentation: https://docs.rustmq.io/security/encryption
- Security: security@rustmq.io
- GitHub Issues: https://github.com/rustmq/rustmq/issues
