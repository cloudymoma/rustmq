# RustMQ Secrets Management with Google Secret Manager

**Phase 2: Configuration Management** (October 2025)

This directory contains secrets management configuration for RustMQ GKE deployment using **Google Cloud native services**.

## Architecture

```
Google Secret Manager (secure storage)
    ↓ (Workload Identity - keyless auth)
External Secrets Operator
    ↓ (syncs every 1 hour)
Kubernetes Secrets
    ↓ (mounted as volumes/env vars)
RustMQ Pods
```

### Google Cloud Native Services

1. **Google Secret Manager** - Secure secret storage
   - Regional replication for availability
   - Automatic encryption at rest
   - Version management
   - IAM-based access control

2. **Workload Identity** - Secure authentication
   - No service account keys needed
   - Kubernetes SA ↔ Google SA binding
   - Principle of least privilege

3. **External Secrets Operator** - Kubernetes integration
   - Automatic secret synchronization
   - Refresh every hour
   - Declarative secret management

## Files

- **`setup-secrets.sh`** - Script to manage secrets in Google Secret Manager
- **`external-secrets/secret-store.yaml`** - SecretStore configuration
- **`external-secrets/external-secrets.yaml`** - ExternalSecret resources

## Quick Start

### 1. Create Secrets in Google Secret Manager

```bash
# Create all secrets (with placeholders)
cd gke/secrets
./setup-secrets.sh prod create

# List secrets
./setup-secrets.sh prod list

# Grant Workload Identity access
./setup-secrets.sh prod grant-access
```

### 2. Update Secret Values

```bash
# Update TLS certificate
gcloud secrets versions add rustmq-production-tls-cert \
  --data-file=path/to/tls-cert.pem

# Update TLS key
gcloud secrets versions add rustmq-production-tls-key \
  --data-file=path/to/tls-key.pem

# Update CA certificate
gcloud secrets versions add rustmq-production-ca-cert \
  --data-file=path/to/ca-cert.pem

# Update GCS credentials
gcloud secrets versions add rustmq-production-gcs-credentials \
  --data-file=path/to/service-account.json
```

### 3. Install External Secrets Operator

```bash
# Add Helm repository
helm repo add external-secrets https://charts.external-secrets.io

# Install operator
helm install external-secrets \
  external-secrets/external-secrets \
  --namespace external-secrets-system \
  --create-namespace \
  --set installCRDs=true
```

### 4. Apply Secret Configuration

```bash
# Create rustmq namespace
kubectl create namespace rustmq

# Create Kubernetes Service Account
kubectl create serviceaccount rustmq-sa -n rustmq

# Annotate for Workload Identity
kubectl annotate serviceaccount rustmq-sa \
  -n rustmq \
  iam.gke.io/gcp-service-account=rustmq-sa@PROJECT_ID.iam.gserviceaccount.com

# Apply SecretStore and ExternalSecrets
kubectl apply -f external-secrets/secret-store.yaml
kubectl apply -f external-secrets/external-secrets.yaml
```

### 5. Verify Secrets Are Synced

```bash
# Check ExternalSecrets status
kubectl get externalsecrets -n rustmq

# Check if Kubernetes Secrets were created
kubectl get secrets -n rustmq

# View secret details (without revealing values)
kubectl describe secret rustmq-tls-cert -n rustmq
```

## Secrets Managed

### TLS/mTLS Secrets

- **`rustmq-tls-cert`** - TLS certificate for services
  - `tls.crt` - Server certificate
  - `tls.key` - Private key
  - `ca.crt` - CA certificate

### GCS Credentials

- **`rustmq-gcs-credentials`** - Service account for object storage
  - `credentials.json` - GCS service account key

### Admin API

- **`rustmq-admin-api-key`** - Admin API authentication
  - `api-key` - API key for admin operations

## Security Best Practices

### ✅ DO

- ✅ Store secrets in Google Secret Manager
- ✅ Use Workload Identity (no service account keys in cluster)
- ✅ Use regional replication for availability
- ✅ Rotate secrets regularly
- ✅ Use least-privilege IAM roles
- ✅ Enable audit logging for secret access
- ✅ Use different secrets per environment

### ❌ DON'T

- ❌ Store secrets in git (ever!)
- ❌ Store secrets in ConfigMaps
- ❌ Share secrets between environments
- ❌ Use service account keys when Workload Identity is available
- ❌ Give broad secret access (use specific secret names)

## Workload Identity Setup

### 1. Enable Workload Identity on Cluster

