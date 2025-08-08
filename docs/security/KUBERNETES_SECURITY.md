# RustMQ Kubernetes Security Deployment Guide

This comprehensive guide covers secure deployment of RustMQ in Kubernetes environments, including mTLS configuration, certificate management, secret handling, and security best practices for production deployments.

## Table of Contents

- [Overview](#overview)
- [Prerequisites](#prerequisites)
- [Security Architecture in Kubernetes](#security-architecture-in-kubernetes)
- [Certificate Management](#certificate-management)
- [Secret Management](#secret-management)
- [mTLS Configuration](#mtls-configuration)
- [Network Security](#network-security)
- [RBAC Configuration](#rbac-configuration)
- [Pod Security](#pod-security)
- [Monitoring and Auditing](#monitoring-and-auditing)
- [Production Deployment](#production-deployment)
- [Troubleshooting](#troubleshooting)

## Overview

RustMQ's Kubernetes security deployment provides enterprise-grade security features designed for cloud-native environments:

- **Zero Trust Architecture**: Every pod and service requires valid certificates and authorization
- **Automatic Certificate Management**: Integration with cert-manager for automated certificate lifecycle
- **Secret Management**: Secure handling of certificates and keys using Kubernetes secrets
- **Network Policies**: Microsegmentation and traffic control at the pod level
- **RBAC Integration**: Role-based access control for Kubernetes resources
- **Pod Security Standards**: Comprehensive pod security policies and constraints

### Security Benefits in Kubernetes

- **Automated Certificate Provisioning**: Certificates automatically provisioned for new pods
- **Service Mesh Integration**: Compatible with Istio, Linkerd, and other service meshes
- **Transparent Encryption**: All traffic encrypted without application changes
- **Zero-Downtime Updates**: Security updates without service interruption
- **Compliance Ready**: Meets enterprise security and compliance requirements

## Prerequisites

### Required Components

```bash
# Kubernetes cluster (1.21+)
kubectl version --client

# cert-manager for certificate automation
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.0/cert-manager.yaml

# Optional: Istio service mesh
istioctl install --set values.pilot.env.EXTERNAL_ISTIOD=false

# Optional: External Secrets Operator
helm repo add external-secrets https://charts.external-secrets.io
helm install external-secrets external-secrets/external-secrets -n external-secrets-system --create-namespace
```

### Storage Requirements

```yaml
# Fast SSD storage class for WAL
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: fast-ssd
provisioner: kubernetes.io/gce-pd
parameters:
  type: pd-ssd
  replication-type: regional-pd
allowVolumeExpansion: true
reclaimPolicy: Delete
volumeBindingMode: WaitForFirstConsumer
```

## Security Architecture in Kubernetes

### Pod-to-Pod Communication

```
┌─────────────────────────────────────────────────────────────────┐
│                    Kubernetes Security Layer                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌────────────┐  mTLS   ┌────────────┐  mTLS   ┌────────────┐  │
│  │   Client   │────────▶│   Broker   │────────▶│Controller  │  │
│  │    Pod     │         │    Pod     │         │    Pod     │  │
│  └────────────┘         └────────────┘         └────────────┘  │
│        │                       │                       │       │
│        │                       │                       │       │
│        ▼                       ▼                       ▼       │
│  ┌────────────┐         ┌────────────┐         ┌────────────┐  │
│  │ Certificate│         │ Certificate│         │ Certificate│  │
│  │  Secret    │         │  Secret    │         │  Secret    │  │
│  └────────────┘         └────────────┘         └────────────┘  │
│        │                       │                       │       │
│        └───────────────────────┼───────────────────────┘       │
│                                ▼                               │
│                    ┌─────────────────────┐                     │
│                    │   cert-manager      │                     │
│                    │   (Certificate      │                     │
│                    │   Lifecycle)        │                     │
│                    └─────────────────────┘                     │
│                                │                               │
│                                ▼                               │
│                    ┌─────────────────────┐                     │
│                    │   RustMQ Root CA    │                     │
│                    │   (ClusterIssuer)   │                     │
│                    └─────────────────────┘                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Security Layers

1. **Pod Security**: Security contexts, policies, and constraints
2. **Network Security**: Network policies and service mesh
3. **Certificate Management**: Automated certificate provisioning and rotation
4. **Secret Management**: Secure storage and distribution of credentials
5. **RBAC**: Kubernetes role-based access control
6. **Audit Logging**: Comprehensive security event logging

## Certificate Management

### cert-manager Integration

#### Root CA Setup
```yaml
# Root CA Secret
apiVersion: v1
kind: Secret
metadata:
  name: rustmq-root-ca
  namespace: rustmq
type: kubernetes.io/tls
data:
  tls.crt: LS0tLS1CRUdJTi... # Base64 encoded root CA certificate
  tls.key: LS0tLS1CRUdJTi... # Base64 encoded root CA private key
---
# Root CA ClusterIssuer
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: rustmq-root-ca
spec:
  ca:
    secretName: rustmq-root-ca
```

#### Intermediate CA for Production
```yaml
# Production CA Certificate
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: rustmq-production-ca
  namespace: rustmq
spec:
  secretName: rustmq-production-ca-secret
  issuerRef:
    name: rustmq-root-ca
    kind: ClusterIssuer
  commonName: "RustMQ Production CA"
  isCA: true
  subject:
    organizationalUnits:
      - "Production"
    organizations:
      - "Your Organization"
    countries:
      - "US"
  duration: "8760h" # 1 year
  renewBefore: "720h" # 30 days
---
# Production CA Issuer
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: rustmq-production-ca
  namespace: rustmq
spec:
  ca:
    secretName: rustmq-production-ca-secret
```

### Certificate Templates

#### Broker Certificate Template
```yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: rustmq-broker-cert-template
  namespace: rustmq
spec:
  secretName: rustmq-broker-cert
  issuerRef:
    name: rustmq-production-ca
    kind: Issuer
  commonName: "rustmq-broker.rustmq.svc.cluster.local"
  dnsNames:
    - "rustmq-broker"
    - "rustmq-broker.rustmq"
    - "rustmq-broker.rustmq.svc"
    - "rustmq-broker.rustmq.svc.cluster.local"
    - "*.rustmq-broker.rustmq.svc.cluster.local"
  ipAddresses:
    - "127.0.0.1"
  subject:
    organizationalUnits:
      - "Brokers"
    organizations:
      - "Your Organization"
  usages:
    - digital signature
    - key encipherment
    - server auth
    - client auth
  duration: "8760h" # 1 year
  renewBefore: "720h" # 30 days
```

#### Client Certificate Template
```yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: rustmq-client-cert-template
  namespace: rustmq
spec:
  secretName: rustmq-client-cert
  issuerRef:
    name: rustmq-production-ca
    kind: Issuer
  commonName: "rustmq-client"
  subject:
    organizationalUnits:
      - "Clients"
    organizations:
      - "Your Organization"
  usages:
    - digital signature
    - client auth
  duration: "2160h" # 90 days
  renewBefore: "360h" # 15 days
```

### Automated Certificate Management

#### Certificate Lifecycle Controller
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rustmq-cert-controller
  namespace: rustmq
spec:
  replicas: 1
  selector:
    matchLabels:
      app: rustmq-cert-controller
  template:
    metadata:
      labels:
        app: rustmq-cert-controller
    spec:
      serviceAccountName: rustmq-cert-controller
      containers:
      - name: cert-controller
        image: rustmq/cert-controller:latest
        env:
        - name: RUSTMQ_NAMESPACE
          value: "rustmq"
        - name: CERT_RENEWAL_THRESHOLD
          value: "720h" # 30 days
        - name: CERT_CHECK_INTERVAL
          value: "6h"
        volumeMounts:
        - name: config
          mountPath: /etc/rustmq/config
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "256Mi"
            cpu: "200m"
      volumes:
      - name: config
        configMap:
          name: rustmq-cert-controller-config
```

## Secret Management

### Kubernetes Secrets Best Practices

#### Certificate Secrets Structure
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: rustmq-broker-certs
  namespace: rustmq
  labels:
    app: rustmq
    component: broker
    cert-type: server
type: kubernetes.io/tls
data:
  tls.crt: LS0tLS1CRUdJTi... # Base64 encoded certificate
  tls.key: LS0tLS1CRUdJTi... # Base64 encoded private key
  ca.crt: LS0tLS1CRUdJTi...  # Base64 encoded CA certificate
---
apiVersion: v1
kind: Secret
metadata:
  name: rustmq-client-certs
  namespace: rustmq
  labels:
    app: rustmq
    component: client
    cert-type: client
type: kubernetes.io/tls
data:
  tls.crt: LS0tLS1CRUdJTi...
  tls.key: LS0tLS1CRUdJTi...
  ca.crt: LS0tLS1CRUdJTi...
```

#### Encrypted Secrets with External Secrets Operator
```yaml
apiVersion: external-secrets.io/v1beta1
kind: SecretStore
metadata:
  name: rustmq-vault
  namespace: rustmq
spec:
  provider:
    vault:
      server: "https://vault.company.com"
      path: "secret"
      version: "v2"
      auth:
        kubernetes:
          mountPath: "kubernetes"
          role: "rustmq"
          serviceAccountRef:
            name: "rustmq"
---
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: rustmq-ca-secret
  namespace: rustmq
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: rustmq-vault
    kind: SecretStore
  target:
    name: rustmq-ca-secret
    creationPolicy: Owner
  data:
  - secretKey: ca.crt
    remoteRef:
      key: rustmq/ca
      property: certificate
  - secretKey: ca.key
    remoteRef:
      key: rustmq/ca
      property: private_key
```

### Secret Rotation and Management

#### Automated Secret Rotation
```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: rustmq-secret-rotation
  namespace: rustmq
spec:
  schedule: "0 2 * * 0" # Weekly on Sunday at 2 AM
  jobTemplate:
    spec:
      template:
        spec:
          serviceAccountName: rustmq-secret-manager
          containers:
          - name: secret-rotator
            image: rustmq/secret-rotator:latest
            command:
            - /bin/sh
            - -c
            - |
              # Check certificate expiry
              if /usr/local/bin/check-cert-expiry --threshold 30d; then
                echo "Certificates have sufficient validity"
                exit 0
              fi
              
              # Rotate certificates
              /usr/local/bin/rotate-certificates --namespace rustmq --dry-run=false
              
              # Update deployment annotations to trigger rolling restart
              kubectl patch deployment rustmq-broker -n rustmq -p '{"spec":{"template":{"metadata":{"annotations":{"kubectl.kubernetes.io/restartedAt":"'$(date +%Y%m%dT%H%M%S)'"}}}}}'
            
            env:
            - name: KUBECONFIG
              value: "/etc/kubeconfig/config"
            volumeMounts:
            - name: kubeconfig
              mountPath: /etc/kubeconfig
              readOnly: true
          volumes:
          - name: kubeconfig
            secret:
              secretName: rustmq-secret-manager-kubeconfig
          restartPolicy: OnFailure
```

## mTLS Configuration

### Broker mTLS Configuration

#### Broker StatefulSet with mTLS
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: rustmq-broker
  namespace: rustmq
spec:
  serviceName: rustmq-broker
  replicas: 3
  selector:
    matchLabels:
      app: rustmq-broker
  template:
    metadata:
      labels:
        app: rustmq-broker
      annotations:
        cert-manager.io/inject-ca-from: "rustmq/rustmq-production-ca"
    spec:
      serviceAccountName: rustmq-broker
      securityContext:
        runAsUser: 1000
        runAsGroup: 1000
        fsGroup: 1000
        runAsNonRoot: true
      containers:
      - name: broker
        image: rustmq/broker:latest
        ports:
        - name: quic
          containerPort: 9092
          protocol: TCP
        - name: rpc
          containerPort: 9093
          protocol: TCP
        env:
        - name: RUSTMQ_TLS_ENABLED
          value: "true"
        - name: RUSTMQ_TLS_CERT_PATH
          value: "/etc/rustmq/certs/tls.crt"
        - name: RUSTMQ_TLS_KEY_PATH
          value: "/etc/rustmq/certs/tls.key"
        - name: RUSTMQ_TLS_CA_PATH
          value: "/etc/rustmq/certs/ca.crt"
        - name: RUSTMQ_TLS_CLIENT_CERT_REQUIRED
          value: "true"
        - name: POD_NAME
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: POD_NAMESPACE
          valueFrom:
            fieldRef:
              fieldPath: metadata.namespace
        - name: POD_IP
          valueFrom:
            fieldRef:
              fieldPath: status.podIP
        volumeMounts:
        - name: certs
          mountPath: /etc/rustmq/certs
          readOnly: true
        - name: config
          mountPath: /etc/rustmq/config
        - name: wal-storage
          mountPath: /var/lib/rustmq/wal
        resources:
          requests:
            memory: "2Gi"
            cpu: "1"
          limits:
            memory: "4Gi"
            cpu: "2"
        livenessProbe:
          exec:
            command:
            - /usr/local/bin/rustmq-health-check
            - --endpoint
            - "https://localhost:9093/health"
            - --cert-file
            - "/etc/rustmq/certs/tls.crt"
            - --key-file
            - "/etc/rustmq/certs/tls.key"
            - --ca-file
            - "/etc/rustmq/certs/ca.crt"
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          exec:
            command:
            - /usr/local/bin/rustmq-health-check
            - --endpoint
            - "https://localhost:9093/ready"
            - --cert-file
            - "/etc/rustmq/certs/tls.crt"
            - --key-file
            - "/etc/rustmq/certs/tls.key"
            - --ca-file
            - "/etc/rustmq/certs/ca.crt"
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: certs
        secret:
          secretName: rustmq-broker-certs
          defaultMode: 0600
      - name: config
        configMap:
          name: rustmq-broker-config
  volumeClaimTemplates:
  - metadata:
      name: wal-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 50Gi
```

### Client mTLS Configuration

#### Client Deployment Example
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rustmq-client-app
  namespace: rustmq
spec:
  replicas: 2
  selector:
    matchLabels:
      app: rustmq-client-app
  template:
    metadata:
      labels:
        app: rustmq-client-app
      annotations:
        cert-manager.io/inject-ca-from: "rustmq/rustmq-production-ca"
    spec:
      serviceAccountName: rustmq-client
      securityContext:
        runAsUser: 1001
        runAsGroup: 1001
        fsGroup: 1001
        runAsNonRoot: true
      containers:
      - name: client
        image: your-app/rustmq-client:latest
        env:
        - name: RUSTMQ_BROKER_URL
          value: "rustmq-broker.rustmq.svc.cluster.local:9092"
        - name: RUSTMQ_TLS_ENABLED
          value: "true"
        - name: RUSTMQ_TLS_CERT_PATH
          value: "/etc/rustmq/certs/tls.crt"
        - name: RUSTMQ_TLS_KEY_PATH
          value: "/etc/rustmq/certs/tls.key"
        - name: RUSTMQ_TLS_CA_PATH
          value: "/etc/rustmq/certs/ca.crt"
        volumeMounts:
        - name: certs
          mountPath: /etc/rustmq/certs
          readOnly: true
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
      volumes:
      - name: certs
        secret:
          secretName: rustmq-client-certs
          defaultMode: 0600
```

### Service Configuration

#### Broker Service with mTLS
```yaml
apiVersion: v1
kind: Service
metadata:
  name: rustmq-broker
  namespace: rustmq
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-ssl-cert: "arn:aws:acm:us-west-2:123456789012:certificate/12345678-1234-1234-1234-123456789012"
    service.beta.kubernetes.io/aws-load-balancer-backend-protocol: "ssl"
    service.beta.kubernetes.io/aws-load-balancer-ssl-ports: "9092,9093"
spec:
  type: LoadBalancer
  selector:
    app: rustmq-broker
  ports:
  - name: quic
    port: 9092
    targetPort: 9092
    protocol: TCP
  - name: rpc
    port: 9093
    targetPort: 9093
    protocol: TCP
---
apiVersion: v1
kind: Service
metadata:
  name: rustmq-broker-headless
  namespace: rustmq
spec:
  clusterIP: None
  selector:
    app: rustmq-broker
  ports:
  - name: quic
    port: 9092
    targetPort: 9092
  - name: rpc
    port: 9093
    targetPort: 9093
```

## Network Security

### Network Policies

#### Default Deny All Policy
```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: default-deny-all
  namespace: rustmq
spec:
  podSelector: {}
  policyTypes:
  - Ingress
  - Egress
```

#### Broker Network Policy
```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: rustmq-broker-policy
  namespace: rustmq
spec:
  podSelector:
    matchLabels:
      app: rustmq-broker
  policyTypes:
  - Ingress
  - Egress
  ingress:
  # Allow client connections to QUIC port
  - from:
    - podSelector:
        matchLabels:
          app: rustmq-client
    - namespaceSelector:
        matchLabels:
          name: rustmq-clients
    ports:
    - protocol: TCP
      port: 9092
  # Allow broker-to-broker communication
  - from:
    - podSelector:
        matchLabels:
          app: rustmq-broker
    ports:
    - protocol: TCP
      port: 9093
  # Allow controller communication
  - from:
    - podSelector:
        matchLabels:
          app: rustmq-controller
    ports:
    - protocol: TCP
      port: 9093
  # Allow monitoring
  - from:
    - namespaceSelector:
        matchLabels:
          name: monitoring
    ports:
    - protocol: TCP
      port: 9642
  egress:
  # Allow broker-to-broker communication
  - to:
    - podSelector:
        matchLabels:
          app: rustmq-broker
    ports:
    - protocol: TCP
      port: 9093
  # Allow controller communication
  - to:
    - podSelector:
        matchLabels:
          app: rustmq-controller
    ports:
    - protocol: TCP
      port: 9094
  # Allow DNS
  - to: []
    ports:
    - protocol: UDP
      port: 53
  # Allow object storage access
  - to: []
    ports:
    - protocol: TCP
      port: 443
```

#### Client Network Policy
```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: rustmq-client-policy
  namespace: rustmq
spec:
  podSelector:
    matchLabels:
      app: rustmq-client
  policyTypes:
  - Egress
  egress:
  # Allow connections to brokers
  - to:
    - podSelector:
        matchLabels:
          app: rustmq-broker
    ports:
    - protocol: TCP
      port: 9092
  # Allow DNS
  - to: []
    ports:
    - protocol: UDP
      port: 53
```

### Service Mesh Integration

#### Istio Configuration
```yaml
# Destination Rule for mTLS
apiVersion: networking.istio.io/v1beta1
kind: DestinationRule
metadata:
  name: rustmq-broker-mtls
  namespace: rustmq
spec:
  host: rustmq-broker.rustmq.svc.cluster.local
  trafficPolicy:
    tls:
      mode: MUTUAL
      clientCertificate: /etc/ssl/certs/tls.crt
      privateKey: /etc/ssl/private/tls.key
      caCertificates: /etc/ssl/certs/ca.crt
---
# Virtual Service for load balancing
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: rustmq-broker
  namespace: rustmq
spec:
  hosts:
  - rustmq-broker.rustmq.svc.cluster.local
  tcp:
  - match:
    - port: 9092
    route:
    - destination:
        host: rustmq-broker.rustmq.svc.cluster.local
        port:
          number: 9092
```

#### Service Mesh mTLS Policy
```yaml
apiVersion: security.istio.io/v1beta1
kind: PeerAuthentication
metadata:
  name: rustmq-mtls
  namespace: rustmq
spec:
  selector:
    matchLabels:
      app: rustmq-broker
  mtls:
    mode: STRICT
---
apiVersion: security.istio.io/v1beta1
kind: AuthorizationPolicy
metadata:
  name: rustmq-access-control
  namespace: rustmq
spec:
  selector:
    matchLabels:
      app: rustmq-broker
  rules:
  - from:
    - source:
        principals: ["cluster.local/ns/rustmq/sa/rustmq-client"]
    to:
    - operation:
        ports: ["9092"]
  - from:
    - source:
        principals: ["cluster.local/ns/rustmq/sa/rustmq-broker"]
    to:
    - operation:
        ports: ["9093"]
```

## RBAC Configuration

### Service Accounts

#### Broker Service Account
```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: rustmq-broker
  namespace: rustmq
  labels:
    app: rustmq
    component: broker
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: rustmq-broker
rules:
# Certificate management
- apiGroups: [""]
  resources: ["secrets"]
  verbs: ["get", "list", "watch"]
- apiGroups: ["cert-manager.io"]
  resources: ["certificates"]
  verbs: ["get", "list", "watch"]
# Pod discovery for clustering
- apiGroups: [""]
  resources: ["pods", "endpoints"]
  verbs: ["get", "list", "watch"]
- apiGroups: ["apps"]
  resources: ["statefulsets"]
  verbs: ["get", "list", "watch"]
# Metrics and monitoring
- apiGroups: [""]
  resources: ["events"]
  verbs: ["create"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: rustmq-broker
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: rustmq-broker
subjects:
- kind: ServiceAccount
  name: rustmq-broker
  namespace: rustmq
```

#### Admin Service Account
```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: rustmq-admin
  namespace: rustmq
  labels:
    app: rustmq
    component: admin
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: rustmq-admin
rules:
# Full certificate management
- apiGroups: [""]
  resources: ["secrets"]
  verbs: ["*"]
- apiGroups: ["cert-manager.io"]
  resources: ["certificates", "certificaterequests", "issuers", "clusterissuers"]
  verbs: ["*"]
# Security management
- apiGroups: [""]
  resources: ["configmaps"]
  verbs: ["get", "list", "watch", "create", "update", "patch"]
# Pod and deployment management for updates
- apiGroups: ["apps"]
  resources: ["deployments", "statefulsets"]
  verbs: ["get", "list", "watch", "update", "patch"]
- apiGroups: [""]
  resources: ["pods"]
  verbs: ["get", "list", "watch", "delete"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: rustmq-admin
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: rustmq-admin
subjects:
- kind: ServiceAccount
  name: rustmq-admin
  namespace: rustmq
```

#### Client Service Account
```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: rustmq-client
  namespace: rustmq
  labels:
    app: rustmq
    component: client
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: rustmq-client
  namespace: rustmq
rules:
# Minimal permissions for client operation
- apiGroups: [""]
  resources: ["secrets"]
  verbs: ["get"]
  resourceNames: ["rustmq-client-certs"]
- apiGroups: [""]
  resources: ["configmaps"]
  verbs: ["get"]
  resourceNames: ["rustmq-client-config"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: rustmq-client
  namespace: rustmq
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: Role
  name: rustmq-client
subjects:
- kind: ServiceAccount
  name: rustmq-client
  namespace: rustmq
```

## Pod Security

### Pod Security Standards

#### Pod Security Policy (PSP) or Pod Security Standards (PSS)
```yaml
# Pod Security Standards for namespace
apiVersion: v1
kind: Namespace
metadata:
  name: rustmq
  labels:
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/audit: restricted
    pod-security.kubernetes.io/warn: restricted
```

#### Security Context Configuration
```yaml
# Security context for broker pods
securityContext:
  # Pod-level security context
  runAsUser: 1000
  runAsGroup: 1000
  fsGroup: 1000
  runAsNonRoot: true
  seccompProfile:
    type: RuntimeDefault
  # Container-level security context  
  containers:
  - name: broker
    securityContext:
      allowPrivilegeEscalation: false
      readOnlyRootFilesystem: true
      capabilities:
        drop:
        - ALL
        add:
        - NET_BIND_SERVICE # For binding to privileged ports if needed
      runAsUser: 1000
      runAsGroup: 1000
      runAsNonRoot: true
```

### Pod Disruption Budgets

#### Broker PDB
```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: rustmq-broker-pdb
  namespace: rustmq
spec:
  minAvailable: 2
  selector:
    matchLabels:
      app: rustmq-broker
```

#### Controller PDB
```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: rustmq-controller-pdb
  namespace: rustmq
spec:
  maxUnavailable: 1
  selector:
    matchLabels:
      app: rustmq-controller
```

### Resource Limits and Requests

#### Resource Quotas
```yaml
apiVersion: v1
kind: ResourceQuota
metadata:
  name: rustmq-quota
  namespace: rustmq
spec:
  hard:
    requests.cpu: "8"
    requests.memory: 16Gi
    limits.cpu: "16"
    limits.memory: 32Gi
    persistentvolumeclaims: "10"
    requests.storage: 500Gi
---
apiVersion: v1
kind: LimitRange
metadata:
  name: rustmq-limits
  namespace: rustmq
spec:
  limits:
  - default:
      cpu: "1"
      memory: "2Gi"
    defaultRequest:
      cpu: "500m"
      memory: "1Gi"
    type: Container
  - max:
      cpu: "4"
      memory: "8Gi"
    min:
      cpu: "100m"
      memory: "128Mi"
    type: Container
```

## Monitoring and Auditing

### Security Monitoring

#### Falco Rules for RustMQ
```yaml
# Falco rules for RustMQ security monitoring
apiVersion: v1
kind: ConfigMap
metadata:
  name: falco-rustmq-rules
  namespace: falco
data:
  rustmq_rules.yaml: |
    - rule: RustMQ Unauthorized Network Connection
      desc: Detect unauthorized network connections to RustMQ brokers
      condition: >
        (evt.type = connect and evt.dir = < and
         fd.typechar = 4 and fd.sip != "127.0.0.1" and
         container.image.repository contains "rustmq" and
         not fd.sport in (9092, 9093, 9642))
      output: >
        Unauthorized connection to RustMQ
        (user=%user.name command=%proc.cmdline connection=%fd.name
         container_id=%container.id image=%container.image.repository)
      priority: WARNING
      tags: [network, rustmq]

    - rule: RustMQ Certificate File Access
      desc: Detect unauthorized access to RustMQ certificate files
      condition: >
        (evt.type = open or evt.type = openat) and evt.dir = < and
        fd.name contains "/etc/rustmq/certs" and
        not proc.name in (rustmq-broker, rustmq-controller, rustmq-admin)
      output: >
        Unauthorized access to RustMQ certificates
        (user=%user.name command=%proc.cmdline file=%fd.name
         container_id=%container.id image=%container.image.repository)
      priority: HIGH
      tags: [filesystem, rustmq, security]

    - rule: RustMQ Privilege Escalation
      desc: Detect privilege escalation in RustMQ containers
      condition: >
        spawned_process and container and
        container.image.repository contains "rustmq" and
        proc.name in (su, sudo, setuid)
      output: >
        Privilege escalation in RustMQ container
        (user=%user.name command=%proc.cmdline
         container_id=%container.id image=%container.image.repository)
      priority: CRITICAL
      tags: [process, rustmq, security]
```

#### Prometheus Security Metrics
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: prometheus-rustmq-rules
  namespace: monitoring
data:
  rustmq-security.yml: |
    groups:
    - name: rustmq.security
      rules:
      - alert: RustMQCertificateExpiry
        expr: rustmq_certificate_expiry_days < 30
        for: 1h
        labels:
          severity: warning
        annotations:
          summary: "RustMQ certificate expiring soon"
          description: "Certificate {{ $labels.certificate_id }} expires in {{ $value }} days"

      - alert: RustMQAuthenticationFailures
        expr: rate(rustmq_authentication_failures_total[5m]) > 0.1
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "High RustMQ authentication failure rate"
          description: "Authentication failure rate is {{ $value }}/second"

      - alert: RustMQUnauthorizedAccess
        expr: rate(rustmq_authorization_denied_total[5m]) > 0.05
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High RustMQ authorization denial rate"
          description: "Authorization denial rate is {{ $value }}/second"

      - alert: RustMQTLSHandshakeFailures
        expr: rate(rustmq_tls_handshake_failures_total[5m]) > 0.01
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "RustMQ TLS handshake failures"
          description: "TLS handshake failure rate is {{ $value }}/second"
```

### Audit Logging

#### Kubernetes Audit Policy for RustMQ
```yaml
apiVersion: audit.k8s.io/v1
kind: Policy
rules:
# Audit RustMQ secret access
- level: Metadata
  namespaces: ["rustmq"]
  resources:
  - group: ""
    resources: ["secrets"]
  verbs: ["get", "list", "create", "update", "patch", "delete"]
  
# Audit certificate operations
- level: RequestResponse
  namespaces: ["rustmq"]
  resources:
  - group: "cert-manager.io"
    resources: ["certificates", "certificaterequests"]
  verbs: ["create", "update", "patch", "delete"]

# Audit RBAC changes
- level: RequestResponse
  resources:
  - group: "rbac.authorization.k8s.io"
    resources: ["roles", "rolebindings", "clusterroles", "clusterrolebindings"]
  verbs: ["create", "update", "patch", "delete"]
  namespaces: ["rustmq"]

# Audit pod security context changes
- level: Request
  namespaces: ["rustmq"]
  resources:
  - group: ""
    resources: ["pods"]
  verbs: ["create", "update", "patch"]
```

## Production Deployment

### Complete Production Manifest
```yaml
# Namespace
apiVersion: v1
kind: Namespace
metadata:
  name: rustmq
  labels:
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/audit: restricted
    pod-security.kubernetes.io/warn: restricted
---
# Root CA Secret (manually created from secure storage)
apiVersion: v1
kind: Secret
metadata:
  name: rustmq-root-ca
  namespace: rustmq
type: kubernetes.io/tls
data:
  tls.crt: LS0tLS1CRUdJTi... # Your root CA certificate
  tls.key: LS0tLS1CRUdJTi... # Your root CA private key
---
# Production CA Issuer
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: rustmq-production-ca
  namespace: rustmq
spec:
  ca:
    secretName: rustmq-root-ca
---
# Broker Certificate
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: rustmq-broker-cert
  namespace: rustmq
spec:
  secretName: rustmq-broker-certs
  issuerRef:
    name: rustmq-production-ca
    kind: Issuer
  commonName: "rustmq-broker.rustmq.svc.cluster.local"
  dnsNames:
    - "rustmq-broker"
    - "rustmq-broker.rustmq"
    - "rustmq-broker.rustmq.svc"
    - "rustmq-broker.rustmq.svc.cluster.local"
    - "*.rustmq-broker.rustmq.svc.cluster.local"
  usages:
    - digital signature
    - key encipherment
    - server auth
    - client auth
  duration: "8760h"
  renewBefore: "720h"
---
# Broker Service Account
apiVersion: v1
kind: ServiceAccount
metadata:
  name: rustmq-broker
  namespace: rustmq
---
# Broker ConfigMap
apiVersion: v1
kind: ConfigMap
metadata:
  name: rustmq-broker-config
  namespace: rustmq
data:
  broker.toml: |
    [broker]
    id = "${POD_NAME}"
    rack_id = "${POD_NODE_NAME}"

    [network]
    quic_listen = "0.0.0.0:9092"
    rpc_listen = "0.0.0.0:9093"
    max_connections = 10000

    [security]
    enabled = true
    
    [security.tls]
    enabled = true
    protocol_version = "1.3"
    server_cert_path = "/etc/rustmq/certs/tls.crt"
    server_key_path = "/etc/rustmq/certs/tls.key"
    client_ca_cert_path = "/etc/rustmq/certs/ca.crt"
    client_cert_required = true
    verify_cert_chain = true

    [security.acl]
    enabled = true
    cache_size_mb = 100
    cache_ttl_seconds = 300

    [wal]
    path = "/var/lib/rustmq/wal"
    capacity_bytes = 53687091200
    fsync_on_write = true

    [object_storage]
    storage_type = "S3"
    bucket = "rustmq-data"
    region = "us-west-2"
---
# Broker StatefulSet
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: rustmq-broker
  namespace: rustmq
spec:
  serviceName: rustmq-broker
  replicas: 3
  selector:
    matchLabels:
      app: rustmq-broker
  template:
    metadata:
      labels:
        app: rustmq-broker
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "9642"
        prometheus.io/path: "/metrics"
    spec:
      serviceAccountName: rustmq-broker
      securityContext:
        runAsUser: 1000
        runAsGroup: 1000
        fsGroup: 1000
        runAsNonRoot: true
        seccompProfile:
          type: RuntimeDefault
      affinity:
        podAntiAffinity:
          requiredDuringSchedulingIgnoredDuringExecution:
          - labelSelector:
              matchLabels:
                app: rustmq-broker
            topologyKey: kubernetes.io/hostname
      containers:
      - name: broker
        image: rustmq/broker:latest
        ports:
        - name: quic
          containerPort: 9092
        - name: rpc
          containerPort: 9093
        - name: metrics
          containerPort: 9642
        env:
        - name: POD_NAME
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: POD_NAMESPACE
          valueFrom:
            fieldRef:
              fieldPath: metadata.namespace
        - name: POD_IP
          valueFrom:
            fieldRef:
              fieldPath: status.podIP
        - name: POD_NODE_NAME
          valueFrom:
            fieldRef:
              fieldPath: spec.nodeName
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          capabilities:
            drop:
            - ALL
          runAsUser: 1000
          runAsGroup: 1000
          runAsNonRoot: true
        volumeMounts:
        - name: certs
          mountPath: /etc/rustmq/certs
          readOnly: true
        - name: config
          mountPath: /etc/rustmq/config
          readOnly: true
        - name: wal-storage
          mountPath: /var/lib/rustmq/wal
        - name: tmp
          mountPath: /tmp
        resources:
          requests:
            memory: "4Gi"
            cpu: "2"
          limits:
            memory: "8Gi"
            cpu: "4"
        livenessProbe:
          httpGet:
            path: /health
            port: 9642
            scheme: HTTPS
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 9642
            scheme: HTTPS
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: certs
        secret:
          secretName: rustmq-broker-certs
          defaultMode: 0600
      - name: config
        configMap:
          name: rustmq-broker-config
      - name: tmp
        emptyDir: {}
  volumeClaimTemplates:
  - metadata:
      name: wal-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 50Gi
---
# Broker Service
apiVersion: v1
kind: Service
metadata:
  name: rustmq-broker
  namespace: rustmq
spec:
  type: LoadBalancer
  selector:
    app: rustmq-broker
  ports:
  - name: quic
    port: 9092
    targetPort: 9092
  - name: rpc
    port: 9093
    targetPort: 9093
```

### Deployment Script
```bash
#!/bin/bash
# deploy-rustmq-secure.sh

set -euo pipefail

NAMESPACE="rustmq"
CLUSTER_NAME="production"

echo "Deploying RustMQ with security to Kubernetes cluster: $CLUSTER_NAME"

# Verify prerequisites
echo "Checking prerequisites..."
kubectl get nodes
kubectl get storageclasses fast-ssd
kubectl get namespace cert-manager

# Create namespace if it doesn't exist
kubectl create namespace $NAMESPACE --dry-run=client -o yaml | kubectl apply -f -

# Deploy cert-manager issuers and certificates
echo "Deploying certificate management..."
kubectl apply -f production-ca-setup.yaml

# Wait for certificates to be ready
echo "Waiting for certificates to be issued..."
kubectl wait --for=condition=Ready certificate/rustmq-broker-cert -n $NAMESPACE --timeout=300s

# Deploy RustMQ components
echo "Deploying RustMQ components..."
kubectl apply -f rustmq-production.yaml

# Wait for deployment to be ready
echo "Waiting for deployment to be ready..."
kubectl wait --for=condition=Available deployment/rustmq-broker -n $NAMESPACE --timeout=600s

# Verify security configuration
echo "Verifying security configuration..."
kubectl exec -n $NAMESPACE rustmq-broker-0 -- \
  openssl x509 -in /etc/rustmq/certs/tls.crt -noout -subject -issuer -dates

# Test connectivity
echo "Testing secure connectivity..."
kubectl run test-client --rm -i --tty --image=alpine/curl -- \
  curl -k --cert /etc/rustmq/certs/tls.crt \
       --key /etc/rustmq/certs/tls.key \
       https://rustmq-broker.rustmq.svc.cluster.local:9642/health

echo "RustMQ deployment with security completed successfully!"
```

## Troubleshooting

### Common Issues

#### Certificate Issues
```bash
# Check certificate status
kubectl get certificates -n rustmq

# Check certificate details
kubectl describe certificate rustmq-broker-cert -n rustmq

# Check certificate secret
kubectl get secret rustmq-broker-certs -n rustmq -o yaml

# Verify certificate validity
kubectl exec rustmq-broker-0 -n rustmq -- \
  openssl x509 -in /etc/rustmq/certs/tls.crt -noout -dates
```

#### mTLS Connection Issues
```bash
# Test TLS handshake
kubectl exec rustmq-broker-0 -n rustmq -- \
  openssl s_client -connect localhost:9092 \
  -cert /etc/rustmq/certs/tls.crt \
  -key /etc/rustmq/certs/tls.key \
  -CAfile /etc/rustmq/certs/ca.crt

# Check TLS configuration
kubectl logs rustmq-broker-0 -n rustmq | grep -i tls

# Verify certificate chain
kubectl exec rustmq-broker-0 -n rustmq -- \
  openssl verify -CAfile /etc/rustmq/certs/ca.crt /etc/rustmq/certs/tls.crt
```

#### Network Policy Issues
```bash
# Check network policies
kubectl get networkpolicies -n rustmq

# Test connectivity between pods
kubectl exec rustmq-client-0 -n rustmq -- \
  nc -zv rustmq-broker.rustmq.svc.cluster.local 9092

# Check service endpoints
kubectl get endpoints rustmq-broker -n rustmq
```

#### RBAC Issues
```bash
# Check service account permissions
kubectl auth can-i get secrets --as=system:serviceaccount:rustmq:rustmq-broker -n rustmq

# Check role bindings
kubectl get rolebindings,clusterrolebindings -n rustmq

# Test certificate access
kubectl exec rustmq-broker-0 -n rustmq -- \
  ls -la /etc/rustmq/certs/
```

### Debugging Commands

```bash
# Comprehensive security status check
kubectl exec rustmq-broker-0 -n rustmq -- \
  /usr/local/bin/rustmq-admin security status

# Check pod security context
kubectl get pod rustmq-broker-0 -n rustmq -o jsonpath='{.spec.securityContext}'

# Verify TLS configuration
kubectl exec rustmq-broker-0 -n rustmq -- \
  cat /etc/rustmq/config/broker.toml | grep -A 10 "\[security\]"

# Check certificate expiry
kubectl get certificates -n rustmq \
  -o custom-columns=NAME:.metadata.name,READY:.status.conditions[0].status,AGE:.metadata.creationTimestamp
```

This guide provides comprehensive coverage of secure RustMQ deployment in Kubernetes environments, ensuring enterprise-grade security while maintaining operational simplicity.