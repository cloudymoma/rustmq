package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	// Example showing JWT token authentication with ACL authorization
	fmt.Println("=== RustMQ Go SDK - Secure Consumer with JWT Authentication ===")
	
	// JWT token (in production, this would be obtained from your auth service)
	jwtToken := "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJzZWN1cmUtY29uc3VtZXIiLCJuYW1lIjoiU2VjdXJlIENvbnN1bWVyIiwiaWF0IjoxNTE2MjM5MDIyLCJleHAiOjE3MzI4NDU0NTksInNjb3BlcyI6WyJyZWFkOnNlY3VyZS1ldmVudHMiLCJyZWFkOmxvZ3MtKiJdfQ.signature"

	// Create client configuration with JWT authentication
	config := &rustmq.ClientConfig{
		Brokers:           []string{"localhost:9092"},
		ClientID:          "secure-consumer-jwt",
		ConnectTimeout:    10 * time.Second,
		RequestTimeout:    30 * time.Second,
		EnableTLS:         true,
		MaxConnections:    3,
		KeepAliveInterval: 30 * time.Second,
		Security: &rustmq.SecurityConfig{
			// JWT Authentication
			Auth: &rustmq.AuthenticationConfig{
				Method: rustmq.AuthMethodJWT,
				Token:  jwtToken,
				TokenRefresh: &rustmq.TokenRefreshConfig{
					Enabled:          true,
					RefreshThreshold: 5 * time.Minute, // Refresh when token expires within 5 minutes
					RefreshEndpoint:  "https://auth.example.com/token/refresh",
					RefreshCredentials: map[string]string{
						"client_id":     "rustmq-consumer",
						"client_secret": "your-client-secret",
					},
				},
			},
			// TLS Configuration (server-side only, no client cert)
			TLS: &rustmq.TLSSecurityConfig{
				Mode:               rustmq.TLSModeEnabled,
				ServerName:         "rustmq-broker",
				InsecureSkipVerify: false,
				MinVersion:         0x0303, // TLS 1.2
			},
			// Enhanced ACL Configuration
			ACL: &rustmq.ACLConfig{
				Enabled:             true,
				ControllerEndpoints: []string{"localhost:9094", "localhost:9095"}, // Multiple endpoints for HA
				RequestTimeout:      10 * time.Second,
				Cache: &rustmq.ACLCacheConfig{
					Size:                2000,
					TTL:                 15 * time.Minute, // Longer TTL for stable permissions
					CleanupInterval:     2 * time.Minute,
					EnableDeduplication: true,
				},
				CircuitBreaker: &rustmq.ACLCircuitBreakerConfig{
					FailureThreshold: 3,  // More sensitive
					SuccessThreshold: 5,  // More conservative recovery
					Timeout:          60 * time.Second,
				},
			},
			// Principal Extraction for JWT
			PrincipalExtraction: &rustmq.PrincipalExtractionConfig{
				UseCommonName:     false, // Not applicable for JWT
				UseSubjectAltName: false,
				Normalize:         true,
				CustomRules: []rustmq.PrincipalExtractionRule{
					{
						Type:     "subject_pattern",
						Pattern:  `"sub":\s*"([^"]+)"`, // Extract from JWT subject claim
						Priority: 100,
					},
				},
			},
			// Detailed Security Metrics
			Metrics: &rustmq.SecurityMetricsConfig{
				Enabled:            true,
				CollectionInterval: 15 * time.Second,
				Detailed:           true,
				Export: &rustmq.MetricsExportConfig{
					Format:   "json",
					Endpoint: "http://metrics.example.com/security",
					Interval: 1 * time.Minute,
				},
			},
		},
	}

	// Create secure client
	fmt.Println("Creating secure client with JWT authentication...")
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	// Verify authentication
	securityContext := client.SecurityContext()
	fmt.Printf("✓ JWT Authentication successful!\n")
	fmt.Printf("  Principal: %s\n", securityContext.Principal)
	fmt.Printf("  Auth Method: %s\n", securityContext.AuthMethod)
	fmt.Printf("  Token Expires: %s\n", securityContext.ExpiresAt.Format(time.RFC3339))
	
	// Display JWT attributes
	if len(securityContext.Attributes) > 0 {
		fmt.Println("  JWT Attributes:")
		for key, value := range securityContext.Attributes {
			fmt.Printf("    %s: %v\n", key, value)
		}
	}

	// Check permissions for multiple topics
	topics := []string{"secure-events", "logs-application", "logs-system", "admin-events"}
	fmt.Println("\nChecking topic permissions:")
	for _, topic := range topics {
		canRead := client.CanReadTopic(topic)
		fmt.Printf("  %s: %s\n", topic, map[bool]string{true: "✓ READ", false: "✗ DENIED"}[canRead])
	}

	// Create consumer for authorized topic
	authorizedTopic := "secure-events"
	consumerGroup := "secure-consumer-group"
	
	if !client.CanReadTopic(authorizedTopic) {
		log.Fatalf("Permission denied: cannot read from topic '%s'", authorizedTopic)
	}

	fmt.Printf("\nCreating consumer for topic '%s'...\n", authorizedTopic)
	consumer, err := client.CreateConsumer(authorizedTopic, consumerGroup, &rustmq.ConsumerConfig{
		EnableAutoCommit:   true,
		AutoCommitInterval: 5 * time.Second,
		FetchSize:          1024,
		FetchTimeout:       500 * time.Millisecond,
		MaxRetryAttempts:   3,
	})
	if err != nil {
		log.Fatalf("Failed to create consumer: %v", err)
	}
	defer consumer.Close()

	// Consume messages with security context
	fmt.Println("Starting secure message consumption...")
	fmt.Println("Waiting for messages (press Ctrl+C to stop)...")

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	messageCount := 0
	maxMessages := 10

	for {
		select {
		case <-ctx.Done():
			fmt.Println("\nTimeout reached, stopping consumer...")
			goto cleanup
		default:
			// Receive a message
			message, err := consumer.Receive(ctx)
			if err != nil {
				log.Printf("Error receiving message: %v", err)
				time.Sleep(1 * time.Second)
				continue
			}

			if message != nil {
				messageCount++
				fmt.Printf("\n--- Message %d ---\n", messageCount)
				fmt.Printf("Topic: %s\n", message.Message.Topic)
				fmt.Printf("Partition: %d\n", message.Partition())
				fmt.Printf("Offset: %d\n", message.Message.Offset)
				fmt.Printf("Key: %s\n", string(message.Message.Key))
				fmt.Printf("Payload: %s\n", string(message.Message.Payload))
				fmt.Printf("Timestamp: %s\n", message.Message.Timestamp.Format(time.RFC3339))
				
				if len(message.Message.Headers) > 0 {
					fmt.Println("Headers:")
					for k, v := range message.Message.Headers {
						fmt.Printf("  %s: %s\n", k, v)
					}
				}

				// Security audit logging
				fmt.Printf("Security Context:\n")
				fmt.Printf("  Consumer Principal: %s\n", securityContext.Principal)
				fmt.Printf("  Auth Method: %s\n", securityContext.AuthMethod)
				fmt.Printf("  Permission Verified: ✓ READ access to %s\n", message.Message.Topic)

				if messageCount >= maxMessages {
					fmt.Printf("\nReceived %d messages, stopping...\n", maxMessages)
					goto cleanup
				}
			}

			// Check if we need to refresh the token
			if time.Until(securityContext.ExpiresAt) < 5*time.Minute {
				fmt.Println("Token expiring soon, refreshing...")
				if err := client.RefreshSecurityContext(); err != nil {
					log.Printf("Failed to refresh token: %v", err)
				} else {
					securityContext = client.SecurityContext()
					fmt.Println("✓ Token refreshed successfully")
				}
			}
		}
	}

