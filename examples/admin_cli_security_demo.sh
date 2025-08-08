#!/bin/bash

# RustMQ Admin CLI Security Extension Demo
# This script demonstrates the comprehensive security command suite

set -e

echo "🔐 RustMQ Admin CLI Security Extension Demo"
echo "==========================================="
echo

# Configuration
API_URL="http://127.0.0.1:8080"
ADMIN_CLI="cargo run --bin rustmq-admin --"

echo "📋 Using API URL: $API_URL"
echo "🔧 Admin CLI: $ADMIN_CLI"
echo

# Function to run admin command with error handling
run_admin_cmd() {
    echo "💻 Running: $ADMIN_CLI $@"
    if $ADMIN_CLI --api-url "$API_URL" "$@"; then
        echo "✅ Command completed successfully"
    else
        echo "❌ Command failed (this is expected in demo without running server)"
    fi
    echo
}

echo "🏛️  CERTIFICATE AUTHORITY MANAGEMENT"
echo "====================================="

echo "1. Initialize Root CA"
run_admin_cmd ca init \
    --cn "RustMQ Root CA" \
    --org "RustMQ Corp" \
    --country US \
    --validity-years 10 \
    --key-size 4096

echo "2. List Certificate Authorities"
run_admin_cmd ca list --format table

echo "3. Create Intermediate CA"
run_admin_cmd ca intermediate \
    --parent-ca root_ca_1 \
    --cn "RustMQ Intermediate CA" \
    --org "RustMQ Corp" \
    --validity-days 1825

echo "📜 CERTIFICATE LIFECYCLE MANAGEMENT"
echo "=================================="

echo "4. Issue Broker Certificate"
run_admin_cmd certs issue \
    --principal "broker-01.rustmq.com" \
    --role broker \
    --ca-id root_ca_1 \
    --san "broker-01" \
    --san "192.168.1.100" \
    --org "RustMQ Corp" \
    --validity-days 365

echo "5. Issue Client Certificate"
run_admin_cmd certs issue \
    --principal "client-app@company.com" \
    --role client \
    --ca-id root_ca_1 \
    --org "Company Inc" \
    --validity-days 90

echo "6. List All Certificates"
run_admin_cmd certs list --format table

echo "7. List Active Certificates Only"
run_admin_cmd certs list --filter active --format json

echo "8. List Broker Certificates"
run_admin_cmd certs list --role broker

echo "9. Get Certificate Information"
run_admin_cmd certs info cert_12345

echo "10. Check Certificate Status"
run_admin_cmd certs status cert_12345

echo "11. List Expiring Certificates (30 days)"
run_admin_cmd certs expiring --days 30

echo "12. Validate Certificate from File"
run_admin_cmd certs validate \
    --cert-file /tmp/test-cert.pem \
    --check-revocation

echo "13. Renew Certificate"
run_admin_cmd certs renew cert_12345

echo "14. Rotate Certificate (new key pair)"
run_admin_cmd certs rotate cert_12345

echo "15. Export Certificate"
run_admin_cmd certs export cert_12345 \
    --format pem \
    --output /tmp/exported-cert.pem

echo "16. Revoke Certificate"
run_admin_cmd certs revoke cert_compromised \
    --reason "key-compromise" \
    --reason-code 1 \
    --force

echo "🛡️  ACCESS CONTROL LIST (ACL) MANAGEMENT"
echo "======================================="

echo "17. Create User ACL Rule"
run_admin_cmd acl create \
    --principal "user@domain.com" \
    --resource "topic.users.*" \
    --resource-type topic \
    --permissions "read,write" \
    --effect allow \
    --conditions "source_ip=192.168.1.0/24"

echo "18. Create Admin ACL Rule"
run_admin_cmd acl create \
    --principal "admin@domain.com" \
    --resource "topic.*" \
    --resource-type topic \
    --permissions "read,write,delete" \
    --effect allow

echo "19. Create Service Account ACL Rule"
run_admin_cmd acl create \
    --principal "service@company.com" \
    --resource "topic.metrics.*" \
    --resource-type topic \
    --permissions "write" \
    --effect allow

echo "20. List All ACL Rules"
run_admin_cmd acl list --format table

echo "21. List Rules for Specific Principal"
run_admin_cmd acl list --principal "user@domain.com"

echo "22. List Rules for Resource Pattern"
run_admin_cmd acl list --resource "topic.users.*"

echo "23. Get ACL Rule Information"
run_admin_cmd acl info rule_12345

echo "24. Test ACL Evaluation"
run_admin_cmd acl test \
    --principal "user@domain.com" \
    --resource "topic.users.data" \
    --operation read

echo "25. Test Different Principal"
run_admin_cmd acl test \
    --principal "service@company.com" \
    --resource "topic.metrics.cpu" \
    --operation write

echo "26. Get Principal Permissions"
run_admin_cmd acl permissions "user@domain.com"

echo "27. Get Rules for Resource"
run_admin_cmd acl rules "topic.users.*"

echo "28. Create Bulk Test File"
cat > /tmp/bulk_test.json << 'EOF'
{
  "evaluations": [
    {
      "principal": "user@domain.com",
      "resource": "topic.users.data",
      "operation": "read"
    },
    {
      "principal": "user@domain.com", 
      "resource": "topic.users.data",
      "operation": "write"
    },
    {
      "principal": "service@company.com",
      "resource": "topic.metrics.cpu",
      "operation": "write"
    },
    {
      "principal": "unauthorized@external.com",
      "resource": "topic.admin.logs",
      "operation": "read"
    }
  ]
}
EOF

