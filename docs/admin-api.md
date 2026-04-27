## 🌐 Web UI

RustMQ includes a modern, responsive web interface for managing and monitoring your clusters. Built with Vue.js 3 and TypeScript, the WebUI provides real-time cluster insights and full topic management capabilities.

### ✨ Features

- **📊 Dashboard**: Real-time cluster overview with health metrics, broker statistics, and recent topics
- **📝 Topics Management**: Create, list, delete topics with configurable partitions, replication, and retention
- **🖥️ Brokers Monitoring**: Live broker health tracking with auto-refresh every 5 seconds
- **⚙️ Configuration Viewer**: View cluster settings, storage configuration, and network ports
- **🔒 ACL Viewer**: Security status and best practices (full ACL management coming soon)

### 🚀 Quick Start

#### Development Mode

```bash
cd web
npm install
npm run dev
```

The WebUI will be available at `http://localhost:3000` with API proxy to the Admin server on port 8080.

#### Production Deployment

The WebUI is served directly by the RustMQ admin server (no separate web server needed):

```bash
# Build the WebUI
cd web
npm run build

# Start the admin server
cargo run --bin admin_server

# Access the WebUI at http://localhost:8080
```

### 📚 Documentation

For detailed documentation including:
- Complete feature list and usage instructions
- API integration details
- Development and deployment guides
- Troubleshooting and configuration

See the **[Web UI Documentation](web/README.md)**

### 🎨 Tech Stack

- **Framework**: Vue.js 3 (Composition API with `<script setup>`)
- **Language**: TypeScript with full type safety
- **Build Tool**: Vite for fast development and optimized builds
- **HTTP Client**: Axios with typed API responses
- **Styling**: Custom CSS with dark theme


## 🛠️ Admin REST API

RustMQ provides a comprehensive REST API for cluster management, monitoring, and operations. The Admin API includes real-time health tracking, topic management, broker monitoring, and operational metrics.

### 🚀 Key Features

- **Real-time Health Monitoring**: Live broker health tracking with automatic timeout detection
- **Cluster Status**: Comprehensive cluster health assessment with leadership tracking
- **Topic Management**: CRUD operations for topics with partition and replication management
- **Broker Operations**: Broker listing with health status and rack awareness
- **Operational Metrics**: Uptime tracking and performance monitoring
- **Advanced Rate Limiting**: Token bucket algorithm with configurable global, per-IP, and endpoint-specific limits
- **Production Ready**: Comprehensive error handling and JSON API responses

### 🏃 Quick Start

Start the Admin API server:

```bash
# Start with default settings (port 8080)
./target/release/rustmq-admin serve-api

# Start on custom port
./target/release/rustmq-admin serve-api 9642

# Docker environment (included in docker-compose)
docker-compose up -d
# Admin API available at http://localhost:9642
```

### 📊 Health Monitoring

The Admin API provides comprehensive health monitoring with real-time broker tracking:

#### Health Endpoint
```bash
# Check service health and uptime
curl http://localhost:8080/health
```

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "is_leader": true,
  "raft_term": 1
}
```

#### Cluster Status
```bash
# Get comprehensive cluster status
curl http://localhost:8080/api/v1/cluster
```

**Response:**
```json
{
  "success": true,
  "data": {
    "brokers": [
      {
        "id": "broker-1",
        "host": "localhost",
        "port_quic": 9092,
        "port_rpc": 9093,
        "rack_id": "rack-1",
        "online": true
      },
      {
        "id": "broker-2",
        "host": "localhost",
        "port_quic": 9192,
        "port_rpc": 9193,
        "rack_id": "rack-2", 
        "online": false
      }
    ],
    "topics": [],
    "leader": "controller-1",
    "term": 1,
    "healthy": true
  },
  "error": null,
  "leader_hint": null
}
```

### 📋 Topic Management

#### List Topics
```bash
curl http://localhost:8080/api/v1/topics
```

#### Create Topic
```bash
curl -X POST http://localhost:8080/api/v1/topics \
  -H "Content-Type: application/json" \
  -d '{
    "name": "user-events",
    "partitions": 12,
    "replication_factor": 3,
    "retention_ms": 604800000,
    "segment_bytes": 1073741824,
    "compression_type": "lz4"
  }'
