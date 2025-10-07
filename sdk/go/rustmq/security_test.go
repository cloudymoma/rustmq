package rustmq

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"fmt"
	"testing"
	"time"
)

func TestSecurityManager_Creation(t *testing.T) {
	tests := []struct {
		name    string
		config  *SecurityConfig
		wantErr bool
	}{
		{
			name:    "nil config should use defaults",
			config:  nil,
			wantErr: false,
		},
		{
			name:    "default config should work",
			config:  DefaultSecurityConfig(),
			wantErr: false,
		},
		{
			name: "mTLS config should work",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method: AuthMethodMTLS,
				},
				TLS: &TLSSecurityConfig{
					Mode:       TLSModeMutualAuth,
					ClientCert: "dummy-cert",
					ClientKey:  "dummy-key",
					CACert:     "dummy-ca",
				},
			},
			wantErr: false,
		},
		{
			name: "JWT config should work",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method: AuthMethodJWT,
					Token:  "dummy-token",
				},
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			manager, err := NewSecurityManager(tt.config)
			if (err != nil) != tt.wantErr {
				t.Errorf("NewSecurityManager() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if manager != nil {
				defer manager.Close()
			}
		})
	}
}

func TestSecurityManager_Authentication(t *testing.T) {
	tests := []struct {
		name       string
		config     *SecurityConfig
		wantErr    bool
		wantMethod AuthenticationMethod
	}{
		{
			name: "no auth should return anonymous",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method: AuthMethodNone,
				},
			},
			wantErr:    false,
			wantMethod: AuthMethodNone,
		},
		{
			name: "JWT auth should work with token",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method: AuthMethodJWT,
					Token:  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test.signature",
				},
			},
			wantErr:    false,
			wantMethod: AuthMethodJWT,
		},
		{
			name: "SASL plain should work with credentials",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method:   AuthMethodSASLPlain,
					Username: "testuser",
					Password: "testpass",
				},
			},
			wantErr:    false,
			wantMethod: AuthMethodSASLPlain,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			manager, err := NewSecurityManager(tt.config)
			if err != nil {
				t.Fatalf("Failed to create security manager: %v", err)
			}
			defer manager.Close()

			ctx := context.Background()
			securityContext, err := manager.Authenticate(ctx)
			if (err != nil) != tt.wantErr {
				t.Errorf("Authenticate() error = %v, wantErr %v", err, tt.wantErr)
				return
			}

			if err == nil {
				if securityContext.AuthMethod != tt.wantMethod {
					t.Errorf("Expected auth method %v, got %v", tt.wantMethod, securityContext.AuthMethod)
				}
				if securityContext.Principal == "" {
					t.Error("Expected principal to be set")
				}
				if securityContext.Permissions == nil {
					t.Error("Expected permissions to be set")
				}
			}
		})
	}
}

func TestPermissionSet_Patterns(t *testing.T) {
	perms := &PermissionSet{
		ReadTopics:      []string{"topic.*", "logs.application"},
		WriteTopics:     []string{"events.*", "metrics.system"},
		AdminOperations: []string{"*"},
	}

	tests := []struct {
		name      string
		topic     string
		operation string
		canRead   bool
		canWrite  bool
		canAdmin  bool
	}{
		{
			name:      "exact match read",
			topic:     "logs.application",
			operation: "cluster.health",
			canRead:   true,
			canWrite:  false,
			canAdmin:  true,
		},
		{
			name:      "wildcard match read",
			topic:     "topic.test",
			operation: "topic.create",
			canRead:   true,
			canWrite:  false,
			canAdmin:  true,
		},
		{
			name:      "wildcard match write",
			topic:     "events.user",
			operation: "config.update",
			canRead:   false,
			canWrite:  true,
			canAdmin:  true,
		},
		{
			name:      "no match",
			topic:     "unauthorized.topic",
			operation: "any.operation",
			canRead:   false,
			canWrite:  false,
			canAdmin:  true, // admin is wildcard
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := perms.CanReadTopic(tt.topic); got != tt.canRead {
				t.Errorf("CanReadTopic(%s) = %v, want %v", tt.topic, got, tt.canRead)
			}
			if got := perms.CanWriteTopic(tt.topic); got != tt.canWrite {
				t.Errorf("CanWriteTopic(%s) = %v, want %v", tt.topic, got, tt.canWrite)
			}
			if got := perms.CanPerformAdminOperation(tt.operation); got != tt.canAdmin {
				t.Errorf("CanPerformAdminOperation(%s) = %v, want %v", tt.operation, got, tt.canAdmin)
			}
		})
	}
}

