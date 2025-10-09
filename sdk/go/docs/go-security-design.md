# Go SDK Security Module Architecture Design

## Overview

This document outlines the design of a comprehensive security module for the RustMQ Go SDK that provides enterprise-grade security features matching the Rust SDK implementation while maintaining Go idioms and performance characteristics.

## ⚠️ Private Key Security

### Server-Side Mandatory Encryption

**IMPORTANT**: RustMQ enforces mandatory private key encryption on the server side. All private keys stored by RustMQ brokers and controllers are encrypted with AES-256-GCM.

**Client Impact**: Go SDK clients do **not** need to encrypt their private keys - this is a server-side security requirement. However, clients should follow these best practices:

1. **Protect Client Private Keys**: Store client private keys securely with appropriate file permissions (0600 on Unix)
2. **Use Secure Storage**: Consider using OS keychains, HashiCorp Vault, or cloud secret managers
3. **Never Commit Keys**: Exclude private keys from version control
4. **Validate Permissions**: Check file permissions before loading keys

**Server Configuration**: The `RUSTMQ_KEY_ENCRYPTION_PASSWORD` environment variable must be set on broker/controller instances (not client applications).

**Example - Secure Client Key Loading in Go**:
```go
package main

import (
    "fmt"
    "io/ioutil"
    "os"
)

// LoadClientKeySecurely loads a private key with permission validation
func LoadClientKeySecurely(path string) ([]byte, error) {
    // Check file permissions (Unix only)
    info, err := os.Stat(path)
    if err != nil {
        return nil, fmt.Errorf("failed to stat key file: %w", err)
    }

    // Verify file is not world-readable or group-readable
    mode := info.Mode()
    if mode&0077 != 0 {
        return nil, fmt.Errorf("key file has overly permissive permissions %v, use chmod 600", mode)
    }

    // Load the key
    return ioutil.ReadFile(path)
}

// Example usage in client configuration
func main() {
    clientKey, err := LoadClientKeySecurely("/path/to/client.key")
    if err != nil {
        panic(err)
    }

    config := &rustmq.SecurityConfig{
        TLS: &rustmq.TLSConfig{
            Mode:       rustmq.TLSModeMutualAuth,
            CACert:     "/path/to/ca.pem",
            ClientCert: "/path/to/client.pem",
            ClientKey:  string(clientKey),
        },
    }
    // ... use config
}
```

**Additional Resources**:
- Server encryption setup: `docs/ENCRYPTION_CONFIGURATION.md`
- Security architecture: `docs/security/SECURITY_ARCHITECTURE.md`
- Kubernetes deployment: `gke/ENCRYPTION_SETUP.md`

## Architecture Components

### 1. Core Security Types (`security/types.go`)

