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

## Security

The RustMQ Go SDK provides enterprise-grade security features including multiple authentication methods, fine-grained authorization, comprehensive certificate management, and security monitoring.

### Security Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                 RustMQ Go SDK Security                     │
├─────────────────────────────────────────────────────────────┤
│  Authentication Layer                                       │
│  ├── mTLS (Client Certificates)                             │
│  ├── JWT (Token-based)                                      │
│  └── SASL (PLAIN, SCRAM-SHA-256, SCRAM-SHA-512)            │
├─────────────────────────────────────────────────────────────┤
│  Authorization Layer                                        │
│  ├── ACL-based Permissions                                  │
│  ├── Topic Access Control                                   │
│  ├── Admin Operation Control                                │
│  └── Client-side Permission Caching                         │
├─────────────────────────────────────────────────────────────┤
│  Transport Security                                         │
│  ├── TLS 1.2/1.3 Encryption                                │
│  ├── Certificate Validation                                 │
│  ├── Revocation Checking (CRL/OCSP)                         │
│  └── Cipher Suite Control                                   │
├─────────────────────────────────────────────────────────────┤
│  Security Monitoring                                        │
│  ├── Authentication Metrics                                 │
│  ├── Authorization Analytics                                │
│  ├── Certificate Lifecycle Tracking                         │
│  └── Security Event Logging                                 │
└─────────────────────────────────────────────────────────────┘
```

### Security Configuration Overview

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    ClientID:  "secure-client",
    Security: &rustmq.SecurityConfig{
        // Authentication configuration
        Auth: &rustmq.AuthenticationConfig{
            Method:   rustmq.AuthMethodMTLS, // or JWT, SASL
            Username: "user",
            Password: "pass",
            Token:    "jwt-token",
        },
        // TLS/mTLS configuration
        TLS: &rustmq.TLSSecurityConfig{
            Mode:       rustmq.TLSModeMutualAuth,
            CACert:     "ca-certificate-pem",
            ClientCert: "client-certificate-pem", 
            ClientKey:  "client-private-key-pem",
        },
        // ACL authorization
        ACL: &rustmq.ACLConfig{
            Enabled: true,
            ControllerEndpoints: []string{"controller:9094"},
        },
        // Security metrics
        Metrics: &rustmq.SecurityMetricsConfig{
            Enabled: true,
            Detailed: true,
        },
    },
}
```

### Authentication Methods

#### 1. mTLS (Mutual TLS) Authentication

mTLS provides strong cryptographic authentication using client certificates:

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    ClientID:  "mtls-client",
    Security: &rustmq.SecurityConfig{
        Auth: &rustmq.AuthenticationConfig{
            Method: rustmq.AuthMethodMTLS,
        },
        TLS: &rustmq.TLSSecurityConfig{
            Mode:               rustmq.TLSModeMutualAuth,
            CACert:             readFile("/etc/ssl/certs/ca.pem"),
            ClientCert:         readFile("/etc/ssl/certs/client.pem"),
            ClientKey:          readFile("/etc/ssl/private/client.key"),
            ServerName:         "rustmq.example.com",
            InsecureSkipVerify: false,
            MinVersion:         tls.VersionTLS12,
            MaxVersion:         tls.VersionTLS13,
            // Certificate validation settings
            Validation: &rustmq.CertificateValidationSettings{
                ValidateChain:   true,
                CheckExpiration: true,
                CheckRevocation: false, // Enable for production
            },
        },
        // Certificate validation rules
        CertValidation: &rustmq.CertificateValidationConfig{
            Enabled: true,
            Rules: []rustmq.CertificateValidationRule{
                {
                    Type: "key_usage",
                    Parameters: map[string]interface{}{
                        "usage": "client_auth",
                    },
                },
                {
                    Type: "subject_pattern",
                    Parameters: map[string]interface{}{
                        "pattern": "CN=.*client.*",
                    },
                },
            },
        },
        // Principal extraction from certificate
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
    },
}