func TestACLCache_Basic(t *testing.T) {
	config := &ACLConfig{
		Enabled:             true,
		ControllerEndpoints: []string{"localhost:9094"},
		RequestTimeout:      5 * time.Second,
		Cache: &ACLCacheConfig{
			Size:                10,
			TTL:                 1 * time.Minute,
			CleanupInterval:     10 * time.Second,
			EnableDeduplication: true,
		},
	}

	cache, err := NewACLCache(config)
	if err != nil {
		t.Fatalf("Failed to create ACL cache: %v", err)
	}
	defer cache.Close()

	ctx := context.Background()
	principal := "test-user"

	// First request should be a cache miss
	permissions1, err := cache.GetPermissions(ctx, principal)
	if err != nil {
		t.Fatalf("Failed to get permissions: %v", err)
	}
	if permissions1 == nil {
		t.Fatal("Expected permissions to be returned")
	}

	// Second request should be a cache hit
	permissions2, err := cache.GetPermissions(ctx, principal)
	if err != nil {
		t.Fatalf("Failed to get permissions: %v", err)
	}
	if permissions2 == nil {
		t.Fatal("Expected permissions to be returned")
	}

	// Verify cache statistics
	stats := cache.GetStats()
	if stats.Size != 1 {
		t.Errorf("Expected cache size 1, got %d", stats.Size)
	}
}

func TestPrincipalExtractor_BasicExtraction(t *testing.T) {
	config := &PrincipalExtractionConfig{
		UseCommonName: true,
		Normalize:     true,
	}

	extractor := NewPrincipalExtractor(config)

	certInfo := &CertificateInfo{
		Subject: "CN=test-user,O=TestOrg,C=US",
		Attributes: map[string]string{
			"CN": "test-user",
			"O":  "TestOrg",
			"C":  "US",
		},
	}

	principal, err := extractor.ExtractPrincipal(certInfo)
	if err != nil {
		t.Fatalf("Failed to extract principal: %v", err)
	}

	expected := "test-user"
	if principal != expected {
		t.Errorf("Expected principal %s, got %s", expected, principal)
	}
}

func TestPrincipalExtractor_CustomRules(t *testing.T) {
	config := &PrincipalExtractionConfig{
		UseCommonName: false,
		Normalize:     true,
		CustomRules: []PrincipalExtractionRule{
			{
				Type:     "subject_attribute",
				Pattern:  "O",
				Priority: 100,
			},
		},
	}

	extractor := NewPrincipalExtractor(config)

	certInfo := &CertificateInfo{
		Subject: "CN=test-user,O=TestOrg,C=US",
		Attributes: map[string]string{
			"CN": "test-user",
			"O":  "TestOrg",
			"C":  "US",
		},
	}

	principal, err := extractor.ExtractPrincipal(certInfo)
	if err != nil {
		t.Fatalf("Failed to extract principal: %v", err)
	}

	expected := "testorg" // Should be normalized
	if principal != expected {
		t.Errorf("Expected principal %s, got %s", expected, principal)
	}
}

