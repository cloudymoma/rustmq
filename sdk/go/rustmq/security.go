// Package rustmq provides security functionality for the RustMQ Go client
package rustmq

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"sync"
	"time"
)

// SecurityConfig holds comprehensive security configuration
type SecurityConfig struct {
	// Authentication configuration
	Auth *AuthenticationConfig `json:"auth,omitempty"`
	
	// TLS configuration  
	TLS *TLSSecurityConfig `json:"tls,omitempty"`
	
	// ACL configuration
	ACL *ACLConfig `json:"acl,omitempty"`
	
	// Certificate validation configuration
	CertValidation *CertificateValidationConfig `json:"cert_validation,omitempty"`
	
	// Principal extraction configuration
	PrincipalExtraction *PrincipalExtractionConfig `json:"principal_extraction,omitempty"`
	
	// Security metrics configuration
	Metrics *SecurityMetricsConfig `json:"metrics,omitempty"`
}

// AuthenticationConfig holds authentication settings
type AuthenticationConfig struct {
	// Authentication method
	Method AuthenticationMethod `json:"method"`
	
	// Username for SASL authentication
	Username string `json:"username,omitempty"`
	
	// Password for SASL authentication
	Password string `json:"password,omitempty"`
	
	// Token for JWT authentication
	Token string `json:"token,omitempty"`
	
	// Additional properties for authentication
	Properties map[string]string `json:"properties,omitempty"`
	
	// Token refresh settings
	TokenRefresh *TokenRefreshConfig `json:"token_refresh,omitempty"`
}

// AuthenticationMethod represents authentication methods
type AuthenticationMethod string

const (
	AuthMethodNone        AuthenticationMethod = "none"
	AuthMethodMTLS        AuthenticationMethod = "mtls"
	AuthMethodJWT         AuthenticationMethod = "jwt"
	AuthMethodSASLPlain   AuthenticationMethod = "sasl_plain"
	AuthMethodSASLScram256 AuthenticationMethod = "sasl_scram_256"
	AuthMethodSASLScram512 AuthenticationMethod = "sasl_scram_512"
)

// TokenRefreshConfig holds JWT token refresh settings
type TokenRefreshConfig struct {
	// Enable automatic token refresh
	Enabled bool `json:"enabled"`
	
	// Refresh threshold (refresh when token expires within this duration)
	RefreshThreshold time.Duration `json:"refresh_threshold"`
	
	// Token endpoint for refresh
	RefreshEndpoint string `json:"refresh_endpoint,omitempty"`
	
	// Refresh credentials
	RefreshCredentials map[string]string `json:"refresh_credentials,omitempty"`
}

// TLSSecurityConfig holds comprehensive TLS configuration
type TLSSecurityConfig struct {
	// TLS mode
	Mode TLSMode `json:"mode"`
	
	// CA certificate (PEM format)
	CACert string `json:"ca_cert,omitempty"`
	
	// Client certificate (PEM format)
	ClientCert string `json:"client_cert,omitempty"`
	
	// Client private key (PEM format)
	ClientKey string `json:"client_key,omitempty"`
	
	// Server name for verification
	ServerName string `json:"server_name,omitempty"`
	
	// Skip certificate verification (insecure)
	InsecureSkipVerify bool `json:"insecure_skip_verify"`
	
	// Minimum TLS version
	MinVersion uint16 `json:"min_version"`
	
	// Maximum TLS version
	MaxVersion uint16 `json:"max_version"`
	
	// Cipher suites
	CipherSuites []uint16 `json:"cipher_suites,omitempty"`
	
	// Certificate validation settings
	Validation *CertificateValidationSettings `json:"validation,omitempty"`
}

// TLSMode represents TLS operation modes
type TLSMode string

const (
	TLSModeDisabled   TLSMode = "disabled"
	TLSModeEnabled    TLSMode = "enabled"
	TLSModeMutualAuth TLSMode = "mutual_auth"
)

// CertificateValidationSettings holds certificate validation settings
type CertificateValidationSettings struct {
	// Enable certificate chain validation
	ValidateChain bool `json:"validate_chain"`
	
	// Enable certificate expiration checking
	CheckExpiration bool `json:"check_expiration"`
	
	// Enable certificate revocation checking
	CheckRevocation bool `json:"check_revocation"`
	
	// Custom validation rules
	CustomValidation map[string]interface{} `json:"custom_validation,omitempty"`
}