```go
package security

import (
    "crypto/tls"
    "crypto/x509"
    "sync"
    "time"
)

// SecurityConfig holds comprehensive security configuration
type SecurityConfig struct {
    TLS                 *TLSConfig                 `json:"tls,omitempty"`
    Auth                *AuthConfig                `json:"auth,omitempty"`
    ACL                 *ACLClientConfig           `json:"acl,omitempty"`
    PrincipalExtraction *PrincipalExtractionConfig `json:"principal_extraction,omitempty"`
    CertValidation      *CertificateValidationConfig `json:"cert_validation,omitempty"`
}

// TLSMode represents TLS configuration modes
type TLSMode int

const (
    TLSModeDisabled TLSMode = iota
    TLSModeEnabled
    TLSModeMutualAuth
)

// Enhanced TLS configuration
type TLSConfig struct {
    Mode               TLSMode `json:"mode"`
    CACert             string  `json:"ca_cert,omitempty"`
    ClientCert         string  `json:"client_cert,omitempty"`
    ClientKey          string  `json:"client_key,omitempty"`
    ServerName         string  `json:"server_name,omitempty"`
    InsecureSkipVerify bool    `json:"insecure_skip_verify"`
    MinVersion         uint16  `json:"min_version,omitempty"`
    MaxVersion         uint16  `json:"max_version,omitempty"`
    CipherSuites       []uint16 `json:"cipher_suites,omitempty"`
}

// AuthMethod represents authentication methods
type AuthMethod int

const (
    AuthMethodNone AuthMethod = iota
    AuthMethodMTLS
    AuthMethodToken
    AuthMethodSASLPlain
    AuthMethodSASLScramSHA256
    AuthMethodSASLScramSHA512
)

// Enhanced authentication configuration
type AuthConfig struct {
    Method     AuthMethod        `json:"method"`
    Username   string            `json:"username,omitempty"`
    Password   string            `json:"password,omitempty"`
    Token      string            `json:"token,omitempty"`
    SASLConfig *SASLConfig       `json:"sasl_config,omitempty"`
    Properties map[string]string `json:"properties,omitempty"`
}

// SASL configuration for various mechanisms
type SASLConfig struct {
    Mechanism    SASLMechanism `json:"mechanism"`
    Username     string        `json:"username"`
    Password     string        `json:"password"`
    Realm        string        `json:"realm,omitempty"`
    Iterations   int           `json:"iterations,omitempty"` // For SCRAM
}

type SASLMechanism int

const (
    SASLMechanismPlain SASLMechanism = iota
    SASLMechanismScramSHA256
    SASLMechanismScramSHA512
)

// SecurityContext for authenticated requests
type SecurityContext struct {
    Principal       string           `json:"principal"`
    CertificateInfo *CertificateInfo `json:"certificate_info,omitempty"`
    Permissions     *PermissionSet   `json:"permissions"`
    AuthTime        time.Time        `json:"auth_time"`
    ExpiresAt       *time.Time       `json:"expires_at,omitempty"`
    RefreshToken    string           `json:"refresh_token,omitempty"`
    mu              sync.RWMutex     // for thread-safe access
}

// PermissionSet for ACL operations with thread-safe access
type PermissionSet struct {
    ReadTopics  []string `json:"read_topics"`
    WriteTopics []string `json:"write_topics"`
    AdminOps    []string `json:"admin_operations"`
    mu          sync.RWMutex
}

// Certificate information extracted from client certificate
type CertificateInfo struct {
    Subject         string            `json:"subject"`
    Issuer          string            `json:"issuer"`
    SerialNumber    string            `json:"serial_number"`
    NotBefore       time.Time         `json:"not_before"`
    NotAfter        time.Time         `json:"not_after"`
    Fingerprint     string            `json:"fingerprint"`
    Attributes      map[string]string `json:"attributes"`
    Extensions      map[string][]byte `json:"extensions,omitempty"`
    IsCA            bool              `json:"is_ca"`
    KeyUsage        x509.KeyUsage     `json:"key_usage"`
}

// ACL client configuration
type ACLClientConfig struct {
    Enabled            bool          `json:"enabled"`
    ServerEndpoints    []string      `json:"server_endpoints"`
    CacheSize          int           `json:"cache_size"`
    CacheTTLSeconds    int           `json:"cache_ttl_seconds"`
    RequestTimeout     time.Duration `json:"request_timeout"`
    MaxConcurrentReq   int           `json:"max_concurrent_requests"`
    RefreshInterval    time.Duration `json:"refresh_interval"`
    BatchSize          int           `json:"batch_size"`
}

// Principal extraction configuration
type PrincipalExtractionConfig struct {
    UseCommonName     bool   `json:"use_common_name"`
    UseSubjectAltName bool   `json:"use_subject_alt_name"`
    UseOrganization   bool   `json:"use_organization"`
    Normalize         bool   `json:"normalize"`
    CustomExtractor   string `json:"custom_extractor,omitempty"`
}

// Certificate validation configuration
type CertificateValidationConfig struct {
    ValidateChain     bool          `json:"validate_chain"`
    CheckRevocation   bool          `json:"check_revocation"`
    MaxChainLength    int           `json:"max_chain_length"`
    RefreshInterval   time.Duration `json:"refresh_interval"`
    CRLEndpoints      []string      `json:"crl_endpoints"`
    OCSPEndpoints     []string      `json:"ocsp_endpoints"`
    AllowSelfSigned   bool          `json:"allow_self_signed"`
}
```

### 2. Security Manager (`security/manager.go`)

