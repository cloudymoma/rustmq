package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	// Example showing SASL SCRAM-SHA-256 authentication for admin operations
	fmt.Println("=== RustMQ Go SDK - Secure Admin Operations with SASL Authentication ===")

	// Create client configuration with SASL SCRAM-SHA-256 authentication
	config := &rustmq.ClientConfig{
		Brokers:           []string{"localhost:9092"},
		ClientID:          "secure-admin-sasl",
		ConnectTimeout:    15 * time.Second,
		RequestTimeout:    60 * time.Second, // Longer timeout for admin operations
		EnableTLS:         true,
		MaxConnections:    2,
		KeepAliveInterval: 45 * time.Second,
		Security: &rustmq.SecurityConfig{
			// SASL SCRAM-SHA-256 Authentication
			Auth: &rustmq.AuthenticationConfig{
				Method:   rustmq.AuthMethodSASLScram256,
				Username: "admin-user",
				Password: "secure-admin-password",
				Properties: map[string]string{
					"sasl.mechanism": "SCRAM-SHA-256",
					"sasl.jaas.config": "org.apache.kafka.common.security.scram.ScramLoginModule required username=\"admin-user\" password=\"secure-admin-password\";",
				},
			},
			// TLS Configuration for secure transport
			TLS: &rustmq.TLSSecurityConfig{
				Mode:               rustmq.TLSModeEnabled,
				ServerName:         "rustmq-broker",
				InsecureSkipVerify: false,
				MinVersion:         0x0303, // TLS 1.2
				MaxVersion:         0x0304, // TLS 1.3
			},
			// Enhanced ACL Configuration for admin operations
			ACL: &rustmq.ACLConfig{
				Enabled:             true,
				ControllerEndpoints: []string{"localhost:9094"},
				RequestTimeout:      15 * time.Second, // Longer timeout for admin ops
				Cache: &rustmq.ACLCacheConfig{
					Size:                500,  // Smaller cache for admin clients
					TTL:                 5 * time.Minute, // Shorter TTL for admin permissions
					CleanupInterval:     30 * time.Second,
					EnableDeduplication: true,
				},
				CircuitBreaker: &rustmq.ACLCircuitBreakerConfig{
					FailureThreshold: 2,  // Very sensitive for admin operations
					SuccessThreshold: 3,
					Timeout:          30 * time.Second,
				},
			},
			// Certificate Validation (for TLS server cert)
			CertValidation: &rustmq.CertificateValidationConfig{
				Enabled: true,
				Rules: []rustmq.CertificateValidationRule{
					{
						Type: "subject_pattern",
						Parameters: map[string]interface{}{
							"pattern": ".*rustmq.*",
						},
					},
				},
			},
			// Principal Extraction for SASL
			PrincipalExtraction: &rustmq.PrincipalExtractionConfig{
				UseCommonName:     false,
				UseSubjectAltName: false,
				Normalize:         true,
			},
			// Detailed Security Metrics for admin operations
			Metrics: &rustmq.SecurityMetricsConfig{
				Enabled:            true,
				CollectionInterval: 10 * time.Second,
				Detailed:           true,
			},
		},
	}

	// Create secure admin client
	fmt.Println("Creating secure admin client with SASL SCRAM-SHA-256 authentication...")
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create admin client: %v", err)
	}
	defer client.Close()

	// Verify authentication and admin permissions
	securityContext := client.SecurityContext()
	fmt.Printf("✓ SASL Authentication successful!\n")
	fmt.Printf("  Principal: %s\n", securityContext.Principal)
	fmt.Printf("  Auth Method: %s\n", securityContext.AuthMethod)
	fmt.Printf("  Auth Time: %s\n", securityContext.AuthTime.Format(time.RFC3339))
	
	// Display SASL attributes
	if len(securityContext.Attributes) > 0 {
		fmt.Println("  SASL Attributes:")
		for key, value := range securityContext.Attributes {
			fmt.Printf("    %s: %v\n", key, value)
		}
	}

	// Check admin permissions
	adminOperations := []string{
		"cluster.health",
		"topic.create",
		"topic.delete",
		"topic.describe",
		"consumer.list",
		"offset.reset",
		"config.update",
	}

	fmt.Println("\nChecking admin operation permissions:")
	authorizedOps := []string{}
	for _, operation := range adminOperations {
		canPerform := client.CanPerformAdminOperation(operation)
		status := map[bool]string{true: "✓ ALLOWED", false: "✗ DENIED"}[canPerform]
		fmt.Printf("  %s: %s\n", operation, status)
		if canPerform {
			authorizedOps = append(authorizedOps, operation)
		}
	}

	if len(authorizedOps) == 0 {
		log.Fatal("No admin operations authorized for this principal")
	}

	// Simulate admin operations
	fmt.Println("\n=== Performing Authorized Admin Operations ===")
	ctx := context.Background()

	// 1. Cluster health check
	if client.CanPerformAdminOperation("cluster.health") {
		fmt.Println("\n1. Checking cluster health...")
		if err := performClusterHealthCheck(ctx, client); err != nil {
			log.Printf("Cluster health check failed: %v", err)
		} else {
			fmt.Println("✓ Cluster health check completed")
		}
	}

	// 2. Topic management operations
	if client.CanPerformAdminOperation("topic.create") {
		fmt.Println("\n2. Creating test topic...")
		if err := performTopicCreate(ctx, client, "admin-test-topic"); err != nil {
			log.Printf("Topic creation failed: %v", err)
		} else {
			fmt.Println("✓ Topic created successfully")
		}
	}

	if client.CanPerformAdminOperation("topic.describe") {
		fmt.Println("\n3. Describing topics...")
		if err := performTopicDescribe(ctx, client); err != nil {
			log.Printf("Topic describe failed: %v", err)
		} else {
			fmt.Println("✓ Topic description completed")
		}
	}

	// 3. Consumer group operations
	if client.CanPerformAdminOperation("consumer.list") {
		fmt.Println("\n4. Listing consumer groups...")
		if err := performConsumerGroupList(ctx, client); err != nil {
			log.Printf("Consumer group listing failed: %v", err)
		} else {
			fmt.Println("✓ Consumer group listing completed")
		}
	}

	// 4. Configuration operations
	if client.CanPerformAdminOperation("config.update") {
		fmt.Println("\n5. Updating broker configuration...")
		if err := performConfigUpdate(ctx, client); err != nil {
			log.Printf("Config update failed: %v", err)
		} else {
			fmt.Println("✓ Configuration update completed")
		}
	}

	// Monitor security during admin operations
	fmt.Println("\n=== Security Monitoring ===")
	time.Sleep(2 * time.Second) // Allow metrics to update

	if securityManager := client.SecurityManager(); securityManager != nil {
		metrics := securityManager.Metrics().GetMetrics()
		
		fmt.Printf("Admin Session Summary:\n")
		fmt.Printf("  Principal: %s\n", securityContext.Principal)
		fmt.Printf("  Session Duration: %s\n", time.Since(securityContext.AuthTime))
		fmt.Printf("  Operations Authorized: %d/%d\n", len(authorizedOps), len(adminOperations))
		
		if authMetrics, exists := metrics.AuthMetrics[string(rustmq.AuthMethodSASLScram256)]; exists {
			fmt.Printf("SASL Authentication:\n")
			fmt.Printf("  Attempts: %d\n", authMetrics.Attempts)
			fmt.Printf("  Success Rate: %.2f%%\n", authMetrics.GetSuccessRate()*100)
			
			if authMetrics.Duration != nil {
				fmt.Printf("  Average Auth Time: %s\n", authMetrics.Duration.Average)
			}
		}

		if metrics.ACLMetrics != nil && metrics.ACLMetrics.Requests > 0 {
			fmt.Printf("Authorization Metrics:\n")
			fmt.Printf("  ACL Requests: %d\n", metrics.ACLMetrics.Requests)
			fmt.Printf("  Cache Hit Rate: %.2f%%\n", metrics.ACLMetrics.GetCacheHitRate()*100)
			if metrics.ACLMetrics.Errors > 0 {
				fmt.Printf("  ⚠️  Authorization Errors: %d\n", metrics.ACLMetrics.Errors)
			}
		}

		if metrics.TLSMetrics != nil {
			fmt.Printf("TLS Security:\n")
			fmt.Printf("  Secure Connections: %d\n", metrics.TLSMetrics.Connections)
			if metrics.TLSMetrics.HandshakeErrors > 0 {
				fmt.Printf("  ⚠️  TLS Errors: %d\n", metrics.TLSMetrics.HandshakeErrors)
			}
		}
	}

	// Security audit summary
	fmt.Println("\n=== Security Audit Summary ===")
	fmt.Printf("✓ Authentication: SASL SCRAM-SHA-256\n")
	fmt.Printf("✓ Transport Security: TLS 1.2+\n")
	fmt.Printf("✓ Authorization: ACL-based with caching\n")
	fmt.Printf("✓ Principal: %s\n", securityContext.Principal)
	fmt.Printf("✓ Admin Operations: %d authorized\n", len(authorizedOps))
	fmt.Printf("✓ Session Monitoring: Active\n")

	fmt.Println("\n✓ Secure admin operations example completed successfully!")
	fmt.Println("This example demonstrated:")
	fmt.Println("  - SASL SCRAM-SHA-256 authentication for admin access")
	fmt.Println("  - Fine-grained authorization for admin operations")
	fmt.Println("  - Secure transport with TLS encryption")
	fmt.Println("  - Real-time security monitoring and audit logging")
	fmt.Println("  - Circuit breaker patterns for admin service resilience")
	fmt.Println("  - Comprehensive security metrics for admin activities")
}

