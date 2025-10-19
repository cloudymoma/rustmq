# RustMQ Network Architecture

**Phase 3: GKE Infrastructure Optimization** (October 2025)

This directory contains network configuration for RustMQ GKE deployment.

## Network Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Internet                                                    │
└─────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ Google Cloud Armor (DDoS Protection + WAF)                  │
└─────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ GKE Ingress (HTTPS Load Balancer)                           │
│ - TLS Termination                                           │
│ - Path-based routing                                        │
│ - Health checks                                             │
└─────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ Admin Server (ClusterIP Service)                            │
│ - Port 8080                                                 │
│ - REST API                                                  │
└─────────────────────────────────────────────────────────────┘


┌─────────────────────────────────────────────────────────────┐
│ VPC (Private Network)                                       │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Internal Load Balancer (Broker)                        │ │
│  │ - Port 9092 (Client - QUIC)                            │ │
│  │ - Port 9093 (Internal - replication)                   │ │
│  └────────────────────────────────────────────────────────┘ │
│                           │                                  │
│                           ▼                                  │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Broker Pods (5-50 replicas)                            │ │
│  │ - QUIC server on 9092                                  │ │
│  │ - Internal gRPC on 9093                                │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Internal Load Balancer (Controller)                    │ │
│  │ - Port 9094 (gRPC)                                     │ │
│  │ - Port 9095 (Admin)                                    │ │
│  └────────────────────────────────────────────────────────┘ │
│                           │                                  │
│                           ▼                                  │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Controller StatefulSet (3-5 replicas)                  │ │
│  │ - gRPC on 9094                                         │ │
│  │ - Admin API on 9095                                    │ │
│  │ - Raft consensus on 9642                               │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Load Balancer Types

### External HTTPS Load Balancer (Ingress)

**Purpose**: Public access to admin API

**Configuration**:
- Type: GKE Ingress (Google Cloud Load Balancer)
- Protocol: HTTPS (TLS 1.2+)
- Certificate: Google-managed SSL certificate
- DDoS Protection: Cloud Armor
- Backend: rustmq-admin-server ClusterIP Service

**Features**:
- Global anycast IP address
- Automatic SSL certificate provisioning
- HTTP to HTTPS redirect
- Health checks
- Connection draining
- Access logging

**Cost**: ~$18-25/month base + $0.008/GB traffic

### Internal TCP Load Balancer (Broker)

**Purpose**: Client connections to brokers from within VPC

**Configuration**:
- Type: Internal passthrough Load Balancer
- Protocol: TCP (QUIC over UDP mapped to TCP)
- Backend: rustmq-broker Service
- Session Affinity: ClientIP (3-hour timeout)

**Features**:
- Low latency (no SSL termination)
- Session persistence (important for QUIC)
- Health checks on port 9092
- Regional load balancing

**Cost**: ~$10-15/month

### Internal TCP Load Balancer (Controller)

**Purpose**: Broker-to-controller communication

**Configuration**:
- Type: Internal passthrough Load Balancer
- Protocol: TCP (gRPC)
- Backend: rustmq-controller-lb Service

**Features**:
- gRPC-aware load balancing
- Health checks on port 9095
- Connection pooling

**Cost**: ~$10-15/month

## Network Policies

### Default Deny

All traffic is denied by default. Only explicitly allowed traffic is permitted.

```yaml
spec:
  podSelector: {}
  policyTypes:
  - Ingress
  - Egress
```

### Controller Network Policy

**Allowed Ingress**:
- Controller ↔ Controller (Raft consensus)
- Broker → Controller (coordination)
- Admin Server → Controller (management)
- Load Balancer → Controller (health checks)

**Allowed Egress**:
- Controller → Controller
- Controller → DNS (name resolution)
- Controller → GCS (optional, for backups)

### Broker Network Policy

**Allowed Ingress**:
- Broker ↔ Broker (replication)
- Load Balancer → Broker (client connections)

**Allowed Egress**:
- Broker → Controller (coordination)
- Broker → Broker (replication)
- Broker → DNS
- Broker → GCS (message storage)

