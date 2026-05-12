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
- **👥 Consumer Groups**: Full consumer group support with coordinator discovery and offset management

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

- Go 1.26.2 or later
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

---

## 📖 Full Documentation

For detailed information, see the [Go SDK Documentation](../../docs/client-sdks.md) in the main Documentation Hub.