client, err := rustmq.NewClient(config)
if err != nil {
    log.Fatalf("Failed to create secure client: %v", err)
}
defer client.Close()

// Verify authentication
securityContext := client.SecurityContext()
fmt.Printf("Authenticated as: %s\n", securityContext.Principal)
fmt.Printf("Certificate expires: %s\n", securityContext.ExpiresAt)
```

#### 2. JWT (JSON Web Token) Authentication

JWT provides stateless token-based authentication with automatic refresh:

```go
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    ClientID:  "jwt-client",
    Security: &rustmq.SecurityConfig{
        Auth: &rustmq.AuthenticationConfig{
            Method: rustmq.AuthMethodJWT,
            Token:  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
            // Automatic token refresh
            TokenRefresh: &rustmq.TokenRefreshConfig{
                Enabled:          true,
                RefreshThreshold: 5 * time.Minute,
                RefreshEndpoint:  "https://auth.example.com/token/refresh",
                RefreshCredentials: map[string]string{
                    "client_id":     "rustmq-client",
                    "client_secret": "secret",
                },
            },
        },
        TLS: &rustmq.TLSSecurityConfig{
            Mode:       rustmq.TLSModeEnabled, // Server-side TLS only
            ServerName: "rustmq.example.com",
        },
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
        securityContext := client.SecurityContext()
        timeToExpiry := time.Until(securityContext.ExpiresAt)
        
        if timeToExpiry < 10*time.Minute {
            log.Printf("Token expires in %v, refreshing...", timeToExpiry)
            if err := client.RefreshSecurityContext(); err != nil {
                log.Printf("Token refresh failed: %v", err)
            } else {
                log.Println("Token refreshed successfully")
            }
        }
    }
}()
```

#### 3. SASL Authentication

SASL provides traditional username/password authentication with multiple mechanisms:

```go
// SASL SCRAM-SHA-256 (recommended)
config := &rustmq.ClientConfig{
    Brokers:   []string{"rustmq.example.com:9092"},
    ClientID:  "sasl-client",
    Security: &rustmq.SecurityConfig{
        Auth: &rustmq.AuthenticationConfig{
            Method:   rustmq.AuthMethodSASLScram256,
            Username: "admin-user",
            Password: "secure-password",
            Properties: map[string]string{
                "sasl.mechanism": "SCRAM-SHA-256",
            },
        },
        TLS: &rustmq.TLSSecurityConfig{
            Mode:       rustmq.TLSModeEnabled,
            ServerName: "rustmq.example.com",
        },
    },
}

// SASL PLAIN (use only with TLS)
plainConfig := &rustmq.SecurityConfig{
    Auth: &rustmq.AuthenticationConfig{
        Method:   rustmq.AuthMethodSASLPlain,
        Username: "user",
        Password: "password",
    },
    TLS: &rustmq.TLSSecurityConfig{
        Mode: rustmq.TLSModeEnabled, // Required for PLAIN
    },
}
```

### Authorization (ACL)

The SDK provides fine-grained access control with client-side caching:

#### ACL Configuration

```go
config := &rustmq.ClientConfig{
    Security: &rustmq.SecurityConfig{
        ACL: &rustmq.ACLConfig{
            Enabled:             true,
            ControllerEndpoints: []string{"controller1:9094", "controller2:9094"},
            RequestTimeout:      10 * time.Second,
            
            // Permission caching for performance
            Cache: &rustmq.ACLCacheConfig{
                Size:                1000,              // Max cached principals
                TTL:                 10 * time.Minute,  // Cache TTL
                CleanupInterval:     1 * time.Minute,   // Cleanup frequency
                EnableDeduplication: true,              // Prevent duplicate requests
            },
            
            // Circuit breaker for resilience
            CircuitBreaker: &rustmq.CircuitBreakerConfig{
                FailureThreshold: 5,               // Open after 5 failures
                SuccessThreshold: 3,               // Close after 3 successes
                Timeout:          30 * time.Second, // Recovery timeout
            },
        },
    },
}