```

**Response:**
```json
{
  "success": true,
  "data": "Topic 'user-events' created",
  "error": null,
  "leader_hint": "controller-1"
}
```

#### Describe Topic
```bash
curl http://localhost:8080/api/v1/topics/user-events
```

**Response:**
```json
{
  "success": true,
  "data": {
    "name": "user-events",
    "partitions": 12,
    "replication_factor": 3,
    "config": {
      "retention_ms": 604800000,
      "segment_bytes": 1073741824,
      "compression_type": "lz4"
    },
    "created_at": "2024-01-15T10:30:00Z",
    "partition_assignments": [
      {
        "partition": 0,
        "leader": "broker-1",
        "replicas": ["broker-1", "broker-2", "broker-3"],
        "in_sync_replicas": ["broker-1", "broker-2"],
        "leader_epoch": 1
      }
    ]
  },
  "error": null,
  "leader_hint": "controller-1"
}
```

#### Delete Topic
```bash
curl -X DELETE http://localhost:8080/api/v1/topics/user-events
```

### 🖥️ Broker Management

#### List Brokers
```bash
curl http://localhost:8080/api/v1/brokers
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "broker-1",
      "host": "localhost",
      "port_quic": 9092,
      "port_rpc": 9093,
      "rack_id": "us-central1-a",
      "online": true
    },
    {
      "id": "broker-2", 
      "host": "localhost",
      "port_quic": 9192,
      "port_rpc": 9193,
      "rack_id": "us-central1-b",
      "online": true
    }
  ],
  "error": null,
  "leader_hint": "controller-1"
}
```

### 🔧 Health Tracking System

The Admin API includes a sophisticated health tracking system with comprehensive broker health monitoring:

#### Features
- **Background Health Monitoring**: Automatic health checks every 15 seconds
- **Timeout-based Health Assessment**: Configurable 30-second health timeout
- **Intelligent Cluster Health**: Smart health calculation for small clusters
- **Real-time Updates**: Live health status in all broker-related endpoints
- **Stale Entry Cleanup**: Automatic cleanup of old health data
- **🆕 Broker Health Check API**: Comprehensive broker health assessment with component-level monitoring

#### Health Check Logic
- **Healthy**: Last successful health check within 30 seconds
- **Unhealthy**: No successful health check or timeout exceeded
- **Cluster Health**: For ≤2 brokers: healthy if ≥1 broker healthy + leader exists
- **Large Clusters**: Healthy if majority of brokers healthy + leader exists

#### Broker Health Check
The newly implemented broker health check provides detailed component-level monitoring:
- **WAL Health**: Write-ahead log performance and status monitoring
- **Cache Health**: Memory cache hit rates and efficiency metrics
- **Object Storage Health**: Cloud storage connectivity and upload performance
- **Network Health**: Connection status and throughput monitoring
- **Replication Health**: Follower sync status and replication lag tracking
- **Resource Usage**: CPU, memory, disk, and network utilization statistics

For detailed configuration and usage, see [Broker Health Monitoring](docs/broker-health-monitoring.md).

### 🚨 Error Handling

The Admin API provides comprehensive error handling with detailed responses:

#### Error Response Format
```json
{
  "success": false,
  "data": null,
  "error": "Detailed error message",
  "leader_hint": "controller-2"
}
```

#### Common Error Scenarios
- **Topic Not Found** (404): Topic doesn't exist
- **Insufficient Brokers**: Not enough brokers for replication factor
- **Leader Not Available**: Controller leader election in progress
- **Invalid Configuration**: Malformed request parameters

### 🛡️ Rate Limiting

The Admin API includes sophisticated rate limiting to protect against abuse and ensure fair resource usage. Rate limiting is implemented using the Token Bucket algorithm with configurable limits for different scenarios.

#### 🚀 Key Features

- **Token Bucket Algorithm**: Industry-standard rate limiting with burst capacity
- **Multi-level Rate Limiting**: Global, per-IP, and endpoint-specific limits
- **Automatic Cleanup**: Background cleanup of expired rate limiters to prevent memory leaks
- **Comprehensive Monitoring**: Real-time metrics and statistics tracking
- **Production Ready**: Thread-safe implementation with minimal performance overhead

#### 📊 Rate Limiting Categories

The Admin API applies different rate limits based on endpoint sensitivity and resource requirements:

##### High-Frequency Endpoints (100 requests/minute)
- `GET /health` - Service health checks
- `GET /api/v1/cluster` - Cluster status monitoring

##### Medium-Frequency Endpoints (30 requests/minute)  
- `GET /api/v1/topics` - Topic listing
- `GET /api/v1/brokers` - Broker listing
- `GET /api/v1/topics/{name}` - Topic details

##### Low-Frequency Endpoints (10 requests/minute)
- `POST /api/v1/topics` - Topic creation
- `DELETE /api/v1/topics/{name}` - Topic deletion

#### ⚙️ Configuration

Rate limiting can be configured through TOML configuration or environment variables:

##### TOML Configuration

```toml
[admin.rate_limiting]
enabled = true                      # Enable/disable rate limiting (default: true)
global_burst_size = 1000           # Global burst capacity (default: 1000)
global_refill_rate = 60            # Global refill rate per minute (default: 60)
per_ip_burst_size = 100            # Per-IP burst capacity (default: 100)
per_ip_refill_rate = 30            # Per-IP refill rate per minute (default: 30)
cleanup_interval_seconds = 3600    # Cleanup interval in seconds (default: 3600)