```go
package security

import (
    "context"
    "crypto/tls"
    "fmt"
    "sync"
    "time"
)

// SecurityManager orchestrates all security operations
type SecurityManager struct {
    config               *SecurityConfig
    authProviders        map[AuthMethod]AuthProvider
    principalExtractor   *PrincipalExtractor
    certificateValidator *CertificateValidator
    aclCache             *ACLCache
    metrics              *SecurityMetrics
    
    // Channels for background operations
    refreshCh   chan refreshRequest
    metricsCh   chan metricsEvent
    shutdownCh  chan struct{}
    
    // Worker management
    workers    *sync.WaitGroup
    workerPool chan struct{} // Semaphore for limiting concurrent operations
    
    mu sync.RWMutex
}

type refreshRequest struct {
    principal string
    responseCh chan refreshResponse
}

type refreshResponse struct {
    context *SecurityContext
    err     error
}

type metricsEvent struct {
    eventType string
    data      interface{}
}

// NewSecurityManager creates a new security manager
func NewSecurityManager(config *SecurityConfig) (*SecurityManager, error) {
    if config == nil {
        config = DefaultSecurityConfig()
    }
    
    sm := &SecurityManager{
        config:      config,
        authProviders: make(map[AuthMethod]AuthProvider),
        refreshCh:   make(chan refreshRequest, 100),
        metricsCh:   make(chan metricsEvent, 1000),
        shutdownCh:  make(chan struct{}),
        workers:     &sync.WaitGroup{},
        workerPool:  make(chan struct{}, 50), // Max 50 concurrent operations
    }
    
    // Initialize components
    if err := sm.initializeComponents(); err != nil {
        return nil, fmt.Errorf("failed to initialize security components: %w", err)
    }
    
    // Register authentication providers
    sm.registerAuthProviders()
    
    return sm, nil
}

// initializeComponents initializes all security components
func (sm *SecurityManager) initializeComponents() error {
    var err error
    
    // Initialize principal extractor
    sm.principalExtractor = NewPrincipalExtractor(sm.config.PrincipalExtraction)
    
    // Initialize certificate validator
    sm.certificateValidator, err = NewCertificateValidator(sm.config.CertValidation)
    if err != nil {
        return fmt.Errorf("failed to create certificate validator: %w", err)
    }
    
    // Initialize ACL cache
    sm.aclCache = NewACLCache(sm.config.ACL)
    
    // Initialize security metrics
    sm.metrics = NewSecurityMetrics()
    
    return nil
}

// registerAuthProviders registers all authentication providers
func (sm *SecurityManager) registerAuthProviders() {
    sm.authProviders[AuthMethodMTLS] = NewMTLSAuthProvider(
        sm.principalExtractor,
        sm.certificateValidator,
        sm.metrics,
    )
    
    sm.authProviders[AuthMethodToken] = NewTokenAuthProvider(sm.metrics)
    
    sm.authProviders[AuthMethodSASLPlain] = NewSASLAuthProvider(sm.metrics)
    sm.authProviders[AuthMethodSASLScramSHA256] = NewSASLAuthProvider(sm.metrics)
    sm.authProviders[AuthMethodSASLScramSHA512] = NewSASLAuthProvider(sm.metrics)
}

// Start starts background workers for security operations
func (sm *SecurityManager) Start(ctx context.Context) error {
    // Start background workers
    go sm.contextRefreshWorker(ctx)
    go sm.metricsAggregator(ctx)
    
    // Start ACL cache cleanup
    if err := sm.aclCache.Start(ctx); err != nil {
        return fmt.Errorf("failed to start ACL cache: %w", err)
    }
    
    // Start certificate validator background tasks
    if err := sm.certificateValidator.Start(ctx); err != nil {
        return fmt.Errorf("failed to start certificate validator: %w", err)
    }
    
    return nil
}

// Authenticate performs authentication and returns security context
func (sm *SecurityManager) Authenticate(ctx context.Context, tlsConfig *TLSConfig, authConfig *AuthConfig) (*SecurityContext, error) {
    provider, exists := sm.authProviders[authConfig.Method]
    if !exists {
        return nil, &SecurityError{
            Type:    ErrorTypeConfiguration,
            Message: fmt.Sprintf("unsupported authentication method: %v", authConfig.Method),
        }
    }
    
    // Perform authentication
    secCtx, err := provider.Authenticate(ctx, authConfig, tlsConfig)
    if err != nil {
        sm.metrics.RecordAuthentication(authConfig.Method.String(), false)
        return nil, err
    }
    
    // Get permissions from ACL cache
    if sm.config.ACL.Enabled {
        permissions, err := sm.aclCache.GetPermissions(ctx, secCtx.Principal)
        if err != nil {
            return nil, fmt.Errorf("failed to get permissions: %w", err)
        }
        secCtx.Permissions = permissions
    }
    
    sm.metrics.RecordAuthentication(authConfig.Method.String(), true)
    return secCtx, nil
}

// Authorize checks if the security context allows the requested operation
func (sm *SecurityManager) Authorize(ctx context.Context, secCtx *SecurityContext, operation string, resource string) error {
    if !sm.config.ACL.Enabled {
        return nil // Authorization disabled
    }
    
    if secCtx == nil || secCtx.Permissions == nil {
        return &SecurityError{
            Type:    ErrorTypeAuthorization,
            Message: "no security context or permissions available",
        }
    }
    
    // Check permissions based on operation type
    switch operation {
    case "read":
        if !secCtx.Permissions.CanReadTopic(resource) {
            return &SecurityError{
                Type:    ErrorTypeAuthorization,
                Message: fmt.Sprintf("permission denied: cannot read from topic %s", resource),
                Details: map[string]interface{}{
                    "principal": secCtx.Principal,
                    "operation": operation,
                    "resource":  resource,
                },
            }
        }
    case "write":
        if !secCtx.Permissions.CanWriteTopic(resource) {
            return &SecurityError{
                Type:    ErrorTypeAuthorization,
                Message: fmt.Sprintf("permission denied: cannot write to topic %s", resource),
                Details: map[string]interface{}{
                    "principal": secCtx.Principal,
                    "operation": operation,
                    "resource":  resource,
                },
            }
        }
    case "admin":
        if !secCtx.Permissions.CanAdmin(resource) {
            return &SecurityError{
                Type:    ErrorTypeAuthorization,
                Message: fmt.Sprintf("permission denied: cannot perform admin operation %s", resource),
                Details: map[string]interface{}{
                    "principal": secCtx.Principal,
                    "operation": operation,
                    "resource":  resource,
                },
            }
        }
    default:
        return &SecurityError{
            Type:    ErrorTypeConfiguration,
            Message: fmt.Sprintf("unknown operation: %s", operation),
        }
    }
    
    return nil
}

// CreateTLSConfig creates a TLS configuration for QUIC/HTTP3 connections
func (sm *SecurityManager) CreateTLSConfig(config *TLSConfig) (*tls.Config, error) {
    if config.Mode == TLSModeDisabled {
        return nil, &SecurityError{
            Type:    ErrorTypeConfiguration,
            Message: "TLS is disabled",
        }
    }
    
    tlsConfig := &tls.Config{
        ServerName:         config.ServerName,
        InsecureSkipVerify: config.InsecureSkipVerify,
        MinVersion:         config.MinVersion,
        MaxVersion:         config.MaxVersion,
        CipherSuites:       config.CipherSuites,
    }
    
    // Load CA certificates
    if config.CACert != "" {
        rootCAs, err := sm.loadCACertificates(config.CACert)
        if err != nil {
            return nil, fmt.Errorf("failed to load CA certificates: %w", err)
        }
        tlsConfig.RootCAs = rootCAs
    }
    
    // Configure client authentication for mTLS
    if config.Mode == TLSModeMutualAuth {
        if config.ClientCert == "" || config.ClientKey == "" {
            return nil, &SecurityError{
                Type:    ErrorTypeConfiguration,
                Message: "client certificate and key required for mutual TLS",
            }
        }
        
        cert, err := tls.LoadX509KeyPair(config.ClientCert, config.ClientKey)
        if err != nil {
            return nil, fmt.Errorf("failed to load client certificate: %w", err)
        }
        
        tlsConfig.Certificates = []tls.Certificate{cert}
    }
    
    return tlsConfig, nil
}

// RefreshContext refreshes the security context (useful for token renewal)
func (sm *SecurityManager) RefreshContext(ctx context.Context, secCtx *SecurityContext) (*SecurityContext, error) {
    req := refreshRequest{
        principal:  secCtx.Principal,
        responseCh: make(chan refreshResponse, 1),
    }
    
    select {
    case sm.refreshCh <- req:
        select {
        case resp := <-req.responseCh:
            return resp.context, resp.err
        case <-ctx.Done():
            return nil, ctx.Err()
        }
    case <-ctx.Done():
        return nil, ctx.Err()
    }
}

// GetMetrics returns security metrics
func (sm *SecurityManager) GetMetrics() *SecurityMetrics {
    return sm.metrics
}

// Shutdown gracefully shuts down the security manager
func (sm *SecurityManager) Shutdown(ctx context.Context) error {
    close(sm.shutdownCh)
    
    // Wait for workers to finish with timeout
    done := make(chan struct{})
    go func() {
        sm.workers.Wait()
        close(done)
    }()
    
    select {
    case <-done:
        return nil
    case <-ctx.Done():
        return ctx.Err()
    }
}

// Background worker methods
func (sm *SecurityManager) contextRefreshWorker(ctx context.Context) {
    sm.workers.Add(1)
    defer sm.workers.Done()
    
    for {
        select {
        case req := <-sm.refreshCh:
            // Process context refresh request
            sm.processRefreshRequest(ctx, req)
        case <-sm.shutdownCh:
            return
        case <-ctx.Done():
            return
        }
    }
}

func (sm *SecurityManager) metricsAggregator(ctx context.Context) {
    sm.workers.Add(1)
    defer sm.workers.Done()
    
    ticker := time.NewTicker(10 * time.Second)
    defer ticker.Stop()
    
    for {
        select {
        case event := <-sm.metricsCh:
            sm.processMetricsEvent(event)
        case <-ticker.C:
            sm.metrics.FlushMetrics()
        case <-sm.shutdownCh:
            return
        case <-ctx.Done():
            return
        }
    }
}

// Helper methods
func (sm *SecurityManager) processRefreshRequest(ctx context.Context, req refreshRequest) {
    // TODO: Implement context refresh logic
    req.responseCh <- refreshResponse{
        context: nil,
        err:     fmt.Errorf("context refresh not implemented"),
    }
}

func (sm *SecurityManager) processMetricsEvent(event metricsEvent) {
    // TODO: Process metrics events
}

func (sm *SecurityManager) loadCACertificates(caCertPEM string) (*x509.CertPool, error) {
    certPool := x509.NewCertPool()
    if !certPool.AppendCertsFromPEM([]byte(caCertPEM)) {
        return nil, &SecurityError{
            Type:    ErrorTypeCertificate,
            Message: "failed to parse CA certificate",
        }
    }
    return certPool, nil
}

// RequiresAuthentication checks if authentication is required
func (sm *SecurityManager) RequiresAuthentication() bool {
    return sm.config.Auth.Method != AuthMethodNone
}

// Default configuration factory
func DefaultSecurityConfig() *SecurityConfig {
    return &SecurityConfig{
        TLS: &TLSConfig{
            Mode:               TLSModeEnabled,
            InsecureSkipVerify: false,
            MinVersion:         tls.VersionTLS12,
        },
        Auth: &AuthConfig{
            Method: AuthMethodNone,
        },
        ACL: &ACLClientConfig{
            Enabled:          false,
            CacheSize:        1000,
            CacheTTLSeconds:  300,
            RequestTimeout:   10 * time.Second,
            MaxConcurrentReq: 100,
            RefreshInterval:  5 * time.Minute,
            BatchSize:        50,
        },
        PrincipalExtraction: &PrincipalExtractionConfig{
            UseCommonName:     true,
            UseSubjectAltName: false,
            Normalize:         true,
        },
        CertValidation: &CertificateValidationConfig{
            ValidateChain:   true,
            CheckRevocation: false,
            MaxChainLength:  5,
            RefreshInterval: time.Hour,
        },
    }
}
```