client, err := rustmq.NewClient(config)
if err != nil {
    log.Fatal(err)
}

// Check permissions before operations
if !client.CanReadTopic("sensitive-topic") {
    log.Fatal("Permission denied: cannot read from sensitive-topic")
}

if !client.CanWriteTopic("logs-topic") {
    log.Fatal("Permission denied: cannot write to logs-topic") 
}

if !client.CanPerformAdminOperation("cluster.health") {
    log.Fatal("Permission denied: cannot perform admin operations")
}
```

#### Permission Patterns

ACL supports flexible permission patterns:

```go
// Example permissions returned by ACL server
permissions := &rustmq.PermissionSet{
    ReadTopics:      []string{"logs.*", "events.user.*", "metrics.system"},
    WriteTopics:     []string{"events.user.*", "notifications.*"},
    AdminOperations: []string{"cluster.health", "topic.describe.*"},
}

// Pattern matching examples
permissions.CanReadTopic("logs.application")     // true (logs.*)
permissions.CanReadTopic("events.user.123")     // true (events.user.*)
permissions.CanReadTopic("admin.sensitive")     // false (no match)

permissions.CanWriteTopic("events.user.456")    // true (events.user.*)
permissions.CanWriteTopic("logs.debug")         // false (not in write list)

permissions.CanPerformAdminOperation("cluster.health")      // true (exact match)
permissions.CanPerformAdminOperation("topic.describe.test") // true (topic.describe.*)
permissions.CanPerformAdminOperation("topic.create")        // false (no match)
```

### Certificate Management

Advanced certificate validation and lifecycle management:

#### Certificate Validation Rules

```go
config := &rustmq.SecurityConfig{
    CertValidation: &rustmq.CertificateValidationConfig{
        Enabled: true,
        Rules: []rustmq.CertificateValidationRule{
            // Validate key usage
            {
                Type: "key_usage",
                Parameters: map[string]interface{}{
                    "usage": "client_auth",
                },
            },
            // Validate subject pattern
            {
                Type: "subject_pattern", 
                Parameters: map[string]interface{}{
                    "pattern": "CN=.*\\.example\\.com",
                },
            },
            // Validate certificate issuer
            {
                Type: "issuer_pattern",
                Parameters: map[string]interface{}{
                    "pattern": "CN=Example CA.*",
                },
            },
        },
        // Certificate revocation checking
        RevocationCheck: &rustmq.RevocationCheckConfig{
            EnableCRL:     true,
            EnableOCSP:    true,
            CRLCacheTTL:   24 * time.Hour,
            OCSPTimeout:   5 * time.Second,
        },
    },
}
```

#### Principal Extraction

Configure how user identity is extracted from certificates:

```go
config := &rustmq.SecurityConfig{
    PrincipalExtraction: &rustmq.PrincipalExtractionConfig{
        UseCommonName:     true,  // Extract from CN
        UseSubjectAltName: false, // Extract from SAN
        Normalize:         true,  // Lowercase and trim
        
        // Custom extraction rules (evaluated by priority)
        CustomRules: []rustmq.PrincipalExtractionRule{
            {
                Type:     "subject_attribute",
                Pattern:  "OU",          // Use Organizational Unit
                Priority: 100,
            },
            {
                Type:     "subject_pattern", 
                Pattern:  `CN=([^,]+)`, // Regex to extract CN value
                Priority: 90,
            },
            {
                Type:     "san_email",
                Pattern:  `([^@]+)@.*`, // Extract username from email SAN
                Priority: 80,
            },
        },
    },
}
```

### Security Monitoring

Comprehensive security metrics and monitoring:

#### Security Metrics Configuration

```go
config := &rustmq.SecurityConfig{
    Metrics: &rustmq.SecurityMetricsConfig{
        Enabled:            true,
        CollectionInterval: 30 * time.Second,
        Detailed:           true,
        
        // Export metrics to external systems
        Export: &rustmq.MetricsExportConfig{
            Format:   "json",
            Endpoint: "http://metrics.example.com/security",
            Interval: 1 * time.Minute,
        },
    },
}

