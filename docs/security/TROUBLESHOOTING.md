# RustMQ Security Troubleshooting Guide

This comprehensive guide provides systematic approaches to diagnosing and resolving security-related issues in RustMQ deployments. It covers common problems, diagnostic procedures, and resolution strategies for certificate management, access control, network security, and performance issues.

## Table of Contents

- [Overview](#overview)
- [General Diagnostic Approach](#general-diagnostic-approach)
- [Certificate and TLS Issues](#certificate-and-tls-issues)
- [Authentication Problems](#authentication-problems)
- [Authorization and ACL Issues](#authorization-and-acl-issues)
- [Network Security Problems](#network-security-problems)
- [Performance and Security Trade-offs](#performance-and-security-trade-offs)
- [Monitoring and Alerting Issues](#monitoring-and-alerting-issues)
- [Configuration Problems](#configuration-problems)
- [Emergency Procedures](#emergency-procedures)
- [Common Error Codes](#common-error-codes)

## Overview

Security troubleshooting in RustMQ requires a systematic approach to isolate and resolve issues across multiple security layers. This guide provides structured diagnostic procedures and proven solutions for common security problems.

### Troubleshooting Principles

1. **Start with Security Status**: Always check overall security health first
2. **Check Logs**: Security audit logs provide detailed error information
3. **Isolate Components**: Test individual security components separately
4. **Verify Configuration**: Ensure configuration matches security requirements
5. **Test Incrementally**: Add complexity gradually when testing fixes

### Essential Diagnostic Tools

```bash
# Core security diagnostic commands
rustmq-admin security status          # Overall security health
rustmq-admin security health          # Component-specific health checks
rustmq-admin audit logs               # Security event logs
rustmq-admin certs health-check       # Certificate validation
rustmq-admin acl test                 # Permission testing
```

## General Diagnostic Approach

### Security Health Assessment

#### Step 1: Overall Security Status
```bash
# Check comprehensive security status
rustmq-admin security status --detailed

# Example healthy output:
# Security Status: HEALTHY
# TLS Enabled: true
# mTLS Enabled: true  
# ACL Enabled: true
# Certificate Authority: HEALTHY (156 active certificates)
# ACL System: HEALTHY (42 active rules, 87% cache hit rate)
# Audit System: HEALTHY (logging operational)

# If any component shows as UNHEALTHY, focus on that area
```

#### Step 2: Component Health Checks
```bash
# Check individual security components
rustmq-admin security health --component certificate-manager
rustmq-admin security health --component authentication-manager
rustmq-admin security health --component authorization-manager
rustmq-admin security health --component audit-system

# Look for specific error messages and recommendations
```

#### Step 3: Recent Security Events
```bash
# Check recent security events for errors
rustmq-admin audit logs --limit 50 --include-errors

# Filter for specific issues
rustmq-admin audit logs --event-type "authentication_failure" --limit 20
rustmq-admin audit logs --event-type "certificate_validation_failed" --limit 20
rustmq-admin audit logs --event-type "authorization_denied" --limit 20
```

### Log Analysis Workflow

#### Security Log Locations
```bash
# Main security logs
/var/log/rustmq/security-audit.log     # Comprehensive security events
/var/log/rustmq/authentication.log     # Authentication events
/var/log/rustmq/authorization.log      # Authorization decisions
/var/log/rustmq/certificate.log        # Certificate operations
/var/log/rustmq/tls.log                # TLS handshake events

# System logs
/var/log/rustmq/broker.log             # Broker security events
/var/log/rustmq/controller.log         # Controller security events
```

#### Log Analysis Commands
```bash
# Search for authentication failures
grep "authentication_failure" /var/log/rustmq/security-audit.log | tail -20

# Look for certificate errors
grep -E "(certificate|cert)" /var/log/rustmq/security-audit.log | grep -i error

# Find authorization denials with context
grep -A 5 -B 5 "authorization_denied" /var/log/rustmq/security-audit.log

# Monitor real-time security events
tail -f /var/log/rustmq/security-audit.log | grep -E "(ERROR|WARN|CRITICAL)"
```

## Certificate and TLS Issues

### Certificate Validation Failures

#### Problem: Certificate Validation Errors
```bash
# Symptoms:
# - "Certificate validation failed" errors
# - TLS handshake failures
# - Authentication rejected with valid certificates

# Diagnostic steps:
# 1. Check certificate health
rustmq-admin certs health-check --cert-id <certificate_id>

# 2. Validate certificate manually
rustmq-admin certs validate --cert-id <certificate_id> --verbose

# 3. Check certificate chain
openssl verify -CAfile /etc/rustmq/certs/ca.pem /etc/rustmq/certs/server.pem

# 4. Verify certificate dates
openssl x509 -in /etc/rustmq/certs/server.pem -noout -dates
```

#### Common Certificate Issues and Solutions

##### Issue: Certificate Expired
```bash
# Problem: Certificate has expired
# Error: "Certificate expired" or "certificate verify failed: certificate has expired"

# Diagnosis:
openssl x509 -in /path/to/cert.pem -noout -dates
# Check if current date is past "notAfter" date

# Solution 1: Renew certificate
rustmq-admin certs renew <certificate_id> --validity-days 365

# Solution 2: Issue new certificate
rustmq-admin certs issue \
  --principal <principal> \
  --role <role> \
  --ca-id <ca_id> \
  --validity-days 365

# Solution 3: For emergency situations, temporarily extend validity
rustmq-admin certs emergency-extend <certificate_id> --days 7
```

##### Issue: Certificate Chain Problems
```bash
# Problem: Incomplete or invalid certificate chain
# Error: "certificate verify failed: unable to get local issuer certificate"

# Diagnosis:
# Check if intermediate certificates are present
openssl verify -CAfile /etc/rustmq/certs/ca.pem -untrusted /etc/rustmq/certs/intermediate.pem /etc/rustmq/certs/server.pem

# Verify chain order
openssl crl2pkcs7 -nocrl -certfile /etc/rustmq/certs/chain.pem | openssl pkcs7 -print_certs -text -noout

# Solution: Rebuild certificate chain
rustmq-admin certs rebuild-chain <certificate_id>

# Or manually create proper chain file
cat server.pem intermediate.pem root.pem > chain.pem
```

##### Issue: Certificate Revocation Problems
```bash
# Problem: Certificate shows as revoked when it shouldn't be
# Error: "Certificate has been revoked"

# Diagnosis:
# Check certificate revocation status
rustmq-admin certs revocation-status <certificate_id>

# Check CRL
openssl crl -in /etc/rustmq/ca/crl.pem -text -noout | grep -A 5 -B 5 <serial_number>

# Solution 1: If incorrectly revoked, restore certificate
rustmq-admin certs unrevoke <certificate_id> --reason "Incorrectly revoked"

# Solution 2: Update CRL
rustmq-admin ca generate-crl --force-update

# Solution 3: Check OCSP responder
curl -X POST -H "Content-Type: application/ocsp-request" --data-binary @ocsp-request.der http://ocsp.company.com
```

### TLS Configuration Issues

#### Problem: TLS Handshake Failures
```bash
# Symptoms:
# - "TLS handshake failed" errors
# - Connection timeouts during SSL negotiation
# - Cipher suite mismatch errors

# Diagnostic steps:
# 1. Test TLS connection manually
openssl s_client -connect localhost:9092 -cert /etc/rustmq/certs/client.pem -key /etc/rustmq/certs/client.key

# 2. Check TLS configuration
rustmq-admin config show security.tls

# 3. Verify supported cipher suites
nmap --script ssl-enum-ciphers -p 9092 localhost

# 4. Test specific TLS version
openssl s_client -connect localhost:9092 -tls1_3
```

#### Common TLS Issues and Solutions

##### Issue: TLS Version Mismatch
```bash
# Problem: Client and server TLS versions incompatible
# Error: "protocol version not supported" or "handshake failure"

# Diagnosis:
# Check configured TLS version
grep -A 5 "\[security.tls\]" /etc/rustmq/config/broker.toml

# Test client TLS capabilities
openssl s_client -connect server:9092 -tls1_2
openssl s_client -connect server:9092 -tls1_3

# Solution 1: Update TLS configuration to support required versions
rustmq-admin config set security.tls.min_protocol_version "1.2"
rustmq-admin config set security.tls.max_protocol_version "1.3"

# Solution 2: Update client to support modern TLS
# Ensure client library supports TLS 1.3
```

##### Issue: Cipher Suite Problems
```bash
# Problem: No common cipher suites between client and server
# Error: "no cipher suites in common" or "sslv3 alert handshake failure"

# Diagnosis:
# Check server cipher suites
nmap --script ssl-enum-ciphers -p 9092 localhost

# Test with specific cipher
openssl s_client -connect localhost:9092 -cipher 'ECDHE+AESGCM'

# Solution 1: Configure compatible cipher suites
rustmq-admin config set security.tls.cipher_suites '["TLS_AES_256_GCM_SHA384", "TLS_CHACHA20_POLY1305_SHA256"]'

# Solution 2: For legacy client support (use with caution)
rustmq-admin config set security.tls.allow_legacy_ciphers true

# Solution 3: Update client cipher configuration
# Ensure client uses modern, secure cipher suites
```

##### Issue: Certificate Hostname Verification
```bash
# Problem: Hostname doesn't match certificate Subject Alternative Names
# Error: "certificate verify failed: Hostname mismatch"

# Diagnosis:
# Check certificate SANs
openssl x509 -in /etc/rustmq/certs/server.pem -noout -text | grep -A 5 "Subject Alternative Name"

# Verify hostname resolution
nslookup broker.rustmq.company.com
dig +short broker.rustmq.company.com

# Solution 1: Add missing hostnames to certificate
rustmq-admin certs reissue <certificate_id> \
  --san "broker.internal" \
  --san "broker.k8s.local" \
  --san "10.0.1.100"

# Solution 2: Update DNS to match certificate
# Ensure DNS names match certificate SANs

# Solution 3: For testing only - disable hostname verification
rustmq-admin config set security.tls.verify_hostname false
# WARNING: Only use in secure testing environments
```

## Authentication Problems

### mTLS Authentication Issues

#### Problem: Client Certificate Authentication Failures
```bash
# Symptoms:
# - "Client certificate required" errors
# - "Invalid client certificate" errors
# - Authentication succeeds but client identity not recognized

# Diagnostic steps:
# 1. Verify client certificate is provided
curl -v --cert /path/to/client.pem --key /path/to/client.key https://broker:9092/health

# 2. Check certificate chain validation
openssl verify -CAfile /etc/rustmq/certs/ca.pem /path/to/client.pem

# 3. Test certificate parsing
openssl x509 -in /path/to/client.pem -noout -subject -issuer

# 4. Check client certificate requirements
rustmq-admin config show security.tls.client_cert_required
```

#### Common Authentication Issues and Solutions

##### Issue: Missing Client Certificate
```bash
# Problem: Client not providing certificate for mTLS
# Error: "Client certificate required" or "peer did not return a certificate"

# Diagnosis:
# Check if client certificate is configured
curl -v https://broker:9092/health  # Should fail
curl -v --cert client.pem --key client.key https://broker:9092/health  # Should succeed

# Solution 1: Configure client with certificate
# For curl:
curl --cert /path/to/client.pem --key /path/to/client.key https://broker:9092/

# For applications, ensure client certificate is loaded:
# - Rust: configure TLS with client certificate
# - Go: use tls.Config with client certificates
# - Python: use ssl.SSLContext with client certificate

# Solution 2: Temporarily disable client certificate requirement (testing only)
rustmq-admin config set security.tls.client_cert_required false
# WARNING: Only for testing - re-enable for production
```

##### Issue: Client Certificate Not Trusted
```bash
# Problem: Client certificate not signed by trusted CA
# Error: "certificate verify failed: unable to get issuer certificate"

# Diagnosis:
# Verify client certificate chain
openssl verify -CAfile /etc/rustmq/certs/ca.pem /path/to/client.pem

# Check if client was issued by correct CA
openssl x509 -in /path/to/client.pem -noout -issuer

# Solution 1: Reissue client certificate from correct CA
rustmq-admin certs issue \
  --principal "client@company.com" \
  --role client \
  --ca-id production_ca_1

# Solution 2: Add client CA to trusted CAs
# Copy client CA certificate to server trust store
cat client-ca.pem >> /etc/rustmq/certs/ca.pem

# Solution 3: Update CA chain
rustmq-admin ca update-chain --include-ca <client_ca_id>
```

##### Issue: Certificate Principal Mapping
```bash
# Problem: Client certificate valid but principal not recognized
# Error: Authentication succeeds but authorization fails unexpectedly

# Diagnosis:
# Check how principal is extracted from certificate
rustmq-admin config show security.authentication.principal_extraction

# Check certificate subject and extensions
openssl x509 -in /path/to/client.pem -noout -subject -text

# Test principal extraction
rustmq-admin auth test-principal-extraction --cert-file /path/to/client.pem

# Solution 1: Configure principal extraction method
rustmq-admin config set security.authentication.principal_from "subject_cn"
# or: "subject_email", "san_email", "san_dns", "custom_oid"

# Solution 2: Reissue certificate with correct subject
rustmq-admin certs issue \
  --principal "service@company.com" \
  --subject-template "CN={{principal}},O=Company,OU=Services"

# Solution 3: Create ACL rule for extracted principal
# First check what principal is being extracted:
rustmq-admin audit logs --event-type authentication --limit 5
# Then create ACL rule for that principal
```

### Certificate Authority Issues

#### Problem: CA Certificate Problems
```bash
# Symptoms:
# - All certificate validations failing
# - "CA certificate not found" errors
# - Certificate chain validation failures

# Diagnostic steps:
# 1. Check CA status
rustmq-admin ca list

# 2. Verify CA certificate
rustmq-admin ca info --ca-id <ca_id>

# 3. Test CA certificate file
openssl x509 -in /etc/rustmq/certs/ca.pem -noout -text

# 4. Check CA certificate permissions
ls -la /etc/rustmq/certs/ca.pem
```

##### Issue: CA Certificate Expired
```bash
# Problem: CA certificate has expired
# Error: "certificate verify failed: certificate has expired"

# Diagnosis:
openssl x509 -in /etc/rustmq/certs/ca.pem -noout -dates

# Solution 1: Renew CA certificate (complex process)
# This requires careful coordination as all issued certificates will need updating

# Solution 2: For root CA, create new CA and migrate
# 1. Create new CA
rustmq-admin ca init --cn "New Root CA" --validity-years 20

# 2. Create migration plan for all certificates
rustmq-admin certs migration-plan --from-ca old_ca --to-ca new_ca

# 3. Execute migration in phases
rustmq-admin certs migrate --migration-plan migration-plan.json --phase 1

# Solution 3: Emergency CA extension (use with extreme caution)
# This should only be done in emergency situations
rustmq-admin ca emergency-extend --ca-id <ca_id> --days 30
```

##### Issue: CA Private Key Problems
```bash
# Problem: CA private key not accessible or corrupted
# Error: "unable to load CA private key" or "bad decrypt"

# Diagnosis:
# Test CA private key
openssl rsa -in /etc/rustmq/ca/ca.key -check -noout

# Verify key matches certificate
openssl x509 -noout -modulus -in /etc/rustmq/ca/ca.pem | openssl md5
openssl rsa -noout -modulus -in /etc/rustmq/ca/ca.key | openssl md5

# Solution 1: Restore CA private key from backup
cp /backup/rustmq/ca/ca.key /etc/rustmq/ca/ca.key
chmod 600 /etc/rustmq/ca/ca.key
chown rustmq:rustmq /etc/rustmq/ca/ca.key

# Solution 2: If key is permanently lost, CA must be recreated
# This is a major incident requiring complete certificate reissuance
# Follow disaster recovery procedures

# Solution 3: For encrypted private keys, check password
openssl rsa -in /etc/rustmq/ca/ca.key -passin pass:your_password -noout
```

## Authorization and ACL Issues

### Permission Denied Problems

#### Problem: Unexpected Authorization Denials
```bash
# Symptoms:
# - Valid users getting "permission denied" errors
# - Previously working operations now failing
# - Inconsistent authorization behavior

# Diagnostic steps:
# 1. Test specific permission
rustmq-admin acl test \
  --principal "user@company.com" \
  --resource "topic.user-events" \
  --operation "read"

# 2. Check ACL rules for principal
rustmq-admin acl list --principal "user@company.com"

# 3. Check for conflicting rules
rustmq-admin acl conflicts --principal "user@company.com" --resource "topic.user-events"

# 4. Verify ACL cache status
rustmq-admin acl cache-stats
```

#### Common Authorization Issues and Solutions

##### Issue: Deny Rule Overriding Allow Rule
```bash
# Problem: High-priority deny rule blocking intended access
# Error: "Permission denied" despite allow rule existing

# Diagnosis:
# Find all rules affecting principal and resource
rustmq-admin acl search \
  --principal "user@company.com" \
  --resource "topic.sensitive-data"

# Check rule priorities and order
rustmq-admin acl list --sort-by priority --descending

# Solution 1: Adjust rule priorities
# Lower priority number = higher precedence
rustmq-admin acl update <allow_rule_id> --priority 50
rustmq-admin acl update <deny_rule_id> --priority 100

# Solution 2: Create specific exception rule
rustmq-admin acl create \
  --name "user_exception" \
  --principal "user@company.com" \
  --resource "topic.sensitive-data.allowed" \
  --permissions "read" \
  --effect "allow" \
  --priority 10

# Solution 3: Modify deny rule to exclude specific principal
rustmq-admin acl update <deny_rule_id> \
  --principal "!user@company.com,*@external.com"
```

##### Issue: Overly Restrictive Wildcard Rules
```bash
# Problem: Wildcard deny rules blocking legitimate access
# Error: Broad permissions blocked by catch-all deny rules

# Diagnosis:
# Find wildcard rules affecting access
rustmq-admin acl list --resource "*" --effect "deny"

# Check specific rule evaluation
rustmq-admin acl explain \
  --principal "user@company.com" \
  --resource "topic.test" \
  --operation "read"

# Solution 1: Create specific allow rules before wildcard denies
rustmq-admin acl create \
  --name "specific_allow_before_wildcard" \
  --principal "user@company.com" \
  --resource "topic.*" \
  --permissions "read" \
  --effect "allow" \
  --priority 100  # Higher priority than wildcard deny

# Solution 2: Modify wildcard rule to be more specific
rustmq-admin acl update <wildcard_deny_rule> \
  --resource "topic.restricted.*" \
  --description "More specific restriction"

# Solution 3: Use exclude patterns in wildcard rules
rustmq-admin acl create \
  --name "wildcard_with_exclusions" \
  --principal "*" \
  --resource "topic.*,!topic.public.*" \
  --permissions "write" \
  --effect "deny"
```

##### Issue: Condition Evaluation Problems
```bash
# Problem: Access denied due to failing conditions
# Error: "Permission denied: condition not met"

# Diagnosis:
# Test specific conditions
rustmq-admin acl test \
  --principal "user@company.com" \
  --resource "topic.data" \
  --operation "read" \
  --context "source_ip=10.0.1.100,time=14:30:00"

# Check condition syntax
rustmq-admin acl validate --rule-id <rule_id>

# Debug condition evaluation
rustmq-admin acl debug-conditions \
  --rule-id <rule_id> \
  --context "source_ip=192.168.1.100"

# Solution 1: Fix condition syntax
rustmq-admin acl update <rule_id> \
  --conditions "source_ip=10.0.0.0/8,time_range=09:00-17:00"

# Solution 2: Add multiple condition options
rustmq-admin acl update <rule_id> \
  --conditions "source_ip=10.0.0.0/8||source_ip=192.168.0.0/16"

# Solution 3: Create alternative rule without conditions
rustmq-admin acl create \
  --name "fallback_rule" \
  --principal "user@company.com" \
  --resource "topic.data" \
  --permissions "read" \
  --effect "allow" \
  --priority 200  # Lower priority than conditional rule
```

### ACL Performance Issues

#### Problem: Slow Authorization Response
```bash
# Symptoms:
# - High authorization latency (>100ms)
# - ACL cache misses
# - Performance degradation under load

# Diagnostic steps:
# 1. Check ACL performance metrics
rustmq-admin acl cache-stats

# 2. Benchmark authorization performance
rustmq-admin acl benchmark --duration 60s --concurrency 10

# 3. Analyze cache hit rates
rustmq-admin acl cache-analysis --show-hit-patterns

# 4. Check for expensive conditions
rustmq-admin acl list --show-performance-impact
```

##### Issue: Low Cache Hit Rate
```bash
# Problem: ACL cache hit rate below 80%
# Impact: High authorization latency due to frequent controller fetches

# Diagnosis:
rustmq-admin acl cache-stats
# Look for low hit_rate values in L1, L2, or L3 caches

# Solution 1: Increase cache sizes
rustmq-admin config set security.acl.l1_cache_size 2000
rustmq-admin config set security.acl.l2_cache_size 20000

# Solution 2: Increase cache TTL
rustmq-admin config set security.acl.l1_cache_ttl_seconds 600
rustmq-admin config set security.acl.l2_cache_ttl_seconds 1200

# Solution 3: Optimize ACL rules for caching
# - Use specific resource patterns instead of wildcards
# - Consolidate frequently-used permissions into single rules
# - Avoid frequently-changing conditions

# Solution 4: Pre-warm cache for common access patterns
rustmq-admin acl cache-warm \
  --principals "common-service@company.com" \
  --resources "topic.high-traffic.*"
```

##### Issue: Expensive ACL Rule Evaluation
```bash
# Problem: Complex ACL rules causing high CPU usage
# Impact: Authorization taking >1ms instead of target <100ns

# Diagnosis:
# Profile ACL rule performance
rustmq-admin acl profile --duration 60s --show-slow-rules

# Identify expensive conditions
rustmq-admin acl analyze --show-condition-costs

# Solution 1: Optimize condition order
# Put fastest conditions first in rule
rustmq-admin acl update <rule_id> \
  --conditions "source_ip=10.0.0.0/8,time_range=09:00-17:00"  # IP check first (faster)

# Solution 2: Replace complex conditions with simpler alternatives
# Instead of: custom_function(check_department(user))
# Use: principal=*@engineering.company.com

# Solution 3: Split complex rules into multiple simpler rules
# Replace one complex rule with multiple specific rules

# Solution 4: Use rule caching for expensive evaluations
rustmq-admin config set security.acl.cache_rule_evaluations true
```

## Network Security Problems

### Firewall and Network Access Issues

#### Problem: Connection Timeouts and Refused Connections
```bash
# Symptoms:
# - Connection timeouts to RustMQ ports
# - "Connection refused" errors
# - Intermittent connectivity issues

# Diagnostic steps:
# 1. Test basic connectivity
telnet broker.company.com 9092
nc -zv broker.company.com 9092

# 2. Check firewall rules
iptables -L -n | grep 9092
ufw status | grep 9092

# 3. Test from different network locations
# From same subnet:
curl -k https://10.0.1.100:9092/health
# From different subnet:
curl -k https://broker.company.com:9092/health

# 4. Check network policies (Kubernetes)
kubectl get networkpolicies -n rustmq
kubectl describe networkpolicy rustmq-broker-policy -n rustmq
```

#### Common Network Issues and Solutions

##### Issue: Firewall Blocking Connections
```bash
# Problem: Firewall rules blocking RustMQ traffic
# Error: "Connection timed out" or "No route to host"

# Diagnosis:
# Check current firewall rules
iptables -L INPUT -n --line-numbers | grep -E "(9092|9093|8080)"
firewall-cmd --list-all

# Test specific ports
nmap -p 9092,9093,8080 broker.company.com

# Solution 1: Open required ports in iptables
iptables -I INPUT -p tcp --dport 9092 -s 10.0.0.0/8 -j ACCEPT
iptables -I INPUT -p tcp --dport 9093 -s 10.0.0.0/8 -j ACCEPT
iptables -I INPUT -p tcp --dport 8080 -s 192.168.100.0/24 -j ACCEPT

# Solution 2: Configure UFW
ufw allow from 10.0.0.0/8 to any port 9092
ufw allow from 10.0.0.0/8 to any port 9093
ufw allow from 192.168.100.0/24 to any port 8080

# Solution 3: Configure firewalld
firewall-cmd --permanent --add-rich-rule='rule family="ipv4" source address="10.0.0.0/8" port protocol="tcp" port="9092" accept'
firewall-cmd --reload

# Save rules persistently
service iptables save  # RHEL/CentOS
iptables-save > /etc/iptables/rules.v4  # Ubuntu/Debian
```

##### Issue: Load Balancer SSL/TLS Problems
```bash
# Problem: Load balancer SSL termination or pass-through issues
# Error: SSL errors or certificate mismatches through load balancer

# Diagnosis:
# Test direct connection to broker
curl -k --cert client.pem --key client.key https://10.0.1.100:9092/health

# Test through load balancer
curl -k --cert client.pem --key client.key https://lb.company.com:9092/health

# Check load balancer configuration
# For HAProxy:
sudo cat /etc/haproxy/haproxy.cfg | grep -A 10 -B 10 "9092"

# Solution 1: Configure SSL pass-through (recommended for mTLS)
# HAProxy configuration:
frontend rustmq_frontend
    bind *:9092
    mode tcp
    default_backend rustmq_brokers

backend rustmq_brokers
    mode tcp
    balance roundrobin
    server broker1 10.0.1.100:9092 check
    server broker2 10.0.1.101:9092 check

# Solution 2: Configure SSL termination with re-encryption
frontend rustmq_frontend
    bind *:9092 ssl crt /etc/ssl/certs/rustmq-lb.pem
    mode http
    default_backend rustmq_brokers_ssl

backend rustmq_brokers_ssl
    mode http
    balance roundrobin
    option ssl-hello-chk
    server broker1 10.0.1.100:9092 ssl verify required ca-file /etc/ssl/certs/ca.pem
    server broker2 10.0.1.101:9092 ssl verify required ca-file /etc/ssl/certs/ca.pem

# Solution 3: Update DNS and certificates for load balancer endpoint
# Ensure load balancer certificate includes all necessary hostnames
```

##### Issue: Kubernetes Network Policy Problems
```bash
# Problem: Kubernetes network policies blocking Pod communication
# Error: Connection refused between Pods despite correct configuration

# Diagnosis:
# Check network policies
kubectl get networkpolicies -n rustmq
kubectl describe networkpolicy rustmq-broker-policy -n rustmq

# Test Pod-to-Pod connectivity
kubectl exec -n rustmq rustmq-client-pod -- nc -zv rustmq-broker.rustmq.svc.cluster.local 9092

# Check service endpoints
kubectl get endpoints rustmq-broker -n rustmq

# Solution 1: Allow broker-to-broker communication
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: rustmq-broker-policy
  namespace: rustmq
spec:
  podSelector:
    matchLabels:
      app: rustmq-broker
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - podSelector:
        matchLabels:
          app: rustmq-broker  # Allow broker-to-broker
    ports:
    - protocol: TCP
      port: 9093

# Solution 2: Allow client access to brokers
  - from:
    - podSelector:
        matchLabels:
          app: rustmq-client
    - namespaceSelector:
        matchLabels:
          name: rustmq-clients
    ports:
    - protocol: TCP
      port: 9092

# Solution 3: Temporarily allow all traffic for debugging
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: allow-all-temporary
  namespace: rustmq
spec:
  podSelector: {}
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - {}
  egress:
  - {}
# Remember to remove this after debugging!
```

### DNS and Service Discovery Issues

#### Problem: Service Discovery Failures
```bash
# Symptoms:
# - "Host not found" or "Name resolution failed"
# - Intermittent connection failures
# - Service mesh communication problems

# Diagnostic steps:
# 1. Test DNS resolution
nslookup rustmq-broker.rustmq.svc.cluster.local
dig rustmq-broker.company.com

# 2. Check service registration
kubectl get services -n rustmq
consul members  # If using Consul
etcdctl get /registry/services  # If using etcd

# 3. Test service connectivity
kubectl exec -n rustmq test-pod -- nslookup rustmq-broker.rustmq.svc.cluster.local
```

##### Issue: DNS Resolution Problems
```bash
# Problem: DNS resolution failing for RustMQ services
# Error: "Host not found" or "Name or service not known"

# Diagnosis:
# Test DNS resolution
nslookup broker.rustmq.company.com
dig +trace broker.rustmq.company.com

# Check DNS configuration
cat /etc/resolv.conf
systemd-resolve --status

# Solution 1: Fix DNS configuration
# Update /etc/resolv.conf with correct nameservers
echo "nameserver 8.8.8.8" >> /etc/resolv.conf
echo "nameserver 8.8.4.4" >> /etc/resolv.conf

# Solution 2: For Kubernetes, check DNS pods
kubectl get pods -n kube-system | grep dns
kubectl logs -n kube-system coredns-xxxx

# Fix CoreDNS configuration if needed
kubectl edit configmap coredns -n kube-system

# Solution 3: Add hosts file entries as temporary workaround
echo "10.0.1.100 broker1.rustmq.company.com" >> /etc/hosts
echo "10.0.1.101 broker2.rustmq.company.com" >> /etc/hosts
```

## Performance and Security Trade-offs

### High Latency with Security Enabled

#### Problem: Authorization Latency Above Target
```bash
# Symptoms:
# - Authorization taking >100ms instead of target <100ns
# - High CPU usage in ACL evaluation
# - Poor cache hit rates

# Diagnostic steps:
# 1. Measure authorization performance
rustmq-admin acl benchmark --duration 30s --show-latency-percentiles

# 2. Analyze cache performance
rustmq-admin acl cache-stats --detailed

# 3. Profile ACL rule evaluation
rustmq-admin acl profile --show-expensive-rules

# 4. Check system resources
top -p $(pgrep rustmq-broker)
iostat -x 1 5
```

#### Common Performance Issues and Solutions

##### Issue: Low ACL Cache Hit Rate
```bash
# Problem: Cache hit rate below 80%
# Impact: Frequent controller queries causing high latency

# Diagnosis:
rustmq-admin acl cache-stats
# Look for:
# - L1 hit rate < 80%
# - L2 hit rate < 70%
# - High controller_fetch_rate

# Solution 1: Optimize cache configuration
rustmq-admin config set security.acl.l1_cache_size 2000
rustmq-admin config set security.acl.l2_cache_size 20000
rustmq-admin config set security.acl.l1_cache_ttl_seconds 600

# Solution 2: Enable string interning for memory efficiency
rustmq-admin config set security.acl.enable_string_interning true
rustmq-admin config set security.acl.string_intern_threshold 10

# Solution 3: Increase batch fetch size from controller
rustmq-admin config set security.acl.controller_fetch_batch_size 100

# Solution 4: Pre-warm cache with common access patterns
rustmq-admin acl cache-warm \
  --principals "app1@company.com,app2@company.com" \
  --resources "topic.high-volume.*"
```

##### Issue: Expensive Certificate Validation
```bash
# Problem: Certificate validation taking too long
# Impact: High TLS handshake latency

# Diagnosis:
# Measure certificate validation time
time openssl verify -CAfile /etc/rustmq/certs/ca.pem /etc/rustmq/certs/client.pem

# Check certificate chain length
openssl crl2pkcs7 -nocrl -certfile /etc/rustmq/certs/chain.pem | openssl pkcs7 -print_certs -text -noout | grep -c "Certificate:"

# Solution 1: Enable certificate validation caching
rustmq-admin config set security.certificates.cache_validation_results true
rustmq-admin config set security.certificates.validation_cache_size 5000
rustmq-admin config set security.certificates.validation_cache_ttl_seconds 300

# Solution 2: Optimize certificate chain
# Use shorter certificate chains (max 3 levels: Root -> Intermediate -> End Entity)
# Consider using ECDSA certificates for faster operations

# Solution 3: Enable OCSP stapling to avoid online revocation checks
rustmq-admin config set security.certificates.enable_ocsp_stapling true

# Solution 4: Use hardware acceleration if available
rustmq-admin config set security.tls.use_hardware_acceleration true
```

##### Issue: High TLS Handshake Overhead
```bash
# Problem: TLS handshakes consuming too much CPU/time
# Impact: High connection establishment latency

# Diagnosis:
# Test TLS handshake performance
time openssl s_client -connect localhost:9092 -cert client.pem -key client.key < /dev/null

# Check TLS session resumption
openssl s_client -connect localhost:9092 -sess_out session1.pem < /dev/null
openssl s_client -connect localhost:9092 -sess_in session1.pem < /dev/null

# Solution 1: Enable TLS session resumption
rustmq-admin config set security.tls.session_resumption true
rustmq-admin config set security.tls.session_cache_size 10000

# Solution 2: Enable TLS 1.3 with 0-RTT (if supported)
rustmq-admin config set security.tls.protocol_version "1.3"
rustmq-admin config set security.tls.enable_early_data true

# Solution 3: Optimize cipher suite selection for performance
rustmq-admin config set security.tls.cipher_suite_preference "performance"

# Solution 4: Use connection pooling in clients to reduce handshake frequency
# Configure clients to maintain persistent connections
```

## Monitoring and Alerting Issues

### Security Metrics Not Available

#### Problem: Missing Security Metrics
```bash
# Symptoms:
# - Security dashboards showing no data
# - Missing authentication/authorization metrics
# - Monitoring alerts not firing

# Diagnostic steps:
# 1. Check if security metrics are enabled
rustmq-admin config show security.metrics

# 2. Test metrics endpoint
curl -k https://localhost:9642/metrics | grep rustmq_security

# 3. Check Prometheus scraping configuration
curl -k https://localhost:9642/metrics | head -20

# 4. Verify audit logging is working
rustmq-admin audit logs --limit 5
```

#### Common Monitoring Issues and Solutions

##### Issue: Prometheus Not Scraping Security Metrics
```bash
# Problem: Security metrics not appearing in Prometheus
# Error: No data in security dashboards

# Diagnosis:
# Check if metrics endpoint is accessible
curl -k https://broker:9642/metrics

# Test from Prometheus server
curl -k --cert prometheus-client.pem --key prometheus-client.key https://broker:9642/metrics

# Check Prometheus configuration
grep -A 10 -B 5 "rustmq" /etc/prometheus/prometheus.yml

# Solution 1: Enable security metrics collection
rustmq-admin config set security.metrics.enabled true
rustmq-admin config set security.metrics.detailed_metrics true

# Solution 2: Fix Prometheus scrape configuration
# Add to prometheus.yml:
- job_name: 'rustmq-security'
  static_configs:
    - targets: ['broker1:9642', 'broker2:9642', 'broker3:9642']
  scheme: https
  tls_config:
    cert_file: /etc/prometheus/certs/client.pem
    key_file: /etc/prometheus/certs/client.key
    ca_file: /etc/prometheus/certs/ca.pem
  metrics_path: /metrics
  scrape_interval: 15s

# Solution 3: Configure certificate authentication for metrics endpoint
rustmq-admin certs issue \
  --principal "prometheus@monitoring.company.com" \
  --role client \
  --ca-id production_ca_1
```

##### Issue: Security Alerts Not Firing
```bash
# Problem: Security incidents not triggering alerts
# Impact: Security events going unnoticed

# Diagnosis:
# Check alert rules syntax
promtool check rules /etc/prometheus/rules/rustmq-security.yml

# Test alert evaluation
curl 'http://prometheus:9090/api/v1/rules' | jq '.data.groups[].rules[] | select(.name | contains("RustMQ"))'

# Check if metrics exist for alert rules
curl 'http://prometheus:9090/api/v1/query?query=rustmq_authentication_failures_total'

# Solution 1: Fix alert rule syntax
# Ensure metric names match exactly
groups:
- name: rustmq.security
  rules:
  - alert: RustMQAuthFailures
    expr: rate(rustmq_authentication_failures_total[5m]) > 0.1
    for: 2m
    labels:
      severity: warning

# Solution 2: Verify metric names
# Check actual metric names from endpoint
curl -k https://broker:9642/metrics | grep rustmq_auth

# Solution 3: Configure Alertmanager routing
# Ensure security alerts reach correct destinations
route:
  group_by: ['alertname']
  routes:
  - match:
      category: security
    receiver: security-team
    group_wait: 10s
    group_interval: 10s
    repeat_interval: 1h
```

### Log Aggregation Problems

#### Problem: Security Logs Not Being Collected
```bash
# Symptoms:
# - Security events missing from centralized logs
# - SIEM not receiving RustMQ security data
# - Incomplete audit trails

# Diagnostic steps:
# 1. Check local log files
ls -la /var/log/rustmq/
tail -f /var/log/rustmq/security-audit.log

# 2. Test log shipping
# For Filebeat:
systemctl status filebeat
tail -f /var/log/filebeat/filebeat

# For Fluentd:
systemctl status fluentd
tail -f /var/log/fluentd/fluentd.log

# 3. Check destination (Elasticsearch, Splunk, etc.)
curl -X GET "elasticsearch:9200/rustmq-security-*/_search?pretty&size=5"
```

##### Issue: Log Shipping Configuration Problems
```bash
# Problem: Security logs not reaching SIEM
# Error: Logs staying local, not forwarded to central system

# Diagnosis:
# Check log shipping agent status
systemctl status filebeat
journalctl -u filebeat -f

# Test log parsing
/usr/share/filebeat/bin/filebeat test config
/usr/share/filebeat/bin/filebeat test output

# Solution 1: Fix Filebeat configuration
# /etc/filebeat/filebeat.yml
filebeat.inputs:
- type: log
  enabled: true
  paths:
    - /var/log/rustmq/security-audit.log
    - /var/log/rustmq/authentication.log
  fields:
    logtype: rustmq-security
  fields_under_root: true
  json.keys_under_root: true

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
  index: "rustmq-security-%{+yyyy.MM.dd}"

# Solution 2: Configure log rotation coordination
# Ensure log rotation doesn't break shipping
logrotate_config: |
  /var/log/rustmq/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 644 rustmq rustmq
    postrotate
      systemctl reload filebeat
    endscript
  }

# Solution 3: Set up monitoring for log shipping
# Alert if logs stop flowing
- alert: RustMQLogShippingFailed
  expr: increase(filebeat_harvester_files_truncated_total[5m]) > 0
  for: 1m
```

## Configuration Problems

### Invalid Security Configuration

#### Problem: Security Features Not Working Due to Configuration
```bash
# Symptoms:
# - Security features disabled unexpectedly
# - Configuration validation errors
# - Inconsistent security behavior

# Diagnostic steps:
# 1. Validate current configuration
rustmq-admin config validate

# 2. Check configuration syntax
rustmq-admin config show --validate-syntax

# 3. Compare with reference configuration
rustmq-admin config diff --reference-config /etc/rustmq/reference-config.toml

# 4. Check configuration file permissions
ls -la /etc/rustmq/config/
```

#### Common Configuration Issues and Solutions

##### Issue: Security Features Disabled in Configuration
```bash
# Problem: Security unexpectedly disabled
# Error: "Security features not enabled" or authentication bypassed

# Diagnosis:
# Check security configuration
rustmq-admin config show security

# Verify configuration file
grep -A 10 "\[security\]" /etc/rustmq/config/broker.toml

# Solution 1: Enable security features
rustmq-admin config set security.enabled true
rustmq-admin config set security.tls.enabled true
rustmq-admin config set security.acl.enabled true

# Solution 2: Restore from backup configuration
cp /backup/rustmq/config/security.toml /etc/rustmq/config/
systemctl restart rustmq-broker

# Solution 3: Use reference configuration
rustmq-admin config restore --reference-config /etc/rustmq/reference/production-security.toml
```

##### Issue: Certificate Path Configuration Problems
```bash
# Problem: Certificate files not found or inaccessible
# Error: "Certificate file not found" or "Permission denied"

# Diagnosis:
# Check certificate file paths in configuration
rustmq-admin config show security.tls

# Verify file existence and permissions
ls -la /etc/rustmq/certs/
stat /etc/rustmq/certs/server.pem

# Solution 1: Fix file paths
rustmq-admin config set security.tls.server_cert_path "/etc/rustmq/certs/server.pem"
rustmq-admin config set security.tls.server_key_path "/etc/rustmq/certs/server.key"
rustmq-admin config set security.tls.client_ca_cert_path "/etc/rustmq/certs/ca.pem"

# Solution 2: Fix file permissions
chown rustmq:rustmq /etc/rustmq/certs/*
chmod 644 /etc/rustmq/certs/*.pem
chmod 600 /etc/rustmq/certs/*.key

# Solution 3: Create symbolic links if files are in different location
ln -s /opt/certificates/server.pem /etc/rustmq/certs/server.pem
ln -s /opt/certificates/server.key /etc/rustmq/certs/server.key
```

##### Issue: Configuration Validation Errors
```bash
# Problem: Configuration file contains invalid settings
# Error: "Configuration validation failed" on startup

# Diagnosis:
# Validate configuration syntax
rustmq-admin config validate --config-file /etc/rustmq/config/broker.toml

# Check for specific validation errors
rustmq-admin config lint --show-warnings

# Solution 1: Fix common validation issues
# Remove invalid configuration options
# Fix data type mismatches (string vs number)
# Correct enum values

# Example fixes:
rustmq-admin config set security.tls.protocol_version "1.3"  # String, not number
rustmq-admin config set security.acl.cache_size_mb 100      # Number, not string

# Solution 2: Use configuration schema validation
rustmq-admin config validate --schema /etc/rustmq/schema/config-schema.json

# Solution 3: Start with minimal valid configuration and add features incrementally
cp /etc/rustmq/examples/minimal-secure.toml /etc/rustmq/config/broker.toml
# Add features one by one, validating each step
```

## Emergency Procedures

### Security Incident Response

#### Emergency Security Lockdown
```bash
# When immediate threat containment is required

# 1. Emergency disable all non-essential access
rustmq-admin acl emergency-disable-all \
  --except-principals "emergency-admin@company.com" \
  --reason "Security emergency lockdown - $(date)"

# 2. Enable maximum security logging
rustmq-admin config set security.audit.log_level debug
rustmq-admin config set security.audit.log_all_events true

# 3. Activate emergency monitoring mode
rustmq-admin security enable-emergency-mode

# 4. Collect immediate evidence
rustmq-admin audit export \
  --time-range "last-1-hour" \
  --output-file "/tmp/emergency-audit-$(date +%Y%m%d%H%M).json"

# 5. Notify security team
echo "RustMQ emergency lockdown activated at $(date)" | \
  mail -s "URGENT: RustMQ Security Emergency" security@company.com
```

#### Emergency Certificate Revocation
```bash
# When certificate compromise is suspected

# 1. Immediately revoke compromised certificate
rustmq-admin certs emergency-revoke <certificate_id> \
  --reason "security-incident" \
  --effective-immediately \
  --notify-clients

# 2. Update CRL immediately
rustmq-admin ca generate-crl --force-update --distribute-immediately

# 3. If CA compromise suspected, disable CA
rustmq-admin ca emergency-disable <ca_id> \
  --reason "suspected-compromise"

# 4. For widespread compromise, rotate all certificates
rustmq-admin certs emergency-rotate-all \
  --ca-id <ca_id> \
  --reason "security-incident" \
  --overlap-hours 1
```

#### Recovery Procedures
```bash
# After emergency response, systematic recovery

# 1. Assess security status
rustmq-admin security status --post-incident-check

# 2. Gradually restore access
# Start with administrative access
rustmq-admin acl enable --rule-id <admin_rule_id>

# Then critical services
rustmq-admin acl enable --principal "critical-service@company.com"

# Finally, general access (with increased monitoring)
rustmq-admin security disable-emergency-mode
rustmq-admin acl enable-all --gradual --monitor-closely

# 3. Generate incident report
rustmq-admin audit incident-report \
  --incident-id "$(date +%Y%m%d%H%M)" \
  --include-timeline \
  --include-actions-taken \
  --output-file "incident-report-$(date +%Y%m%d).json"
```

## Common Error Codes

### Security Error Code Reference

#### Authentication Errors (AUTH_xxx)
```bash
AUTH_001: "Certificate required but not provided"
# Solution: Configure client with valid certificate
curl --cert client.pem --key client.key https://broker:9092/

AUTH_002: "Certificate validation failed"
# Solution: Check certificate validity and chain
openssl verify -CAfile ca.pem client.pem

AUTH_003: "Certificate expired"
# Solution: Renew certificate
rustmq-admin certs renew <cert_id>

AUTH_004: "Certificate revoked"
# Solution: Issue new certificate
rustmq-admin certs issue --principal <principal> --role <role>

AUTH_005: "Invalid certificate chain"
# Solution: Rebuild certificate chain
rustmq-admin certs rebuild-chain <cert_id>

AUTH_006: "Certificate hostname mismatch"
# Solution: Add hostname to certificate SANs
rustmq-admin certs reissue <cert_id> --san "hostname.domain.com"
```

#### Authorization Errors (AUTHZ_xxx)
```bash
AUTHZ_001: "Permission denied"
# Solution: Check and create appropriate ACL rule
rustmq-admin acl test --principal <principal> --resource <resource>

AUTHZ_002: "No matching ACL rule found"
# Solution: Create ACL rule for principal and resource
rustmq-admin acl create --principal <principal> --resource <resource> --permissions <perms>

AUTHZ_003: "Access denied by explicit deny rule"
# Solution: Check rule priorities and create exception or modify deny rule
rustmq-admin acl list --principal <principal> --sort-by priority

AUTHZ_004: "Condition evaluation failed"
# Solution: Check condition syntax and context
rustmq-admin acl debug-conditions --rule-id <rule_id>

AUTHZ_005: "Principal not recognized"
# Solution: Check principal extraction configuration
rustmq-admin config show security.authentication.principal_extraction
```

#### Certificate Management Errors (CERT_xxx)
```bash
CERT_001: "CA not found"
# Solution: Check CA exists and is active
rustmq-admin ca list

CERT_002: "Certificate issuance failed"
# Solution: Check CA status and certificate template
rustmq-admin ca info --ca-id <ca_id>

CERT_003: "Private key not accessible"
# Solution: Check key file permissions and encryption
ls -la /etc/rustmq/certs/
openssl rsa -in key.pem -check

CERT_004: "Certificate serial number conflict"
# Solution: CA database corruption, rebuild CA index
rustmq-admin ca rebuild-index --ca-id <ca_id>
```

#### TLS Errors (TLS_xxx)
```bash
TLS_001: "Handshake failure"
# Solution: Check TLS versions and cipher suites
openssl s_client -connect host:port -tls1_3

TLS_002: "No cipher suites in common"
# Solution: Configure compatible cipher suites
rustmq-admin config set security.tls.cipher_suites '["TLS_AES_256_GCM_SHA384"]'

TLS_003: "Protocol version not supported"
# Solution: Configure supported TLS versions
rustmq-admin config set security.tls.min_protocol_version "1.2"

TLS_004: "Certificate verification failed in TLS"
# Solution: Check certificate chain and hostname
openssl verify -CAfile ca.pem server.pem
```

This comprehensive troubleshooting guide provides systematic approaches to diagnosing and resolving the most common security issues in RustMQ deployments. For complex issues not covered here, consult the detailed security architecture documentation and consider engaging with the RustMQ security team for assistance.