// ACLConfig holds ACL client configuration
type ACLConfig struct {
	// Enable ACL checking
	Enabled bool `json:"enabled"`
	
	// Controller endpoints for ACL requests
	ControllerEndpoints []string `json:"controller_endpoints"`
	
	// Request timeout
	RequestTimeout time.Duration `json:"request_timeout"`
	
	// Cache configuration
	Cache *ACLCacheConfig `json:"cache,omitempty"`
	
	// Circuit breaker configuration  
	CircuitBreaker *ACLCircuitBreakerConfig `json:"circuit_breaker,omitempty"`
}

// ACLCacheConfig holds ACL cache settings
type ACLCacheConfig struct {
	// Cache size (number of entries)
	Size int `json:"size"`
	
	// Cache TTL
	TTL time.Duration `json:"ttl"`
	
	// Cleanup interval
	CleanupInterval time.Duration `json:"cleanup_interval"`
	
	// Enable request deduplication
	EnableDeduplication bool `json:"enable_deduplication"`
}

// ACLCircuitBreakerConfig holds circuit breaker settings for ACL operations
type ACLCircuitBreakerConfig struct {
	// Failure threshold
	FailureThreshold int `json:"failure_threshold"`
	
	// Success threshold for half-open state
	SuccessThreshold int `json:"success_threshold"`
	
	// Timeout for circuit breaker recovery
	Timeout time.Duration `json:"timeout"`
}

// CertificateValidationConfig holds certificate validation settings
type CertificateValidationConfig struct {
	// Enable certificate validation
	Enabled bool `json:"enabled"`
	
	// Certificate validation rules
	Rules []CertificateValidationRule `json:"rules,omitempty"`
	
	// Revocation checking settings
	RevocationCheck *RevocationCheckConfig `json:"revocation_check,omitempty"`
}

// CertificateValidationRule represents a certificate validation rule
type CertificateValidationRule struct {
	// Rule type
	Type string `json:"type"`
	
	// Rule parameters
	Parameters map[string]interface{} `json:"parameters,omitempty"`
}

// RevocationCheckConfig holds certificate revocation checking settings
type RevocationCheckConfig struct {
	// Enable CRL checking
	EnableCRL bool `json:"enable_crl"`
	
	// Enable OCSP checking
	EnableOCSP bool `json:"enable_ocsp"`
	
	// CRL cache TTL
	CRLCacheTTL time.Duration `json:"crl_cache_ttl"`
	
	// OCSP request timeout
	OCSPTimeout time.Duration `json:"ocsp_timeout"`
}

// PrincipalExtractionConfig holds principal extraction settings
type PrincipalExtractionConfig struct {
	// Use common name from certificate subject
	UseCommonName bool `json:"use_common_name"`
	
	// Use subject alternative name
	UseSubjectAltName bool `json:"use_subject_alt_name"`
	
	// Normalize principal (lowercase, trim)
	Normalize bool `json:"normalize"`
	
	// Custom extraction rules
	CustomRules []PrincipalExtractionRule `json:"custom_rules,omitempty"`
}

// PrincipalExtractionRule represents a principal extraction rule
type PrincipalExtractionRule struct {
	// Rule type
	Type string `json:"type"`
	
	// Extraction pattern
	Pattern string `json:"pattern"`
	
	// Rule priority
	Priority int `json:"priority"`
}

// SecurityMetricsConfig holds security metrics settings
type SecurityMetricsConfig struct {
	// Enable metrics collection
	Enabled bool `json:"enabled"`
	
	// Metrics collection interval
	CollectionInterval time.Duration `json:"collection_interval"`
	
	// Enable detailed metrics
	Detailed bool `json:"detailed"`
	
	// Metrics export configuration
	Export *MetricsExportConfig `json:"export,omitempty"`
}

// MetricsExportConfig holds metrics export settings
type MetricsExportConfig struct {
	// Export format (prometheus, json, etc.)
	Format string `json:"format"`
	
	// Export endpoint
	Endpoint string `json:"endpoint,omitempty"`
	
	// Export interval
	Interval time.Duration `json:"interval"`
}

// SecurityContext represents an authenticated security context
type SecurityContext struct {
	// Principal identity
	Principal string `json:"principal"`
	
	// Authentication method used
	AuthMethod AuthenticationMethod `json:"auth_method"`
	
	// Certificate information (for mTLS)
	CertificateInfo *CertificateInfo `json:"certificate_info,omitempty"`
	
	// Permissions for this principal
	Permissions *PermissionSet `json:"permissions"`
	
	// Authentication timestamp
	AuthTime time.Time `json:"auth_time"`
	
	// Context expiration time
	ExpiresAt time.Time `json:"expires_at"`
	
	// Additional attributes
	Attributes map[string]interface{} `json:"attributes,omitempty"`
}

