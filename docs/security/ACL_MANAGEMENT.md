# RustMQ ACL Management Guide

This comprehensive guide covers Access Control List (ACL) management in RustMQ, including policy creation, testing, optimization, and troubleshooting for production deployments.

## Table of Contents

- [Overview](#overview)
- [ACL Concepts](#acl-concepts)
- [ACL Rule Creation](#acl-rule-creation)
- [Resource Patterns](#resource-patterns)
- [Conditional Access](#conditional-access)
- [Policy Testing](#policy-testing)
- [Performance Optimization](#performance-optimization)
- [Bulk Operations](#bulk-operations)
- [Monitoring and Auditing](#monitoring-and-auditing)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)

## Overview

RustMQ's Access Control List (ACL) system provides fine-grained authorization control with enterprise-grade performance. The system features:

- **Sub-100ns Authorization**: Multi-level caching for ultra-low latency
- **Flexible Pattern Matching**: Sophisticated resource pattern support
- **Conditional Access**: IP-based, time-based, and custom conditions
- **Distributed Storage**: Raft consensus for consistent ACL replication
- **Comprehensive Testing**: Policy testing and validation tools
- **Performance Optimization**: String interning and intelligent caching

### ACL Architecture

```
Authorization Flow:
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Principal     │───▶│  Resource       │───▶│   Operation     │
│ (who wants)     │    │ (what resource) │    │ (what action)   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 ▼
                ┌─────────────────────────────────────┐
                │         ACL Rule Evaluation         │
                │                                     │
                │  1. Pattern Matching               │
                │  2. Condition Evaluation           │
                │  3. Effect Determination           │
                │  4. Cache Result                   │
                └─────────────────────────────────────┘
```

## ACL Concepts

### Core Components

#### 1. Principal
The entity requesting access (user, service account, or system):
```
Examples:
- user@company.com
- analytics-service@company.com
- broker-01.internal.company.com
- admin@company.com
```

#### 2. Resource
The target of the access request:
```
Examples:
- topic.user-events
- topic.logs.application.*
- consumer-group.analytics
- admin.cluster.config
```

#### 3. Operation
The action being performed:
```
Common Operations:
- read          # Read messages from topics
- write         # Write messages to topics  
- create        # Create topics/consumer groups
- delete        # Delete topics/consumer groups
- describe      # Get topic/consumer group details
- alter         # Modify topic/consumer group configuration
- all           # All operations (use with caution)
```

#### 4. Effect
The result of rule evaluation:
```
Effects:
- allow         # Grant access
- deny          # Explicitly deny access (takes precedence)
```

#### 5. Conditions
Additional constraints for rule application:
```
Condition Types:
- source_ip     # IP address/CIDR restrictions
- time_range    # Time-based access control
- client_type   # Client application restrictions
- custom        # Custom condition attributes
```

### ACL Rule Structure

Each ACL rule has the following structure:

```rust
pub struct AclRule {
    // Rule identification
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    
    // Principal matching (who)
    pub principal: ResourcePattern,
    pub principal_type: PrincipalType,
    
    // Resource matching (what)
    pub resource: ResourcePattern,
    pub resource_type: ResourceType,
    
    // Operation specification (action)
    pub operations: Vec<Operation>,
    pub effect: Effect,
    
    // Conditional access
    pub conditions: Vec<Condition>,
    
    // Rule metadata
    pub priority: u32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}
```

## ACL Rule Creation

### Basic ACL Rules

#### Allow Topic Access
```bash
# Allow user to read from user-events topic
rustmq-admin acl create \
  --principal "analytics@company.com" \
  --resource "topic.user-events" \
  --operations "read" \
  --effect "allow" \
  --description "Analytics service access to user events"

# Allow user to write to specific topic pattern
rustmq-admin acl create \
  --principal "logger@company.com" \
  --resource "topic.logs.*" \
  --operations "write" \
  --effect "allow" \
  --description "Logger service can write to any log topic"
```

#### Consumer Group Access
```bash
# Allow consumer group access
rustmq-admin acl create \
  --principal "analytics@company.com" \
  --resource "consumer-group.analytics-*" \
  --operations "read,create,delete" \
  --effect "allow" \
  --description "Analytics service consumer group access"
```

#### Administrative Access
```bash
# Allow admin operations
rustmq-admin acl create \
  --principal "admin@company.com" \
  --resource "admin.*" \
  --operations "all" \
  --effect "allow" \
  --description "Full administrative access"
```

### Advanced ACL Rules

#### Multi-Resource Rules
```bash
# Allow access to multiple resource types
rustmq-admin acl create \
  --principal "microservice@company.com" \
  --resources "topic.orders.*,consumer-group.order-processor" \
  --operations "read,write,create" \
  --effect "allow" \
  --priority 100 \
  --description "Order processing service access"
```

#### Deny Rules (Security Hardening)
```bash
# Explicitly deny access to sensitive topics
rustmq-admin acl create \
  --principal "*@external.com" \
  --resource "topic.internal.*" \
  --operations "all" \
  --effect "deny" \
  --priority 1000 \
  --description "Block external access to internal topics"

# Deny delete operations for production topics
rustmq-admin acl create \
  --principal "*" \
  --resource "topic.prod.*" \
  --operations "delete" \
  --effect "deny" \
  --priority 500 \
  --description "Protect production topics from deletion"
```

#### Time-Limited Access
```bash
# Temporary access for maintenance
rustmq-admin acl create \
  --principal "maintenance@company.com" \
  --resource "admin.cluster.*" \
  --operations "read,alter" \
  --effect "allow" \
  --expires-at "2024-12-31T23:59:59Z" \
  --description "Temporary maintenance access"
```

## Resource Patterns

RustMQ supports sophisticated pattern matching for flexible resource authorization:

### Pattern Types

#### 1. Literal Patterns
Exact string matching:
```bash
# Exact topic match
rustmq-admin acl create \
  --principal "service@company.com" \
  --resource "topic.user-events" \
  --operations "read" \
  --effect "allow"
```

#### 2. Wildcard Patterns
Single-level wildcards using `*`:
```bash
# Single-level wildcard
rustmq-admin acl create \
  --principal "logger@company.com" \
  --resource "topic.logs.*" \
  --operations "write" \
  --effect "allow"

# This matches:
# - topic.logs.application
# - topic.logs.database
# - topic.logs.security
# But NOT:
# - topic.logs.application.debug (two levels)
```

#### 3. Multi-Level Wildcards
Multi-level wildcards using `**`:
```bash
# Multi-level wildcard
rustmq-admin acl create \
  --principal "monitoring@company.com" \
  --resource "topic.metrics.**" \
  --operations "read" \
  --effect "allow"

# This matches:
# - topic.metrics.cpu
# - topic.metrics.application.response-time
# - topic.metrics.database.connections.pool-1
```

#### 4. Character Wildcards
Single character wildcards using `?`:
```bash
# Character wildcard for numbered resources
rustmq-admin acl create \
  --principal "worker@company.com" \
  --resource "topic.queue-?" \
  --operations "read" \
  --effect "allow"

# This matches:
# - topic.queue-1
# - topic.queue-a
# - topic.queue-x
# But NOT:
# - topic.queue-10 (two characters)
```

#### 5. Range Patterns
Character ranges using `[a-z]` or `[0-9]`:
```bash
# Numeric range pattern
rustmq-admin acl create \
  --principal "partition-reader@company.com" \
  --resource "topic.data-partition-[0-9]" \
  --operations "read" \
  --effect "allow"

# Alphabetic range pattern  
rustmq-admin acl create \
  --principal "region-service@company.com" \
  --resource "topic.region-[a-z].*" \
  --operations "read,write" \
  --effect "allow"
```

### Pattern Precedence

When multiple patterns match, RustMQ uses the following precedence rules:

1. **Effect Precedence**: Deny rules always take precedence over allow rules
2. **Specificity Precedence**: More specific patterns take precedence over general patterns
3. **Priority Precedence**: Higher priority values take precedence
4. **Creation Time**: Newer rules take precedence for equal priority

```bash
# Example of pattern precedence
# Rule 1: General allow rule
rustmq-admin acl create \
  --principal "user@company.com" \
  --resource "topic.*" \
  --operations "read" \
  --effect "allow" \
  --priority 100

# Rule 2: Specific deny rule (takes precedence)
rustmq-admin acl create \
  --principal "user@company.com" \
  --resource "topic.sensitive-data" \
  --operations "read" \
  --effect "deny" \
  --priority 200

# Result: user@company.com can read from all topics EXCEPT topic.sensitive-data
```

## Conditional Access

RustMQ supports sophisticated conditional access control for fine-grained security:

### IP-Based Conditions

#### Single IP Address
```bash
rustmq-admin acl create \
  --principal "admin@company.com" \
  --resource "admin.*" \
  --operations "all" \
  --effect "allow" \
  --conditions "source_ip=192.168.1.100" \
  --description "Admin access only from specific workstation"
```

#### CIDR Ranges
```bash
rustmq-admin acl create \
  --principal "internal-service@company.com" \
  --resource "topic.internal.*" \
  --operations "read,write" \
  --effect "allow" \
  --conditions "source_ip=10.0.0.0/8,172.16.0.0/12,192.168.0.0/16" \
  --description "Internal services from private networks only"
```

#### IP Exclusions
```bash
rustmq-admin acl create \
  --principal "public-api@company.com" \
  --resource "topic.public.*" \
  --operations "read" \
  --effect "allow" \
  --conditions "source_ip=!192.168.1.100" \
  --description "Public API access except from blocked IP"
```

### Time-Based Conditions

#### Business Hours Access
```bash
rustmq-admin acl create \
  --principal "business-user@company.com" \
  --resource "topic.reports.*" \
  --operations "read" \
  --effect "allow" \
  --conditions "time_range=09:00-17:00,timezone=America/New_York,days=Mon-Fri" \
  --description "Business reports access during business hours only"
```

#### Maintenance Windows
```bash
rustmq-admin acl create \
  --principal "maintenance@company.com" \
  --resource "admin.cluster.*" \
  --operations "alter" \
  --effect "allow" \
  --conditions "time_range=02:00-04:00,timezone=UTC,days=Sat-Sun" \
  --description "Maintenance access during designated windows"
```

#### Date Ranges
```bash
rustmq-admin acl create \
  --principal "temp-contractor@company.com" \
  --resource "topic.project-alpha.*" \
  --operations "read" \
  --effect "allow" \
  --conditions "date_range=2024-01-01:2024-06-30" \
  --description "Temporary contractor access for project duration"
```

### Custom Conditions

#### Client Version Requirements
```bash
rustmq-admin acl create \
  --principal "mobile-app@company.com" \
  --resource "topic.mobile.*" \
  --operations "read,write" \
  --effect "allow" \
  --conditions "client_version>=1.5.0" \
  --description "Require minimum client version for mobile access"
```

#### Request Rate Limiting
```bash
rustmq-admin acl create \
  --principal "batch-processor@company.com" \
  --resource "topic.high-volume.*" \
  --operations "write" \
  --effect "allow" \
  --conditions "max_requests_per_minute=1000" \
  --description "Rate-limited access for batch processing"
```

#### Custom Attributes
```bash
rustmq-admin acl create \
  --principal "service@company.com" \
  --resource "topic.environment.*" \
  --operations "read,write" \
  --effect "allow" \
  --conditions "environment=production,team=platform" \
  --description "Production access for platform team services"
```

### Complex Conditional Logic

#### AND Conditions (Default)
```bash
# Multiple conditions must ALL be true
rustmq-admin acl create \
  --principal "secure-service@company.com" \
  --resource "topic.confidential.*" \
  --operations "read" \
  --effect "allow" \
  --conditions "source_ip=10.0.0.0/8,time_range=09:00-17:00,client_version>=2.0.0" \
  --description "Secure access requiring IP, time, and version checks"
```

#### OR Conditions
```bash
# Any condition can be true
rustmq-admin acl create \
  --principal "multi-region-service@company.com" \
  --resource "topic.global.*" \
  --operations "read,write" \
  --effect "allow" \
  --conditions-logic "OR" \
  --conditions "source_ip=10.1.0.0/16,source_ip=10.2.0.0/16,source_ip=10.3.0.0/16" \
  --description "Access from any regional data center"
```

## Policy Testing

RustMQ provides comprehensive policy testing tools to validate ACL rules before deployment:

### Single Permission Testing

```bash
# Test specific permission
rustmq-admin acl test \
  --principal "analytics@company.com" \
  --resource "topic.user-events" \
  --operation "read" \
  --source-ip "192.168.1.100" \
  --timestamp "2024-01-15T10:30:00Z"

# Expected output:
# Result: ALLOWED
# Matched Rule: analytics_user_events_read (ID: acl_12345)
# Evaluation Time: 45ns
# Cache Level: L1
```

### Batch Permission Testing

```bash
# Test multiple permissions from file
rustmq-admin acl bulk-test \
  --input-file test-cases.json \
  --output-file test-results.json \
  --format detailed

# Example test-cases.json:
{
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
}
```

### Policy Simulation

```bash
# Simulate policy changes before applying
rustmq-admin acl simulate \
  --policy-file new-policy.json \
  --test-cases existing-access-patterns.json \
  --report-file simulation-report.html

# Check impact of rule changes
rustmq-admin acl impact-analysis \
  --rule-id acl_12345 \
  --proposed-changes "operations=read,write,create" \
  --affected-principals-limit 100
```

### Permission Discovery

```bash
# Discover current permissions for a principal
rustmq-admin acl permissions \
  --principal "service@company.com" \
  --format table

# Example output:
RESOURCE                 OPERATIONS    EFFECT    CONDITIONS
topic.logs.*            write         allow     source_ip=10.0.0.0/8
topic.metrics.*         read          allow     time_range=00:00-23:59
consumer-group.service  read,create   allow     none
admin.health           read          allow     none

# Discover principals with access to a resource  
rustmq-admin acl principals \
  --resource "topic.sensitive-data" \
  --operation "read" \
  --format json
```

### Compliance Testing

```bash
# Test compliance with security policies
rustmq-admin acl compliance-check \
  --policy-framework "SOX" \
  --report-format "json" \
  --output-file "sox-compliance-report.json"

# Check for overprivileged access
rustmq-admin acl security-audit \
  --check-excessive-permissions \
  --check-stale-rules \
  --check-wildcard-abuse \
  --output-file "security-audit.json"
```

## Performance Optimization

### Cache Configuration

#### L1 Cache (Connection-Local)
```toml
[security.acl.l1_cache]
enabled = true
size_per_connection = 100        # Entries per connection
ttl_seconds = 60                 # Short TTL for connection-local cache
```

#### L2 Cache (Broker-Wide)
```toml
[security.acl.l2_cache]
enabled = true
total_size = 10000              # Total entries across all shards
shard_count = 32                # Number of shards for reduced contention
ttl_seconds = 300               # Standard TTL
eviction_policy = "lru"         # LRU eviction policy
```

#### L3 Bloom Filter
```toml
[security.acl.l3_bloom]
enabled = true
size = 1_000_000               # Number of expected elements
false_positive_rate = 0.01     # 1% false positive rate
hash_functions = 3             # Optimal hash functions for given FPR
```

### String Interning

Enable string interning for memory efficiency:

```bash
# Configure string interning
rustmq-admin config set security.acl.string_interning.enabled true
rustmq-admin config set security.acl.string_interning.initial_capacity 10000
rustmq-admin config set security.acl.string_interning.max_memory_mb 100

# Monitor string interning effectiveness
rustmq-admin acl string-interning-stats
```

### Batch Operations

#### Batch ACL Updates
```bash
# Batch rule creation from JSON
rustmq-admin acl batch-create \
  --input-file acl-rules.json \
  --batch-size 100 \
  --validate-before-create

# Batch rule updates
rustmq-admin acl batch-update \
  --rule-ids "acl_1,acl_2,acl_3" \
  --field "enabled=true" \
  --dry-run
```

#### Batch Testing
```bash
# Batch permission testing
rustmq-admin acl batch-test \
  --input-file test-cases.json \
  --parallel-workers 10 \
  --timeout-per-test 1s
```

### Performance Monitoring

```bash
# Monitor ACL performance metrics
rustmq-admin acl metrics \
  --duration 1h \
  --include-cache-stats \
  --include-latency-percentiles

# Cache hit rate analysis
rustmq-admin acl cache-analysis \
  --show-hit-rates \
  --show-eviction-patterns \
  --recommend-optimizations
```

## Bulk Operations

### Bulk Rule Management

#### Import/Export Rules
```bash
# Export all ACL rules
rustmq-admin acl export \
  --output-file acl-backup.json \
  --include-metadata \
  --format json

# Import ACL rules
rustmq-admin acl import \
  --input-file acl-rules.json \
  --merge-strategy "replace" \
  --validate-before-import \
  --dry-run

# Export rules for specific principals
rustmq-admin acl export \
  --principals "service1@company.com,service2@company.com" \
  --output-file service-acls.json
```

#### Bulk Rule Operations
```bash
# Bulk enable/disable rules
rustmq-admin acl bulk-update \
  --filter "principal=*@external.com" \
  --set "enabled=false" \
  --reason "Security review"

# Bulk rule deletion
rustmq-admin acl bulk-delete \
  --filter "created_before=2023-01-01" \
  --confirm-with-phrase "DELETE_OLD_RULES"

# Bulk priority adjustment
rustmq-admin acl bulk-update \
  --filter "effect=deny" \
  --set "priority=1000" \
  --reason "Ensure deny rules take precedence"
```

### Migration Operations

#### Environment Migration
```bash
# Export from development environment
rustmq-admin acl export \
  --environment dev \
  --output-file dev-acls.json

# Transform for production (remove test users, adjust patterns)
rustmq-admin acl transform \
  --input-file dev-acls.json \
  --transformation-rules prod-transform.yaml \
  --output-file prod-acls.json

# Import to production
rustmq-admin acl import \
  --input-file prod-acls.json \
  --environment prod \
  --validate-principals \
  --dry-run
```

#### Version Migration
```bash
# Migrate from legacy ACL format
rustmq-admin acl migrate \
  --from-format "legacy-v1" \
  --input-file legacy-acls.xml \
  --to-format "rustmq-v1" \
  --output-file migrated-acls.json \
  --validation-report migration-report.json
```

## Monitoring and Auditing

### Real-Time Monitoring

#### ACL Decision Monitoring
```bash
# Monitor ACL decisions in real-time
rustmq-admin acl monitor \
  --show-denials \
  --show-cache-misses \
  --filter "principal=suspicious@domain.com"

# Monitor specific resources
rustmq-admin acl monitor \
  --resource "topic.sensitive.*" \
  --alert-on-denied-access \
  --webhook-url "https://security.company.com/alerts"
```

#### Performance Monitoring
```bash
# Monitor ACL performance metrics
rustmq-admin acl performance \
  --duration 1h \
  --percentiles "50,95,99,99.9" \
  --break-down-by-cache-level

# Monitor cache effectiveness
rustmq-admin acl cache-monitor \
  --show-hit-rates \
  --show-memory-usage \
  --alert-on-low-hit-rate 0.8
```

### Audit Logging

#### Enable Audit Logging
```toml
[security.audit]
enabled = true
log_authorization_events = true  # Enable for detailed auditing
log_file_path = "/var/log/rustmq/acl-audit.log"
```

#### Audit Log Analysis
```bash
# Analyze audit logs
rustmq-admin audit analyze \
  --log-file /var/log/rustmq/acl-audit.log \
  --time-range "2024-01-01T00:00:00Z:2024-01-31T23:59:59Z" \
  --format summary

# Generate compliance reports
rustmq-admin audit compliance-report \
  --framework "SOX,PCI-DSS" \
  --period "monthly" \
  --output-format "pdf" \
  --output-file "compliance-report-jan-2024.pdf"
```

### Security Analytics

#### Access Pattern Analysis
```bash
# Analyze access patterns for anomalies
rustmq-admin acl analyze-patterns \
  --principal "user@company.com" \
  --lookback-days 30 \
  --detect-anomalies \
  --baseline-file user-baseline.json

# Generate security dashboard data
rustmq-admin acl security-dashboard \
  --metrics "denied_requests,unusual_access_patterns,rule_usage" \
  --format prometheus \
  --output-file acl-metrics.prom
```

## Best Practices

### Rule Design Principles

#### 1. Principle of Least Privilege
```bash
# Good: Specific permissions
rustmq-admin acl create \
  --principal "analytics@company.com" \
  --resource "topic.user-events" \
  --operations "read" \
  --effect "allow"

# Avoid: Overly broad permissions
# Don't use: --operations "all" unless absolutely necessary
```

#### 2. Use Deny Rules for Security Hardening
```bash
# Explicitly deny sensitive operations
rustmq-admin acl create \
  --principal "*" \
  --resource "topic.financial.*" \
  --operations "delete" \
  --effect "deny" \
  --priority 1000 \
  --description "Protect financial topics from deletion"
```

#### 3. Implement Defense in Depth
```bash
# Layer 1: IP restrictions
rustmq-admin acl create \
  --principal "external-partner@partner.com" \
  --resource "topic.partner-data" \
  --operations "read" \
  --effect "allow" \
  --conditions "source_ip=203.0.113.0/24"

# Layer 2: Time restrictions
rustmq-admin acl create \
  --principal "external-partner@partner.com" \
  --resource "topic.partner-data" \
  --operations "read" \
  --effect "allow" \
  --conditions "time_range=09:00-17:00,timezone=UTC,days=Mon-Fri"
```

### Operational Best Practices

#### 1. Regular Access Reviews
```bash
# Monthly access review
rustmq-admin acl access-review \
  --generate-report \
  --include-unused-rules \
  --include-overprivileged-users \
  --output-file "monthly-access-review.json"

# Quarterly privilege cleanup
rustmq-admin acl cleanup \
  --remove-unused-rules \
  --days-without-use 90 \
  --dry-run
```

#### 2. Rule Documentation
```bash
# Always include descriptions
rustmq-admin acl create \
  --principal "service@company.com" \
  --resource "topic.events.*" \
  --operations "read,write" \
  --effect "allow" \
  --description "Event processing service - Ticket #INFRA-1234" \
  --tags "service,production,events"
```

#### 3. Testing Before Deployment
```bash
# Test new rules before applying
rustmq-admin acl test-rule \
  --rule-definition new-rule.json \
  --test-cases test-scenarios.json \
  --report-file test-results.json

# Validate rule syntax
rustmq-admin acl validate \
  --rule-file new-rules.json \
  --check-syntax \
  --check-conflicts \
  --check-security-best-practices
```

### Performance Best Practices

#### 1. Optimize Rule Ordering
```bash
# High-priority deny rules first
rustmq-admin acl create \
  --principal "*@blocked-domain.com" \
  --resource "*" \
  --operations "all" \
  --effect "deny" \
  --priority 2000

# Common access patterns with higher priority
rustmq-admin acl create \
  --principal "common-service@company.com" \
  --resource "topic.common.*" \
  --operations "read,write" \
  --effect "allow" \
  --priority 1500
```

#### 2. Use Efficient Patterns
```bash
# Efficient: Specific patterns
rustmq-admin acl create \
  --principal "logs@company.com" \
  --resource "topic.logs.*" \
  --operations "write" \
  --effect "allow"

# Less efficient: Overly broad patterns with many conditions
# Avoid complex regex-like patterns when simple wildcards suffice
```

#### 3. Monitor and Tune Cache Performance
```bash
# Regular cache performance checks
rustmq-admin acl cache-stats \
  --show-hit-rates \
  --show-eviction-rates \
  --recommend-tuning

# Adjust cache sizes based on usage patterns
rustmq-admin config set security.acl.l2_cache.total_size 20000  # Increase if hit rate is low
```

## Troubleshooting

### Common ACL Issues

#### 1. Permission Denied Errors
```bash
# Problem: Unexpected permission denied
# Error: Access denied for principal 'user@company.com' to resource 'topic.events'

# Debug: Check rule evaluation
rustmq-admin acl debug \
  --principal "user@company.com" \
  --resource "topic.events" \
  --operation "read" \
  --verbose

# Check for conflicting rules
rustmq-admin acl conflicts \
  --principal "user@company.com" \
  --resource "topic.events"
```

#### 2. Performance Issues
```bash
# Problem: High ACL lookup latency
# Symptoms: Authorization taking > 1ms consistently

# Check cache performance
rustmq-admin acl performance-report \
  --duration 1h \
  --include-slow-queries \
  --threshold-ms 1

# Optimize cache configuration
rustmq-admin acl tune-cache \
  --target-hit-rate 0.95 \
  --max-memory-mb 200
```

#### 3. Rule Conflicts
```bash
# Problem: Rules producing unexpected results
# Debug rule evaluation order
rustmq-admin acl explain \
  --principal "user@company.com" \
  --resource "topic.test" \
  --operation "read" \
  --show-rule-chain \
  --show-evaluation-steps
```

#### 4. Cache Issues
```bash
# Problem: Cache not updating after rule changes
# Force cache refresh
rustmq-admin acl cache-refresh \
  --level "all" \
  --wait-for-completion

# Check cache synchronization
rustmq-admin acl cache-sync-status \
  --show-lag \
  --show-inconsistencies
```

### Diagnostic Commands

```bash
# Comprehensive ACL health check
rustmq-admin acl health-check \
  --check-rule-conflicts \
  --check-performance \
  --check-cache-health \
  --output-file acl-health.json

# ACL rule analysis
rustmq-admin acl analyze \
  --check-unused-rules \
  --check-overprivileged-access \
  --check-pattern-efficiency \
  --report-file acl-analysis.html

# Performance benchmarking
rustmq-admin acl benchmark \
  --duration 60s \
  --concurrency 100 \
  --test-scenarios realistic-workload.json
```

### Emergency Procedures

#### 1. Emergency Access Disable
```bash
# Disable all access for a compromised principal
rustmq-admin acl emergency-disable \
  --principal "compromised@company.com" \
  --reason "Security incident #INC-2024-001" \
  --audit-log-entry "Emergency disable due to suspected compromise"
```

#### 2. Emergency Rule Rollback
```bash
# Rollback recent ACL changes
rustmq-admin acl rollback \
  --to-timestamp "2024-01-15T09:00:00Z" \
  --affected-principals "user1@company.com,user2@company.com" \
  --dry-run

# Rollback specific rule changes
rustmq-admin acl rollback-rules \
  --rule-ids "acl_123,acl_124,acl_125" \
  --confirm-with-phrase "EMERGENCY_ROLLBACK"
```

This ACL management guide provides comprehensive coverage of RustMQ's authorization system. For production deployments, follow the best practices and implement proper monitoring and testing procedures to ensure security and performance.