### 3. Authentication Provider Interface (`security/auth/provider.go`)

```go
package auth

import (
    "context"
)

// AuthProvider interface for pluggable authentication
type AuthProvider interface {
    // Authenticate performs authentication and returns security context
    Authenticate(ctx context.Context, authConfig *AuthConfig, tlsConfig *TLSConfig) (*SecurityContext, error)
    
    // SupportsMethod checks if the provider supports the authentication method
    SupportsMethod(method AuthMethod) bool
    
    // RefreshContext refreshes an existing security context (for token renewal)
    RefreshContext(ctx context.Context, secCtx *SecurityContext) (*SecurityContext, error)
    
    // ValidateConfig validates the authentication configuration
    ValidateConfig(authConfig *AuthConfig) error
}

// AuthProviderFactory creates authentication providers
type AuthProviderFactory interface {
    CreateProvider(method AuthMethod) (AuthProvider, error)
}

// BaseAuthProvider provides common functionality for auth providers
type BaseAuthProvider struct {
    metrics *SecurityMetrics
}

func NewBaseAuthProvider(metrics *SecurityMetrics) *BaseAuthProvider {
    return &BaseAuthProvider{
        metrics: metrics,
    }
}

func (b *BaseAuthProvider) recordAuth(method string, success bool) {
    if b.metrics != nil {
        b.metrics.RecordAuthentication(method, success)
    }
}
```