// CertificateInfo holds certificate information
type CertificateInfo struct {
	// Certificate subject
	Subject string `json:"subject"`
	
	// Certificate issuer
	Issuer string `json:"issuer"`
	
	// Serial number
	SerialNumber string `json:"serial_number"`
	
	// Valid from timestamp
	NotBefore time.Time `json:"not_before"`
	
	// Valid until timestamp
	NotAfter time.Time `json:"not_after"`
	
	// Certificate fingerprint (SHA-256)
	Fingerprint string `json:"fingerprint"`
	
	// Subject attributes
	Attributes map[string]string `json:"attributes"`
	
	// Subject alternative names
	SubjectAltNames []string `json:"subject_alt_names,omitempty"`
}

// PermissionSet represents a set of permissions for a principal
type PermissionSet struct {
	// Topics the principal can read from
	ReadTopics []string `json:"read_topics"`
	
	// Topics the principal can write to
	WriteTopics []string `json:"write_topics"`
	
	// Admin operations the principal can perform
	AdminOperations []string `json:"admin_operations"`
	
	// Custom permissions
	CustomPermissions map[string][]string `json:"custom_permissions,omitempty"`
}

// CanReadTopic checks if the permission set allows reading from a topic
func (ps *PermissionSet) CanReadTopic(topic string) bool {
	return ps.matchesAnyPattern(topic, ps.ReadTopics)
}

// CanWriteTopic checks if the permission set allows writing to a topic
func (ps *PermissionSet) CanWriteTopic(topic string) bool {
	return ps.matchesAnyPattern(topic, ps.WriteTopics)
}

// CanPerformAdminOperation checks if the permission set allows admin operations
func (ps *PermissionSet) CanPerformAdminOperation(operation string) bool {
	return ps.matchesAnyPattern(operation, ps.AdminOperations)
}

// matchesAnyPattern checks if text matches any of the patterns
func (ps *PermissionSet) matchesAnyPattern(text string, patterns []string) bool {
	for _, pattern := range patterns {
		if matchesPattern(text, pattern) {
			return true
		}
	}
	return false
}

// SecurityManager manages security operations for the client
type SecurityManager struct {
	config              *SecurityConfig
	authProviders       map[AuthenticationMethod]AuthenticationProvider
	certificateManager  *CertificateManager
	aclCache           *ACLCache
	principalExtractor *PrincipalExtractor
	metrics            *SecurityMetrics
	
	// Context and synchronization
	ctx    context.Context
	cancel context.CancelFunc
	mu     sync.RWMutex
	
	// Background workers
	workers sync.WaitGroup
}

// AuthenticationProvider interface for pluggable authentication
type AuthenticationProvider interface {
	// Authenticate using the provided configuration
	Authenticate(ctx context.Context, config *AuthenticationConfig, tlsConfig *tls.Config) (*SecurityContext, error)
	
	// Validate if the provider can handle the authentication method
	CanHandle(method AuthenticationMethod) bool
	
	// Get provider name
	Name() string
}

// NewSecurityManager creates a new security manager
func NewSecurityManager(config *SecurityConfig) (*SecurityManager, error) {
	if config == nil {
		config = DefaultSecurityConfig()
	}
	
	ctx, cancel := context.WithCancel(context.Background())
	
	manager := &SecurityManager{
		config:        config,
		authProviders: make(map[AuthenticationMethod]AuthenticationProvider),
		ctx:           ctx,
		cancel:        cancel,
	}
	
	// Initialize components
	if err := manager.initialize(); err != nil {
		cancel()
		return nil, fmt.Errorf("failed to initialize security manager: %w", err)
	}
	
	return manager, nil
}

// initialize sets up all security components
func (sm *SecurityManager) initialize() error {
	var err error
	
	// Initialize certificate manager
	sm.certificateManager, err = NewCertificateManager(sm.config.CertValidation)
	if err != nil {
		return fmt.Errorf("failed to initialize certificate manager: %w", err)
	}
	
	// Initialize ACL cache
	sm.aclCache, err = NewACLCache(sm.config.ACL)
	if err != nil {
		return fmt.Errorf("failed to initialize ACL cache: %w", err)
	}
	
	// Initialize principal extractor
	sm.principalExtractor = NewPrincipalExtractor(sm.config.PrincipalExtraction)
	
	// Initialize security metrics
	sm.metrics = NewSecurityMetrics(sm.config.Metrics)
	
	// Register authentication providers
	sm.registerAuthProviders()
	
	// Start background workers
	sm.startBackgroundWorkers()
	
	return nil
}