```bash
gcloud container clusters update CLUSTER_NAME \
  --workload-pool=PROJECT_ID.svc.id.goog \
  --region=REGION
```

### 2. Create Google Service Account

```bash
gcloud iam service-accounts create rustmq-sa \
  --project=PROJECT_ID \
  --display-name="RustMQ Service Account"
```

### 3. Grant Secret Manager Access

```bash
gcloud projects add-iam-policy-binding PROJECT_ID \
  --member="serviceAccount:rustmq-sa@PROJECT_ID.iam.gserviceaccount.com" \
  --role="roles/secretmanager.secretAccessor"
```

### 4. Bind Kubernetes SA to Google SA

```bash
gcloud iam service-accounts add-iam-policy-binding \
  rustmq-sa@PROJECT_ID.iam.gserviceaccount.com \
  --role roles/iam.workloadIdentityUser \
  --member "serviceAccount:PROJECT_ID.svc.id.goog[rustmq/rustmq-sa]"
```

## Secret Rotation

### Rotating TLS Certificates

```bash
# 1. Generate new certificate
openssl req -x509 -newkey rsa:4096 -keyout new-key.pem -out new-cert.pem -days 365

# 2. Update secret in Secret Manager
gcloud secrets versions add rustmq-production-tls-cert --data-file=new-cert.pem
gcloud secrets versions add rustmq-production-tls-key --data-file=new-key.pem

# 3. External Secrets Operator will sync automatically within 1 hour
# Or force immediate sync:
kubectl delete externalsecret rustmq-tls-cert -n rustmq
kubectl apply -f external-secrets/external-secrets.yaml

# 4. Restart pods to use new secret
kubectl rollout restart deployment/rustmq-broker -n rustmq
kubectl rollout restart statefulset/rustmq-controller -n rustmq
```

## Monitoring Secret Sync

### Check ExternalSecret Status

```bash
# Get all ExternalSecrets
kubectl get externalsecrets -n rustmq

# Describe specific ExternalSecret
kubectl describe externalsecret rustmq-tls-cert -n rustmq

# Check sync status
kubectl get externalsecret rustmq-tls-cert -n rustmq -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}'
```

### Common Issues

**ExternalSecret not syncing:**
```bash
# Check operator logs
kubectl logs -n external-secrets-system deployment/external-secrets -f

# Check Workload Identity binding
gcloud iam service-accounts get-iam-policy \
  rustmq-sa@PROJECT_ID.iam.gserviceaccount.com
```

**Secret not found in Google Secret Manager:**
```bash
# List all secrets
gcloud secrets list --project=PROJECT_ID --filter="labels.environment=prod"

# Check secret exists
gcloud secrets describe SECRET_NAME --project=PROJECT_ID
```

**Permission denied:**
```bash
# Check IAM permissions
gcloud projects get-iam-policy PROJECT_ID \
  --flatten="bindings[].members" \
  --filter="bindings.members:serviceAccount:rustmq-sa@*"
```

## Secret Naming Convention

Secrets follow this naming pattern:
```
rustmq-{environment}-{purpose}

Examples:
- rustmq-production-tls-cert
- rustmq-production-tls-key
- rustmq-production-gcs-credentials
- rustmq-staging-admin-api-key
- rustmq-dev-tls-cert
```

## Cost Optimization

**Google Secret Manager Pricing:**
- $0.06 per 10,000 access operations
- $0.03 per active secret version per month
- $0.01 per regional replica

**Recommendations:**
- Use appropriate refresh intervals (1 hour is usually sufficient)
- Clean up old secret versions
- Use regional (not global) replication for most secrets

## Compliance

Secrets stored in Google Secret Manager provide:
- ✅ Encryption at rest (automatic)
- ✅ Encryption in transit (TLS)
- ✅ Audit logging (Cloud Audit Logs)
- ✅ Access control (IAM)
- ✅ Version history
- ✅ Regional data residency

Meets compliance requirements for:
- SOC 2
- GDPR
- HIPAA (with appropriate configuration)
- PCI-DSS

## References

- [Google Secret Manager Documentation](https://cloud.google.com/secret-manager/docs)
- [Workload Identity Documentation](https://cloud.google.com/kubernetes-engine/docs/how-to/workload-identity)
- [External Secrets Operator](https://external-secrets.io/)
- [GKE Security Best Practices](https://cloud.google.com/kubernetes-engine/docs/how-to/hardening-your-cluster)

---

**Version**: 1.0
**Last Updated**: October 2025
**Phase**: Phase 2 - Configuration Management
