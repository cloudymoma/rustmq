# Private Key Encryption Configuration

⚠️ **MANDATORY SECURITY REQUIREMENT**: All private keys in RustMQ MUST be encrypted with AES-256-GCM. Plaintext private keys are NOT supported.

## Configuration

### Environment Variable (Recommended)

The most secure way to configure the encryption password is via environment variable:

```bash
export RUSTMQ_KEY_ENCRYPTION_PASSWORD="$(openssl rand -base64 32)"
```

### Application Configuration

The `key_encryption_password` field is **REQUIRED** in `EnhancedCertificateManagementConfig`:

```rust
EnhancedCertificateManagementConfig {
    key_encryption_password: std::env::var("RUSTMQ_KEY_ENCRYPTION_PASSWORD")
        .expect("RUSTMQ_KEY_ENCRYPTION_PASSWORD must be set"),
    // ... other fields
}
```

## Password Requirements

### Minimum Requirements
- **Length**: Minimum 16 characters (32+ recommended for production)
- **Entropy**: High entropy password from secure random generator
- **Source**: Environment variable or secrets manager (NEVER hardcoded)

### Generating Strong Passwords

```bash
# Generate 32-character base64 password (recommended)
openssl rand -base64 32

# Generate 64-character hex password (maximum security)
openssl rand -hex 32
```

## Deployment Scenarios

### Docker

```bash
# Generate and store password
export RUSTMQ_KEY_ENCRYPTION_PASSWORD="$(openssl rand -base64 32)"

# Run container with password
docker run \
  -e RUSTMQ_KEY_ENCRYPTION_PASSWORD \
  rustmq/broker:latest
```

### Docker Compose

```yaml
services:
  broker:
    image: rustmq/broker:latest
    environment:
      - RUSTMQ_KEY_ENCRYPTION_PASSWORD=${RUSTMQ_KEY_ENCRYPTION_PASSWORD}
    # Or use Docker secrets:
    secrets:
      - rustmq_key_password

secrets:
  rustmq_key_password:
    external: true
```

### Kubernetes

#### Using Kubernetes Secrets

```bash
# Create secret
kubectl create secret generic rustmq-key-password \
  --from-literal=password="$(openssl rand -base64 32)"
```

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: rustmq-broker
spec:
  containers:
  - name: broker
    image: rustmq/broker:latest
    env:
    - name: RUSTMQ_KEY_ENCRYPTION_PASSWORD
      valueFrom:
        secretKeyRef:
          name: rustmq-key-password
          key: password
```

#### Using External Secrets Operator

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: rustmq-key-password
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: aws-secrets-manager
    kind: SecretStore
  target:
    name: rustmq-key-password
  data:
  - secretKey: password
    remoteRef:
      key: rustmq/production/key-encryption-password
```

### Cloud Secrets Managers

#### AWS Secrets Manager

```bash
# Store password in AWS Secrets Manager
aws secretsmanager create-secret \
  --name rustmq/production/key-encryption-password \
  --secret-string "$(openssl rand -base64 32)"

# Retrieve in application
export RUSTMQ_KEY_ENCRYPTION_PASSWORD=$(aws secretsmanager get-secret-value \
  --secret-id rustmq/production/key-encryption-password \
  --query SecretString \
  --output text)
```

#### Google Cloud Secret Manager

```bash
# Store password
echo -n "$(openssl rand -base64 32)" | \
  gcloud secrets create rustmq-key-encryption-password \
  --data-file=-

# Retrieve in application
export RUSTMQ_KEY_ENCRYPTION_PASSWORD=$(gcloud secrets versions access latest \
  --secret=rustmq-key-encryption-password)
```

#### HashiCorp Vault

```bash
# Store password
vault kv put secret/rustmq/production \
  key_encryption_password="$(openssl rand -base64 32)"

# Retrieve in application
export RUSTMQ_KEY_ENCRYPTION_PASSWORD=$(vault kv get \
  -field=key_encryption_password \
  secret/rustmq/production)
```

## Password Rotation

⚠️ **CRITICAL**: Changing the encryption password requires re-encrypting all private keys.

### Rotation Procedure

1. **Backup Current State**
   ```bash
   # Backup certificate storage
   tar -czf rustmq-certs-backup-$(date +%Y%m%d).tar.gz /var/lib/rustmq/certificates
   ```

2. **Generate New Password**
   ```bash
   NEW_PASSWORD="$(openssl rand -base64 32)"
   ```

