package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	// Check for required certificate files
	caCertPath := os.Getenv("RUSTMQ_CA_CERT")
	clientCertPath := os.Getenv("RUSTMQ_CLIENT_CERT") 
	clientKeyPath := os.Getenv("RUSTMQ_CLIENT_KEY")
	
	if caCertPath == "" || clientCertPath == "" || clientKeyPath == "" {
		fmt.Println("📋 Required environment variables:")
		fmt.Println("   export RUSTMQ_CA_CERT=/path/to/ca.pem")
		fmt.Println("   export RUSTMQ_CLIENT_CERT=/path/to/client.pem")
		fmt.Println("   export RUSTMQ_CLIENT_KEY=/path/to/client-key.pem")
		fmt.Println()
		fmt.Println("💡 Generate certificates using:")
		fmt.Println("   ./target/release/rustmq-admin ca init --cn \"RustMQ Root CA\"")
		fmt.Println("   ./target/release/rustmq-admin cert create --cn \"client@company.com\"")
		os.Exit(1)
	}

	// Production mTLS configuration
	config := &rustmq.ClientConfig{
		Brokers:           []string{"localhost:9093"}, // TLS port
		ClientID:          "secure-producer-mtls",
		ConnectTimeout:    30 * time.Second,
		RequestTimeout:    15 * time.Second,
		EnableTLS:         true,
		TLSConfig: &rustmq.TLSConfig{
			CACert:             caCertPath,
			ClientCert:         clientCertPath,
			ClientKey:          clientKeyPath,
			ServerName:         "localhost", // Must match certificate
			InsecureSkipVerify: false, // Always verify in production
		},
		MaxConnections:    10,
		KeepAliveInterval: 30 * time.Second,
		RetryConfig: &rustmq.RetryConfig{
			MaxRetries: 3,
			BaseDelay:  500 * time.Millisecond,
			MaxDelay:   10 * time.Second,
			Multiplier: 2.0,
			Jitter:     true,
		},
		// Advanced security configuration
		Security: &rustmq.SecurityConfig{
			Auth: &rustmq.AuthenticationConfig{
				Method: rustmq.AuthMethodMTLS, // Use mTLS authentication
			},
			TLS: &rustmq.TLSSecurityConfig{
				Mode:               rustmq.TLSModeMutualAuth,
				CACert:             caCertPath,
				ClientCert:         clientCertPath,
				ClientKey:          clientKeyPath,
				ServerName:         "localhost",
				InsecureSkipVerify: false,
				MinVersion:         13, // TLS 1.3
				CipherSuites:       []uint16{
					0x1302, // TLS_AES_256_GCM_SHA384
					0x1303, // TLS_CHACHA20_POLY1305_SHA256
				},
			},
		},
	}

	fmt.Println("🔐 Creating secure mTLS connection to RustMQ...")
	
	// Create secure client
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create secure client: %v", err)
	}
	defer client.Close()

	fmt.Println("✅ Secure mTLS connection established")
	
	// Verify security context
	if !client.IsSecured() {
		log.Fatal("❌ Security not properly initialized")
	}
	
	secCtx := client.SecurityContext()
	if secCtx != nil {
		fmt.Printf("🔑 Authenticated as: %s\n", secCtx.Principal)
		if secCtx.Permissions != nil {
			fmt.Printf("📝 Permissions available\n")
		}
	}

	// Health check with security
	if err := client.HealthCheck(); err != nil {
		log.Fatalf("Secure health check failed: %v", err)
	}
	fmt.Println("✅ Secure health check passed")

	// Verify topic permissions before creating producer
	topic := "secure.financial.transactions"
	if !client.CanWriteTopic(topic) {
		log.Fatalf("❌ No write permission for topic: %s", topic)
	}
	fmt.Printf("✅ Write permission verified for topic: %s\n", topic)

	// Production-grade secure producer
	producerConfig := &rustmq.ProducerConfig{
		ProducerID:     "secure-financial-producer",
		BatchSize:      10, // Smaller batches for sensitive data
		BatchTimeout:   100 * time.Millisecond,
		MaxMessageSize: 512 * 1024, // 512KB max for financial data
		AckLevel:       rustmq.AckAll, // Maximum durability for financial data
		Idempotent:     true, // Prevent duplicate transactions
		DefaultProps: map[string]string{
			"data-classification": "confidential",
			"encryption":          "enabled",
			"audit-required":      "true",
			"compliance":          "PCI-DSS",
			"producer-cert":       clientCertPath,
		},
	}

	producer, err := client.CreateProducer(topic, producerConfig)
	if err != nil {
		log.Fatalf("Failed to create secure producer: %v", err)
	}
	defer producer.Close()

	fmt.Printf("🏦 Secure producer created for financial data: %s\n", producer.ID())

	// Send secure financial transaction messages
	transactions := []struct {
		id     string
		amount float64
		from   string
		to     string
	}{
		{"TXN-001", 1250.50, "account-12345", "account-67890"},
		{"TXN-002", 750.25, "account-67890", "account-11111"},
		{"TXN-003", 2500.00, "account-11111", "account-22222"},
	}

	fmt.Println("\n💰 Sending secure financial transactions...")

	for _, txn := range transactions {
		// Build secure message with encryption headers
		message := rustmq.NewMessage().
			Topic(topic).
			KeyString(txn.id). // Partition by transaction ID
			PayloadString(fmt.Sprintf(`{
				"transaction_id": "%s",
				"amount": %.2f,
				"from_account": "%s",
				"to_account": "%s",
				"timestamp": "%s",
				"currency": "USD",
				"type": "transfer"
			}`, txn.id, txn.amount, txn.from, txn.to, time.Now().Format(time.RFC3339))).
			Header("transaction-id", txn.id).
			Header("message-type", "financial.transaction").
			Header("encryption", "required").
			Header("classification", "confidential").
			Header("audit-trail", "enabled").
			Header("compliance-flags", "PCI-DSS,SOX").
			Header("client-cert-fingerprint", "sha256:..."). // In production, add actual fingerprint
			Header("correlation-id", fmt.Sprintf("corr-%s-%d", txn.id, time.Now().Unix())).
			Build()

		// Send with secure context
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		result, err := producer.Send(ctx, message)
		cancel()

		if err != nil {
			fmt.Printf("❌ Failed to send transaction %s: %v\n", txn.id, err)
		} else {
			fmt.Printf("✅ Transaction %s sent securely: offset=%d, partition=%d\n",
				txn.id, result.Offset, result.Partition)
		}

		// Small delay between sensitive transactions
		time.Sleep(500 * time.Millisecond)
	}

	// Secure flush
	fmt.Println("\n🔄 Securely flushing remaining transactions...")
	flushCtx, flushCancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer flushCancel()
	
	if err := producer.Flush(flushCtx); err != nil {
		fmt.Printf("⚠️ Secure flush error: %v\n", err)
	} else {
		fmt.Println("✅ All transactions securely flushed")
	}

	// Security metrics
	pMetrics := producer.Metrics()
	fmt.Printf("\n🔐 Secure Producer Metrics:\n")
	fmt.Printf("   Messages sent: %d\n", pMetrics.MessagesSent)
	fmt.Printf("   Messages failed: %d\n", pMetrics.MessagesFailed)
	fmt.Printf("   Bytes sent: %d\n", pMetrics.BytesSent)
	fmt.Printf("   Security level: mTLS with client certificates\n")
	
	// Connection security info
	stats := client.Stats()
	fmt.Printf("\n🛡️ Secure Connection Stats:\n")
	fmt.Printf("   Encrypted bytes sent: %d\n", stats.BytesSent)
	fmt.Printf("   Encrypted bytes received: %d\n", stats.BytesReceived)
	fmt.Printf("   TLS errors: %d\n", stats.Errors)
	
	fmt.Println("\n🔒 Secure mTLS producer demonstration completed")
	fmt.Println("   All financial data transmitted with:")
	fmt.Println("   • mTLS mutual authentication")
	fmt.Println("   • TLS 1.3 encryption")
	fmt.Println("   • Certificate-based authorization")
	fmt.Println("   • Audit trail enabled")
	fmt.Println("   • Compliance headers included")
}