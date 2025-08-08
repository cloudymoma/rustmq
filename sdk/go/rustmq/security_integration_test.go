package rustmq

import (
	"context"
	"testing"
	"time"
)

// TestSecurityIntegration_FullWorkflow tests the complete security workflow
func TestSecurityIntegration_FullWorkflow(t *testing.T) {
	// Create a comprehensive security configuration
	config := &SecurityConfig{
		Auth: &AuthenticationConfig{
			Method:   AuthMethodJWT,
			Token:    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0LXVzZXIiLCJleHAiOjk5OTk5OTk5OTl9.signature",
			Username: "test-user",
		},
		TLS: &TLSSecurityConfig{
			Mode:               TLSModeEnabled,
			ServerName:         "rustmq-test",
			InsecureSkipVerify: true, // For testing
		},
		ACL: &ACLConfig{
			Enabled:             true,
			ControllerEndpoints: []string{"localhost:9094"},
			RequestTimeout:      5 * time.Second,
			Cache: &ACLCacheConfig{
				Size:                100,
				TTL:                 5 * time.Minute,
				CleanupInterval:     30 * time.Second,
				EnableDeduplication: true,
			},
			CircuitBreaker: &ACLCircuitBreakerConfig{
				FailureThreshold: 3,
				SuccessThreshold: 2,
				Timeout:          30 * time.Second,
			},
		},
		PrincipalExtraction: &PrincipalExtractionConfig{
			UseCommonName: false,
			Normalize:     true,
		},
		Metrics: &SecurityMetricsConfig{
			Enabled:            true,
			CollectionInterval: 1 * time.Second,
			Detailed:           true,
		},
	}

	// Create security manager
	manager, err := NewSecurityManager(config)
	if err != nil {
		t.Fatalf("Failed to create security manager: %v", err)
	}
	defer manager.Close()

	// Test authentication
	ctx := context.Background()
	securityContext, err := manager.Authenticate(ctx)
	if err != nil {
		t.Fatalf("Authentication failed: %v", err)
	}

	// Verify security context
	if securityContext.Principal == "" {
		t.Error("Principal should be set")
	}
	if securityContext.AuthMethod != AuthMethodJWT {
		t.Errorf("Expected auth method JWT, got %v", securityContext.AuthMethod)
	}
	if securityContext.Permissions == nil {
		t.Error("Permissions should be set")
	}

	// Test permission checks
	topics := []string{"test.topic", "user.events", "admin.operations"}
	for _, topic := range topics {
		// These should not fail (default permissions in mock)
		canRead := securityContext.Permissions.CanReadTopic(topic)
		canWrite := securityContext.Permissions.CanWriteTopic(topic)
		t.Logf("Topic %s: read=%v, write=%v", topic, canRead, canWrite)
	}

	// Wait for metrics collection
	time.Sleep(2 * time.Second)

	// Verify metrics
	metrics := manager.Metrics().GetMetrics()
	if metrics.CollectionCount == 0 {
		t.Error("Expected metrics to be collected")
	}

	if authMetrics, exists := metrics.AuthMetrics[string(AuthMethodJWT)]; exists {
		if authMetrics.Attempts == 0 {
			t.Error("Expected authentication attempts to be recorded")
		}
		if authMetrics.Successes == 0 {
			t.Error("Expected successful authentications to be recorded")
		}
	} else {
		t.Error("Expected JWT authentication metrics")
	}

	if metrics.ContextMetrics.Active == 0 {
		t.Error("Expected active security contexts to be tracked")
	}
}