3. **Re-encrypt Keys** (Future: use admin tool)
   ```bash
   # This will be implemented in rustmq-admin tool
   rustmq-admin certs rotate-encryption-password \
     --old-password-env RUSTMQ_KEY_ENCRYPTION_PASSWORD \
     --new-password-env NEW_RUSTMQ_KEY_ENCRYPTION_PASSWORD
   ```

4. **Update Secrets Manager**
   ```bash
   # Update in your secrets manager
   aws secretsmanager update-secret \
     --secret-id rustmq/production/key-encryption-password \
     --secret-string "$NEW_PASSWORD"
   ```

5. **Rolling Restart**
   ```bash
   # Kubernetes rolling restart
   kubectl rollout restart deployment/rustmq-broker
   ```

## Security Best Practices

### ✅ DO

- **Use secrets managers** (AWS Secrets Manager, GCP Secret Manager, Vault)
- **Generate passwords** using cryptographically secure random generators
- **Rotate passwords** regularly (e.g., every 90 days for high-security deployments)
- **Backup passwords** securely and separately from certificates
- **Use environment variables** or secrets mounts (never configuration files)
- **Audit access** to encryption passwords
- **Monitor** for failed decryption attempts

### ❌ DON'T

- **Hardcode passwords** in configuration files or source code
- **Commit passwords** to version control systems
- **Share passwords** via insecure channels (email, chat, etc.)
- **Reuse passwords** across environments (dev, staging, prod)
- **Use weak passwords** (dictionary words, predictable patterns)
- **Store passwords** in plain text files
- **Skip password backup** (lost password = lost all private keys!)

## Disaster Recovery

### Password Loss

⚠️ **WARNING**: If the encryption password is lost, **all private keys are irrecoverable**.

**Prevention**:
1. Store password in multiple secure locations
2. Document recovery procedures
3. Maintain offline backup of password (encrypted, in secure location)
4. Implement password escrow for enterprise deployments

### Recovery Procedure

If password is available in backup:
1. Restore password from secure backup
2. Verify password works: attempt to start broker/controller
3. If successful, update active secrets manager
4. Consider password rotation after recovery

## Compliance

This encryption implementation meets:
- **PCI DSS 3.4**: Private keys encrypted at rest
- **FIPS 140-2**: AES-256-GCM and PBKDF2-HMAC-SHA256 are FIPS-approved
- **SOC 2**: Encryption of sensitive data at rest
- **GDPR**: Technical measures for data protection

## Troubleshooting

### Error: "key_encryption_password must be configured"

**Cause**: Environment variable not set or application can't access it.

**Solution**:
```bash
export RUSTMQ_KEY_ENCRYPTION_PASSWORD="your-password-here"
```

### Error: "Private key is not encrypted"

**Cause**: Attempting to load a plaintext key (from migration or old deployment).

**Solution**: All keys must be re-issued with encryption enabled. Plaintext keys are not supported for security reasons.

### Error: "Decryption failed - wrong password or corrupted data"

**Cause**: Password changed or corrupted key file.

**Solution**:
1. Verify correct password is configured
2. Check for recent password changes
3. Restore from backup if key file corrupted

## Migration from Plaintext Keys

⚠️ **NOT SUPPORTED**: RustMQ does not support plaintext private keys.

If migrating from a system with plaintext keys:
1. **Generate new certificates** with encryption enabled
2. **Deploy new certificates** to all services
3. **Remove old certificates** after migration complete

**There is no automatic migration path** from plaintext to encrypted keys for security reasons.

## Example: Complete Production Setup

```bash
#!/bin/bash
set -euo pipefail

# Generate strong password
PASSWORD="$(openssl rand -base64 32)"

# Store in AWS Secrets Manager
aws secretsmanager create-secret \
  --name rustmq/production/key-encryption-password \
  --description "RustMQ private key encryption password" \
  --secret-string "$PASSWORD"

# Create Kubernetes secret from AWS
kubectl create secret generic rustmq-key-password \
  --from-literal=password="$PASSWORD"

# Deploy RustMQ with secret
kubectl apply -f - <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rustmq-broker
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: broker
        image: rustmq/broker:latest
        env:
        - name: RUSTMQ_KEY_ENCRYPTION_PASSWORD
          valueFrom:
            secretKeyRef:
              name: rustmq-key-password
              key: password
EOF

# Verify deployment
kubectl wait --for=condition=ready pod -l app=rustmq-broker --timeout=300s

echo "✅ RustMQ deployed with encrypted private keys"
```

## Support

For questions about encryption configuration:
- Documentation: https://docs.rustmq.io/security/encryption
- Security: security@rustmq.io
- GitHub Issues: https://github.com/rustmq/rustmq/issues