### Admin Server Network Policy

**Allowed Ingress**:
- Ingress → Admin Server (public API)

**Allowed Egress**:
- Admin Server → Controller (gRPC calls)
- Admin Server → DNS

## Security Features

### Cloud Armor (DDoS Protection + WAF)

**Enabled by default on Ingress**

**Protection Rules**:
1. **Rate Limiting**: Max 1000 requests/minute per IP
2. **Geo-blocking**: Block traffic from specific countries (optional)
3. **SQL Injection Prevention**: Block common SQL injection patterns
4. **XSS Prevention**: Block cross-site scripting attempts
5. **OWASP Top 10**: Protection against common web attacks

**Configuration**:
```yaml
annotations:
  cloud.google.com/armor-config: '{"rustmq-admin-policy": "enabled"}'
```

**Cost**: ~$6/month base + $1 per million requests

### TLS/SSL Configuration

**Managed Certificates**:
- Automatic provisioning via Google
- Auto-renewal before expiration
- TLS 1.2 and 1.3 support
- Strong cipher suites only

**mTLS (Mutual TLS)**:
- Enabled between RustMQ components
- Certificate rotation via secrets management (Phase 2)
- Workload Identity for secure certificate distribution

### Private GKE Cluster

**Node Configuration**:
- Nodes have private IPs only
- No direct internet access from nodes
- Egress via Cloud NAT
- Control plane access via authorized networks

**Benefits**:
- Reduced attack surface
- Compliance with security standards
- Better isolation from public internet

## Network Optimization

### Connection Pooling

**gRPC Connection Pools**:
- Broker → Controller: 10 connections per broker
- Load balancer → Backend: Automatic connection reuse

**Benefits**:
- Reduced connection overhead
- Lower latency (no handshake delay)
- Better resource utilization

### Session Affinity

**QUIC Client Connections**:
- Session affinity enabled (ClientIP)
- 3-hour timeout
- Ensures clients reconnect to same broker

**Benefits**:
- Better cache utilization
- Reduced state synchronization
- Improved client experience

### Regional Deployment

**Single Region** (dev/staging):
- All resources in one region
- Lower latency within region
- Lower cost (no cross-region traffic)

**Multi-Region** (production, optional):
- Controllers distributed across regions
- Brokers in multiple regions
- Cross-region replication

## Monitoring and Observability

### Load Balancer Metrics

**Available in Cloud Monitoring**:
- Request count
- Request latency (P50, P95, P99)
- Error rate (4xx, 5xx)
- Bytes sent/received
- Backend health status

**Alerts**:
```yaml
# High error rate
- alert: HighErrorRate
  expr: rate(loadbalancer_errors_total[5m]) > 0.05
  for: 5m
  labels:
    severity: warning

# Backend unhealthy
- alert: BackendUnhealthy
  expr: loadbalancer_backend_healthy == 0
  for: 2m
  labels:
    severity: critical
```

### Network Policy Auditing

**Enable network policy logging**:
```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  annotations:
    cloud.google.com/enable-logging: "true"
```

**View logs in Cloud Logging**:
```bash
gcloud logging read "resource.type=k8s_pod AND labels.network_policy_name=rustmq-broker"
```

## Troubleshooting

### Ingress Not Working

**Problem**: Cannot access admin API via Ingress

**Diagnosis**:
```bash
# Check Ingress status
kubectl describe ingress rustmq-admin-ingress -n rustmq

# Check backend service
kubectl get backendconfig rustmq-backend-config -n rustmq -o yaml

# Check managed certificate
kubectl get managedcertificate rustmq-admin-cert -n rustmq -o yaml

# Check Load Balancer status in GCP
gcloud compute forwarding-rules list
gcloud compute backend-services list
```

**Common Issues**:
- Managed certificate not provisioned (takes 15-30 minutes)
- DNS not pointing to load balancer IP
- Backend service unhealthy (check pod health)
- Firewall rules blocking traffic

### Load Balancer Health Checks Failing

**Problem**: Backend showing unhealthy in load balancer