// TestSecurityIntegration_ClientWorkflow tests security integration with client
func TestSecurityIntegration_ClientWorkflow(t *testing.T) {
	// Create client config with security
	clientConfig := &ClientConfig{
		Brokers:           []string{"localhost:9092"},
		ClientID:          "test-client",
		ConnectTimeout:    10 * time.Second,
		RequestTimeout:    30 * time.Second,
		EnableTLS:         true,
		MaxConnections:    5,
		KeepAliveInterval: 30 * time.Second,
		Security: &SecurityConfig{
			Auth: &AuthenticationConfig{
				Method: AuthMethodJWT,
				Token:  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0LXVzZXIiLCJleHAiOjk5OTk5OTk5OTl9.signature",
			},
			TLS: &TLSSecurityConfig{
				Mode:               TLSModeEnabled,
				InsecureSkipVerify: true, // For testing
			},
			ACL: &ACLConfig{
				Enabled: true,
				Cache: &ACLCacheConfig{
					Size: 100,
					TTL:  5 * time.Minute,
				},
			},
			Metrics: &SecurityMetricsConfig{
				Enabled: true,
			},
		},
	}

	// Test security config merging
	client := &Client{config: clientConfig}
	mergedConfig := client.mergeSecurityConfig()

	if mergedConfig == nil {
		t.Fatal("Expected merged security config")
	}
	if mergedConfig.Auth.Method != AuthMethodJWT {
		t.Errorf("Expected JWT auth method, got %v", mergedConfig.Auth.Method)
	}
	if mergedConfig.TLS.Mode != TLSModeEnabled {
		t.Errorf("Expected TLS enabled, got %v", mergedConfig.TLS.Mode)
	}

	// Test legacy config support
	legacyConfig := &ClientConfig{
		EnableTLS: true,
		TLSConfig: &TLSConfig{
			ClientCert:         "cert-data",
			ClientKey:          "key-data",
			CACert:             "ca-data",
			InsecureSkipVerify: false,
		},
		Auth: &AuthConfig{
			Method:   "mtls",
			Username: "legacy-user",
		},
	}

	legacyClient := &Client{config: legacyConfig}
	legacyMerged := legacyClient.mergeSecurityConfig()

	if legacyMerged == nil {
		t.Fatal("Expected merged legacy config")
	}
	if legacyMerged.Auth.Method != AuthMethodMTLS {
		t.Errorf("Expected mTLS auth method, got %v", legacyMerged.Auth.Method)
	}
	if legacyMerged.TLS.Mode != TLSModeMutualAuth {
		t.Errorf("Expected mutual TLS mode, got %v", legacyMerged.TLS.Mode)
	}
}

// TestSecurityIntegration_ErrorHandling tests security error scenarios
func TestSecurityIntegration_ErrorHandling(t *testing.T) {
	tests := []struct {
		name        string
		config      *SecurityConfig
		expectError bool
		errorType   SecurityErrorType
	}{
		{
			name: "missing JWT token",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method: AuthMethodJWT,
					// Token missing
				},
			},
			expectError: true,
			errorType:   SecurityErrorInvalidCredentials,
		},
		{
			name: "missing SASL credentials",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method: AuthMethodSASLPlain,
					// Username/password missing
				},
			},
			expectError: true,
			errorType:   SecurityErrorInvalidCredentials,
		},
		{
			name: "invalid mTLS config",
			config: &SecurityConfig{
				Auth: &AuthenticationConfig{
					Method: AuthMethodMTLS,
				},
				TLS: &TLSSecurityConfig{
					Mode: TLSModeEnabled, // Should be MutualAuth for mTLS
				},
			},
			expectError: true,
			errorType:   SecurityErrorInvalidConfig,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			manager, err := NewSecurityManager(tt.config)
			if err != nil {
				// Creation might fail for some configs
				if !tt.expectError {
					t.Errorf("Unexpected error creating manager: %v", err)
				}
				return
			}
			defer manager.Close()

			ctx := context.Background()
			_, err = manager.Authenticate(ctx)

			if tt.expectError {
				if err == nil {
					t.Error("Expected authentication to fail")
					return
				}

				// Check if it's the right type of security error
				if secErr, ok := AsSecurityError(err); ok {
					if secErr.Type != tt.errorType {
						t.Errorf("Expected error type %v, got %v", tt.errorType, secErr.Type)
					}
				} else {
					t.Errorf("Expected SecurityError, got %T", err)
				}
			} else if err != nil {
				t.Errorf("Unexpected authentication error: %v", err)
			}
		})
	}
}

