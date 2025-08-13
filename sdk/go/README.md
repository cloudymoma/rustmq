# RustMQ Go SDK

High-performance Go client library for RustMQ message queue system with QUIC transport.

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Installation](#installation)
- [Building](#building)
- [Configuration](#configuration)
- [Security](#security)
- [Quick Start](#quick-start)
- [Usage Guide](#usage-guide)
- [Performance Tuning](#performance-tuning)
- [Best Practices](#best-practices)
- [Testing](#testing)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)
- [API Reference](#api-reference)
- [Contributing](#contributing)

## Overview

The RustMQ Go SDK provides a native, high-performance client library for interacting with RustMQ message queues. Built with modern Go patterns and QUIC transport, it offers efficient connection pooling, automatic retries, and comprehensive streaming capabilities.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    RustMQ Go SDK                           │
├─────────────────────────────────────────────────────────────┤
│  Client                                                     │
│  ├── Producer (Batching, Compression)                       │
│  ├── Consumer (Streaming, Auto-commit)                      │
│  ├── MessageStream (Real-time Processing)                   │
│  └── Connection (QUIC Pool, Health Check)                   │
├─────────────────────────────────────────────────────────────┤
│  QUIC Transport Layer (quic-go)                             │
├─────────────────────────────────────────────────────────────┤
│  RustMQ Broker Cluster                                      │
└─────────────────────────────────────────────────────────────┘
```

## Features

- **🚀 High Performance**: Built with QUIC transport for low latency
- **🔌 Connection Pooling**: Efficient connection management and reuse
- **📦 Message Batching**: Configurable batching for high throughput
- **🗜️ Compression**: Multiple compression algorithms (LZ4, Zstd, Gzip)
- **🔐 Enterprise Security**: Comprehensive security with mTLS, JWT, SASL authentication, ACL authorization, and certificate management
- **📊 Observability**: Built-in metrics and monitoring support
- **🎯 Type Safety**: Comprehensive type definitions and error handling
- **🌊 Streaming**: Real-time message processing pipelines
- **⚡ Goroutines**: Efficient concurrent processing
- **🔄 Auto-retry**: Configurable retry logic with backoff

## Installation

### From RustMQ Repository

Since this is part of the RustMQ repository, you can import it directly:

```go
import "github.com/rustmq/rustmq/sdk/go/rustmq"
```

### Go Module

Add to your `go.mod`:

```go
require github.com/rustmq/rustmq/sdk/go v0.1.0
```

### Prerequisites

- Go 1.21 or later
- Network access to RustMQ brokers

## Building

### Build Commands

```bash
# Build the SDK
go build ./...

# Run tests
go test ./...

# Run tests with coverage
go test -cover ./...

# Run benchmarks
go test -bench=. ./benchmarks/...

# Build examples
go build -o examples/producer examples/simple_producer.go
go build -o examples/consumer examples/simple_consumer.go

# Format code
go fmt ./...

# Run linter
golangci-lint run

# Generate documentation
go doc -all ./rustmq
```

### Cross-compilation

```bash
# Build for different platforms
GOOS=linux GOARCH=amd64 go build ./...
GOOS=windows GOARCH=amd64 go build ./...
GOOS=darwin GOARCH=arm64 go build ./...
```

### Build Tags

```bash
# Build with specific tags
go build -tags "debug" ./...
go build -tags "production" ./...
```

## Configuration

### Client Configuration

The Go SDK supports comprehensive configuration for production deployments:

```go
config := &rustmq.ClientConfig{
    Brokers:           []string{"localhost:9092", "localhost:9093"},
    ClientID:          "my-app",
    ConnectTimeout:    10 * time.Second,
    RequestTimeout:    30 * time.Second,
    EnableTLS:         true,
    MaxConnections:    10,
    KeepAliveInterval: 30 * time.Second,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ClientCert: "/etc/ssl/certs/client.pem", 
        ClientKey:  "/etc/ssl/private/client.key",
        ServerName: "rustmq.example.com",
    },
    RetryConfig: &rustmq.RetryConfig{
        MaxRetries: 5,
        BaseDelay:  100 * time.Millisecond,
        MaxDelay:   30 * time.Second,
        Multiplier: 2.0,
        Jitter:     true,
    },
}
```

### TLS/mTLS Configuration

The SDK provides comprehensive TLS support for secure connections:

#### Basic TLS with Server Verification

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ServerName: "rustmq.example.com",
    },
}
```

#### Mutual TLS (mTLS) with Client Certificates

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        CACert:     "/etc/ssl/certs/ca.pem",
        ClientCert: "/etc/ssl/certs/client.pem",
        ClientKey:  "/etc/ssl/private/client.key", 
        ServerName: "rustmq.example.com",
    },
}
```

#### Development/Testing with Insecure Skip Verify

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"localhost:9092"},
    EnableTLS: true,
    TLSConfig: &rustmq.TLSConfig{
        InsecureSkipVerify: true, // Only for development!
    },
}
```

### Connection Health Checks

The SDK includes comprehensive health checking with automatic reconnection:

#### Health Check Configuration

```go
config := &rustmq.ClientConfig{
    Brokers:           []string{"localhost:9092"},
    KeepAliveInterval: 30 * time.Second, // Health check interval
    RequestTimeout:    5 * time.Second,  // Health check timeout
}

client, err := rustmq.NewClient(config)
if err != nil {
    log.Fatal(err)
}

// Manual health check
if err := client.HealthCheck(); err != nil {
    log.Printf("Health check failed: %v", err)
}
```

#### Health Check Response Format

Health checks exchange JSON messages with brokers:

```json
{
  "status": "healthy",
  "details": {
    "broker_id": "broker-001",
    "uptime": "24h30m15s",
    "memory_usage": "45%"
  },
  "timestamp": "2023-10-15T14:30:00Z"
}
```

### Reconnection Configuration

Robust reconnection with exponential backoff and jitter:

#### Basic Reconnection Setup

```go
config := &rustmq.ClientConfig{
    Brokers: []string{"localhost:9092", "localhost:9093"},
    RetryConfig: &rustmq.RetryConfig{
        MaxRetries: 10,                    // Maximum retry attempts per broker
        BaseDelay:  100 * time.Millisecond, // Initial retry delay
        MaxDelay:   30 * time.Second,      // Maximum retry delay
        Multiplier: 2.0,                   // Exponential backoff multiplier
        Jitter:     true,                  // Add random jitter to prevent thundering herd
    },
}
```

#### Advanced Reconnection Patterns

```go
// Create client with custom reconnection strategy
client, err := rustmq.NewClient(config)
if err != nil {
    log.Fatal(err)
}

// Monitor connection status
go func() {
    ticker := time.NewTicker(5 * time.Second)
    defer ticker.Stop()
    
    for range ticker.C {
        if !client.IsConnected() {
            log.Println("Connection lost, automatic reconnection in progress...")
        }
        
        // Get detailed connection statistics
        stats := client.Stats()
        log.Printf("Active connections: %d/%d, Reconnect attempts: %d",
            stats.ActiveConnections, stats.TotalConnections, stats.ReconnectAttempts)
    }
}()
```

### Connection Statistics and Monitoring

Comprehensive statistics for monitoring and observability:

#### Available Statistics

```go
stats := client.Stats()

// Connection health
fmt.Printf("Active connections: %d/%d\n", stats.ActiveConnections, stats.TotalConnections)
fmt.Printf("Brokers: %v\n", stats.Brokers)

// Traffic statistics  
fmt.Printf("Bytes sent: %d, received: %d\n", stats.BytesSent, stats.BytesReceived)
fmt.Printf("Requests: %d, responses: %d\n", stats.RequestsSent, stats.ResponsesReceived)

// Error tracking
fmt.Printf("Total errors: %d\n", stats.Errors)

// Health monitoring
fmt.Printf("Health checks: %d (errors: %d)\n", stats.HealthChecks, stats.HealthCheckErrors)
fmt.Printf("Last heartbeat: %v\n", stats.LastHeartbeat)

// Reconnection tracking
fmt.Printf("Reconnection attempts: %d\n", stats.ReconnectAttempts)
```

#### JSON Statistics Export

```go
// Export statistics as JSON for monitoring systems
statsJSON, err := json.MarshalIndent(client.Stats(), "", "  ")
if err == nil {
    fmt.Println(string(statsJSON))
}
```

## 🔐 Security Guide

The RustMQ Go SDK provides enterprise-grade security features with comprehensive support for both development and production environments. This guide covers security setup, configuration, and best practices for secure Go client applications.

### 🚀 Environment-Based Security Setup

Choose your security setup based on your environment:

#### 🛠️ Development Environment Setup

For local development, use the automated development setup:

```bash
# Set up complete development environment with certificates
cd ../../  # Go to RustMQ project root  
./generate-certs.sh develop

# This creates:
# - certs/ca.pem           (Root CA certificate)
# - certs/client.pem       (Client certificate)
# - certs/client.key       (Client private key)
# - config/client-dev.toml (Development client config)
```

**Development Client Configuration:**

```go
package main

import (
    "io/ioutil"
    "log"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Development configuration with self-signed certificates
    config := &rustmq.ClientConfig{
        Brokers:  []string{"localhost:9092"},
        ClientID: "go-dev-client",
        
        // Development TLS configuration
        EnableTLS: true,
        TLSConfig: &rustmq.TLSConfig{
            CACert:             "../../certs/ca.pem",
            ClientCert:         "../../certs/client.pem",
            ClientKey:          "../../certs/client.key",
            ServerName:         "localhost",
            InsecureSkipVerify: false, // Still validate certificates in dev
        },
        
        // Development-friendly timeouts
        ConnectTimeout:    10 * time.Second,
        RequestTimeout:    30 * time.Second,
        KeepAliveInterval: 30 * time.Second,
        
        // Development retry configuration
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 3,
            BaseDelay:  200 * time.Millisecond,
            MaxDelay:   5 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatalf("Failed to create development client: %v", err)
    }
    defer client.Close()
    
    log.Println("✅ Connected to RustMQ with development mTLS")
}
```

**Quick Development Example:**

```go
package main

import (
    "context"
    "fmt"
    "log"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Load development configuration
    config := rustmq.LoadDevelopmentConfig()
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()
    
    // Create secure producer
    producer, err := client.CreateProducer("dev-topic")
    if err != nil {
        log.Fatal(err)
    }
    defer producer.Close()
    
    message := rustmq.NewMessage().
        Topic("dev-topic").
        PayloadString("Development message").
        Header("environment", "development").
        Build()
    
    result, err := producer.Send(context.Background(), message)
    if err != nil {
        log.Fatal(err)
    }
    
    fmt.Printf("✅ Message sent with mTLS: offset=%d\n", result.Offset)
}
```

#### 🏭 Production Environment Setup

For production, use proper CA-signed certificates and production configuration:

```bash
# Generate production setup guidance
./generate-certs.sh production

# Or use RustMQ Admin CLI for certificate management:
./target/release/rustmq-admin ca init --cn "MyCompany RustMQ Root CA" --org "MyCompany"
./target/release/rustmq-admin certs issue \
  --principal "client@mycompany.com" \
  --role client \
  --validity-days 90
```

**Production Client Configuration:**

```go
package main

import (
    "crypto/tls"
    "io/ioutil"
    "log"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Production configuration with CA-signed certificates
    config := &rustmq.ClientConfig{
        Brokers: []string{
            "rustmq-broker-01.mycompany.com:9092",
            "rustmq-broker-02.mycompany.com:9092", 
            "rustmq-broker-03.mycompany.com:9092",
        },
        ClientID: "production-go-client",
        
        // Production TLS configuration
        EnableTLS: true,
        TLSConfig: &rustmq.TLSConfig{
            CACert:             "/etc/ssl/certs/rustmq-ca.pem",
            ClientCert:         "/etc/ssl/certs/client.pem",
            ClientKey:          "/etc/ssl/private/client.key",
            ServerName:         "rustmq.mycompany.com",
            InsecureSkipVerify: false,
            MinVersion:         tls.VersionTLS12,
            MaxVersion:         tls.VersionTLS13,
            CipherSuites: []uint16{
                tls.TLS_AES_256_GCM_SHA384,
                tls.TLS_CHACHA20_POLY1305_SHA256,
                tls.TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
            },
        },
        
        // Production timeouts and retry configuration
        ConnectTimeout:    30 * time.Second,
        RequestTimeout:    60 * time.Second,
        KeepAliveInterval: 30 * time.Second,
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 5,
            BaseDelay:  100 * time.Millisecond,
            MaxDelay:   30 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
        
        // Connection pooling for production load
        MaxConnections: 20,
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatalf("Failed to create production client: %v", err)
    }
    defer client.Close()
    
    log.Println("✅ Connected to RustMQ with production mTLS")
}
```

### 🔒 Authentication Methods

#### mTLS (Mutual TLS) Authentication

**Recommended for production environments:**

```go
package main

import (
    "context"
    "crypto/tls"
    "fmt"
    "io/ioutil"
    "log"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Read certificate files
    caCert, err := ioutil.ReadFile("/etc/ssl/certs/rustmq-ca.pem")
    if err != nil {
        log.Fatal(err)
    }
    
    clientCert, err := ioutil.ReadFile("/etc/ssl/certs/client.pem")
    if err != nil {
        log.Fatal(err)
    }
    
    clientKey, err := ioutil.ReadFile("/etc/ssl/private/client.key")
    if err != nil {
        log.Fatal(err)
    }
    
    // Create mTLS configuration
    config := &rustmq.ClientConfig{
        Brokers:  []string{"rustmq.mycompany.com:9092"},
        ClientID: "mtls-client",
        
        // mTLS configuration
        EnableTLS: true,
        TLSConfig: &rustmq.TLSConfig{
            CACertData:         caCert,        // CA certificate data
            ClientCertData:     clientCert,    // Client certificate data
            ClientKeyData:      clientKey,     // Client private key data
            ServerName:         "rustmq.mycompany.com",
            InsecureSkipVerify: false,
            MinVersion:         tls.VersionTLS12,
            
            // Certificate validation
            ValidateCertificateChain: true,
            CheckCertificateExpiry:   true,
            RequireClientCertificate: true,
        },
        
        // Authentication method
        AuthMethod: rustmq.AuthMethodMTLS,
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatalf("Failed to create mTLS client: %v", err)
    }
    defer client.Close()
    
    // Verify client authentication status
    if authInfo := client.GetAuthenticationInfo(); authInfo != nil {
        fmt.Printf("✅ Authenticated as: %s\n", authInfo.Principal)
        fmt.Printf("📋 Certificate expires: %s\n", authInfo.ExpiresAt)
        fmt.Printf("🔐 Authentication method: %s\n", authInfo.Method)
    } else {
        log.Fatal("❌ Authentication failed")
    }
}
```

#### JWT Token Authentication

**Suitable for service-to-service authentication:**

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // JWT token configuration
    config := &rustmq.ClientConfig{
        Brokers:  []string{"rustmq.mycompany.com:9092"},
        ClientID: "jwt-client",
        
        // JWT authentication
        AuthMethod: rustmq.AuthMethodJWT,
        AuthConfig: &rustmq.AuthConfig{
            JWTToken: "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9...", // JWT token
            
            // Automatic token refresh
            TokenRefresh: &rustmq.TokenRefreshConfig{
                Enabled:          true,
                RefreshThreshold: 5 * time.Minute,
                RefreshEndpoint:  "https://auth.mycompany.com/token/refresh",
                RefreshCredentials: map[string]string{
                    "client_id":     "rustmq-client",
                    "client_secret": "secret",
                },
            },
        },
        
        // TLS for transport encryption (server-side only)
        EnableTLS: true,
        TLSConfig: &rustmq.TLSConfig{
            ServerName:         "rustmq.mycompany.com",
            InsecureSkipVerify: false,
        },
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatalf("Failed to create JWT client: %v", err)
    }
    defer client.Close()
    
    // Monitor token expiration
    go func() {
        ticker := time.NewTicker(1 * time.Minute)
        defer ticker.Stop()
        
        for range ticker.C {
            if authInfo := client.GetAuthenticationInfo(); authInfo != nil {
                timeToExpiry := time.Until(authInfo.ExpiresAt)
                
                if timeToExpiry < 10*time.Minute {
                    log.Printf("🔄 Token expires in %v, refreshing...", timeToExpiry)
                    if err := client.RefreshToken(); err != nil {
                        log.Printf("❌ Token refresh failed: %v", err)
                    } else {
                        log.Println("✅ Token refreshed successfully")
                    }
                }
            }
        }
    }()
    
    log.Println("✅ Connected with JWT authentication")
}
```

#### Environment Variable Configuration

**Secure configuration through environment variables:**

```go
package main

import (
    "os"
    "log"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Set environment variables
    os.Setenv("RUSTMQ_BROKERS", "rustmq.mycompany.com:9092")
    os.Setenv("RUSTMQ_CA_CERT", "/etc/ssl/certs/rustmq-ca.pem")
    os.Setenv("RUSTMQ_CLIENT_CERT", "/etc/ssl/certs/client.pem")
    os.Setenv("RUSTMQ_CLIENT_KEY", "/etc/ssl/private/client.key")
    os.Setenv("RUSTMQ_SERVER_NAME", "rustmq.mycompany.com")
    
    // Load configuration from environment
    config := rustmq.NewClientConfigFromEnv()
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()
    
    log.Println("✅ Connected using environment configuration")
}
```

### 🛡️ Authorization & Access Control

#### ACL-Based Authorization

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    config := &rustmq.ClientConfig{
        Brokers:  []string{"rustmq.mycompany.com:9092"},
        ClientID: "acl-client",
        
        // Enable ACL authorization
        EnableACL: true,
        ACLConfig: &rustmq.ACLConfig{
            ControllerEndpoints: []string{
                "controller-01.mycompany.com:9094",
                "controller-02.mycompany.com:9094",
            },
            RequestTimeout: 10 * time.Second,
            
            // Permission caching for performance
            Cache: &rustmq.ACLCacheConfig{
                Size:            1000,             // Max cached principals
                TTL:             10 * time.Minute, // Cache TTL
                CleanupInterval: 1 * time.Minute,  // Cleanup frequency
            },
            
            // Circuit breaker for resilience
            CircuitBreaker: &rustmq.CircuitBreakerConfig{
                FailureThreshold: 5,                // Open after 5 failures
                SuccessThreshold: 3,                // Close after 3 successes
                Timeout:          30 * time.Second, // Recovery timeout
            },
        },
        
        // mTLS authentication
        EnableTLS: true,
        TLSConfig: &rustmq.TLSConfig{
            CACert:     "/etc/ssl/certs/rustmq-ca.pem",
            ClientCert: "/etc/ssl/certs/client.pem",
            ClientKey:  "/etc/ssl/private/client.key",
            ServerName: "rustmq.mycompany.com",
        },
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()
    
    // Check permissions before operations
    if !client.CanWriteTopic("sensitive-financial-data") {
        log.Fatal("❌ Permission denied: cannot write to sensitive-financial-data")
    }
    
    if !client.CanReadTopic("audit-logs") {
        log.Fatal("❌ Permission denied: cannot read from audit-logs")
    }
    
    if !client.CanJoinConsumerGroup("financial-processors") {
        log.Fatal("❌ Permission denied: cannot join consumer group financial-processors")
    }
    
    // Proceed with operations
    producer, err := client.CreateProducer("sensitive-financial-data")
    if err != nil {
        log.Fatal(err)
    }
    defer producer.Close()
    
    consumer, err := client.CreateConsumer("audit-logs", "financial-processors")
    if err != nil {
        log.Fatal(err)
    }
    defer consumer.Close()
    
    fmt.Println("✅ All permission checks passed")
}
```

#### Pattern-Based Access Control

```go
// Check pattern-based permissions
permissions := client.GetPermissions()

// Example permission patterns
fmt.Println("📋 Current permissions:")
fmt.Printf("  Read topics: %v\n", permissions.ReadTopics)      // ["logs.*", "events.user.*"]
fmt.Printf("  Write topics: %v\n", permissions.WriteTopics)    // ["events.user.*", "notifications.*"]
fmt.Printf("  Admin ops: %v\n", permissions.AdminOperations)  // ["cluster.health", "topic.describe.*"]

// Pattern matching examples
canRead := permissions.CanReadTopic("logs.application")      // true (logs.*)
canRead = permissions.CanReadTopic("events.user.123")       // true (events.user.*)
canRead = permissions.CanReadTopic("admin.sensitive")       // false (no match)

canWrite := permissions.CanWriteTopic("events.user.456")    // true (events.user.*)
canWrite = permissions.CanWriteTopic("logs.debug")         // false (not in write list)

canAdmin := permissions.CanPerformAdminOperation("cluster.health")      // true (exact match)
canAdmin = permissions.CanPerformAdminOperation("topic.describe.test") // true (topic.describe.*)
canAdmin = permissions.CanPerformAdminOperation("topic.create")        // false (no match)
```

### 📋 Certificate Management

#### Development Certificate Setup

The development environment provides automatic certificate generation:

```bash
# Development setup creates these certificates:
ls -la certs/
# ca.pem      - Self-signed root CA (valid for 1 year)
# client.pem  - Client certificate signed by CA (valid for 90 days)
# client.key  - Client private key (2048-bit RSA)
# server.pem  - Server certificate for localhost (valid for 90 days)
# server.key  - Server private key (2048-bit RSA)
```

**Certificate Validation in Development:**

```go
package main

import (
    "fmt"
    "log"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Validate development certificates
    validator := rustmq.NewCertificateValidator()
    
    result, err := validator.ValidateCertificateChain(
        "../../certs/client.pem",
        "../../certs/ca.pem",
    )
    if err != nil {
        log.Fatal(err)
    }
    
    if result.IsValid {
        fmt.Println("✅ Certificate chain is valid")
        fmt.Printf("📅 Expires: %s\n", result.ExpiryDate)
        fmt.Printf("🔐 Subject: %s\n", result.Subject)
        fmt.Printf("🏛️ Issuer: %s\n", result.Issuer)
    } else {
        fmt.Println("❌ Certificate validation failed:")
        for _, err := range result.Errors {
            fmt.Printf("  - %s\n", err)
        }
    }
}
```

#### Production Certificate Management

For production environments, integrate with your certificate management system:

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Certificate lifecycle management
    certManager := rustmq.NewCertificateManager()
    
    // Monitor certificate expiry
    expiryInfo, err := certManager.CheckCertificateExpiry("/etc/ssl/certs/client.pem")
    if err != nil {
        log.Fatal(err)
    }
    
    if expiryInfo.ExpiresWithinDays(30) {
        fmt.Printf("⚠️  Certificate expires in %d days\n", expiryInfo.DaysUntilExpiry)
        
        // Trigger certificate renewal
        renewalRequest := &rustmq.CertificateRenewal{
            CertificatePath:     "/etc/ssl/certs/client.pem",
            PrivateKeyPath:      "/etc/ssl/private/client.key",
            CAEndpoint:          "https://ca.mycompany.com/api/v1/certificates",
            RenewalThresholdDays: 30,
        }
        
        if err := certManager.ScheduleRenewal(context.Background(), renewalRequest); err != nil {
            log.Printf("❌ Failed to schedule certificate renewal: %v", err)
        } else {
            fmt.Println("✅ Certificate renewal scheduled")
        }
    }
}
```

### 🔧 Security Configuration Examples

#### High-Security Production Configuration

```go
package main

import (
    "crypto/tls"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // High-security production configuration
    config := &rustmq.ClientConfig{
        Brokers:  []string{"rustmq.mycompany.com:9092"},
        ClientID: "high-security-client",
        
        // Strict TLS configuration
        EnableTLS: true,
        TLSConfig: &rustmq.TLSConfig{
            CACert:             "/etc/ssl/certs/rustmq-ca.pem",
            ClientCert:         "/etc/ssl/certs/client.pem",
            ClientKey:          "/etc/ssl/private/client.key",
            ServerName:         "rustmq.mycompany.com",
            InsecureSkipVerify: false,
            MinVersion:         tls.VersionTLS13, // Require TLS 1.3
            MaxVersion:         tls.VersionTLS13,
            CipherSuites: []uint16{
                tls.TLS_AES_256_GCM_SHA384,       // Strong encryption
                tls.TLS_CHACHA20_POLY1305_SHA256,
            },
            
            // Strict certificate validation
            ValidateCertificateChain:   true,
            CheckCertificateExpiry:     true,
            CheckCertificateRevocation: true,  // Enable OCSP/CRL checking
            RequireClientCertificate:   true,
        },
        
        // Comprehensive authentication
        AuthMethod: rustmq.AuthMethodMTLS,
        AuthConfig: &rustmq.AuthConfig{
            StrictValidation: &rustmq.AuthValidationConfig{
                CheckCertificateRevocation: true,
                RequireValidCertificateChain: true,
                ValidateCertificatePurpose: true,
                EnforceCertificateExpiry: true,
            },
        },
        
        // Security monitoring
        SecurityMonitoring: &rustmq.SecurityMonitoringConfig{
            LogAuthenticationAttempts:    true,
            LogAuthorizationDecisions:    true,
            AlertOnFailedAuth:            true,
            TrackSecurityContextChanges:  true,
        },
        
        // Strict timeouts
        ConnectTimeout: 15 * time.Second,
        RequestTimeout: 30 * time.Second,
    }
    
    // Additional security validation would be implemented here
}
```

#### Development Security Configuration

```go
package main

import (
    "crypto/tls"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Development configuration (still secure, but development-friendly)
    config := &rustmq.ClientConfig{
        Brokers:  []string{"localhost:9092"},
        ClientID: "dev-client",
        
        // Development TLS (still secure, but development-friendly)
        EnableTLS: true,
        TLSConfig: &rustmq.TLSConfig{
            CACert:                       "../../certs/ca.pem",
            ClientCert:                   "../../certs/client.pem",
            ClientKey:                    "../../certs/client.key",
            ServerName:                   "localhost",
            InsecureSkipVerify:           false, // Still validate certificates
            MinVersion:                   tls.VersionTLS12,
            AllowSelfSignedCertificates:  true, // Allow for development
            ValidateCertificateChain:     true,
            CheckCertificateExpiry:       true,
            CheckCertificateRevocation:   false, // Disable for development
        },
        
        // Development authentication
        AuthMethod: rustmq.AuthMethodMTLS,
        
        // More permissive timeouts for debugging
        ConnectTimeout: 30 * time.Second,
        RequestTimeout: 60 * time.Second,
    }
    
    // Development configuration would be used here
}
```

### 🚨 Security Monitoring & Troubleshooting

#### Security Event Monitoring

```go
package main

import (
    "fmt"
    "log"
    "time"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    client := createSecureClient() // Implementation from above examples
    defer client.Close()
    
    // Monitor security events
    securityMonitor := client.GetSecurityMonitor()
    eventChan := securityMonitor.Subscribe()
    
    go func() {
        for event := range eventChan {
            switch event.Type {
            case rustmq.SecurityEventAuthSuccess:
                fmt.Printf("✅ Authentication successful: %s at %s\n", 
                    event.Principal, event.Timestamp)
                
            case rustmq.SecurityEventAuthFailure:
                fmt.Printf("❌ Authentication failed: %s from %s at %s\n", 
                    event.Reason, event.RemoteAddr, event.Timestamp)
                
            case rustmq.SecurityEventAuthzDenied:
                fmt.Printf("🚫 Authorization denied: %s tried %s on %s at %s\n", 
                    event.Principal, event.Operation, event.Resource, event.Timestamp)
                
            case rustmq.SecurityEventCertExpiring:
                fmt.Printf("⚠️  Certificate expiring: %s in %d days\n", 
                    event.CertificatePath, event.DaysUntilExpiry)
                
            case rustmq.SecurityEventSecurityContextChange:
                fmt.Printf("🔄 Security context changed: %s -> %s at %s\n", 
                    event.OldPrincipal, event.NewPrincipal, event.Timestamp)
            }
        }
    }()
    
    // Keep monitoring
    select {}
}

func createSecureClient() *rustmq.Client {
    // Implementation from previous examples
    return nil
}
```

#### Common Security Issues & Solutions

**Certificate Validation Failures:**

```go
package main

import (
    "fmt"
    "log"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Issue: Certificate validation failed
    // Error: "certificate has expired"
    
    // Solution: Check certificate expiry and renewal
    certValidator := rustmq.NewCertificateValidator()
    certInfo, err := certValidator.GetCertificateInfo("client.pem")
    if err != nil {
        log.Fatal(err)
    }
    
    if certInfo.IsExpired() {
        fmt.Printf("❌ Certificate expired on: %s\n", certInfo.NotAfter)
        fmt.Println("💡 Renew certificate using:")
        fmt.Println("   ./target/release/rustmq-admin certs renew cert_id")
        return
    }
    
    if certInfo.ExpiresWithinDays(7) {
        fmt.Printf("⚠️  Certificate expires in %d days\n", certInfo.DaysUntilExpiry())
        // Schedule renewal
    }
}
```

**Connection Security Issues:**

```go
package main

import (
    "fmt"
    "log"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Issue: TLS handshake failures
    // Error: "tls: handshake failure"
    
    // Solution: Debug TLS configuration
    tlsConfig := &rustmq.TLSConfig{
        // Enable TLS debugging
        Debug:              true,
        LogTLSKeys:         true, // Development only!
        CACert:             "certs/ca.pem",
        VerifyConfiguration: true,
    }
    
    // Test TLS connection
    testResult, err := rustmq.TestTLSConnection("localhost:9092", tlsConfig)
    if err != nil {
        log.Fatal(err)
    }
    
    if !testResult.Success {
        fmt.Println("❌ TLS test failed:")
        for _, err := range testResult.Errors {
            fmt.Printf("   - %s\n", err)
        }
        
        fmt.Println("💡 Common solutions:")
        fmt.Println("   - Check server certificate matches server_name")
        fmt.Println("   - Verify CA certificate is correct")
        fmt.Println("   - Ensure client certificate is valid")
    }
}
```

**Authorization Failures:**

```go
package main

import (
    "fmt"
    "log"
    
    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Issue: Authorization denied
    // Error: "insufficient permissions for topic 'secure-topic'"
    
    client := createSecureClient() // Implementation from previous examples
    defer client.Close()
    
    // Solution: Check and update ACL permissions
    if authInfo := client.GetAuthenticationInfo(); authInfo != nil {
        fmt.Printf("🔍 Current permissions for %s:\n", authInfo.Principal)
        
        permissions := client.GetPermissions()
        fmt.Printf("   Read topics: %v\n", permissions.ReadTopics)
        fmt.Printf("   Write topics: %v\n", permissions.WriteTopics)
        fmt.Printf("   Admin operations: %v\n", permissions.AdminOperations)
        
        // Check specific permission
        if !permissions.CanWriteTopic("secure-topic") {
            fmt.Println("❌ Missing WRITE permission for 'secure-topic'")
            fmt.Println("💡 Request access using:")
            fmt.Printf("   ./target/release/rustmq-admin acl create \\\n")
            fmt.Printf("     --principal '%s' \\\n", authInfo.Principal)
            fmt.Printf("     --resource 'topic.secure-topic' \\\n")
            fmt.Printf("     --permissions write \\\n")
            fmt.Printf("     --effect allow\n")
        }
    }
}
```

### 📚 Security Examples

The SDK includes comprehensive security examples:

- [`examples/secure_producer_mtls.go`](examples/secure_producer_mtls.go) - mTLS producer with certificate authentication
- [`examples/secure_consumer_jwt.go`](examples/secure_consumer_jwt.go) - JWT consumer with ACL authorization
- [`examples/secure_admin_sasl.go`](examples/secure_admin_sasl.go) - SASL authentication for admin operations
- [`examples/certificate_management.go`](examples/certificate_management.go) - Certificate lifecycle management
- [`examples/security_monitoring.go`](examples/security_monitoring.go) - Security event monitoring

### 🔒 Security Best Practices

#### Development Environment
- ✅ Use automated certificate setup: `./generate-certs.sh develop`
- ✅ Enable certificate validation even with self-signed certificates
- ✅ Use development-specific ACL rules with restricted permissions
- ✅ Monitor authentication and authorization events
- ⚠️ Never use `InsecureSkipVerify: true` even in development

#### Production Environment
- ✅ Use CA-signed certificates with proper certificate chains
- ✅ Enable strict TLS validation and hostname verification
- ✅ Implement certificate monitoring and automatic renewal
- ✅ Use fine-grained ACL permissions with least privilege principle
- ✅ Enable comprehensive security event logging and monitoring
- ✅ Regular security audits and certificate rotation
- ❌ Never disable certificate validation in production
- ❌ Never store private keys in application code or logs

#### Certificate Management
- 🔄 Rotate certificates every 90 days maximum
- 📊 Monitor certificate expiry and automate renewal
- 🔐 Store private keys securely (HSM for production)
- 📝 Maintain certificate inventory and lifecycle tracking
- 🚨 Implement immediate revocation capabilities

For complete security documentation, configuration examples, and advanced security features, see [`../../docs/security/`](../../docs/security/).

## Quick Start

### Producer Example

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Create client with TLS and retry configuration
    config := &rustmq.ClientConfig{
        Brokers:           []string{"localhost:9092", "localhost:9093"},
        ClientID:          "go-producer-example",
        ConnectTimeout:    10 * time.Second,
        RequestTimeout:    30 * time.Second,
        EnableTLS:         false, // Set to true for production
        KeepAliveInterval: 30 * time.Second,
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 5,
            BaseDelay:  100 * time.Millisecond,
            MaxDelay:   10 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Monitor connection health
    go func() {
        ticker := time.NewTicker(10 * time.Second)
        defer ticker.Stop()
        
        for range ticker.C {
            if err := client.HealthCheck(); err != nil {
                log.Printf("Health check failed: %v", err)
            } else {
                stats := client.Stats()
                log.Printf("Health check OK - Active connections: %d/%d", 
                    stats.ActiveConnections, stats.TotalConnections)
            }
        }
    }()

    // Create producer
    producer, err := client.CreateProducer("my-topic")
    if err != nil {
        log.Fatal(err)
    }
    defer producer.Close()

    // Send message with connection monitoring
    message := rustmq.NewMessage().
        Topic("my-topic").
        PayloadString("Hello, RustMQ with enhanced connection layer!").
        Header("sender", "go-client").
        Header("version", "v2.0").
        Build()

    ctx := context.Background()
    result, err := producer.Send(ctx, message)
    if err != nil {
        log.Fatal(err)
    }

    fmt.Printf("Message sent: offset=%d, partition=%d\n", result.Offset, result.Partition)
    
    // Print connection statistics
    stats := client.Stats()
    fmt.Printf("Connection stats: Sent=%d bytes, Received=%d bytes, Errors=%d\n",
        stats.BytesSent, stats.BytesReceived, stats.Errors)
}
```

### Consumer Example

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Create client with production-ready configuration
    config := &rustmq.ClientConfig{
        Brokers:           []string{"localhost:9092", "localhost:9093"},
        ClientID:          "go-consumer-example",
        ConnectTimeout:    10 * time.Second,
        RequestTimeout:    30 * time.Second,
        EnableTLS:         false, // Set to true for production
        KeepAliveInterval: 30 * time.Second,
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 5,
            BaseDelay:  100 * time.Millisecond,
            MaxDelay:   10 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }
    
    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Start connection monitoring
    go func() {
        ticker := time.NewTicker(30 * time.Second)
        defer ticker.Stop()
        
        for range ticker.C {
            stats := client.Stats()
            log.Printf("Connection Health: Active=%d/%d, Reconnects=%d, Health Checks=%d", 
                stats.ActiveConnections, stats.TotalConnections,
                stats.ReconnectAttempts, stats.HealthChecks)
                
            if stats.HealthCheckErrors > 0 {
                log.Printf("Health check errors detected: %d", stats.HealthCheckErrors)
            }
        }
    }()

    // Create consumer
    consumer, err := client.CreateConsumer("my-topic", "my-group")
    if err != nil {
        log.Fatal(err)
    }
    defer consumer.Close()

    // Consume messages with connection resilience
    for {
        ctx := context.Background()
        message, err := consumer.Receive(ctx)
        if err != nil {
            log.Printf("Receive error: %v", err)
            
            // Check if it's a connection issue
            if !client.IsConnected() {
                log.Println("Connection lost, waiting for automatic reconnection...")
                time.Sleep(1 * time.Second)
            }
            continue
        }

        fmt.Printf("Received: %s\n", message.Message.PayloadAsString())
        
        // Acknowledge message
        if err := message.Ack(); err != nil {
            log.Printf("Ack error: %v", err)
        }
    }
}
```

### Stream Processing Example

```go
package main

import (
    "context"
    "strings"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

// Custom processor that converts messages to uppercase
type UppercaseProcessor struct{}

func (p *UppercaseProcessor) Process(ctx context.Context, message *rustmq.Message) (*rustmq.Message, error) {
    transformed := rustmq.NewMessage().
        Topic("processed-topic").
        PayloadString(strings.ToUpper(message.PayloadAsString())).
        Header("processor", "uppercase").
        Build()
    
    return transformed, nil
}

func (p *UppercaseProcessor) OnStart(ctx context.Context) error { return nil }
func (p *UppercaseProcessor) OnStop(ctx context.Context) error { return nil }
func (p *UppercaseProcessor) OnError(ctx context.Context, message *rustmq.Message, err error) error { return nil }
func (p *UppercaseProcessor) ProcessBatch(ctx context.Context, messages []*rustmq.Message) ([]*rustmq.Message, error) {
    // Implementation for batch processing
    return nil, nil
}

func main() {
    client, _ := rustmq.NewClient(rustmq.DefaultClientConfig())
    defer client.Close()

    // Configure stream
    streamConfig := &rustmq.StreamConfig{
        InputTopics:   []string{"input-topic"},
        OutputTopic:   "output-topic",
        ConsumerGroup: "processors",
        Parallelism:   4,
    }

    // Create and start stream
    stream, _ := client.CreateStream(streamConfig)
    stream = stream.WithProcessor(&UppercaseProcessor{})
    
    ctx := context.Background()
    stream.Start(ctx)
    defer stream.Stop(ctx)

    // Stream processes messages automatically
    select {} // Keep running
}
```

### Production TLS Example

Complete example with mTLS for production environments:

```go
package main

import (
    "context"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    // Production TLS configuration
    config := &rustmq.ClientConfig{
        Brokers:           []string{"rustmq-prod-1.example.com:9092", "rustmq-prod-2.example.com:9092"},
        ClientID:          "production-service",
        ConnectTimeout:    15 * time.Second,
        RequestTimeout:    30 * time.Second,
        EnableTLS:         true,
        KeepAliveInterval: 30 * time.Second,
        TLSConfig: &rustmq.TLSConfig{
            CACert:     "/etc/ssl/certs/rustmq-ca.pem",
            ClientCert: "/etc/ssl/certs/client.pem",
            ClientKey:  "/etc/ssl/private/client.key",
            ServerName: "rustmq.example.com",
        },
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 10,
            BaseDelay:  200 * time.Millisecond,
            MaxDelay:   30 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }

    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatalf("Failed to create client: %v", err)
    }
    defer client.Close()

    // Verify TLS connection
    if err := client.HealthCheck(); err != nil {
        log.Fatalf("Initial health check failed: %v", err)
    }
    
    log.Println("Successfully connected with TLS")
    
    // Your application logic here...
}
```

### Connection Resilience Example

Example showing robust connection handling:

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    "github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
    config := &rustmq.ClientConfig{
        Brokers: []string{"localhost:9092", "localhost:9093"},
        RetryConfig: &rustmq.RetryConfig{
            MaxRetries: 5,
            BaseDelay:  100 * time.Millisecond,
            MaxDelay:   10 * time.Second,
            Multiplier: 2.0,
            Jitter:     true,
        },
    }

    client, err := rustmq.NewClient(config)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Connection monitoring and alerting
    go monitorConnection(client)

    producer, err := client.CreateProducer("events")
    if err != nil {
        log.Fatal(err)
    }
    defer producer.Close()

    // Resilient message sending
    for i := 0; i < 1000; i++ {
        message := rustmq.NewMessage().
            Topic("events").
            PayloadString(fmt.Sprintf("Event %d", i)).
            Build()

        // Retry sending with exponential backoff
        err := retryWithBackoff(func() error {
            _, err := producer.Send(context.Background(), message)
            return err
        }, 3, 100*time.Millisecond)

        if err != nil {
            log.Printf("Failed to send message %d after retries: %v", i, err)
        } else {
            log.Printf("Sent message %d", i)
        }

        time.Sleep(100 * time.Millisecond)
    }
}

func monitorConnection(client *rustmq.Client) {
    ticker := time.NewTicker(10 * time.Second)
    defer ticker.Stop()

    for range ticker.C {
        stats := client.Stats()
        
        if stats.ActiveConnections == 0 {
            log.Println("ALERT: No active connections!")
        } else if stats.ActiveConnections < stats.TotalConnections {
            log.Printf("WARNING: %d/%d connections active", 
                stats.ActiveConnections, stats.TotalConnections)
        }

        // Check error rates
        if stats.Errors > 0 {
            errorRate := float64(stats.Errors) / float64(stats.RequestsSent)
            if errorRate > 0.05 { // 5% error rate threshold
                log.Printf("ALERT: High error rate: %.2f%%", errorRate*100)
            }
        }

        // Health check performance
        if stats.HealthCheckErrors > 0 {
            log.Printf("Health check issues: %d errors out of %d checks",
                stats.HealthCheckErrors, stats.HealthChecks)
        }
    }
}

func retryWithBackoff(operation func() error, maxRetries int, baseDelay time.Duration) error {
    var err error
    for i := 0; i < maxRetries; i++ {
        err = operation()
        if err == nil {
            return nil
        }

        if i < maxRetries-1 {
            delay := time.Duration(1<<uint(i)) * baseDelay
            log.Printf("Operation failed, retrying in %v: %v", delay, err)
            time.Sleep(delay)
        }
    }
    return err
}
```

## Configuration

### Client Configuration

```go
config := &rustmq.ClientConfig{
    Brokers:           []string{"localhost:9092", "localhost:9093"},
    ClientID:          "my-app",
    ConnectTimeout:    10 * time.Second,
    RequestTimeout:    30 * time.Second,
    EnableTLS:         true,
    MaxConnections:    10,
    KeepAliveInterval: 30 * time.Second,
}
```

### Producer Configuration

```go
producerConfig := &rustmq.ProducerConfig{
    BatchSize:      100,
    BatchTimeout:   100 * time.Millisecond,
    MaxMessageSize: 1024 * 1024, // 1MB
    AckLevel:       rustmq.AckAll,
    Idempotent:     true,
}

producer, err := client.CreateProducer("topic", producerConfig)
```

### Consumer Configuration

```go
consumerConfig := &rustmq.ConsumerConfig{
    ConsumerGroup:      "my-group",
    AutoCommitInterval: 5 * time.Second,
    EnableAutoCommit:   true,
    FetchSize:          100,
    FetchTimeout:       1 * time.Second,
    MaxRetryAttempts:   3,
    DeadLetterQueue:    "dlq-topic", // For failed messages
    StartPosition: rustmq.StartPosition{
        Type: rustmq.StartEarliest,
    },
}

consumer, err := client.CreateConsumer("topic", "group", consumerConfig)
```

#### Consumer Start Positions

The consumer supports various starting positions:

```go
// Start from earliest available message
startPos := rustmq.StartPosition{Type: rustmq.StartEarliest}

// Start from latest message
startPos := rustmq.StartPosition{Type: rustmq.StartLatest}

// Start from specific offset
offset := uint64(12345)
startPos := rustmq.StartPosition{
    Type:   rustmq.StartOffset,
    Offset: &offset,
}

// Start from specific timestamp
timestamp := time.Now().Add(-1 * time.Hour)
startPos := rustmq.StartPosition{
    Type:      rustmq.StartTimestamp,
    Timestamp: &timestamp,
}
```

#### Consumer Operations

```go
// Manual offset commits
ctx := context.Background()
if err := consumer.Commit(ctx); err != nil {
    log.Printf("Commit failed: %v", err)
}

// Seek to specific offset
if err := consumer.Seek(ctx, 12345); err != nil {
    log.Printf("Seek failed: %v", err)
}

// Seek to timestamp
targetTime := time.Now().Add(-1 * time.Hour)
if err := consumer.SeekToTimestamp(ctx, targetTime); err != nil {
    log.Printf("Seek to timestamp failed: %v", err)
}

// Get consumer metrics
metrics := consumer.Metrics()
fmt.Printf("Messages: received=%d, processed=%d, failed=%d, lag=%d\n",
    metrics.MessagesReceived, metrics.MessagesProcessed, 
    metrics.MessagesFailed, metrics.Lag)
```

#### Failed Message Handling

The consumer includes comprehensive retry and dead letter queue support:

```go
consumerConfig := &rustmq.ConsumerConfig{
    ConsumerGroup:    "processing-group",
    MaxRetryAttempts: 5,                    // Retry failed messages up to 5 times
    DeadLetterQueue:  "failed-messages",    // Send to DLQ after max retries
}

// Message processing with retry logic
message, err := consumer.Receive(ctx)
if err != nil {
    log.Printf("Receive error: %v", err)
    continue
}

// Process message
if err := processMessage(message.Message); err != nil {
    // Negative acknowledgment triggers retry
    if err := message.Nack(); err != nil {
        log.Printf("Nack failed: %v", err)
    }
} else {
    // Positive acknowledgment
    if err := message.Ack(); err != nil {
        log.Printf("Ack failed: %v", err)
    }
}
```

#### Message Acknowledgment

Each received message supports acknowledgment operations:

```go
// Receive message
message, err := consumer.Receive(ctx)
if err != nil {
    log.Fatal(err)
}

// Access message properties
fmt.Printf("Message ID: %s\n", message.Message.ID)
fmt.Printf("Partition: %d\n", message.Partition())
fmt.Printf("Retry count: %d\n", message.RetryCount())
fmt.Printf("Age: %v\n", message.Age())

// Process and acknowledge
if processSuccessfully(message.Message) {
    message.Ack()  // Mark as successfully processed
} else {
    message.Nack() // Mark as failed (will retry)
}
```

## Advanced Features

### Message Building

```go
message := rustmq.NewMessage().
    Topic("events").
    KeyString("user-123").
    PayloadJSON(map[string]interface{}{
        "event": "user_login",
        "timestamp": time.Now(),
    }).
    Header("version", "1.0").
    Header("source", "auth-service").
    Build()
```

### Batch Operations

```go
// Send multiple messages
messages := []*rustmq.Message{msg1, msg2, msg3}
results, err := producer.SendBatch(ctx, messages)

// Process message batches
batch := rustmq.NewMessageBatch(messages)
smallerBatches := batch.SplitBySize(1024 * 1024) // 1MB chunks
```

### Error Handling

```go
// Configure retry strategy
streamConfig.ErrorStrategy = rustmq.ErrorStrategy{
    Type:        rustmq.ErrorStrategyRetry,
    MaxAttempts: 3,
    BackoffMs:   1000,
}

// Or use dead letter queue
streamConfig.ErrorStrategy = rustmq.ErrorStrategy{
    Type:            rustmq.ErrorStrategyDeadLetter,
    DeadLetterTopic: "failed-messages",
}
```

### Metrics and Monitoring

```go
// Producer metrics
metrics := producer.Metrics()
fmt.Printf("Sent: %d, Failed: %d, Batches: %d\n", 
    metrics.MessagesSent, metrics.MessagesFailed, metrics.BatchesSent)

// Consumer metrics  
metrics := consumer.Metrics()
fmt.Printf("Received: %d, Processed: %d, Lag: %d\n",
    metrics.MessagesReceived, metrics.MessagesProcessed, metrics.Lag)

// Connection stats
stats := client.Stats()
fmt.Printf("Active: %d/%d connections\n", 
    stats.ActiveConnections, stats.TotalConnections)
```

## Testing

Run the test suite:

```bash
# Unit tests
go test ./tests/...

# Benchmarks
go test -bench=. ./benchmarks/...

# With coverage
go test -cover ./tests/...
```

## Examples

See the `examples/` directory for complete working examples:

### Basic Examples
- `simple_producer.go` - Basic message production
- `simple_consumer.go` - Basic message consumption  
- `stream_processor.go` - Real-time stream processing
- `advanced_stream_processor.go` - Advanced stream processing with custom processors

### Security Examples
- `secure_producer_mtls.go` - **mTLS Authentication** - Demonstrates mutual TLS authentication with client certificates, comprehensive certificate validation, ACL authorization, and security metrics monitoring
- `secure_consumer_jwt.go` - **JWT Authentication** - Shows JWT token authentication with automatic refresh, ACL permission checking, circuit breaker patterns, and security event monitoring
- `secure_admin_sasl.go` - **SASL Authentication** - Illustrates SASL SCRAM-SHA-256 authentication for admin operations, fine-grained permissions, and comprehensive security audit logging

Each security example includes:
- Complete authentication setup with enterprise-grade security
- Permission validation and access control
- Security metrics collection and monitoring
- Error handling and recovery strategies
- Production-ready security configurations

## Performance

The Go client is designed for high performance:

- **QUIC Transport**: Reduced connection overhead vs TCP
- **Connection Pooling**: Efficient connection reuse
- **Message Batching**: Configurable batching for throughput
- **Zero-Copy**: Efficient memory usage where possible
- **Async Operations**: Non-blocking message sending

## Requirements

- Go 1.21 or later
- RustMQ broker running
- Network access to broker endpoints

## License

MIT OR Apache-2.0

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## Support

- GitHub Issues: Report bugs and feature requests
- Documentation: See RustMQ main repository
- Examples: Check the examples directory