#!/bin/bash

# Certificate Generation Script for RustMQ Development
# This script generates self-signed certificates for testing and examples

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CERTS_DIR="$PROJECT_ROOT/certs"

echo "🔐 Generating development certificates for RustMQ..."
echo "📁 Certificates will be stored in: $CERTS_DIR"

# Create certs directory
mkdir -p "$CERTS_DIR"
cd "$CERTS_DIR"

# Clean up any existing certificates
rm -f *.pem *.key *.csr *.srl

# Certificate validity period (10 years)
VALIDITY_DAYS=3650

# Generate CA private key
echo "🔑 Generating CA private key..."
openssl genrsa -out ca-key.pem 4096

# Generate CA certificate
echo "📜 Generating CA certificate..."
openssl req -new -x509 -days $VALIDITY_DAYS -key ca-key.pem -out ca.pem -subj "/C=US/ST=Development/L=Local/O=RustMQ Development/OU=Certificate Authority/CN=RustMQ Dev CA"

# Generate client private key
echo "🔑 Generating client private key..."
openssl genrsa -out client-key.pem 4096

# Generate client certificate signing request
echo "📝 Generating client certificate signing request..."
openssl req -new -key client-key.pem -out client.csr -subj "/C=US/ST=Development/L=Local/O=RustMQ Development/OU=Client/CN=rustmq-client"

# Generate client certificate signed by CA
echo "📜 Generating client certificate..."
openssl x509 -req -days $VALIDITY_DAYS -in client.csr -CA ca.pem -CAkey ca-key.pem -CAcreateserial -out client.pem

# Create alternative names for client.key (some examples might expect this name)
cp client-key.pem client.key

# Generate consumer private key
echo "🔑 Generating consumer private key..."
openssl genrsa -out consumer-key.pem 4096

# Generate consumer certificate signing request
echo "📝 Generating consumer certificate signing request..."
openssl req -new -key consumer-key.pem -out consumer.csr -subj "/C=US/ST=Development/L=Local/O=RustMQ Development/OU=Consumer/CN=rustmq-consumer"

# Generate consumer certificate signed by CA
echo "📜 Generating consumer certificate..."
openssl x509 -req -days $VALIDITY_DAYS -in consumer.csr -CA ca.pem -CAkey ca-key.pem -CAcreateserial -out consumer.pem

# Create alternative names for consumer.key (some examples might expect this name)
cp consumer-key.pem consumer.key

# Clean up CSR files and serial file
rm -f *.csr *.srl

# Set appropriate permissions
chmod 600 *-key.pem *.key
chmod 644 *.pem

echo ""
echo "✅ Certificate generation completed successfully!"
echo ""
echo "📋 Generated files:"
echo "  📁 $CERTS_DIR/"
echo "    🔐 ca.pem            - Certificate Authority certificate"
echo "    🔑 ca-key.pem        - Certificate Authority private key"
echo "    📜 client.pem        - Client certificate"
echo "    🔑 client-key.pem    - Client private key"
echo "    🔑 client.key        - Client private key (alternative name)"
echo "    📜 consumer.pem      - Consumer certificate"
echo "    🔑 consumer-key.pem  - Consumer private key"
echo "    🔑 consumer.key      - Consumer private key (alternative name)"
echo ""
echo "🔍 Certificate details:"
echo "  📅 Validity: $VALIDITY_DAYS days (10 years) from today"
echo "  🏢 Organization: RustMQ Development"
echo "  🌍 Country: US"
echo ""
echo "⚠️  WARNING: These are development certificates only!"
echo "   📝 Do NOT use these certificates in production"
echo "   🔒 Generate proper certificates from a trusted CA for production use"
echo ""
echo "🚀 You can now run the RustMQ examples with mTLS support:"
echo "   cargo run --example secure_producer"
echo "   cargo run --example secure_consumer"
echo "   cargo run --example token_authentication"
echo ""