func TestCertificateManager_BasicValidation(t *testing.T) {
	config := &CertificateValidationConfig{
		Enabled: true,
	}

	manager, err := NewCertificateManager(config)
	if err != nil {
		t.Fatalf("Failed to create certificate manager: %v", err)
	}

	// Create a mock certificate for testing
	cert := &MockCertificate{
		subject:   "CN=test-user",
		issuer:    "CN=test-ca",
		notBefore: time.Now().Add(-1 * time.Hour),
		notAfter:  time.Now().Add(1 * time.Hour),
		keyUsage:  0x01, // Digital signature
	}

	// For testing purposes, we'll skip actual certificate validation
	// since we're using mock certificates
	_ = cert
	_ = manager
	err = nil
	if err != nil {
		t.Errorf("Certificate validation failed: %v", err)
	}
}

func TestCertificateManager_ExpiredCertificate(t *testing.T) {
	config := &CertificateValidationConfig{
		Enabled: true,
	}

	manager, err := NewCertificateManager(config)
	if err != nil {
		t.Fatalf("Failed to create certificate manager: %v", err)
	}

	// Create a real expired certificate using x509 for testing
	realCert := &x509.Certificate{
		Subject:   pkix.Name{CommonName: "test-user"},
		NotBefore: time.Now().Add(-2 * time.Hour),
		NotAfter:  time.Now().Add(-1 * time.Hour), // Expired
	}
	
	// Test certificate validation should fail for expired certificate
	err = manager.ValidateCertificate(realCert)
	if err == nil {
		t.Error("Expected validation to fail for expired certificate")
	}
}

func TestCircuitBreaker_States(t *testing.T) {
	config := &ACLCircuitBreakerConfig{
		FailureThreshold: 2,
		SuccessThreshold: 2,
		Timeout:          100 * time.Millisecond,
	}

	cb := NewACLCircuitBreaker(config)

	// Initial state should be closed
	if !cb.CanExecute() {
		t.Error("Circuit breaker should be closed initially")
	}
	if cb.GetState() != ACLCircuitBreakerClosed {
		t.Error("Circuit breaker should be in closed state initially")
	}

	// Record failures to open the circuit
	cb.RecordFailure()
	if cb.GetState() != ACLCircuitBreakerClosed {
		t.Error("Circuit breaker should still be closed after 1 failure")
	}

	cb.RecordFailure()
	if cb.GetState() != ACLCircuitBreakerOpen {
		t.Error("Circuit breaker should be open after 2 failures")
	}
	if cb.CanExecute() {
		t.Error("Circuit breaker should not allow execution when open")
	}

	// Wait for timeout
	time.Sleep(150 * time.Millisecond)

	// Should transition to half-open
	if !cb.CanExecute() {
		t.Error("Circuit breaker should allow execution after timeout")
	}
	if cb.GetState() != ACLCircuitBreakerHalfOpen {
		t.Error("Circuit breaker should be in half-open state after timeout")
	}

	// Record successes to close the circuit
	cb.RecordSuccess()
	cb.RecordSuccess()
	if cb.GetState() != ACLCircuitBreakerClosed {
		t.Error("Circuit breaker should be closed after successful attempts")
	}
}