**Diagnosis**:
```bash
# Check pod health
kubectl get pods -n rustmq

# Check service endpoints
kubectl get endpoints rustmq-broker -n rustmq

# Test health endpoint directly
kubectl exec rustmq-broker-0 -n rustmq -- curl localhost:9092/health
```

**Solution**:
- Verify liveness/readiness probes are passing
- Check network policy allows health check traffic
- Verify port configuration matches

### Connection Refused

**Problem**: Cannot connect to broker or controller

**Diagnosis**:
```bash
# Check network policies
kubectl get networkpolicies -n rustmq

# Test connectivity from pod
kubectl run test-pod --image=nicolaka/netshoot -it --rm -- /bin/bash
# Inside pod:
nc -zv rustmq-broker 9092
nc -zv rustmq-controller-lb 9094
```

**Solution**:
- Verify network policy allows traffic
- Check service selector matches pod labels
- Verify load balancer has healthy backends

## Performance Tuning

### Latency Optimization

**Regional Deployment**:
- Deploy all resources in same region
- Use regional persistent disks
- Enable VPC-native cluster

**Connection Pooling**:
- Increase connection pool size for high throughput
- Enable HTTP/2 for gRPC

**Load Balancer**:
- Use Internal load balancer for intra-cluster traffic
- Enable session affinity for QUIC connections

### Throughput Optimization

**Network Bandwidth**:
- Use n2-standard or higher machine types (32 Gbps+)
- Enable network performance tier (Premium)
- Use jumbo frames (MTU 8896) for internal traffic

**Load Balancer**:
- Distribute traffic across multiple backends
- Monitor for overloaded backends
- Scale brokers horizontally

## Cost Optimization

### Development ($20-30/month)

- 1 Internal LB (controller) = $10/month
- 1 Internal LB (broker) = $10/month
- No external ingress (optional)
- **Total**: ~$20/month

### Staging ($40-60/month)

- 2 Internal LBs = $20/month
- 1 External HTTPS LB = $18/month
- Cloud Armor = $6/month
- Traffic costs = $10-20/month
- **Total**: ~$54-64/month

### Production ($80-120/month)

- 2 Internal LBs = $20/month
- 1 External HTTPS LB = $18/month
- Cloud Armor = $6/month
- Traffic costs (high) = $40-80/month
- **Total**: ~$84-124/month

### Cost Reduction Tips

- Use Internal load balancers only (no external access)
- Implement Cloud CDN for static content
- Use Standard network tier instead of Premium
- Enable HTTP compression
- Implement traffic throttling

## Best Practices

### ✅ DO

- ✅ Use Cloud Armor for DDoS protection
- ✅ Enable TLS everywhere (mTLS for internal traffic)
- ✅ Implement network policies (default deny)
- ✅ Use Internal load balancers for intra-VPC traffic
- ✅ Enable session affinity for QUIC connections
- ✅ Monitor load balancer metrics
- ✅ Use health checks on all backends
- ✅ Enable connection draining
- ✅ Use VPC-native cluster
- ✅ Implement firewall rules

### ❌ DON'T

- ❌ Expose unnecessary ports publicly
- ❌ Allow all traffic (use network policies)
- ❌ Use insecure protocols (always use TLS)
- ❌ Skip health checks
- ❌ Use ephemeral IPs for production
- ❌ Ignore network policy audit logs
- ❌ Deploy without DDoS protection

## References

- [GKE Ingress](https://cloud.google.com/kubernetes-engine/docs/concepts/ingress)
- [Internal Load Balancer](https://cloud.google.com/kubernetes-engine/docs/how-to/internal-load-balancing)
- [Network Policies](https://cloud.google.com/kubernetes-engine/docs/how-to/network-policy)
- [Cloud Armor](https://cloud.google.com/armor/docs)
- [Managed Certificates](https://cloud.google.com/kubernetes-engine/docs/how-to/managed-certs)
- [VPC-Native Clusters](https://cloud.google.com/kubernetes-engine/docs/concepts/alias-ips)

---

**Version**: 1.0
**Last Updated**: October 2025
**Phase**: Phase 3 - GKE Infrastructure Optimization
