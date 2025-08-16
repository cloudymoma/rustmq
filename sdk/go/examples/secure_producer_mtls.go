package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	// Example showing mTLS authentication with client certificates - Enhanced
	fmt.Println("=== RustMQ Go SDK - Secure Producer with mTLS Authentication (Enhanced) ===")
	
	// Read certificate and key files (in production, these would be proper file paths)
	clientCert := `-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAK8K8K8K8K8KMA0GCSqGSIb3DQEBCwUAMBUxEzARBgNVBAMMCnJ1
c3RtcS1jbGllbnQwHhcNMjQwMTAxMDAwMDAwWhcNMjUwMTAxMDAwMDAwWjAVMRMw
EQYDVQQDDApydXN0bXEtY2xpZW50MFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAK8K
8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K
8K8K8K8K8K8K8K8CAwEAATANBgkqhkiG9w0BAQsFAANBAK8K8K8K8K8K8K8K8K8K
8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8=
-----END CERTIFICATE-----`

	clientKey := `-----BEGIN PRIVATE KEY-----
MIIBVAIBADANBgkqhkiG9w0BAQEFAASCAT4wggE6AgEAAkEArwrwrwrwrwrwrwrw
rwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrw
rwIDAQABAkEArwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrw
rwrwrwrwrwrwrwrwrwrwrwrwrwrwrwQIhAK8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K
8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K
8K8KAiEArwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrw
rwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwQIhAK8K8K8K8K8K8K8K8K8K8K8K8K8K
8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K8K
8K8K8K8KAiEArwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrw
rwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwECIQCvCvCvCvCvCvCvCvCvCvCv
CvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCvCv
CvCvAg==
-----END PRIVATE KEY-----`

	caCert := `-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAK8K8K8K8K8KMA0GCSqGSIb3DQEBCwUAMBUxEzARBgNVBAMMCnJ1
c3RtcS1jYTA0HhcNMjQwMTAxMDAwMDAwWhcNMjUwMTAxMDAwMDAwWjAVMRMwEQYD
VQQDDApydXN0bXEtY2EwXDANBgkqhkiG9w0BAQEFAANLADBIAkEArwrwrwrwrwrw
rwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrw
rwrwrwrwrwIDAQABMA0GCSqGSIb3DQEBCwUAA0EArwrwrwrwrwrwrwrwrwrwrwrw
rwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrwrw=
-----END CERTIFICATE-----`

	// Create client configuration with comprehensive security settings
	config := &rustmq.ClientConfig{
		Brokers:           []string{"localhost:9092"},
		ClientID:          "secure-producer-mtls",
		ConnectTimeout:    10 * time.Second,
		RequestTimeout:    30 * time.Second,
		EnableTLS:         true,
		MaxConnections:    5,
		KeepAliveInterval: 30 * time.Second,
		Security: &rustmq.SecurityConfig{
			// mTLS Authentication
			Auth: &rustmq.AuthenticationConfig{
				Method: rustmq.AuthMethodMTLS,
			},
			// Comprehensive TLS Configuration
			TLS: &rustmq.TLSSecurityConfig{
				Mode:               rustmq.TLSModeMutualAuth,
				CACert:             caCert,
				ClientCert:         clientCert,
				ClientKey:          clientKey,
				ServerName:         "rustmq-broker",
				InsecureSkipVerify: false,
				MinVersion:         0x0303, // TLS 1.2
				MaxVersion:         0x0304, // TLS 1.3
				Validation: &rustmq.CertificateValidationSettings{
					ValidateChain:   true,
					CheckExpiration: true,
					CheckRevocation: false, // Disable for demo
					// Advanced enhancements
					EnableWebPKI:            true,
					EnableCaching:           true,
					CacheTTL:               10 * time.Minute,
					CacheSize:              500,
					EnableBatchValidation:   true,
					PerformanceTargetUs:     245, // Advanced target: 245μs
				},
			},
			// ACL Configuration
			ACL: &rustmq.ACLConfig{
				Enabled:             true,
				ControllerEndpoints: []string{"localhost:9094"},
				RequestTimeout:      5 * time.Second,
				Cache: &rustmq.ACLCacheConfig{
					Size:                1000,
					TTL:                 10 * time.Minute,
					CleanupInterval:     1 * time.Minute,
					EnableDeduplication: true,
				},
				CircuitBreaker: &rustmq.ACLCircuitBreakerConfig{
					FailureThreshold: 5,
					SuccessThreshold: 3,
					Timeout:          30 * time.Second,
				},
			},
			// Certificate Validation
			CertValidation: &rustmq.CertificateValidationConfig{
				Enabled: true,
				Rules: []rustmq.CertificateValidationRule{
					{
						Type: "key_usage",
						Parameters: map[string]interface{}{
							"usage": "client_auth",
						},
					},
				},
			},
			// Principal Extraction
			PrincipalExtraction: &rustmq.PrincipalExtractionConfig{
				UseCommonName:     true,
				UseSubjectAltName: false,
				Normalize:         true,
				CustomRules: []rustmq.PrincipalExtractionRule{
					{
						Type:     "subject_attribute",
						Pattern:  "CN",
						Priority: 100,
					},
				},
			},
			// Security Metrics with advanced enhancements
			Metrics: &rustmq.SecurityMetricsConfig{
				Enabled:            true,
				CollectionInterval: 30 * time.Second,
				Detailed:           true,
				// Advanced performance tracking
				EnableAdvancedMetrics: true,
				TrackCertValidation: true,
				TrackACLLookup:      true,
				TrackWebPKIUsage:    true,
			},
		},
	}

	// Create client with security
	fmt.Println("Creating secure client with mTLS authentication...")
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	// Verify security context
	if !client.IsSecured() {
		log.Fatal("Expected client to be secured, but it's not")
	}

	securityContext := client.SecurityContext()
	fmt.Printf("✓ Authentication successful!\n")
	fmt.Printf("  Principal: %s\n", securityContext.Principal)
	fmt.Printf("  Auth Method: %s\n", securityContext.AuthMethod)
	fmt.Printf("  Auth Time: %s\n", securityContext.AuthTime.Format(time.RFC3339))
	fmt.Printf("  Expires At: %s\n", securityContext.ExpiresAt.Format(time.RFC3339))
	
	if securityContext.CertificateInfo != nil {
		fmt.Printf("  Certificate Subject: %s\n", securityContext.CertificateInfo.Subject)
		fmt.Printf("  Certificate Issuer: %s\n", securityContext.CertificateInfo.Issuer)
		fmt.Printf("  Certificate Valid Until: %s\n", securityContext.CertificateInfo.NotAfter.Format(time.RFC3339))
	}

	// Check topic permissions
	topicName := "secure-events"
	if !client.CanWriteTopic(topicName) {
		log.Fatalf("Permission denied: cannot write to topic '%s'", topicName)
	}
	fmt.Printf("✓ Permission granted to write to topic '%s'\n", topicName)

	// Create producer
	fmt.Println("\nCreating secure producer...")
	producer, err := client.CreateProducer(topicName)
	if err != nil {
		log.Fatalf("Failed to create producer: %v", err)
	}
	defer producer.Close()

	// Send secure messages
	fmt.Println("Sending secure messages...")
	ctx := context.Background()
	
	for i := 0; i < 5; i++ {
		message := &rustmq.Message{
			Topic:     topicName,
			Key:       []byte(fmt.Sprintf("secure-key-%d", i)),
			Payload:   []byte(fmt.Sprintf("Secure message %d from mTLS authenticated client", i)),
			Headers: map[string]string{
				"client-id":     config.ClientID,
				"auth-method":   string(securityContext.AuthMethod),
				"principal":     securityContext.Principal,
				"security-enabled": "true",
			},
			Timestamp: time.Now(),
		}

		_, err := producer.Send(ctx, message)
		if err != nil {
			log.Printf("Failed to send message %d: %v", i, err)
			continue
		}

		fmt.Printf("✓ Sent secure message %d\n", i)
		time.Sleep(1 * time.Second)
	}

	// Display security metrics with advanced performance data
	fmt.Println("\n=== Security Metrics (Enhanced) ===")
	if securityManager := client.SecurityManager(); securityManager != nil {
		metrics := securityManager.Metrics().GetMetrics()
		fmt.Printf("Collection Count: %d\n", metrics.CollectionCount)
		fmt.Printf("Uptime: %s\n", metrics.Uptime)
		
		for method, authMetrics := range metrics.AuthMetrics {
			fmt.Printf("Auth Method '%s':\n", method)
			fmt.Printf("  Attempts: %d\n", authMetrics.Attempts)
			fmt.Printf("  Successes: %d\n", authMetrics.Successes)
			fmt.Printf("  Failures: %d\n", authMetrics.Failures)
			fmt.Printf("  Success Rate: %.2f%%\n", authMetrics.GetSuccessRate()*100)
		}
		
		if metrics.ACLMetrics != nil {
			fmt.Printf("ACL Metrics:\n")
			fmt.Printf("  Requests: %d\n", metrics.ACLMetrics.Requests)
			fmt.Printf("  Cache Hits: %d\n", metrics.ACLMetrics.CacheHits)
			fmt.Printf("  Cache Misses: %d\n", metrics.ACLMetrics.CacheMisses)
			fmt.Printf("  Cache Hit Rate: %.2f%%\n", metrics.ACLMetrics.GetCacheHitRate()*100)
		}
		
		if metrics.CertificateMetrics != nil {
			fmt.Printf("Certificate Metrics:\n")
			fmt.Printf("  Validations: %d\n", metrics.CertificateMetrics.Validations)
			fmt.Printf("  Errors: %d\n", metrics.CertificateMetrics.Errors)
			fmt.Printf("  Error Rate: %.2f%%\n", metrics.CertificateMetrics.GetErrorRate()*100)
		}
		
		if metrics.TLSMetrics != nil {
			fmt.Printf("TLS Metrics:\n")
			fmt.Printf("  Connections: %d\n", metrics.TLSMetrics.Connections)
			fmt.Printf("  Handshake Errors: %d\n", metrics.TLSMetrics.HandshakeErrors)
			fmt.Printf("  Handshake Error Rate: %.2f%%\n", metrics.TLSMetrics.GetHandshakeErrorRate()*100)
		}
		
		// Display advanced performance metrics
		if securityManager.IsAdvancedFeaturesEnabled() {
			fmt.Println("\n=== Advanced Performance Metrics ===")
			advancedMetrics := securityManager.GetPerformanceMetrics()
			fmt.Printf("Certificate Validation Target: %v\n", advancedMetrics.CertValidationTarget)
			fmt.Printf("ACL Lookup Target: %v\n", advancedMetrics.ACLLookupTarget)
			fmt.Printf("Performance Target Met: %v\n", advancedMetrics.PerformanceTargetMet)
			fmt.Printf("WebPKI Validations: %d\n", advancedMetrics.WebPKIValidationCount)
			fmt.Printf("Cache Hit Rate: %.2f%%\n", advancedMetrics.CacheHitRate*100)
		}
	}

	fmt.Println("\n✓ Secure producer example completed successfully!")
	fmt.Println("This example demonstrated:")
	fmt.Println("  - mTLS client certificate authentication with advanced enhancements")
	fmt.Println("  - WebPKI-based certificate validation with advanced caching")
	fmt.Println("  - ACL-based authorization with intelligent caching (target: 1200ns)")
	fmt.Println("  - Certificate validation performance optimization (target: 245μs)")
	fmt.Println("  - Advanced performance metrics and monitoring")
	fmt.Println("  - Batch certificate operations and prefetching")
	fmt.Println("  - Enterprise-grade security controls with performance optimization")
}