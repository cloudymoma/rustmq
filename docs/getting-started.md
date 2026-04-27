# Getting Started with RustMQ

This guide covers building RustMQ from source, configuring a local development cluster, and running your first message publisher and consumer.

## Prerequisites

- **Rust**: Version 1.88+ (`rustup default 1.88`)
- **Docker**: For local services (MinIO, etc.)
- **Make**: For running convenience scripts
- **OpenSSL**: For mTLS testing

## 1. Build from Source

```bash
# Clone the repository
git clone https://github.com/rustmq/rustmq.git
cd rustmq

# Run the complete test suite (includes debug build)
cargo test --lib

# Build all binaries in release mode
cargo build --release
```

## 2. Local Development Environment

We provide a Docker Compose file to easily spin up dependencies like MinIO (for local S3 object storage).

```bash
# Start MinIO
cd docker
docker-compose up -d minio
```

## 3. Generate Security Certificates

RustMQ requires mTLS by default.

```bash
# Initialize CA and generate node/client certificates
cd scripts
./setup-development.sh
```

## 4. Run the Cluster

Start a single-node controller and broker instance locally.

```bash
# Terminal 1: Start Controller
cargo run --bin rustmq-controller -- --config config/controller-dev.toml

# Terminal 2: Start Broker
cargo run --bin rustmq-broker -- --config config/broker-dev.toml
```

## 5. Send and Receive Messages

With the cluster running, you can use the Rust SDK examples to verify functionality.

```bash
# Produce messages
cargo run -p rustmq-client --example simple_producer

# Consume messages
cargo run -p rustmq-client --example simple_consumer
```

## Next Steps

- Explore the [Architecture Overview](../README.md#-architecture-overview)
- Dive into [Client SDKs](client-sdks.md)
- Deploy to [Google Kubernetes Engine](deployment/gke.md)