### 4. mTLS Authentication Provider (`security/auth/mtls.go`)

```go
package auth

import (
    "context"
    "crypto/x509"
    "fmt"
)

// MTLSAuthProvider handles mTLS authentication
type MTLSAuthProvider struct {
    *BaseAuthProvider
    principalExtractor   *PrincipalExtractor
    certificateValidator *CertificateValidator
}

// NewMTLSAuthProvider creates a new mTLS authentication provider
func NewMTLSAuthProvider(
    extractor *PrincipalExtractor,
    validator *CertificateValidator,
    metrics *SecurityMetrics,
) *MTLSAuthProvider {
    return &MTLSAuthProvider{
        BaseAuthProvider:     NewBaseAuthProvider(metrics),
        principalExtractor:   extractor,
        certificateValidator: validator,
    }
}

// Authenticate performs mTLS authentication
func (m *MTLSAuthProvider) Authenticate(ctx context.Context, authConfig *AuthConfig, tlsConfig *TLSConfig) (*SecurityContext, error) {
    if tlsConfig.Mode != TLSModeMutualAuth {
        return nil, &SecurityError{
            Type:    ErrorTypeConfiguration,
            Message: "mTLS authentication requires TlsMode::MutualAuth",
        }
    }
    
    // Load and parse client certificate
    cert, err := m.loadClientCertificate(tlsConfig)
    if err != nil {
        m.recordAuth("mtls", false)
        return nil, fmt.Errorf("failed to load client certificate: %w", err)
    }
    
    // Validate certificate
    if err := m.certificateValidator.ValidateCertificate(ctx, cert, tlsConfig); err != nil {
        m.recordAuth("mtls", false)
        return nil, fmt.Errorf("certificate validation failed: %w", err)
    }
    
    // Extract certificate information
    certInfo, err := m.certificateValidator.ExtractCertificateInfo(cert)
    if err != nil {
        m.recordAuth("mtls", false)
        return nil, fmt.Errorf("failed to extract certificate info: %w", err)
    }
    
    // Extract principal
    principal, err := m.principalExtractor.ExtractPrincipal(certInfo)
    if err != nil {
        m.recordAuth("mtls", false)
        return nil, fmt.Errorf("failed to extract principal: %w", err)
    }
    
    m.recordAuth("mtls", true)
    
    return &SecurityContext{
        Principal:       principal,
        CertificateInfo: certInfo,
        Permissions:     NewPermissionSet(), // Will be populated by ACL cache
        AuthTime:        time.Now(),
        ExpiresAt:       &certInfo.NotAfter,
    }, nil
}

// SupportsMethod checks if the provider supports the authentication method
func (m *MTLSAuthProvider) SupportsMethod(method AuthMethod) bool {
    return method == AuthMethodMTLS
}

// RefreshContext refreshes mTLS context (certificate renewal)
func (m *MTLSAuthProvider) RefreshContext(ctx context.Context, secCtx *SecurityContext) (*SecurityContext, error) {
    // For mTLS, refresh means re-validating the certificate
    if secCtx.CertificateInfo == nil {
        return nil, &SecurityError{
            Type:    ErrorTypeConfiguration,
            Message: "no certificate info in security context",
        }
    }
    
    // Check if certificate is still valid
    now := time.Now()
    if now.After(secCtx.CertificateInfo.NotAfter) {
        return nil, &SecurityError{
            Type:    ErrorTypeCertificate,
            Message: "certificate has expired",
        }
    }
    
    // Update auth time
    secCtx.AuthTime = now
    return secCtx, nil
}

// ValidateConfig validates mTLS configuration
func (m *MTLSAuthProvider) ValidateConfig(authConfig *AuthConfig) error {
    if authConfig.Method != AuthMethodMTLS {
        return &SecurityError{
            Type:    ErrorTypeConfiguration,
            Message: "invalid auth method for mTLS provider",
        }
    }
    return nil
}

// loadClientCertificate loads and parses the client certificate
func (m *MTLSAuthProvider) loadClientCertificate(tlsConfig *TLSConfig) (*x509.Certificate, error) {
    if tlsConfig.ClientCert == "" {
        return nil, &SecurityError{
            Type:    ErrorTypeConfiguration,
            Message: "client certificate path required for mTLS",
        }
    }
    
    // Load certificate from file or PEM string
    cert, err := tls.LoadX509KeyPair(tlsConfig.ClientCert, tlsConfig.ClientKey)
    if err != nil {
        return nil, &SecurityError{
            Type:    ErrorTypeCertificate,
            Message: fmt.Sprintf("failed to load certificate: %v", err),
        }
    }
    
    if len(cert.Certificate) == 0 {
        return nil, &SecurityError{
            Type:    ErrorTypeCertificate,
            Message: "no certificates found in certificate file",
        }
    }
    
    // Parse the certificate
    x509Cert, err := x509.ParseCertificate(cert.Certificate[0])
    if err != nil {
        return nil, &SecurityError{
            Type:    ErrorTypeCertificate,
            Message: fmt.Sprintf("failed to parse certificate: %v", err),
        }
    }
    
    return x509Cert, nil
}
```