// registerAuthProviders registers all authentication providers
func (sm *SecurityManager) registerAuthProviders() {
	// Register mTLS provider
	sm.authProviders[AuthMethodMTLS] = &MTLSAuthProvider{
		certificateManager: sm.certificateManager,
		principalExtractor: sm.principalExtractor,
	}
	
	// Register JWT provider
	sm.authProviders[AuthMethodJWT] = &JWTAuthProvider{}
	
	// Register SASL providers
	sm.authProviders[AuthMethodSASLPlain] = &SASLAuthProvider{method: AuthMethodSASLPlain}
	sm.authProviders[AuthMethodSASLScram256] = &SASLAuthProvider{method: AuthMethodSASLScram256}
	sm.authProviders[AuthMethodSASLScram512] = &SASLAuthProvider{method: AuthMethodSASLScram512}
}

// startBackgroundWorkers starts background maintenance tasks
func (sm *SecurityManager) startBackgroundWorkers() {
	// Start ACL cache cleanup worker
	sm.workers.Add(1)
	go sm.aclCacheCleanupWorker()
	
	// Start metrics collection worker
	if sm.config.Metrics != nil && sm.config.Metrics.Enabled {
		sm.workers.Add(1)
		go sm.metricsCollectionWorker()
	}
	
	// Start token refresh worker (if JWT is enabled)
	if sm.config.Auth != nil && sm.config.Auth.Method == AuthMethodJWT {
		if sm.config.Auth.TokenRefresh != nil && sm.config.Auth.TokenRefresh.Enabled {
			sm.workers.Add(1)
			go sm.tokenRefreshWorker()
		}
	}
}

// Authenticate performs authentication and returns security context
func (sm *SecurityManager) Authenticate(ctx context.Context) (*SecurityContext, error) {
	if sm.config.Auth == nil || sm.config.Auth.Method == AuthMethodNone {
		securityContext := &SecurityContext{
			Principal:   "anonymous",
			AuthMethod:  AuthMethodNone,
			Permissions: &PermissionSet{},
			AuthTime:    time.Now(),
			ExpiresAt:   time.Now().Add(24 * time.Hour),
		}
		sm.metrics.RecordAuthentication(string(AuthMethodNone), true)
		sm.metrics.RecordSecurityContextCreation()
		return securityContext, nil
	}
	
	provider, exists := sm.authProviders[sm.config.Auth.Method]
	if !exists {
		return nil, fmt.Errorf("no authentication provider for method: %s", sm.config.Auth.Method)
	}
	
	// Create TLS configuration
	tlsConfig, err := sm.createTLSConfig()
	if err != nil {
		// Preserve SecurityError types without wrapping
		if _, ok := AsSecurityError(err); ok {
			return nil, err
		}
		return nil, fmt.Errorf("failed to create TLS config: %w", err)
	}
	
	// Perform authentication
	securityContext, err := provider.Authenticate(ctx, sm.config.Auth, tlsConfig)
	if err != nil {
		sm.metrics.RecordAuthentication(string(sm.config.Auth.Method), false)
		// Preserve SecurityError types without wrapping
		if _, ok := AsSecurityError(err); ok {
			return nil, err
		}
		return nil, fmt.Errorf("authentication failed: %w", err)
	}
	
	// Get permissions from ACL
	permissions, err := sm.aclCache.GetPermissions(ctx, securityContext.Principal)
	if err != nil {
		return nil, fmt.Errorf("failed to get permissions: %w", err)
	}
	securityContext.Permissions = permissions
	
	sm.metrics.RecordAuthentication(string(sm.config.Auth.Method), true)
	sm.metrics.RecordSecurityContextCreation()
	return securityContext, nil
}