# Endpoint-specific configuration
[admin.rate_limiting.endpoints]
"/health" = { burst_size = 50, refill_rate = 100 }
"/api/v1/cluster" = { burst_size = 50, refill_rate = 100 }
"/api/v1/topics" = { burst_size = 20, refill_rate = 30 }
"/api/v1/brokers" = { burst_size = 20, refill_rate = 30 }
"POST:/api/v1/topics" = { burst_size = 5, refill_rate = 10 }
"DELETE:/api/v1/topics" = { burst_size = 5, refill_rate = 10 }
```

##### Environment Variables

```bash
# Global rate limiting settings
RUSTMQ_ADMIN_RATE_LIMITING_ENABLED=true
RUSTMQ_ADMIN_GLOBAL_BURST_SIZE=1000
RUSTMQ_ADMIN_GLOBAL_REFILL_RATE=60
RUSTMQ_ADMIN_PER_IP_BURST_SIZE=100
RUSTMQ_ADMIN_PER_IP_REFILL_RATE=30
RUSTMQ_ADMIN_CLEANUP_INTERVAL_SECONDS=3600
```

#### 🔍 Rate Limit Headers

All API responses include rate limiting information in the headers:

```bash
# Example response headers
HTTP/1.1 200 OK
X-RateLimit-Limit: 30              # Requests per minute allowed
X-RateLimit-Remaining: 25          # Remaining requests in current window
X-RateLimit-Reset: 1640995260      # Unix timestamp when limit resets
X-RateLimit-Type: endpoint         # Type of rate limit applied (global/ip/endpoint)
```

#### 🚫 Rate Limit Exceeded Response

When rate limits are exceeded, the API returns a 429 status code:

```bash
# Request
curl -H "X-Forwarded-For: 192.168.1.100" http://localhost:8080/api/v1/topics

# Response when rate limited
HTTP/1.1 429 Too Many Requests
X-RateLimit-Limit: 30
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1640995320
X-RateLimit-Type: ip
Retry-After: 60