### 5. Permission Set with Thread Safety (`security/acl/permissions.go`)

```go
package acl

import (
    "regexp"
    "strings"
    "sync"
)

// PermissionSet for ACL operations with thread-safe access
type PermissionSet struct {
    readTopics  []string
    writeTopics []string
    adminOps    []string
    mu          sync.RWMutex
}

// NewPermissionSet creates a new empty permission set
func NewPermissionSet() *PermissionSet {
    return &PermissionSet{
        readTopics:  make([]string, 0),
        writeTopics: make([]string, 0),
        adminOps:    make([]string, 0),
    }
}

// CanReadTopic checks if the permission set allows reading from a topic
func (ps *PermissionSet) CanReadTopic(topic string) bool {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    return ps.matchesAnyPattern(topic, ps.readTopics)
}

// CanWriteTopic checks if the permission set allows writing to a topic
func (ps *PermissionSet) CanWriteTopic(topic string) bool {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    return ps.matchesAnyPattern(topic, ps.writeTopics)
}

// CanAdmin checks if the permission set allows admin operations
func (ps *PermissionSet) CanAdmin(operation string) bool {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    return ps.matchesAnyPattern(operation, ps.adminOps)
}

// AddReadTopic adds a read permission pattern
func (ps *PermissionSet) AddReadTopic(pattern string) {
    ps.mu.Lock()
    defer ps.mu.Unlock()
    
    ps.readTopics = append(ps.readTopics, pattern)
}

// AddWriteTopic adds a write permission pattern
func (ps *PermissionSet) AddWriteTopic(pattern string) {
    ps.mu.Lock()
    defer ps.mu.Unlock()
    
    ps.writeTopics = append(ps.writeTopics, pattern)
}

// AddAdminOp adds an admin operation permission pattern
func (ps *PermissionSet) AddAdminOp(pattern string) {
    ps.mu.Lock()
    defer ps.mu.Unlock()
    
    ps.adminOps = append(ps.adminOps, pattern)
}

// GetReadTopics returns a copy of read topic patterns
func (ps *PermissionSet) GetReadTopics() []string {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    topics := make([]string, len(ps.readTopics))
    copy(topics, ps.readTopics)
    return topics
}

// GetWriteTopics returns a copy of write topic patterns
func (ps *PermissionSet) GetWriteTopics() []string {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    topics := make([]string, len(ps.writeTopics))
    copy(topics, ps.writeTopics)
    return topics
}

// GetAdminOps returns a copy of admin operation patterns
func (ps *PermissionSet) GetAdminOps() []string {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    ops := make([]string, len(ps.adminOps))
    copy(ops, ps.adminOps)
    return ops
}

// Clone creates a deep copy of the permission set
func (ps *PermissionSet) Clone() *PermissionSet {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    clone := &PermissionSet{
        readTopics:  make([]string, len(ps.readTopics)),
        writeTopics: make([]string, len(ps.writeTopics)),
        adminOps:    make([]string, len(ps.adminOps)),
    }
    
    copy(clone.readTopics, ps.readTopics)
    copy(clone.writeTopics, ps.writeTopics)
    copy(clone.adminOps, ps.adminOps)
    
    return clone
}

// Merge combines another permission set into this one
func (ps *PermissionSet) Merge(other *PermissionSet) {
    ps.mu.Lock()
    defer ps.mu.Unlock()
    
    other.mu.RLock()
    defer other.mu.RUnlock()
    
    ps.readTopics = append(ps.readTopics, other.readTopics...)
    ps.writeTopics = append(ps.writeTopics, other.writeTopics...)
    ps.adminOps = append(ps.adminOps, other.adminOps...)
}

// IsEmpty checks if the permission set is empty
func (ps *PermissionSet) IsEmpty() bool {
    ps.mu.RLock()
    defer ps.mu.RUnlock()
    
    return len(ps.readTopics) == 0 && len(ps.writeTopics) == 0 && len(ps.adminOps) == 0
}

// matchesAnyPattern checks if text matches any pattern in the list
func (ps *PermissionSet) matchesAnyPattern(text string, patterns []string) bool {
    for _, pattern := range patterns {
        if ps.matchesPattern(text, pattern) {
            return true
        }
    }
    return false
}

// matchesPattern checks if text matches a permission pattern
func (ps *PermissionSet) matchesPattern(text, pattern string) bool {
    if pattern == "*" {
        return true
    }
    
    if strings.Contains(pattern, "*") {
        // Convert glob pattern to regex
        regexPattern := strings.ReplaceAll(pattern, "*", ".*")
        regexPattern = "^" + regexPattern + "$"
        
        matched, err := regexp.MatchString(regexPattern, text)
        if err != nil {
            return false
        }
        return matched
    }
    
    return text == pattern
}
```

