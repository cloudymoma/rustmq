# RustMQ Authentication and Authorization Design

This document outlines the recommended design for implementing a robust and high-performance security model for RustMQ using Mutual TLS (mTLS) for authentication and a cached Access Control List (ACL) system for authorization.

## Guiding Principles

1.  **Performance First:** The security mechanisms on the hot path (per-request processing) must have minimal overhead.
2.  **Strong Security:** Leverage standard, battle-tested cryptographic protocols for authentication.
3.  **Manageability:** Provide administrators with clear, simple tools to manage the security of the cluster.

---

## Recommended Architecture

The proposed model consists of two layers that work together:

1.  **Authentication via Mutual TLS (mTLS):** Establishes and cryptographically verifies client identity *at connection time*.
2.  **Authorization via Cached ACLs:** Enforces permissions for authenticated clients *at request time* using a fast, in-memory cache.

### Layer 1: Authentication via Mutual TLS (mTLS)

This is the most natural and performant way to handle authentication over QUIC.

#### How it Works

1.  **Certificate Authority (CA):** A central Certificate Authority is established for the cluster. This CA is the root of trust and is used to sign all broker and client certificates.
2.  **Client & Broker Certificates:** Every client (producer/consumer) and every broker is issued a unique TLS certificate signed by the cluster's CA.
    *   **Client Identity:** The client's identity (the **Principal**) is embedded directly into the certificate's **Subject Common Name (CN)**, for example, `CN=payment-service-producer`.
    *   **Broker Identity:** The broker's certificate contains its hostname and IP address in the **Subject Alternative Name (SAN)** fields to prevent spoofing.
3.  **Broker Configuration:** Each RustMQ broker is configured to trust the cluster's CA. When a client attempts to connect, the broker demands a certificate.
4.  **Connection-Time Verification (The Handshake):**
    *   The broker validates that the client's certificate was signed by the trusted CA.
    *   If validation succeeds, the connection is accepted.
    *   The broker then extracts the client's Principal (e.g., "payment-service-producer") from the certificate and associates it with that QUIC connection for its entire lifetime.

#### Why it's Performant

*   **Zero Per-Request Overhead:** The expensive cryptographic operations (public key signature verification) happen only **once** when the connection is established. Every subsequent request on that connection is implicitly authenticated. There are no tokens to parse or passwords to check on the hot path.
*   **Native to QUIC:** QUIC is built on TLS 1.3. mTLS is a standard feature, meaning we leverage the protocol's native, highly-optimized security capabilities.

### Layer 2: Authorization via Cached ACLs

Once a client is authenticated, we must determine what it is allowed to do.

#### How it Works

1.  **Centralized ACLs:** The **Controller** acts as the source of truth for all ACLs. An admin tool is used to define rules that are stored in the Controller's durable, replicated Raft log.
    *   **Example Rule:** `Principal: "CN=payment-service-producer", Topic: "payments", Permission: Write`
2.  **Broker-Side Caching:** To avoid costly RPCs to the controller on every request, each broker maintains an **in-memory cache of ACLs**.
3.  **The Authorization Flow (Hot Path):**
    a. A `ProduceRequest` for topic "payments" arrives on a connection authenticated as "payment-service-producer".
    b. The broker performs a lookup in its local ACL cache for the tuple `("payment-service-producer", "payments", Write)`.
    c. **Cache Hit:** The cache confirms the permission is allowed. The request is processed immediately. This is the common case and is extremely fast (nanoseconds).
    d. **Cache Miss:** If the entry is not in the cache, the broker makes a one-time RPC to the Controller to fetch the permissions for this principal and topic.
    e. The Controller responds with the permissions.
    f. The broker **caches the result** with a Time-To-Live (TTL) (e.g., 5 minutes) to serve future requests.
    g. The broker proceeds with the check as in a cache hit.

#### Why it's Performant

*   **Amortized Cost:** The network cost of fetching ACLs from the controller is amortized over thousands of requests.
*   **Minimal Latency:** The hot path for authorization is a local hash map lookup.

---

## Implementation Plan

### Step 1: Certificate Authority (CA) Management in `rustmq-admin`

The admin tool will be extended to manage the full lifecycle of certificates.

*   **New Command:** `rustmq-admin security ca create`
    *   **Action:** Generates a new CA key (`ca.key`) and a CA certificate (`ca.pem`). This is the root of trust for the cluster.
    *   **Implementation:** Use the `rcgen` crate to create a self-signed certificate with the `is_ca` flag set.

*   **New Command:** `rustmq-admin security certs create-broker --host <hostname> --ip <ip_address>`
    *   **Action:** Generates a server certificate (`broker.pem`) and key (`broker.key`) signed by the cluster CA.
    *   **Implementation:** Loads the CA, generates a new keypair, and creates a certificate with the broker's hostname/IP in the SAN fields for identity verification.

*   **New Command:** `rustmq-admin security certs create-client --principal <client_name>`
    *   **Action:** Generates a client certificate (`<client_name>.pem`) and key (`<client_name>.key`) signed by the cluster CA.
    *   **Implementation:** Loads the CA, generates a new keypair, and creates a certificate with the client's identity (`<client_name>`) in the Subject's Common Name (CN).

### Step 2: Update Broker Configuration (`config.rs`)

A new `TlsConfig` struct will be added to the `NetworkConfig`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ca_cert_path: String,
    pub server_cert_path: String,
    pub server_key_path: String,
}
```

### Step 3: Update QUIC Server to Enforce mTLS (`network/quic_server.rs`)

The `QuicServer` will be modified to load the TLS configuration and enforce client certificate validation.

*   The `create_server_config()` function will be updated to:
    1.  Load the server's certificate and private key from the configured paths.
    2.  Load the trusted CA certificate (`ca.pem`).
    3.  Create a `rustls::server::AllowAnyAuthenticatedClient` verifier, which requires that any connecting client present a certificate signed by the trusted CA.
    4.  Build the `quinn::ServerConfig` with this mTLS configuration.

### Step 4: Extract Client Identity on Connection

When a client connects, the broker must extract its identity (the Principal) from the certificate.

*   The `handle_connection` function in `quic_server.rs` will be modified:
    1.  After a connection is established, call `connection.peer_identity()`.
    2.  Downcast the identity to `Vec<rustls::Certificate>`.
    3.  Parse the first certificate in the chain using a library like `x509-parser`.
    4.  Extract the Common Name (CN) from the certificate's Subject field.
    5.  Store this Principal string in the `ConnectionPool` state associated with this specific connection.
    6.  Pass the Principal to the `RequestRouter` for every subsequent request on that connection.