{
  "success": false,
  "data": null,
  "error": "Rate limit exceeded for IP 192.168.1.100. Limit: 30 requests per minute",
  "leader_hint": null
}
```

#### 🎯 Rate Limiting Strategy

The Admin API employs a hierarchical rate limiting strategy:

1. **Global Rate Limit**: Applied to all requests to prevent system overload
2. **Per-IP Rate Limit**: Applied per client IP address to prevent individual abuse
3. **Endpoint-Specific Rate Limit**: Applied per endpoint based on resource intensity

Rate limits are checked in order, and the most restrictive limit applies. For example:
- Global limit: 60 requests/minute 
- Per-IP limit: 30 requests/minute
- Endpoint limit: 10 requests/minute
- **Result**: Client is limited to 10 requests/minute for that endpoint

#### 🔧 Operational Benefits

##### Security
- **DDoS Protection**: Prevents overwhelming the API with excessive requests
- **Resource Protection**: Ensures critical operations aren't starved by high-frequency requests
- **Fair Usage**: Prevents individual clients from monopolizing resources

##### Performance
- **Memory Efficient**: Automatic cleanup prevents unbounded memory growth
- **Low Latency**: Token bucket algorithm adds minimal overhead (<1μs per request)
- **Thread Safe**: Concurrent request handling without performance degradation

#### 📈 Monitoring Rate Limiting

Rate limiting statistics are available through the health endpoint:

```bash
# Check rate limiting statistics
curl http://localhost:8080/health

# Response includes rate limiting metrics
{
  "status": "ok",
  "version": "0.1.0", 
  "uptime_seconds": 3600,
  "is_leader": true,
  "raft_term": 1,
  "rate_limiting": {
    "enabled": true,
    "active_limiters": 15,
    "total_requests": 1250,
    "blocked_requests": 25,
    "last_cleanup": "2024-01-15T10:30:00Z"
  }
}
```

#### 🛠️ Development and Testing

For development environments, rate limiting can be disabled or configured with higher limits:

```toml
# Development configuration
[admin.rate_limiting]
enabled = false                     # Disable for local development

# Or use high limits for testing
enabled = true
global_refill_rate = 10000         # Very high global limit
per_ip_refill_rate = 1000          # High per-IP limit
```

#### 🚀 Production Recommendations

For production deployments:

1. **Monitor Rate Limiting Metrics**: Track blocked requests and adjust limits as needed
2. **Configure Endpoint-Specific Limits**: Set appropriate limits based on operational patterns
3. **Use Load Balancers**: Distribute traffic across multiple Admin API instances
4. **Alert on High Block Rates**: Set up alerts if > 5% of requests are being blocked
5. **Regular Review**: Periodically review and adjust rate limits based on usage patterns

### 📈 Production Deployment

#### Production Deployment
For production deployment with Kubernetes, see the comprehensive guide in [docker/README.md](docker/README.md) which includes:

- Complete Kubernetes manifests
- Service configuration
- Health check setup
- Production resource limits
- Security configurations

### 🧪 Testing

The Admin API includes comprehensive test coverage:

- **11 Unit Tests**: All API endpoints and health tracking functionality
- **Integration Testing**: End-to-end API workflows with mock backends
- **Error Scenario Testing**: Comprehensive error condition validation
- **Performance Testing**: Health tracking timeout and expiration behavior

#### Running Tests
```bash
# Run admin API tests
cargo test admin::api

# Run specific health tracking tests
cargo test test_broker_health_tracking test_cluster_health_calculation

# All admin tests pass
# test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured
```

### 🔧 Configuration

#### Environment Variables
```bash
# Admin API configuration
ADMIN_API_PORT=8080
HEALTH_CHECK_INTERVAL=15    # seconds
HEALTH_TIMEOUT=30          # seconds
```

#### TOML Configuration
```toml
[admin]
port = 8080
health_check_interval_ms = 15000
health_timeout_ms = 30000
enable_cors = true
log_requests = true
```

### 🔍 Monitoring Integration

The Admin API provides monitoring endpoints for observability:

#### Metrics Endpoint (Future)
```bash
# Prometheus metrics (planned)
curl http://localhost:8080/metrics
```

#### Log Analysis
```bash
# View API request logs
docker-compose logs rustmq-admin

# Filter for health check logs
docker-compose logs rustmq-admin | grep "Health check"
```