// createTLSConfig creates TLS configuration from security config
func (sm *SecurityManager) createTLSConfig() (*tls.Config, error) {
	if sm.config.TLS == nil || sm.config.TLS.Mode == TLSModeDisabled {
		return nil, nil
	}
	
	// Validate TLS mode compatibility with authentication method
	if sm.config.Auth != nil && sm.config.Auth.Method == AuthMethodMTLS {
		if sm.config.TLS.Mode != TLSModeMutualAuth {
			return nil, &SecurityError{
				Type:    SecurityErrorInvalidConfig,
				Message: "mTLS authentication requires TLS mutual authentication mode",
				Details: map[string]interface{}{
					"auth_method": sm.config.Auth.Method,
					"tls_mode":    sm.config.TLS.Mode,
				},
			}
		}
	}
	
	tlsConfig := &tls.Config{
		ServerName:         sm.config.TLS.ServerName,
		InsecureSkipVerify: sm.config.TLS.InsecureSkipVerify,
		MinVersion:         sm.config.TLS.MinVersion,
		MaxVersion:         sm.config.TLS.MaxVersion,
		CipherSuites:       sm.config.TLS.CipherSuites,
	}
	
	// Load CA certificates
	if sm.config.TLS.CACert != "" {
		caCertPool := x509.NewCertPool()
		if !caCertPool.AppendCertsFromPEM([]byte(sm.config.TLS.CACert)) {
			return nil, fmt.Errorf("failed to parse CA certificate")
		}
		tlsConfig.RootCAs = caCertPool
	}
	
	// Load client certificate for mTLS
	if sm.config.TLS.Mode == TLSModeMutualAuth {
		if sm.config.TLS.ClientCert == "" || sm.config.TLS.ClientKey == "" {
			return nil, fmt.Errorf("client certificate and key required for mutual TLS")
		}
		
		cert, err := tls.X509KeyPair([]byte(sm.config.TLS.ClientCert), []byte(sm.config.TLS.ClientKey))
		if err != nil {
			return nil, fmt.Errorf("failed to load client certificate: %w", err)
		}
		
		tlsConfig.Certificates = []tls.Certificate{cert}
	}
	
	return tlsConfig, nil
}

// Close shuts down the security manager
func (sm *SecurityManager) Close() error {
	sm.cancel()
	sm.workers.Wait()
	
	if sm.aclCache != nil {
		sm.aclCache.Close()
	}
	
	return nil
}

// Metrics returns security metrics
func (sm *SecurityManager) Metrics() *SecurityMetrics {
	return sm.metrics
}

// Background worker methods
func (sm *SecurityManager) aclCacheCleanupWorker() {
	defer sm.workers.Done()
	
	if sm.config.ACL == nil || sm.config.ACL.Cache == nil {
		return
	}
	
	ticker := time.NewTicker(sm.config.ACL.Cache.CleanupInterval)
	defer ticker.Stop()
	
	for {
		select {
		case <-sm.ctx.Done():
			return
		case <-ticker.C:
			sm.aclCache.Cleanup()
		}
	}
}

func (sm *SecurityManager) metricsCollectionWorker() {
	defer sm.workers.Done()
	
	interval := sm.config.Metrics.CollectionInterval
	if interval == 0 {
		interval = 1 * time.Minute
	}
	
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	
	for {
		select {
		case <-sm.ctx.Done():
			return
		case <-ticker.C:
			sm.metrics.Collect()
		}
	}
}

func (sm *SecurityManager) tokenRefreshWorker() {
	defer sm.workers.Done()
	
	// TODO: Implement JWT token refresh logic
	// This would monitor token expiration and refresh tokens automatically
}

// DefaultSecurityConfig returns default security configuration
func DefaultSecurityConfig() *SecurityConfig {
	return &SecurityConfig{
		Auth: &AuthenticationConfig{
			Method: AuthMethodNone,
		},
		TLS: &TLSSecurityConfig{
			Mode:               TLSModeDisabled,
			InsecureSkipVerify: false,
			MinVersion:         tls.VersionTLS12,
		},
		ACL: &ACLConfig{
			Enabled:        false,
			RequestTimeout: 5 * time.Second,
			Cache: &ACLCacheConfig{
				Size:                1000,
				TTL:                 10 * time.Minute,
				CleanupInterval:     1 * time.Minute,
				EnableDeduplication: true,
			},
			CircuitBreaker: &ACLCircuitBreakerConfig{
				FailureThreshold: 5,
				SuccessThreshold: 3,
				Timeout:         30 * time.Second,
			},
		},
		CertValidation: &CertificateValidationConfig{
			Enabled: true,
		},
		PrincipalExtraction: &PrincipalExtractionConfig{
			UseCommonName:     true,
			UseSubjectAltName: false,
			Normalize:         true,
		},
		Metrics: &SecurityMetricsConfig{
			Enabled:            false,
			CollectionInterval: 1 * time.Minute,
			Detailed:           false,
		},
	}
}

// Helper function for pattern matching
func matchesPattern(text, pattern string) bool {
	if pattern == "*" {
		return true
	}
	
	// Simple wildcard matching (could be enhanced with regex)
	if len(pattern) > 0 && pattern[len(pattern)-1] == '*' {
		prefix := pattern[:len(pattern)-1]
		return len(text) >= len(prefix) && text[:len(prefix)] == prefix
	}
	
	return text == pattern
}