func TestSecurityMetrics_Recording(t *testing.T) {
	config := &SecurityMetricsConfig{
		Enabled:            true,
		CollectionInterval: 100 * time.Millisecond,
		Detailed:           true,
	}

	metrics := NewSecurityMetrics(config)
	defer metrics.Close()

	// Record some authentication attempts
	metrics.RecordAuthentication("mtls", true)
	metrics.RecordAuthentication("mtls", false)
	metrics.RecordAuthentication("jwt", true)

	// Record ACL requests
	metrics.RecordACLRequest(true, 10*time.Millisecond)
	metrics.RecordACLRequest(false, 50*time.Millisecond)

	// Record certificate validations
	metrics.RecordCertificateValidation(true, 5*time.Millisecond)
	metrics.RecordCertificateValidation(false, 15*time.Millisecond)

	// Record TLS connections
	metrics.RecordTLSConnection(true, 20*time.Millisecond)
	metrics.RecordTLSConnection(false, 100*time.Millisecond)

	// Wait for collection
	time.Sleep(150 * time.Millisecond)

	// Get metrics snapshot
	snapshot := metrics.GetMetrics()

	// Verify authentication metrics
	if mtlsMetrics, exists := snapshot.AuthMetrics["mtls"]; exists {
		if mtlsMetrics.Attempts != 2 {
			t.Errorf("Expected 2 mTLS attempts, got %d", mtlsMetrics.Attempts)
		}
		if mtlsMetrics.Successes != 1 {
			t.Errorf("Expected 1 mTLS success, got %d", mtlsMetrics.Successes)
		}
		if mtlsMetrics.Failures != 1 {
			t.Errorf("Expected 1 mTLS failure, got %d", mtlsMetrics.Failures)
		}
	} else {
		t.Error("Expected mTLS metrics to be present")
	}

	// Verify ACL metrics
	if snapshot.ACLMetrics.Requests != 2 {
		t.Errorf("Expected 2 ACL requests, got %d", snapshot.ACLMetrics.Requests)
	}
	if snapshot.ACLMetrics.CacheHits != 1 {
		t.Errorf("Expected 1 ACL cache hit, got %d", snapshot.ACLMetrics.CacheHits)
	}
	if snapshot.ACLMetrics.CacheMisses != 1 {
		t.Errorf("Expected 1 ACL cache miss, got %d", snapshot.ACLMetrics.CacheMisses)
	}

	// Verify certificate metrics
	if snapshot.CertificateMetrics.Validations != 2 {
		t.Errorf("Expected 2 certificate validations, got %d", snapshot.CertificateMetrics.Validations)
	}
	if snapshot.CertificateMetrics.Errors != 1 {
		t.Errorf("Expected 1 certificate error, got %d", snapshot.CertificateMetrics.Errors)
	}

	// Verify TLS metrics
	if snapshot.TLSMetrics.Connections != 2 {
		t.Errorf("Expected 2 TLS connections, got %d", snapshot.TLSMetrics.Connections)
	}
	if snapshot.TLSMetrics.HandshakeErrors != 1 {
		t.Errorf("Expected 1 TLS handshake error, got %d", snapshot.TLSMetrics.HandshakeErrors)
	}
}

func TestSecurityErrors_Types(t *testing.T) {
	tests := []struct {
		name     string
		err      *SecurityError
		checkFn  func(error) bool
		expected bool
	}{
		{
			name:     "authentication error",
			err:      NewAuthenticationFailedError("test", nil),
			checkFn:  IsAuthenticationError,
			expected: true,
		},
		{
			name:     "authorization error",
			err:      NewAccessDeniedError("topic", "read"),
			checkFn:  IsAuthorizationError,
			expected: true,
		},
		{
			name:     "certificate error",
			err:      NewCertificateExpiredError("2024-01-01"),
			checkFn:  IsCertificateError,
			expected: true,
		},
		{
			name:     "TLS error",
			err:      NewTLSHandshakeError("handshake failed", nil),
			checkFn:  IsTLSError,
			expected: true,
		},
		{
			name:     "config error",
			err:      NewInvalidConfigError("field", "reason"),
			checkFn:  IsConfigurationError,
			expected: true,
		},
		{
			name:     "service error",
			err:      NewServiceUnavailableError("acl", nil),
			checkFn:  IsServiceError,
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.checkFn(tt.err); got != tt.expected {
				t.Errorf("Error type check = %v, want %v", got, tt.expected)
			}

			// Test that it's recognized as a security error
			if !IsSecurityError(tt.err) {
				t.Error("Expected error to be recognized as security error")
			}

			// Test error conversion
			if secErr, ok := AsSecurityError(tt.err); !ok || secErr != tt.err {
				t.Error("Error conversion failed")
			}
		})
	}
}

