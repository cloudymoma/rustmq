# RustMQ Rate Limiting Configuration

This document describes the comprehensive rate limiting configuration system for the RustMQ Admin API. The rate limiting uses the Token Bucket algorithm with the `governor` crate for efficient and scalable request throttling.

## Overview

The rate limiting system provides:

- **Global rate limiting** across all clients and endpoints
- **Per-IP rate limiting** with burst allowance and IP tracking
- **Endpoint-specific rate limiting** categorized by operation type
- **Automatic cleanup** of expired rate limiters
- **Runtime configuration validation** with sensible defaults

## Configuration Structure

### Main Rate Limiting Configuration

```toml
[rate_limiting]
enabled = true  # Global toggle for all rate limiting
```

### Global Rate Limits

Apply across all clients and endpoints:

```toml
[rate_limiting.global]
requests_per_second = 1000  # Max 1000 RPS globally
burst_capacity = 2000       # Allow burst up to 2000 requests
window_size_secs = 60       # 1-minute rate calculation window
```

### Per-IP Rate Limits

Track and limit individual IP addresses:

```toml
[rate_limiting.per_ip]
enabled = true
requests_per_second = 50    # 50 RPS per IP
burst_capacity = 100        # Allow burst up to 100 requests per IP
window_size_secs = 60       # 1-minute window
max_tracked_ips = 10000     # Track up to 10,000 IPs
ip_expiry_secs = 3600       # Remove IP limiters after 1 hour
```

### Endpoint-Specific Rate Limits

Different rate limits for different types of operations:

#### Health Check Endpoints
```toml
[rate_limiting.endpoints.health]
enabled = true
requests_per_second = 100   # Higher limits for monitoring
burst_capacity = 200
window_size_secs = 60
endpoint_patterns = ["/health", "/api/v1/health"]
```

#### Read Operations
```toml
[rate_limiting.endpoints.read_operations]
enabled = true
requests_per_second = 30    # Moderate limits for reads
burst_capacity = 60
window_size_secs = 60
endpoint_patterns = [
    "/api/v1/cluster",
    "/api/v1/topics",
    "/api/v1/topics/*",
    "/api/v1/brokers"
]
```

#### Write Operations
```toml
[rate_limiting.endpoints.write_operations]
enabled = true
requests_per_second = 10    # Lower limits for writes (expensive)
burst_capacity = 20
window_size_secs = 60
endpoint_patterns = [
    "POST:/api/v1/topics",
    "DELETE:/api/v1/topics/*"
]
```

#### Cluster Management Operations
```toml
[rate_limiting.endpoints.cluster_operations]
enabled = true
requests_per_second = 5     # Very restrictive (most expensive)
burst_capacity = 10
window_size_secs = 60
endpoint_patterns = [
    "/api/v1/cluster/rebalance",
    "/api/v1/cluster/scale",
    "/api/v1/brokers/*/decommission"
]
```

### Cleanup Configuration

Automatic cleanup of expired rate limiters:

```toml
[rate_limiting.cleanup]
enabled = true
cleanup_interval_secs = 300    # Clean up every 5 minutes
max_age_secs = 3600           # Remove after 1 hour of inactivity
max_cleanup_per_run = 1000    # Clean up to 1000 limiters per run
```

## Configuration Validation

The system includes comprehensive validation:

- **Positive values**: All numeric values must be greater than 0 when enabled
- **Burst capacity**: Must be >= requests_per_second
- **Reasonable limits**: Upper bounds prevent excessive resource usage
- **Pattern validation**: Endpoint patterns cannot be empty strings
- **Conditional validation**: Disabled categories skip validation

### Validation Rules

| Field | Validation |
|-------|------------|
| `requests_per_second` | > 0, <= 1,000,000 |
| `burst_capacity` | > 0, >= requests_per_second |
| `window_size_secs` | > 0 |
| `max_tracked_ips` | > 0, <= 1,000,000 |
| `cleanup_interval_secs` | >= 10 |
| `max_cleanup_per_run` | > 0, <= 100,000 |
| `endpoint_patterns` | Non-empty when category enabled |

## Environment-Specific Examples

### Production Environment
```toml
[rate_limiting.global]
requests_per_second = 500    # More conservative
burst_capacity = 1000

[rate_limiting.per_ip]
requests_per_second = 25     # Stricter per-IP limits
burst_capacity = 50

[rate_limiting.endpoints.write_operations]
requests_per_second = 5      # Very limited writes
burst_capacity = 10

[rate_limiting.endpoints.cluster_operations]
requests_per_second = 2      # Extremely limited cluster ops
burst_capacity = 5
```

### Development Environment
```toml
[rate_limiting.global]
requests_per_second = 2000   # More permissive
burst_capacity = 4000

[rate_limiting.per_ip]
requests_per_second = 100    # Relaxed IP limits
burst_capacity = 200
```

### Disable Rate Limiting
```toml
[rate_limiting]
enabled = false              # Completely disable (dev/testing)
```

## Integration with Admin API

The rate limiting configuration integrates seamlessly with the existing RustMQ configuration system:

```rust
use rustmq::config::Config;

// Load configuration from file
let config = Config::from_file("config/admin.toml")?;

// Validate all configuration including rate limiting
config.validate()?;

// Access rate limiting configuration
let rate_config = &config.rate_limiting;
if rate_config.enabled {
    println!("Global RPS limit: {}", rate_config.global.requests_per_second);
}
```

## Token Bucket Algorithm

The rate limiting uses the Token Bucket algorithm:

1. **Tokens** are added to a bucket at a steady rate (requests_per_second)
2. **Burst capacity** determines the maximum tokens the bucket can hold
3. **Each request** consumes one token
4. **Requests are rejected** when no tokens are available
5. **Tokens replenish** over time, allowing sustained throughput

## Performance Considerations

- **Memory usage**: ~24 bytes per tracked IP address
- **CPU overhead**: Minimal, O(1) token bucket operations
- **Cleanup efficiency**: Batched cleanup prevents performance degradation
- **Concurrent access**: Thread-safe operations with minimal contention

## Monitoring and Observability

Rate limiting metrics (when implemented) will include:

- Requests allowed/rejected per endpoint category
- Current token counts per rate limiter
- IP address tracking statistics
- Cleanup operation metrics

## Error Handling

Rate limit exceeded responses will include:

- HTTP 429 status code
- Retry-After header with recommended wait time
- Clear error message indicating which limit was exceeded
- Leader hint for client redirection if applicable

## Testing

The configuration includes comprehensive test coverage:

- Default configuration validation
- TOML serialization/deserialization
- Edge case validation (extreme values)
- Disabled category handling
- Endpoint pattern validation

Run tests with:
```bash
cargo test config::tests::test_rate_limiting --lib
```

## Example Usage

See `/config/admin-rate-limiting-example.toml` for a complete configuration example with detailed comments and multiple environment scenarios.