cleanup:
	// Display final security metrics
	fmt.Println("\n=== Final Security Metrics ===")
	if securityManager := client.SecurityManager(); securityManager != nil {
		metrics := securityManager.Metrics().GetMetrics()
		
		fmt.Printf("Session Summary:\n")
		fmt.Printf("  Messages Consumed: %d\n", messageCount)
		fmt.Printf("  Session Duration: %s\n", time.Since(securityContext.AuthTime))
		fmt.Printf("  Auth Method: %s\n", securityContext.AuthMethod)
		
		if authMetrics, exists := metrics.AuthMetrics[string(rustmq.AuthMethodJWT)]; exists {
			fmt.Printf("JWT Authentication:\n")
			fmt.Printf("  Total Attempts: %d\n", authMetrics.Attempts)
			fmt.Printf("  Successes: %d\n", authMetrics.Successes)
			fmt.Printf("  Success Rate: %.2f%%\n", authMetrics.GetSuccessRate()*100)
		}

		if metrics.ACLMetrics != nil {
			fmt.Printf("Authorization:\n")
			fmt.Printf("  ACL Requests: %d\n", metrics.ACLMetrics.Requests)
			fmt.Printf("  Cache Efficiency: %.2f%%\n", metrics.ACLMetrics.GetCacheHitRate()*100)
			if metrics.ACLMetrics.Errors > 0 {
				fmt.Printf("  ⚠️  ACL Errors: %d\n", metrics.ACLMetrics.Errors)
			}
		}

		if metrics.TLSMetrics != nil {
			fmt.Printf("TLS Security:\n")
			fmt.Printf("  Secure Connections: %d\n", metrics.TLSMetrics.Connections)
			if metrics.TLSMetrics.HandshakeErrors > 0 {
				fmt.Printf("  ⚠️  Handshake Errors: %d\n", metrics.TLSMetrics.HandshakeErrors)
			}
		}

		fmt.Printf("Security Context:\n")
		fmt.Printf("  Active Contexts: %d\n", metrics.ContextMetrics.Active)
		fmt.Printf("  Context Creations: %d\n", metrics.ContextMetrics.Creations)
	}

	fmt.Println("\n✓ Secure consumer example completed successfully!")
	fmt.Println("This example demonstrated:")
	fmt.Println("  - JWT token authentication with automatic refresh")
	fmt.Println("  - ACL-based topic access control")
	fmt.Println("  - Multi-endpoint controller failover")
	fmt.Println("  - Circuit breaker pattern for resilience")
	fmt.Println("  - Comprehensive security metrics and monitoring")
	fmt.Println("  - Token expiration handling and refresh")
	fmt.Println("  - Audit logging for security events")
}