// TestSecurityIntegration_PermissionCaching tests ACL permission caching
func TestSecurityIntegration_PermissionCaching(t *testing.T) {
	config := &ACLConfig{
		Enabled:             true,
		ControllerEndpoints: []string{"localhost:9094"},
		RequestTimeout:      2 * time.Second,
		Cache: &ACLCacheConfig{
			Size:                10,
			TTL:                 30 * time.Second,
			CleanupInterval:     5 * time.Second,
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

	// First request - cache miss
	start := time.Now()
	permissions1, err := cache.GetPermissions(ctx, principal)
	if err != nil {
		t.Fatalf("Failed to get permissions: %v", err)
	}
	firstDuration := time.Since(start)

	// Second request - cache hit (should be faster)
	start = time.Now()
	permissions2, err := cache.GetPermissions(ctx, principal)
	if err != nil {
		t.Fatalf("Failed to get permissions: %v", err)
	}
	secondDuration := time.Since(start)

	// Verify permissions are the same
	if permissions1 == nil || permissions2 == nil {
		t.Fatal("Permissions should not be nil")
	}

	// Second request should generally be faster (cached)
	t.Logf("First request: %v, Second request: %v", firstDuration, secondDuration)

	// Check cache statistics
	stats := cache.GetStats()
	if stats.Size == 0 {
		t.Error("Cache should contain entries")
	}
}

// TestSecurityIntegration_CircuitBreaker tests circuit breaker functionality
func TestSecurityIntegration_CircuitBreaker(t *testing.T) {
	config := &ACLCircuitBreakerConfig{
		FailureThreshold: 2,
		SuccessThreshold: 2,
		Timeout:          100 * time.Millisecond,
	}

	cb := NewACLCircuitBreaker(config)

	// Initially closed
	if !cb.CanExecute() || cb.GetState() != ACLCircuitBreakerClosed {
		t.Error("Circuit breaker should start in closed state")
	}

	// Record failures to open
	cb.RecordFailure()
	cb.RecordFailure()

	if cb.CanExecute() || cb.GetState() != ACLCircuitBreakerOpen {
		t.Error("Circuit breaker should be open after threshold failures")
	}

	// Wait for timeout
	time.Sleep(150 * time.Millisecond)

	// Should be half-open now
	if !cb.CanExecute() || cb.GetState() != ACLCircuitBreakerHalfOpen {
		t.Error("Circuit breaker should be half-open after timeout")
	}

	// Record successes to close
	cb.RecordSuccess()
	cb.RecordSuccess()

	if !cb.CanExecute() || cb.GetState() != ACLCircuitBreakerClosed {
		t.Error("Circuit breaker should be closed after successful attempts")
	}
}

// TestSecurityIntegration_MetricsCollection tests comprehensive metrics collection
func TestSecurityIntegration_MetricsCollection(t *testing.T) {
	config := &SecurityMetricsConfig{
		Enabled:            true,
		CollectionInterval: 100 * time.Millisecond,
		Detailed:           true,
	}

	metrics := NewSecurityMetrics(config)
	defer metrics.Close()

	// Simulate various security operations
	metrics.RecordAuthentication("mtls", true)
	metrics.RecordAuthentication("mtls", false)
	metrics.RecordAuthentication("jwt", true)
	metrics.RecordAuthentication("sasl", true)

	metrics.RecordACLRequest(true, 5*time.Millisecond)
	metrics.RecordACLRequest(false, 20*time.Millisecond)
	metrics.RecordACLRequest(true, 3*time.Millisecond)

	metrics.RecordCertificateValidation(true, 10*time.Millisecond)
	metrics.RecordCertificateValidation(false, 50*time.Millisecond)

	metrics.RecordTLSConnection(true, 30*time.Millisecond)
	metrics.RecordTLSConnection(true, 25*time.Millisecond)

	metrics.RecordSecurityContextCreation()
	metrics.RecordSecurityContextCreation()

	// Wait for collection
	time.Sleep(200 * time.Millisecond)

	// Get snapshot
	snapshot := metrics.GetMetrics()

	// Verify authentication metrics
	totalAttempts := int64(0)
	totalSuccesses := int64(0)
	for method, authMetrics := range snapshot.AuthMetrics {
		t.Logf("Auth method %s: attempts=%d, successes=%d, failures=%d",
			method, authMetrics.Attempts, authMetrics.Successes, authMetrics.Failures)
		totalAttempts += authMetrics.Attempts
		totalSuccesses += authMetrics.Successes
	}

	if totalAttempts != 4 {
		t.Errorf("Expected 4 total auth attempts, got %d", totalAttempts)
	}
	if totalSuccesses != 3 {
		t.Errorf("Expected 3 total auth successes, got %d", totalSuccesses)
	}

	// Verify ACL metrics
	if snapshot.ACLMetrics.Requests != 3 {
		t.Errorf("Expected 3 ACL requests, got %d", snapshot.ACLMetrics.Requests)
	}
	if snapshot.ACLMetrics.CacheHits != 2 {
		t.Errorf("Expected 2 ACL cache hits, got %d", snapshot.ACLMetrics.CacheHits)
	}
	if snapshot.ACLMetrics.CacheMisses != 1 {
		t.Errorf("Expected 1 ACL cache miss, got %d", snapshot.ACLMetrics.CacheMisses)
	}

	// Verify rate calculations
	aclHitRate := snapshot.ACLMetrics.GetCacheHitRate()
	expectedHitRate := 2.0 / 3.0 // 2 hits out of 3 requests
	if aclHitRate < expectedHitRate-0.01 || aclHitRate > expectedHitRate+0.01 {
		t.Errorf("Expected ACL hit rate ~%.2f, got %.2f", expectedHitRate, aclHitRate)
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
	if snapshot.TLSMetrics.HandshakeErrors != 0 {
		t.Errorf("Expected 0 TLS handshake errors, got %d", snapshot.TLSMetrics.HandshakeErrors)
	}

	// Verify context metrics
	if snapshot.ContextMetrics.Active != 2 {
		t.Errorf("Expected 2 active contexts, got %d", snapshot.ContextMetrics.Active)
	}
	if snapshot.ContextMetrics.Creations != 2 {
		t.Errorf("Expected 2 context creations, got %d", snapshot.ContextMetrics.Creations)
	}
}

// TestSecurityIntegration_ErrorSummary tests security error analysis
func TestSecurityIntegration_ErrorSummary(t *testing.T) {
	errors := []*SecurityError{
		NewAuthenticationFailedError("auth failed", nil),
		NewAuthenticationFailedError("another auth failed", nil),
		NewAccessDeniedError("topic", "read"),
		NewCertificateExpiredError("2024-01-01"),
		NewTLSHandshakeError("handshake failed", nil),
		NewInvalidConfigError("field", "invalid"),
		NewServiceUnavailableError("acl", nil),
	}

	summary := CreateErrorSummary(errors)

	if summary.TotalErrors != 7 {
		t.Errorf("Expected 7 total errors, got %d", summary.TotalErrors)
	}

	if summary.AuthenticationRate != 2.0/7.0 {
		t.Errorf("Expected auth error rate %.2f, got %.2f", 2.0/7.0, summary.AuthenticationRate)
	}

	if summary.AuthorizationRate != 1.0/7.0 {
		t.Errorf("Expected authz error rate %.2f, got %.2f", 1.0/7.0, summary.AuthorizationRate)
	}

	if summary.MostCommonError != SecurityErrorAuthenticationFailed {
		t.Errorf("Expected most common error %v, got %v", SecurityErrorAuthenticationFailed, summary.MostCommonError)
	}

	// Test error type counts
	expectedCounts := map[SecurityErrorType]int{
		SecurityErrorAuthenticationFailed: 2,
		SecurityErrorAccessDenied:         1,
		SecurityErrorCertificateExpired:   1,
		SecurityErrorTLSHandshake:         1,
		SecurityErrorInvalidConfig:        1,
		SecurityErrorServiceUnavailable:   1,
	}

	for errorType, expectedCount := range expectedCounts {
		if actualCount, exists := summary.ErrorsByType[errorType]; !exists || actualCount != expectedCount {
			t.Errorf("Expected %d errors of type %v, got %d", expectedCount, errorType, actualCount)
		}
	}
}

// Benchmark integration tests

func BenchmarkSecurityIntegration_AuthenticationWorkflow(b *testing.B) {
	config := &SecurityConfig{
		Auth: &AuthenticationConfig{
			Method: AuthMethodJWT,
			Token:  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0LXVzZXIifQ.signature",
		},
		ACL: &ACLConfig{
			Enabled: true,
			Cache: &ACLCacheConfig{
				Size: 1000,
				TTL:  10 * time.Minute,
			},
		},
		Metrics: &SecurityMetricsConfig{
			Enabled: false, // Disable for benchmarking
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

func BenchmarkSecurityIntegration_PermissionCheck(b *testing.B) {
	perms := &PermissionSet{
		ReadTopics:      []string{"topic.*", "logs.*", "events.*"},
		WriteTopics:     []string{"output.*", "results.*"},
		AdminOperations: []string{"cluster.*", "topic.create"},
	}

	topics := []string{"topic.test", "logs.application", "events.user", "other.topic"}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		topic := topics[i%len(topics)]
		perms.CanReadTopic(topic)
		perms.CanWriteTopic(topic)
	}
}