echo "29. Run Bulk ACL Test"
run_admin_cmd acl bulk-test --input-file /tmp/bulk_test.json

echo "30. Update ACL Rule"
run_admin_cmd acl update rule_12345 \
    --permissions "read" \
    --effect allow

echo "31. Get ACL Version"
run_admin_cmd acl version

echo "32. Invalidate ACL Cache"
run_admin_cmd acl cache invalidate --principals "user@domain.com,service@company.com"

echo "33. Warm ACL Cache"
run_admin_cmd acl cache warm --principals "user@domain.com,admin@domain.com"

echo "34. Sync ACL Rules to Brokers"
run_admin_cmd acl sync --force

echo "35. Delete ACL Rule"
run_admin_cmd acl delete rule_obsolete --force

echo "📊 SECURITY AUDIT AND MONITORING"
echo "==============================="

echo "36. View Recent Audit Logs"
run_admin_cmd audit logs --limit 20

echo "37. View Audit Logs with Time Filter"
run_admin_cmd audit logs \
    --since "2024-01-01T00:00:00Z" \
    --until "2024-01-31T23:59:59Z" \
    --limit 50

echo "38. View Certificate-Related Audit Logs"
run_admin_cmd audit logs --type certificate_issued --limit 10

echo "39. View Audit Logs for Principal"
run_admin_cmd audit logs --principal "admin@rustmq.com" --limit 15

echo "40. View Real-time Security Events"
run_admin_cmd audit events --filter authentication

echo "41. View Certificate Operation Audit"
run_admin_cmd audit certificates --operation revoke

echo "42. View ACL Change Audit"
run_admin_cmd audit acl --principal "admin@domain.com"

echo "43. View ACL Operation Audit"
run_admin_cmd audit acl --operation create

echo "🏥 GENERAL SECURITY OPERATIONS"
echo "============================="

echo "44. Get Overall Security Status"
run_admin_cmd security status

echo "45. Get Security Performance Metrics"
run_admin_cmd security metrics

echo "46. Perform Security Health Checks"
run_admin_cmd security health

echo "47. Get Security Configuration"
run_admin_cmd security config

echo "48. Clean Up Expired Certificates (Dry Run)"
run_admin_cmd security cleanup \
    --expired-certs \
    --cache-entries \
    --dry-run

echo "49. Perform Actual Cleanup"
run_admin_cmd security cleanup \
    --expired-certs \
    --cache-entries

echo "50. Create Security Backup"
run_admin_cmd security backup \
    --output /tmp/security_backup.json \
    --include-certs \
    --include-acl

echo "51. Restore from Security Backup"
run_admin_cmd security restore \
    --input /tmp/security_backup.json \
    --force

echo "🎨 OUTPUT FORMAT DEMONSTRATIONS" 
echo "=============================="

echo "52. Table Format (Default)"
run_admin_cmd certs list --format table

echo "53. JSON Format"
run_admin_cmd certs list --format json | head -20

echo "54. YAML Format"
run_admin_cmd security status --format yaml | head -20

echo "55. CSV Format"
run_admin_cmd acl list --format csv | head -10

echo "🌈 COLOR AND FORMATTING OPTIONS"
echo "=============================="

echo "56. Colored Output (Default)"
run_admin_cmd security status

echo "57. No Color Output"
run_admin_cmd security status --no-color

echo "58. Verbose Output"
run_admin_cmd --verbose security status

echo "🏛️  TOPIC MANAGEMENT (Legacy Commands)"
echo "====================================="

echo "59. Create Topic (Legacy Command)"
run_admin_cmd topic create test-topic 3 2 \
    --retention-ms 86400000 \
    --segment-bytes 1073741824 \
    --compression lz4

echo "60. List Topics"
run_admin_cmd topic list

echo "61. Describe Topic"
run_admin_cmd topic describe test-topic

echo "62. Check Cluster Health"
run_admin_cmd cluster-health

echo "🧹 CLEANUP"
echo "========="

echo "Cleaning up demo files..."
rm -f /tmp/bulk_test.json
rm -f /tmp/security_backup.json
rm -f /tmp/test-cert.pem
rm -f /tmp/exported-cert.pem

echo "✅ Demo completed successfully!"
echo
echo "📝 SUMMARY"
echo "========="
echo "This demo showcased:"
echo "• Certificate Authority management (init, list, intermediate)"
echo "• Complete certificate lifecycle (issue, renew, rotate, revoke, validate)"
echo "• Comprehensive ACL management (create, list, test, sync)"
echo "• Security auditing (logs, events, operation history)"
echo "• System operations (status, metrics, health, maintenance)"
echo "• Multiple output formats (table, JSON, YAML, CSV)"
echo "• User experience features (colors, progress, confirmations)"
echo "• Backward compatibility with existing topic commands"
echo
echo "🚀 The RustMQ Admin CLI now provides comprehensive security management capabilities!"

echo
echo "📖 For detailed documentation, see: docs/admin_cli_security.md"
echo "🧪 To run unit tests: cargo test --bin rustmq-admin"
echo "🔧 To start with a real server: rustmq-admin --api-url http://your-server:8080 security status"