# RustMQ Authentication and Authorization Design

This document outlines a production-ready security architecture for RustMQ using Mutual TLS (mTLS) for authentication and a multi-level cached Access Control List (ACL) system for authorization.

## Design Principles

1. **Performance First:** Sub-100ns authorization on hot path via multi-level caching
2. **Production Ready:** Complete certificate lifecycle, audit logging, runtime configuration
3. **Cloud Native:** Kubernetes integration, certificate automation, scalable architecture
4. **Security First:** mTLS, certificate revocation, comprehensive audit trails

---

## Architecture Overview

Three-layer security model optimized for cloud-native message queuing:

1. **mTLS Authentication:** Connection-time identity verification with certificate lifecycle management
2. **Multi-Level Authorization:** L1/L2/L3 cache hierarchy achieving 50-100ns lookup performance
3. **Operations Integration:** Hot configuration updates, certificate rotation, audit logging

### Layer 1: mTLS Authentication

#### Architecture
- **CA Hierarchy:** Root CA + intermediate CAs for scalability and security isolation
- **Certificate Distribution:** Kubernetes Secret/ConfigMap integration with auto-reload
- **Identity Extraction:** Principal from certificate CN with group membership support
- **Connection Pooling:** Authenticated connections cached with 0-RTT session resumption

#### Performance Characteristics
- **Initial Handshake:** 2-3ms (first connection)
- **Session Resumption:** 0.1ms (subsequent connections)
- **Certificate Validation:** Cached with background refresh
- **Memory Overhead:** ~1KB per active connection

#### Certificate Lifecycle
- **Automated Rotation:** 30-day advance renewal with zero-downtime switchover
- **Revocation Support:** CRL and OCSP integration with 1-minute propagation
- **Emergency Procedures:** Rapid certificate replacement via controller commands

### Layer 2: Multi-Level Authorization

#### Cache Hierarchy
- **L1 Cache:** Per-connection LRU (10ns lookup, no contention)
- **L2 Cache:** Sharded broker cache (50ns lookup, 32 shards)
- **L3 Cache:** Bloom filter negative cache (20ns rejection)
- **Controller RPC:** Batch fetch fallback (1-5ms, amortized)

#### ACL Rule Engine
- **Flexible Patterns:** Wildcard support (`payments.*`, `user-{id}-*`)
- **Group Membership:** Role-based access with inheritance
- **Conditional Access:** Time windows, IP restrictions, rate limits
- **Audit Integration:** All authorization decisions logged

#### Performance Optimizations
- **String Interning:** 60-80% memory reduction for principals/topics
- **Batch Fetching:** 10-100x RPC reduction via bulk operations
- **Pre-warming:** Cache population on connection establishment
- **Memory Footprint:** 10-50MB per broker (typical workload)

---

## Implementation Architecture

### Certificate Management API

```bash
# CA Operations
rustmq-admin security ca create [--hsm] [--validity-years 10]
rustmq-admin security ca rotate --grace-period 7d
rustmq-admin security ca info [--show-private-key]

# Certificate Lifecycle
rustmq-admin security certs create-broker --host <hostname> [--ip <ip>] [--validity-days 365]
rustmq-admin security certs create-client --principal <name> [--groups <group1,group2>]
rustmq-admin security certs list [--expiring-within 30d] [--format table|json]
rustmq-admin security certs renew --cert-id <id> [--validity-days 365]
rustmq-admin security certs revoke --cert-id <id> --reason <reason>
rustmq-admin security certs inspect --cert-id <id> [--verify]

# ACL Management
rustmq-admin security acl create --principal <name> --resource <topic> --permissions read,write
rustmq-admin security acl list [--principal <name>] [--resource <topic>] [--format table]
rustmq-admin security acl delete --rule-id <id>
rustmq-admin security acl test --principal <name> --resource <topic> --permission read

# Operations
rustmq-admin security audit --type auth [--since 1h] [--principal <name>]
rustmq-admin security status [--broker-id <id>] [--show-cache-stats]
```

### Configuration Schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub tls: TlsConfig,
    pub acl: AclConfig,
    pub audit: AuditConfig,
    pub certificate_management: CertificateManagementConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ca_cert_path: String,
    pub ca_cert_chain_path: Option<String>,
    pub server_cert_path: String,
    pub server_key_path: String,
    pub client_cert_required: bool,
    pub cert_verify_mode: CertVerifyMode,
    pub crl_path: Option<String>,
    pub ocsp_url: Option<String>,
    pub min_tls_version: TlsVersion,
    pub cert_refresh_interval_hours: u64,
    pub cipher_suites: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclConfig {
    pub enabled: bool,
    pub cache_size_mb: usize,
    pub cache_ttl_seconds: u64,
    pub bloom_filter_size: usize,
    pub batch_fetch_size: usize,
    pub enable_audit_logging: bool,
}
```

### Core Components

#### AuthenticationManager
```rust
pub struct AuthenticationManager {
    certificate_store: Arc<RwLock<HashMap<String, CertificateInfo>>>,
    revoked_certificates: Arc<RwLock<HashSet<String>>>,
    ca_chain: Arc<RwLock<Vec<Certificate>>>,
    crl_cache: Arc<RwLock<RevocationList>>,
}
```

#### Multi-Level Authorization Cache
```rust
pub struct AuthorizationManager {
    l1_cache: ThreadLocal<LruCache<AuthKey, AuthResult>>,
    l2_cache: ShardedCache<AuthKey, AuthResult>,
    bloom_filter: Arc<BloomFilter<AuthKey>>,
    controller_client: Arc<dyn ControllerClient>,
    metrics: Arc<AuthMetrics>,
}
```

#### Error Handling Extensions
```rust
#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Authentication failed: {0}")]
    Authentication(String),
    #[error("Authorization denied: {principal} lacks {permission} on {resource}")]
    AuthorizationDenied { principal: String, permission: String, resource: String },
    #[error("Certificate expired: {subject}")]
    CertificateExpired { subject: String },
    #[error("Certificate revoked: {subject}")]
    CertificateRevoked { subject: String },
    #[error("Rate limit exceeded for principal: {0}")]
    RateLimitExceeded(String),
}
```

## Operational Integration

### Runtime Configuration Updates
- Hot certificate reloading without connection disruption
- ACL cache invalidation with graceful degradation
- Security policy updates via `RuntimeConfigManager`

### Kubernetes Integration
- Certificate distribution via Secrets and ConfigMaps
- Pod Security Context with non-root execution
- Service mesh compatibility (Istio, Linkerd)

### Monitoring and Audit
- Prometheus metrics for authentication/authorization events
- Comprehensive audit logging for compliance
- Real-time security event alerting

## Performance Guarantees

- **Authorization Latency:** 50-100ns (cache hit), <5ms (cache miss)
- **Memory Overhead:** 10-50MB per broker (typical workload)
- **Connection Establishment:** 2-3ms initial, 0.1ms with session resumption
- **Certificate Validation:** Cached with background refresh
- **Cache Hit Rate:** >99% in steady state with pre-warming

## Security Compliance

- **Encryption:** TLS 1.3 with strong cipher suites
- **Certificate Management:** Automated rotation, revocation support
- **Audit Logging:** All security events with tamper-evident logs
- **Zero-Trust:** mTLS for all internal and external communication
- **Operational Security:** Secure key storage, emergency procedures