client, err := rustmq.NewClient(config)
if err != nil {
    log.Fatal(err)
}

// Access security metrics
securityManager := client.SecurityManager()
metrics := securityManager.Metrics().GetMetrics()

// Authentication metrics
for method, authMetrics := range metrics.AuthMetrics {
    fmt.Printf("Auth method %s:\n", method)
    fmt.Printf("  Attempts: %d\n", authMetrics.Attempts)
    fmt.Printf("  Success rate: %.2f%%\n", authMetrics.GetSuccessRate()*100)
    
    if authMetrics.Duration != nil {
        fmt.Printf("  Average duration: %s\n", authMetrics.Duration.Average)
    }
}

// Authorization metrics
if metrics.ACLMetrics != nil {
    fmt.Printf("ACL Performance:\n")
    fmt.Printf("  Cache hit rate: %.2f%%\n", metrics.ACLMetrics.GetCacheHitRate()*100)
    fmt.Printf("  Total requests: %d\n", metrics.ACLMetrics.Requests)
    fmt.Printf("  Errors: %d\n", metrics.ACLMetrics.Errors)
}

// Certificate metrics
if metrics.CertificateMetrics != nil {
    fmt.Printf("Certificate Validation:\n")
    fmt.Printf("  Validations: %d\n", metrics.CertificateMetrics.Validations)
    fmt.Printf("  Error rate: %.2f%%\n", metrics.CertificateMetrics.GetErrorRate()*100)
    fmt.Printf("  Expiring certificates: %d\n", metrics.CertificateMetrics.Expiring)
}