### 6. Integration with Existing Client

```go
// Enhanced ClientConfig in client.go
type ClientConfig struct {
    // ... existing fields ...
    
    // Enhanced security configuration
    Security *SecurityConfig `json:"security,omitempty"`
}

// Enhanced Client with security manager
type Client struct {
    // ... existing fields ...
    
    securityManager *SecurityManager
    securityContext *SecurityContext
}

// Enhanced NewClient with security initialization
func NewClient(config *ClientConfig) (*Client, error) {
    // ... existing logic ...
    
    // Initialize security manager if security config is provided
    if config.Security != nil {
        securityManager, err := NewSecurityManager(config.Security)
        if err != nil {
            return nil, fmt.Errorf("failed to create security manager: %w", err)
        }
        client.securityManager = securityManager
        
        // Start security manager
        if err := securityManager.Start(ctx); err != nil {
            return nil, fmt.Errorf("failed to start security manager: %w", err)
        }
        
        // Perform initial authentication if required
        if securityManager.RequiresAuthentication() {
            secCtx, err := securityManager.Authenticate(ctx, config.TLSConfig, config.Auth)
            if err != nil {
                return nil, fmt.Errorf("authentication failed: %w", err)
            }
            client.securityContext = secCtx
        }
    }
    
    return client, nil
}
```

