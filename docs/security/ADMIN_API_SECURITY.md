# RustMQ Admin API Security Guide

This comprehensive guide covers the security features and endpoints of RustMQ's Admin REST API, providing detailed examples for certificate management, ACL operations, security monitoring, and API integration.

## Table of Contents

- [Overview](#overview)
- [API Authentication](#api-authentication)
- [Certificate Management API](#certificate-management-api)
- [ACL Management API](#acl-management-api)
- [Security Monitoring API](#security-monitoring-api)
- [Audit API](#audit-api)
- [Rate Limiting](#rate-limiting)
- [API Security Best Practices](#api-security-best-practices)
- [Error Handling](#error-handling)
- [Integration Examples](#integration-examples)

## Overview

RustMQ's Admin REST API provides comprehensive security management capabilities through a RESTful interface. The API includes 30+ security endpoints organized into several categories:

- **Certificate Authority Operations**: CA initialization, management, and status
- **Certificate Lifecycle**: Issue, renew, rotate, revoke, and validate certificates
- **ACL Management**: Create, update, delete, and test access control rules
- **Security Monitoring**: Real-time security metrics and health status
- **Audit Operations**: Security event logs and compliance reporting

### API Base Configuration

```bash
# Default API endpoint
export RUSTMQ_ADMIN_API_URL="http://localhost:8080"

# For production with HTTPS
export RUSTMQ_ADMIN_API_URL="https://admin.rustmq.company.com"

# API authentication (if configured)
export RUSTMQ_API_TOKEN="your-api-token"
```

## API Authentication

### API Token Authentication

When API authentication is enabled, include the authorization header:

```bash
# Set up authentication
curl -H "Authorization: Bearer ${RUSTMQ_API_TOKEN}" \
     -H "Content-Type: application/json" \
     "${RUSTMQ_ADMIN_API_URL}/api/v1/security/status"
```

### Client Certificate Authentication

For environments requiring client certificate authentication:

```bash
# Using client certificates
curl --cert /etc/rustmq/certs/admin-cert.pem \
     --key /etc/rustmq/certs/admin-key.pem \
     --cacert /etc/rustmq/certs/ca.pem \
     "${RUSTMQ_ADMIN_API_URL}/api/v1/security/status"
```

## Certificate Management API

### Certificate Authority Operations

#### Initialize Root CA
```bash
POST /api/v1/security/ca/init

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/ca/init" \
  -H "Content-Type: application/json" \
  -d '{
    "common_name": "RustMQ Root CA",
    "organization": "Your Organization",
    "country": "US",
    "state": "California",
    "city": "San Francisco",
    "validity_years": 10,
    "key_size": 4096,
    "signature_algorithm": "SHA256WithRSA"
  }'

# Response:
{
  "success": true,
  "data": {
    "ca_id": "root_ca_1",
    "certificate_pem": "-----BEGIN CERTIFICATE-----\n...",
    "created_at": "2024-01-15T10:30:00Z",
    "expires_at": "2034-01-15T10:30:00Z"
  },
  "message": "Root CA initialized successfully"
}
```

#### List Certificate Authorities
```bash
GET /api/v1/security/ca

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/ca"

# Response:
{
  "success": true,
  "data": [
    {
      "ca_id": "root_ca_1",
      "common_name": "RustMQ Root CA",
      "status": "active",
      "created_at": "2024-01-15T10:30:00Z",
      "expires_at": "2034-01-15T10:30:00Z",
      "certificate_count": 156
    }
  ]
}
```

#### Create Intermediate CA
```bash
POST /api/v1/security/ca/intermediate

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/ca/intermediate" \
  -H "Content-Type: application/json" \
  -d '{
    "parent_ca_id": "root_ca_1",
    "common_name": "RustMQ Production CA",
    "organization": "Your Organization",
    "organizational_unit": "Production",
    "validity_years": 5,
    "path_length_constraint": 0
  }'
```

#### Get CA Information
```bash
GET /api/v1/security/ca/{ca_id}

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/ca/root_ca_1"

# Response:
{
  "success": true,
  "data": {
    "ca_id": "root_ca_1",
    "common_name": "RustMQ Root CA",
    "status": "active",
    "certificate_pem": "-----BEGIN CERTIFICATE-----\n...",
    "public_key_info": {
      "algorithm": "RSA",
      "key_size": 4096
    },
    "validity": {
      "not_before": "2024-01-15T10:30:00Z",
      "not_after": "2034-01-15T10:30:00Z"
    },
    "statistics": {
      "total_certificates_issued": 156,
      "active_certificates": 142,
      "revoked_certificates": 14
    }
  }
}
```

### Certificate Lifecycle Operations

#### Issue Certificate
```bash
POST /api/v1/security/certificates/issue

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates/issue" \
  -H "Content-Type: application/json" \
  -d '{
    "principal": "broker-01.internal.company.com",
    "role": "broker",
    "ca_id": "production_ca_1",
    "validity_days": 365,
    "key_size": 2048,
    "subject_alt_names": {
      "dns_names": ["broker-01", "broker-01.k8s.local"],
      "ip_addresses": ["192.168.1.100", "10.0.1.100"]
    },
    "key_usage": ["digital_signature", "key_encipherment"],
    "extended_key_usage": ["server_auth", "client_auth"]
  }'

# Response:
{
  "success": true,
  "data": {
    "certificate_id": "cert_12345",
    "certificate_pem": "-----BEGIN CERTIFICATE-----\n...",
    "private_key_pem": "-----BEGIN PRIVATE KEY-----\n...",
    "certificate_chain": [
      "-----BEGIN CERTIFICATE-----\n...",  // End entity cert
      "-----BEGIN CERTIFICATE-----\n..."   // Intermediate CA cert
    ],
    "serial_number": "1234567890ABCDEF",
    "expires_at": "2025-01-15T10:30:00Z"
  },
  "message": "Certificate issued successfully"
}
```

#### List Certificates
```bash
GET /api/v1/security/certificates

# With filtering
curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates?role=broker&status=active&limit=50"

# Response:
{
  "success": true,
  "data": {
    "certificates": [
      {
        "certificate_id": "cert_12345",
        "subject": "CN=broker-01.internal.company.com,O=Your Organization",
        "role": "broker",
        "status": "active",
        "issued_at": "2024-01-15T10:30:00Z",
        "expires_at": "2025-01-15T10:30:00Z",
        "days_until_expiry": 335
      }
    ],
    "total_count": 142,
    "page": 1,
    "page_size": 50
  }
}
```

#### Renew Certificate
```bash
POST /api/v1/security/certificates/{certificate_id}/renew

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates/cert_12345/renew" \
  -H "Content-Type: application/json" \
  -d '{
    "validity_days": 365,
    "preserve_key": true,
    "reason": "Scheduled renewal"
  }'
```

#### Rotate Certificate
```bash
POST /api/v1/security/certificates/{certificate_id}/rotate

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates/cert_12345/rotate" \
  -H "Content-Type: application/json" \
  -d '{
    "overlap_days": 7,
    "notification_email": "ops@company.com",
    "reason": "Security policy rotation"
  }'
```

#### Revoke Certificate
```bash
POST /api/v1/security/certificates/{certificate_id}/revoke

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates/cert_12345/revoke" \
  -H "Content-Type: application/json" \
  -d '{
    "reason": "key_compromise",
    "effective_immediately": true,
    "notify_clients": true,
    "revocation_comment": "Security incident #INC-2024-001"
  }'
```

#### Validate Certificate
```bash
POST /api/v1/security/certificates/validate

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates/validate" \
  -H "Content-Type: application/json" \
  -d '{
    "certificate_pem": "-----BEGIN CERTIFICATE-----\n...",
    "check_revocation": true,
    "check_chain": true,
    "check_expiry": true
  }'

# Response:
{
  "success": true,
  "data": {
    "valid": true,
    "validation_results": {
      "signature_valid": true,
      "chain_valid": true,
      "not_expired": true,
      "not_revoked": true,
      "hostname_valid": true
    },
    "certificate_info": {
      "subject": "CN=broker-01.internal.company.com",
      "issuer": "CN=RustMQ Production CA",
      "serial_number": "1234567890ABCDEF",
      "not_before": "2024-01-15T10:30:00Z",
      "not_after": "2025-01-15T10:30:00Z"
    }
  }
}
```

#### Get Expiring Certificates
```bash
GET /api/v1/security/certificates/expiring

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates/expiring?days=30"

# Response:
{
  "success": true,
  "data": {
    "expiring_certificates": [
      {
        "certificate_id": "cert_67890",
        "subject": "CN=admin@company.com",
        "expires_at": "2024-02-01T10:30:00Z",
        "days_until_expiry": 17,
        "auto_renewal_enabled": false,
        "requires_manual_action": true
      }
    ],
    "total_expiring": 1,
    "auto_renewable": 0,
    "requires_manual_action": 1
  }
}
```

## ACL Management API

### ACL Rule Operations

#### Create ACL Rule
```bash
POST /api/v1/security/acl/rules

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/rules" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "analytics_user_events_access",
    "description": "Analytics service access to user events",
    "principal": "analytics@company.com",
    "principal_type": "service",
    "resource": "topic.user-events.*",
    "resource_type": "topic",
    "operations": ["read", "write"],
    "effect": "allow",
    "priority": 100,
    "conditions": [
      {
        "type": "source_ip",
        "value": "10.0.0.0/8"
      },
      {
        "type": "time_range",
        "value": "09:00-17:00",
        "timezone": "UTC",
        "days_of_week": ["Mon", "Tue", "Wed", "Thu", "Fri"]
      }
    ],
    "enabled": true,
    "expires_at": null
  }'

# Response:
{
  "success": true,
  "data": {
    "rule_id": "acl_rule_12345",
    "name": "analytics_user_events_access",
    "created_at": "2024-01-15T10:30:00Z",
    "status": "active"
  },
  "message": "ACL rule created successfully"
}
```

#### List ACL Rules
```bash
GET /api/v1/security/acl/rules

# With filtering
curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/rules?principal=analytics@company.com&effect=allow&enabled=true"

# Response:
{
  "success": true,
  "data": {
    "rules": [
      {
        "rule_id": "acl_rule_12345",
        "name": "analytics_user_events_access",
        "principal": "analytics@company.com",
        "resource": "topic.user-events.*",
        "operations": ["read", "write"],
        "effect": "allow",
        "priority": 100,
        "enabled": true,
        "created_at": "2024-01-15T10:30:00Z",
        "last_used": "2024-01-15T14:22:00Z"
      }
    ],
    "total_count": 1,
    "page": 1,
    "page_size": 50
  }
}
```

#### Update ACL Rule
```bash
PUT /api/v1/security/acl/rules/{rule_id}

curl -X PUT "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/rules/acl_rule_12345" \
  -H "Content-Type: application/json" \
  -d '{
    "operations": ["read", "write", "create"],
    "priority": 150,
    "description": "Updated to include create permission"
  }'
```

#### Delete ACL Rule
```bash
DELETE /api/v1/security/acl/rules/{rule_id}

curl -X DELETE "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/rules/acl_rule_12345" \
  -H "Content-Type: application/json" \
  -d '{
    "reason": "No longer needed - service decommissioned"
  }'
```

#### Test ACL Permission
```bash
POST /api/v1/security/acl/test

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/test" \
  -H "Content-Type: application/json" \
  -d '{
    "principal": "analytics@company.com",
    "resource": "topic.user-events.login",
    "operation": "read",
    "context": {
      "source_ip": "10.0.1.100",
      "timestamp": "2024-01-15T14:30:00Z",
      "client_version": "1.5.0"
    }
  }'

# Response:
{
  "success": true,
  "data": {
    "result": "allowed",
    "matched_rule": {
      "rule_id": "acl_rule_12345",
      "name": "analytics_user_events_access",
      "effect": "allow"
    },
    "evaluation_time_ns": 45,
    "cache_level": "L1",
    "conditions_evaluated": [
      {
        "type": "source_ip",
        "result": "matched",
        "value": "10.0.0.0/8"
      },
      {
        "type": "time_range",
        "result": "matched",
        "value": "09:00-17:00"
      }
    ]
  }
}
```

#### Bulk Test ACL Permissions
```bash
POST /api/v1/security/acl/bulk-test

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/bulk-test" \
  -H "Content-Type: application/json" \
  -d '{
    "test_cases": [
      {
        "name": "analytics_read_access",
        "principal": "analytics@company.com",
        "resource": "topic.user-events",
        "operation": "read",
        "expected_result": "allowed"
      },
      {
        "name": "unauthorized_write_attempt",
        "principal": "guest@company.com",
        "resource": "topic.sensitive-data",
        "operation": "write",
        "expected_result": "denied"
      }
    ]
  }'

# Response:
{
  "success": true,
  "data": {
    "results": [
      {
        "test_case": "analytics_read_access",
        "result": "allowed",
        "expected": "allowed",
        "passed": true,
        "evaluation_time_ns": 42
      },
      {
        "test_case": "unauthorized_write_attempt",
        "result": "denied",
        "expected": "denied",
        "passed": true,
        "evaluation_time_ns": 38
      }
    ],
    "summary": {
      "total_tests": 2,
      "passed": 2,
      "failed": 0,
      "average_evaluation_time_ns": 40
    }
  }
}
```

#### Get Principal Permissions
```bash
GET /api/v1/security/acl/principals/{principal}/permissions

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/principals/analytics@company.com/permissions"

# Response:
{
  "success": true,
  "data": {
    "principal": "analytics@company.com",
    "permissions": [
      {
        "resource": "topic.user-events.*",
        "operations": ["read", "write"],
        "effect": "allow",
        "conditions": [
          {
            "type": "source_ip",
            "value": "10.0.0.0/8"
          }
        ]
      }
    ],
    "effective_permissions_count": 1,
    "last_access": "2024-01-15T14:22:00Z"
  }
}
```

### ACL Cache Management

#### Get Cache Statistics
```bash
GET /api/v1/security/acl/cache/stats

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/cache/stats"

# Response:
{
  "success": true,
  "data": {
    "l1_cache": {
      "total_entries": 1250,
      "hit_rate": 0.89,
      "average_lookup_time_ns": 12,
      "evictions_per_hour": 15
    },
    "l2_cache": {
      "total_entries": 8750,
      "hit_rate": 0.76,
      "average_lookup_time_ns": 52,
      "shard_distribution": [275, 280, 265, 278, ...]
    },
    "l3_bloom_filter": {
      "size": 1000000,
      "false_positive_rate": 0.008,
      "average_lookup_time_ns": 18
    },
    "controller_fetch": {
      "requests_per_hour": 45,
      "average_batch_size": 12,
      "average_response_time_ms": 2.5
    }
  }
}
```

#### Refresh ACL Cache
```bash
POST /api/v1/security/acl/cache/refresh

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/cache/refresh" \
  -H "Content-Type: application/json" \
  -d '{
    "level": "all",
    "wait_for_completion": true
  }'
```

#### Warm ACL Cache
```bash
POST /api/v1/security/acl/cache/warm

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/cache/warm" \
  -H "Content-Type: application/json" \
  -d '{
    "principals": ["analytics@company.com", "logger@company.com"],
    "resources": ["topic.user-events.*", "topic.logs.*"]
  }'
```

## Security Monitoring API

### Security Status and Health

#### Get Security Status
```bash
GET /api/v1/security/status

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/status"

# Response:
{
  "success": true,
  "data": {
    "security_enabled": true,
    "tls_enabled": true,
    "mtls_enabled": true,
    "acl_enabled": true,
    "certificate_authority": {
      "status": "healthy",
      "active_cas": 2,
      "total_certificates": 156,
      "certificates_expiring_30_days": 3
    },
    "acl_system": {
      "status": "healthy",
      "total_rules": 45,
      "enabled_rules": 42,
      "cache_hit_rate": 0.87
    },
    "audit_system": {
      "status": "healthy",
      "events_logged_last_hour": 1250,
      "log_file_size_mb": 45.2
    },
    "last_updated": "2024-01-15T14:30:00Z"
  }
}
```

#### Get Security Metrics
```bash
GET /api/v1/security/metrics

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/metrics?duration=1h"

# Response:
{
  "success": true,
  "data": {
    "authentication": {
      "total_attempts": 15750,
      "successful_authentications": 15698,
      "failed_authentications": 52,
      "success_rate": 0.997,
      "average_auth_time_ms": 8.5
    },
    "authorization": {
      "total_requests": 125600,
      "allowed_requests": 124890,
      "denied_requests": 710,
      "cache_hits": 109024,
      "cache_hit_rate": 0.868,
      "average_authz_time_ns": 47
    },
    "certificates": {
      "validation_requests": 3420,
      "successful_validations": 3398,
      "failed_validations": 22,
      "revocation_checks": 3420,
      "average_validation_time_ms": 12.3
    },
    "period": {
      "start": "2024-01-15T13:30:00Z",
      "end": "2024-01-15T14:30:00Z",
      "duration_seconds": 3600
    }
  }
}
```

#### Security Health Check
```bash
GET /api/v1/security/health

curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/health"

# Response:
{
  "success": true,
  "data": {
    "overall_health": "healthy",
    "components": {
      "certificate_manager": {
        "status": "healthy",
        "last_check": "2024-01-15T14:29:45Z",
        "details": "All certificate operations functioning normally"
      },
      "authentication_manager": {
        "status": "healthy",
        "last_check": "2024-01-15T14:29:45Z",
        "details": "mTLS authentication working correctly"
      },
      "authorization_manager": {
        "status": "healthy",
        "last_check": "2024-01-15T14:29:45Z",
        "details": "ACL cache hit rate: 87%"
      },
      "audit_system": {
        "status": "healthy",
        "last_check": "2024-01-15T14:29:45Z",
        "details": "Audit logging operational"
      }
    },
    "recommendations": [
      "Consider increasing L2 cache size to improve hit rate",
      "3 certificates expire within 30 days - schedule renewal"
    ]
  }
}
```

## Audit API

### Audit Log Operations

#### Get Audit Logs
```bash
GET /api/v1/security/audit/logs

# With filtering
curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/audit/logs?event_type=certificate_issued&since=2024-01-15T00:00:00Z&limit=100"

# Response:
{
  "success": true,
  "data": {
    "events": [
      {
        "event_id": "audit_12345",
        "timestamp": "2024-01-15T10:30:00Z",
        "event_type": "certificate_issued",
        "principal": "admin@company.com",
        "resource": "cert_67890",
        "details": {
          "subject": "CN=broker-02.internal.company.com",
          "validity_days": 365,
          "ca_id": "production_ca_1"
        },
        "source_ip": "192.168.1.50",
        "user_agent": "rustmq-admin/1.0.0",
        "result": "success"
      }
    ],
    "total_count": 1,
    "page": 1,
    "page_size": 100
  }
}
```

#### Get Audit Events (Real-time)
```bash
GET /api/v1/security/audit/events

# Stream audit events (Server-Sent Events)
curl -N "${RUSTMQ_ADMIN_API_URL}/api/v1/security/audit/events?follow=true&filter=authentication"

# Response (SSE stream):
data: {"event_type":"authentication_failure","principal":"unknown@external.com","timestamp":"2024-01-15T14:35:00Z","source_ip":"203.0.113.50"}

data: {"event_type":"certificate_validation","principal":"broker-01@company.com","timestamp":"2024-01-15T14:35:15Z","result":"success"}
```

#### Generate Audit Report
```bash
POST /api/v1/security/audit/reports

curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/audit/reports" \
  -H "Content-Type: application/json" \
  -d '{
    "report_type": "security_summary",
    "period": {
      "start": "2024-01-01T00:00:00Z",
      "end": "2024-01-31T23:59:59Z"
    },
    "include_sections": [
      "authentication_summary",
      "authorization_summary", 
      "certificate_operations",
      "security_violations"
    ],
    "format": "json"
  }'

# Response:
{
  "success": true,
  "data": {
    "report_id": "report_12345",
    "status": "generating",
    "estimated_completion": "2024-01-15T14:40:00Z",
    "download_url": "/api/v1/security/audit/reports/report_12345/download"
  }
}
```

## Rate Limiting

The Admin API includes comprehensive rate limiting to protect against abuse:

### Rate Limit Categories

#### Health Check Endpoints (100 requests/minute)
```bash
# High-frequency endpoints for monitoring
GET /api/v1/health
GET /api/v1/security/status
```

#### Read Operations (30 requests/minute)
```bash
# Standard read operations
GET /api/v1/security/certificates
GET /api/v1/security/acl/rules
GET /api/v1/security/audit/logs
```

#### Write Operations (10 requests/minute)
```bash
# Certificate and ACL modifications
POST /api/v1/security/certificates/issue
PUT /api/v1/security/acl/rules/{rule_id}
DELETE /api/v1/security/certificates/{cert_id}
```

#### Administrative Operations (5 requests/minute)
```bash
# High-privilege operations
POST /api/v1/security/ca/init
POST /api/v1/security/certificates/{cert_id}/revoke
POST /api/v1/security/acl/cache/refresh
```

### Rate Limit Headers

All API responses include rate limiting information:

```bash
curl -I "${RUSTMQ_ADMIN_API_URL}/api/v1/security/status"

# Response headers:
HTTP/1.1 200 OK
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1640995260
X-RateLimit-Type: endpoint
```

### Rate Limit Exceeded Response

When rate limits are exceeded:

```bash
# Request
curl "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates"

# Response (429 Too Many Requests)
{
  "success": false,
  "error": "Rate limit exceeded for endpoint. Limit: 30 requests per minute",
  "retry_after": 42,
  "rate_limit": {
    "limit": 30,
    "remaining": 0,
    "reset_time": "2024-01-15T14:35:00Z",
    "type": "endpoint"
  }
}
```

## API Security Best Practices

### 1. Authentication and Authorization

```bash
# Always use HTTPS in production
export RUSTMQ_ADMIN_API_URL="https://admin.rustmq.company.com"

# Use API tokens with limited scope
export RUSTMQ_API_TOKEN="rustmq_token_security_read_only_..."

# Rotate API tokens regularly
curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/auth/tokens/rotate" \
  -H "Authorization: Bearer ${RUSTMQ_API_TOKEN}"
```

### 2. Input Validation

The API performs comprehensive input validation:

```bash
# Example: Invalid certificate issuance request
curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/certificates/issue" \
  -H "Content-Type: application/json" \
  -d '{
    "principal": "",
    "role": "invalid_role",
    "validity_days": -1
  }'

# Response:
{
  "success": false,
  "error": "Validation failed",
  "validation_errors": [
    {
      "field": "principal",
      "error": "Principal cannot be empty"
    },
    {
      "field": "role",
      "error": "Invalid role. Must be one of: broker, client, admin"
    },
    {
      "field": "validity_days",
      "error": "Validity days must be positive"
    }
  ]
}
```

### 3. Secure Headers

The API includes security headers in all responses:

```bash
# Security headers in responses
Strict-Transport-Security: max-age=31536000; includeSubDomains
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
X-XSS-Protection: 1; mode=block
Content-Security-Policy: default-src 'self'
```

### 4. Request Size Limits

```bash
# Request body size limits
# Certificate requests: 10KB max
# ACL rules: 5KB max
# Batch operations: 1MB max

# Example: Request too large
curl -X POST "${RUSTMQ_ADMIN_API_URL}/api/v1/security/acl/bulk-test" \
  -H "Content-Type: application/json" \
  -d '{"test_cases": [...]}'  # Very large payload

# Response:
{
  "success": false,
  "error": "Request body too large. Maximum size: 1MB"
}
```

## Error Handling

### Standard Error Response Format

```json
{
  "success": false,
  "error": "Human-readable error message",
  "error_code": "SPECIFIC_ERROR_CODE",
  "details": {
    "additional": "error context"
  },
  "request_id": "req_12345",
  "timestamp": "2024-01-15T14:30:00Z"
}
```

### Common Error Codes

#### Authentication Errors (401)
```bash
# Missing or invalid authentication
{
  "success": false,
  "error": "Authentication required",
  "error_code": "AUTH_REQUIRED"
}

# Expired API token
{
  "success": false,
  "error": "API token expired",
  "error_code": "TOKEN_EXPIRED"
}
```

#### Authorization Errors (403)
```bash
# Insufficient permissions
{
  "success": false,
  "error": "Insufficient permissions for this operation",
  "error_code": "INSUFFICIENT_PERMISSIONS",
  "required_permission": "security:certificates:issue"
}
```

#### Resource Not Found (404)
```bash
# Certificate not found
{
  "success": false,
  "error": "Certificate not found",
  "error_code": "CERTIFICATE_NOT_FOUND",
  "certificate_id": "cert_12345"
}
```

#### Validation Errors (400)
```bash
# Invalid request format
{
  "success": false,
  "error": "Invalid request format",
  "error_code": "VALIDATION_ERROR",
  "validation_errors": [...]
}
```

#### Rate Limiting (429)
```bash
# Rate limit exceeded
{
  "success": false,
  "error": "Rate limit exceeded",
  "error_code": "RATE_LIMIT_EXCEEDED",
  "retry_after": 60
}
```

#### Internal Errors (500)
```bash
# Internal server error
{
  "success": false,
  "error": "Internal server error",
  "error_code": "INTERNAL_ERROR",
  "request_id": "req_12345"
}
```

## Integration Examples

### Python Integration

```python
import requests
import json
from datetime import datetime, timezone

class RustMQSecurityAPI:
    def __init__(self, base_url, api_token=None):
        self.base_url = base_url
        self.session = requests.Session()
        if api_token:
            self.session.headers.update({
                'Authorization': f'Bearer {api_token}',
                'Content-Type': 'application/json'
            })
    
    def issue_certificate(self, principal, role, ca_id, validity_days=365):
        """Issue a new certificate"""
        data = {
            'principal': principal,
            'role': role,
            'ca_id': ca_id,
            'validity_days': validity_days
        }
        
        response = self.session.post(
            f'{self.base_url}/api/v1/security/certificates/issue',
            json=data
        )
        
        if response.status_code == 201:
            return response.json()['data']
        else:
            raise Exception(f"Certificate issuance failed: {response.text}")
    
    def create_acl_rule(self, principal, resource, operations, effect='allow'):
        """Create a new ACL rule"""
        data = {
            'principal': principal,
            'resource': resource,
            'operations': operations,
            'effect': effect,
            'enabled': True
        }
        
        response = self.session.post(
            f'{self.base_url}/api/v1/security/acl/rules',
            json=data
        )
        
        if response.status_code == 201:
            return response.json()['data']
        else:
            raise Exception(f"ACL rule creation failed: {response.text}")
    
    def test_permission(self, principal, resource, operation, source_ip=None):
        """Test if a principal has permission for an operation"""
        data = {
            'principal': principal,
            'resource': resource,
            'operation': operation
        }
        
        if source_ip:
            data['context'] = {'source_ip': source_ip}
        
        response = self.session.post(
            f'{self.base_url}/api/v1/security/acl/test',
            json=data
        )
        
        if response.status_code == 200:
            return response.json()['data']
        else:
            raise Exception(f"Permission test failed: {response.text}")

# Usage example
api = RustMQSecurityAPI('https://admin.rustmq.company.com', 'your-api-token')

# Issue a certificate
cert = api.issue_certificate(
    principal='app-server@company.com',
    role='client',
    ca_id='production_ca_1',
    validity_days=90
)
print(f"Certificate issued: {cert['certificate_id']}")

# Create an ACL rule
rule = api.create_acl_rule(
    principal='app-server@company.com',
    resource='topic.application.*',
    operations=['read', 'write']
)
print(f"ACL rule created: {rule['rule_id']}")

# Test permission
result = api.test_permission(
    principal='app-server@company.com',
    resource='topic.application.logs',
    operation='write',
    source_ip='10.0.1.100'
)
print(f"Permission result: {result['result']}")
```

### Go Integration

```go
package main

import (
    "bytes"
    "encoding/json"
    "fmt"
    "net/http"
    "time"
)

type RustMQSecurityAPI struct {
    BaseURL   string
    APIToken  string
    Client    *http.Client
}

func NewRustMQSecurityAPI(baseURL, apiToken string) *RustMQSecurityAPI {
    return &RustMQSecurityAPI{
        BaseURL:  baseURL,
        APIToken: apiToken,
        Client:   &http.Client{Timeout: 30 * time.Second},
    }
}

func (api *RustMQSecurityAPI) makeRequest(method, endpoint string, body interface{}) (*http.Response, error) {
    var reqBody bytes.Buffer
    if body != nil {
        if err := json.NewEncoder(&reqBody).Encode(body); err != nil {
            return nil, err
        }
    }
    
    req, err := http.NewRequest(method, api.BaseURL+endpoint, &reqBody)
    if err != nil {
        return nil, err
    }
    
    req.Header.Set("Content-Type", "application/json")
    if api.APIToken != "" {
        req.Header.Set("Authorization", "Bearer "+api.APIToken)
    }
    
    return api.Client.Do(req)
}

type CertificateIssueRequest struct {
    Principal    string   `json:"principal"`
    Role         string   `json:"role"`
    CAID         string   `json:"ca_id"`
    ValidityDays int      `json:"validity_days"`
}

type CertificateResponse struct {
    CertificateID  string `json:"certificate_id"`
    CertificatePEM string `json:"certificate_pem"`
    PrivateKeyPEM  string `json:"private_key_pem"`
    ExpiresAt      string `json:"expires_at"`
}

func (api *RustMQSecurityAPI) IssueCertificate(principal, role, caID string, validityDays int) (*CertificateResponse, error) {
    req := CertificateIssueRequest{
        Principal:    principal,
        Role:         role,
        CAID:         caID,
        ValidityDays: validityDays,
    }
    
    resp, err := api.makeRequest("POST", "/api/v1/security/certificates/issue", req)
    if err != nil {
        return nil, err
    }
    defer resp.Body.Close()
    
    if resp.StatusCode != http.StatusCreated {
        return nil, fmt.Errorf("certificate issuance failed with status %d", resp.StatusCode)
    }
    
    var result struct {
        Success bool                `json:"success"`
        Data    CertificateResponse `json:"data"`
    }
    
    if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
        return nil, err
    }
    
    return &result.Data, nil
}

func main() {
    api := NewRustMQSecurityAPI("https://admin.rustmq.company.com", "your-api-token")
    
    cert, err := api.IssueCertificate("service@company.com", "client", "production_ca_1", 90)
    if err != nil {
        fmt.Printf("Error issuing certificate: %v\n", err)
        return
    }
    
    fmt.Printf("Certificate issued: %s\n", cert.CertificateID)
    fmt.Printf("Expires at: %s\n", cert.ExpiresAt)
}
```

This Admin API Security guide provides comprehensive coverage of RustMQ's security API endpoints with practical examples and best practices for production integration.