// TLS connection metrics
if metrics.TLSMetrics != nil {
    fmt.Printf("TLS Connections:\n")
    fmt.Printf("  Total connections: %d\n", metrics.TLSMetrics.Connections)
    fmt.Printf("  Handshake error rate: %.2f%%\n", metrics.TLSMetrics.GetHandshakeErrorRate()*100)
}
```

#### Security Event Monitoring

```go
// Monitor security events in real-time
go func() {
    ticker := time.NewTicker(10 * time.Second)
    defer ticker.Stop()
    
    for range ticker.C {
        metrics := securityManager.Metrics().GetMetrics()
        
        // Alert on authentication failures
        for method, authMetrics := range metrics.AuthMetrics {
            successRate := authMetrics.GetSuccessRate()
            if successRate < 0.95 && authMetrics.Attempts > 10 {
                log.Printf("SECURITY ALERT: Low success rate for %s: %.2f%%", 
                    method, successRate*100)
            }
        }
        
        // Alert on ACL errors
        if metrics.ACLMetrics != nil && metrics.ACLMetrics.Errors > 0 {
            errorRate := float64(metrics.ACLMetrics.Errors) / float64(metrics.ACLMetrics.Requests)
            if errorRate > 0.05 { // 5% error threshold
                log.Printf("SECURITY ALERT: High ACL error rate: %.2f%%", errorRate*100)
            }
        }
        
        // Alert on certificate issues
        if metrics.CertificateMetrics != nil {
            if metrics.CertificateMetrics.Expiring > 0 {
                log.Printf("SECURITY WARNING: %d certificates expiring soon", 
                    metrics.CertificateMetrics.Expiring)
            }
            
            errorRate := metrics.CertificateMetrics.GetErrorRate()
            if errorRate > 0.1 { // 10% error threshold
                log.Printf("SECURITY ALERT: High certificate error rate: %.2f%%", errorRate*100)
            }
        }
    }
}()
```

### Security Error Handling

Comprehensive error handling for security operations:

#### Security Error Types

```go
// Check for specific security error types
_, err := client.CreateProducer("secured-topic")
if err != nil {
    if rustmq.IsAuthenticationError(err) {
        log.Println("Authentication failed - check credentials")
    } else if rustmq.IsAuthorizationError(err) {
        log.Println("Authorization failed - insufficient permissions")
    } else if rustmq.IsCertificateError(err) {
        log.Println("Certificate error - check certificate validity")
    } else if rustmq.IsTLSError(err) {
        log.Println("TLS error - check TLS configuration")
    } else if rustmq.IsConfigurationError(err) {
        log.Println("Configuration error - check security settings")
    }
    
    // Get detailed error information
    if secErr, ok := rustmq.AsSecurityError(err); ok {
        fmt.Printf("Error type: %s\n", secErr.Type)
        fmt.Printf("Error message: %s\n", secErr.Message)
        fmt.Printf("Error code: %s\n", secErr.Code)
        
        for key, value := range secErr.Details {
            fmt.Printf("%s: %v\n", key, value)
        }
    }
}
```

#### Security Error Recovery

```go
// Implement security error recovery strategies
func handleSecurityError(err error) error {
    if rustmq.IsAuthenticationError(err) {
        // Refresh authentication credentials
        return client.RefreshSecurityContext()
    } else if rustmq.IsAuthorizationError(err) {
        // Request updated permissions
        return requestPermissionUpdate()
    } else if rustmq.IsCertificateError(err) {
        // Renew certificate
        return renewCertificate()
    }
    return err
}
```

### Security Best Practices

#### Production Security Checklist

- ✅ **Always use TLS in production** - Never send credentials over unencrypted connections
- ✅ **Prefer mTLS for service-to-service** - Strongest authentication for backend services  
- ✅ **Use JWT for user authentication** - Stateless and scalable for user-facing applications
- ✅ **Enable ACL authorization** - Implement principle of least privilege
- ✅ **Monitor certificate expiration** - Set up alerts for certificate renewal
- ✅ **Enable security metrics** - Monitor authentication and authorization patterns
- ✅ **Validate all certificates** - Enable chain validation and revocation checking
- ✅ **Use strong cipher suites** - Configure TLS 1.2+ with secure ciphers
- ✅ **Implement proper error handling** - Don't leak sensitive information in errors
- ✅ **Regular security audits** - Review permissions and access patterns

#### Development vs Production

```go
// Development configuration (relaxed security)
devConfig := &rustmq.SecurityConfig{
    TLS: &rustmq.TLSSecurityConfig{
        Mode:               rustmq.TLSModeEnabled,
        InsecureSkipVerify: true, // Only for development!
    },
    Metrics: &rustmq.SecurityMetricsConfig{
        Enabled: true,
        Detailed: true, // Enable detailed metrics for debugging
    },
}

// Production configuration (strict security)
prodConfig := &rustmq.SecurityConfig{
    Auth: &rustmq.AuthenticationConfig{
        Method: rustmq.AuthMethodMTLS,
    },
    TLS: &rustmq.TLSSecurityConfig{
        Mode:               rustmq.TLSModeMutualAuth,
        CACert:             loadCACert(),
        ClientCert:         loadClientCert(),
        ClientKey:          loadClientKey(),
        InsecureSkipVerify: false,
        MinVersion:         tls.VersionTLS12,
        Validation: &rustmq.CertificateValidationSettings{
            ValidateChain:   true,
            CheckExpiration: true,
            CheckRevocation: true,
        },
    },
    ACL: &rustmq.ACLConfig{
        Enabled: true,
        ControllerEndpoints: []string{"controller:9094"},
        Cache: &rustmq.ACLCacheConfig{
            Size: 1000,
            TTL:  5 * time.Minute, // Shorter TTL for production
        },
    },
    Metrics: &rustmq.SecurityMetricsConfig{
        Enabled: true,
        Export: &rustmq.MetricsExportConfig{
            Format:   "json",
            Endpoint: "https://metrics.company.com/security",
            Interval: 30 * time.Second,
        },
    },
}
```

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