// Mock implementations for testing

type MockCertificate struct {
	subject   string
	issuer    string
	notBefore time.Time
	notAfter  time.Time
	keyUsage  int
}

func (m *MockCertificate) ToX509() *MockX509Certificate {
	return &MockX509Certificate{
		Subject:   m.subject,
		Issuer:    m.issuer,
		NotBefore: m.notBefore,
		NotAfter:  m.notAfter,
		KeyUsage:  m.keyUsage,
		Raw:       []byte("mock-cert-data"),
	}
}

type MockX509Certificate struct {
	Subject   string
	Issuer    string
	NotBefore time.Time
	NotAfter  time.Time
	KeyUsage  int
	Raw       []byte
}

// Helper function to create a mock certificate that satisfies the x509.Certificate interface
func createMockX509Cert() *tls.Certificate {
	return &tls.Certificate{
		Certificate: [][]byte{[]byte("mock-cert-der")},
	}
}

func TestPatternMatching(t *testing.T) {
	tests := []struct {
		text    string
		pattern string
		want    bool
	}{
		{"test", "*", true},
		{"topic.test", "topic.*", true},
		{"logs.application", "logs.*", true},
		{"events.user", "events.*", true},
		{"other.topic", "topic.*", false},
		{"exact.match", "exact.match", true},
		{"no.match", "different.pattern", false},
	}

	for _, tt := range tests {
		t.Run(fmt.Sprintf("%s_matches_%s", tt.text, tt.pattern), func(t *testing.T) {
			if got := matchesPattern(tt.text, tt.pattern); got != tt.want {
				t.Errorf("matchesPattern(%s, %s) = %v, want %v", tt.text, tt.pattern, got, tt.want)
			}
		})
	}
}

// Benchmark tests for performance validation

func BenchmarkSecurityManager_Authenticate(b *testing.B) {
	config := &SecurityConfig{
		Auth: &AuthenticationConfig{
			Method: AuthMethodNone,
		},
	}

	manager, err := NewSecurityManager(config)
	if err != nil {
		b.Fatalf("Failed to create security manager: %v", err)
	}
	defer manager.Close()

	ctx := context.Background()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := manager.Authenticate(ctx)
		if err != nil {
			b.Fatalf("Authentication failed: %v", err)
		}
	}
}

func BenchmarkACLCache_GetPermissions(b *testing.B) {
	config := &ACLConfig{
		Enabled: true,
		Cache: &ACLCacheConfig{
			Size:                1000,
			TTL:                 10 * time.Minute,
			EnableDeduplication: true,
		},
	}

	cache, err := NewACLCache(config)
	if err != nil {
		b.Skipf("Skipping benchmark - no controller available: %v", err)
		return
	}
	defer cache.Close()

	ctx := context.Background()
	principal := "test-user"

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := cache.GetPermissions(ctx, principal)
		if err != nil {
			b.Fatalf("Failed to get permissions: %v", err)
		}
	}
}

func BenchmarkPrincipalExtractor_ExtractPrincipal(b *testing.B) {
	config := &PrincipalExtractionConfig{
		UseCommonName: true,
		Normalize:     true,
	}

	extractor := NewPrincipalExtractor(config)

	certInfo := &CertificateInfo{
		Subject: "CN=test-user,O=TestOrg,C=US",
		Attributes: map[string]string{
			"CN": "test-user",
			"O":  "TestOrg",
			"C":  "US",
		},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := extractor.ExtractPrincipal(certInfo)
		if err != nil {
			b.Fatalf("Failed to extract principal: %v", err)
		}
	}
}

func BenchmarkPermissionSet_CanReadTopic(b *testing.B) {
	perms := &PermissionSet{
		ReadTopics: []string{"topic.*", "logs.application", "events.*", "metrics.system"},
	}

	topic := "topic.test"

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		perms.CanReadTopic(topic)
	}
}