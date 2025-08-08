//! TLS Configuration
//!
//! Configuration structures for TLS/mTLS setup in RustMQ.

use serde::{Deserialize, Serialize};

/// TLS configuration for RustMQ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Whether TLS is enabled
    pub enabled: bool,
    
    /// TLS mode (server, client, mutual)
    pub mode: TlsMode,
    
    /// Server-side TLS configuration
    pub server: Option<TlsServerConfig>,
    
    /// Client-side TLS configuration
    pub client: Option<TlsClientConfig>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: TlsMode::Mutual,
            server: None,
            client: None,
        }
    }
}

/// TLS mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TlsMode {
    /// Server-side TLS only
    Server,
    /// Client-side TLS only
    Client,
    /// Mutual TLS (both sides authenticate)
    Mutual,
}

/// Server-side TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsServerConfig {
    /// Path to server certificate file
    pub cert_path: String,
    
    /// Path to server private key file
    pub key_path: String,
    
    /// Path to CA certificate file (for client verification)
    pub ca_cert_path: Option<String>,
    
    /// Whether to require client certificates
    pub require_client_cert: bool,
    
    /// Supported TLS versions
    pub supported_versions: Vec<String>,
    
    /// Supported cipher suites
    pub cipher_suites: Vec<String>,
}

impl Default for TlsServerConfig {
    fn default() -> Self {
        Self {
            cert_path: "/etc/rustmq/server.pem".to_string(),
            key_path: "/etc/rustmq/server.key".to_string(),
            ca_cert_path: Some("/etc/rustmq/ca.pem".to_string()),
            require_client_cert: true,
            supported_versions: vec!["TLSv1.3".to_string(), "TLSv1.2".to_string()],
            cipher_suites: Vec::new(), // Use rustls defaults
        }
    }
}

/// Client-side TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsClientConfig {
    /// Path to client certificate file (for mTLS)
    pub cert_path: Option<String>,
    
    /// Path to client private key file (for mTLS)
    pub key_path: Option<String>,
    
    /// Path to CA certificate file (for server verification)
    pub ca_cert_path: String,
    
    /// Whether to verify server certificates
    pub verify_server_cert: bool,
    
    /// Server name for SNI and certificate verification
    pub server_name: Option<String>,
    
    /// Supported TLS versions
    pub supported_versions: Vec<String>,
}

impl Default for TlsClientConfig {
    fn default() -> Self {
        Self {
            cert_path: None,
            key_path: None,
            ca_cert_path: "/etc/rustmq/ca.pem".to_string(),
            verify_server_cert: true,
            server_name: None,
            supported_versions: vec!["TLSv1.3".to_string(), "TLSv1.2".to_string()],
        }
    }
}