## Implementation Roadmap

### Stage 1: Core Security Infrastructure
1. Create security package structure
2. Implement core types and configuration
3. Create SecurityManager framework
4. Add basic error handling

### Stage 2: Authentication Providers
1. Implement mTLS authentication provider
2. Implement JWT token authentication provider  
3. Implement SASL authentication providers
4. Add authentication provider registry

### Stage 3: Certificate Management
1. Implement certificate validator
2. Add certificate information extraction
3. Implement CRL/OCSP checking
4. Add certificate lifecycle management

### Stage 4: ACL and Authorization
1. Implement ACL cache with TTL
2. Add gRPC client for ACL server communication
3. Implement permission pattern matching
4. Add authorization middleware

### Stage 5: Security Metrics and Integration
1. Implement comprehensive metrics collection
2. Add Prometheus metrics integration
3. Integrate with existing Go SDK components
4. Add comprehensive testing

## Key Design Principles

1. **Go Idioms**: Use interfaces, composition, and Go concurrency patterns
2. **Thread Safety**: Leverage sync.RWMutex, sync.Map, and channels for safe concurrent access
3. **Performance**: Implement caching, string interning, and request deduplication
4. **Resilience**: Add circuit breakers, retries, and graceful error handling
5. **Testability**: Design for easy mocking and comprehensive testing
6. **Backwards Compatibility**: Ensure existing SDK functionality remains unchanged

This architecture provides enterprise-grade security capabilities that match the Rust SDK implementation while maintaining Go's simplicity and performance characteristics.