// Simulated admin operations (these would call actual RustMQ admin APIs)

func performClusterHealthCheck(ctx context.Context, client *rustmq.Client) error {
	fmt.Println("  Checking broker connectivity...")
	fmt.Println("  Verifying controller status...")
	fmt.Println("  Validating partition leadership...")
	
	// Simulate health check
	time.Sleep(500 * time.Millisecond)
	
	fmt.Println("  Cluster Status: ✓ Healthy")
	fmt.Println("  Active Brokers: 3")
	fmt.Println("  Controller: broker-1")
	fmt.Println("  Partition Replicas: ✓ In Sync")
	
	return nil
}

func performTopicCreate(ctx context.Context, client *rustmq.Client, topicName string) error {
	fmt.Printf("  Creating topic: %s\n", topicName)
	fmt.Println("  Partitions: 3")
	fmt.Println("  Replication Factor: 2")
	fmt.Println("  Configuration: retention.ms=604800000, cleanup.policy=delete")
	
	// Simulate topic creation
	time.Sleep(300 * time.Millisecond)
	
	return nil
}

func performTopicDescribe(ctx context.Context, client *rustmq.Client) error {
	fmt.Println("  Scanning cluster for topics...")
	
	topics := []string{"secure-events", "logs-application", "admin-test-topic"}
	for _, topic := range topics {
		fmt.Printf("  Topic: %s\n", topic)
		fmt.Printf("    Partitions: 3, Replicas: 2\n")
		fmt.Printf("    Leader: broker-1, ISR: [1,2]\n")
	}
	
	return nil
}

func performConsumerGroupList(ctx context.Context, client *rustmq.Client) error {
	fmt.Println("  Scanning for active consumer groups...")
	
	groups := []string{"secure-consumer-group", "analytics-group", "monitoring-group"}
	for _, group := range groups {
		fmt.Printf("  Consumer Group: %s\n", group)
		fmt.Printf("    State: Stable, Members: 2\n")
		fmt.Printf("    Coordinator: broker-2\n")
	}
	
	return nil
}

func performConfigUpdate(ctx context.Context, client *rustmq.Client) error {
	fmt.Println("  Updating broker configuration...")
	fmt.Println("  Setting: log.retention.hours=168")
	fmt.Println("  Setting: compression.type=lz4")
	
	// Simulate config update
	time.Sleep(400 * time.Millisecond)
	
	fmt.Println("  Configuration changes applied to all brokers")
	
	return nil
}