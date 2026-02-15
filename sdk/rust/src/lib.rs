//! RustMQ Client Library
//!
//! High-performance async client for RustMQ message queue system with QUIC transport.

pub mod admin;
pub mod client;
pub mod config;
pub mod connection;
pub mod consumer;
pub mod error;
pub mod message;
pub mod producer;
pub mod security;
pub mod stream;
pub mod types;

pub use admin::{
    AclEvaluationRequest, AclEvaluationResponse, AclRuleResponse, AdminClient, AdminConfig,
    AuditLogEntry, AuditLogRequest, AuditLogResponse, BrokerStatus, CaInfo, CertificateDetail,
    CertificateListItem, ClusterStatus, CreateAclRuleRequest, CreateTopicRequest,
    GenerateCaRequest, HealthResponse, IssueCertificateRequest, RevokeCertificateRequest,
    TopicDetail, TopicSummary,
};
pub use client::RustMqClient;
pub use config::{
    AclClientConfig, AuthConfig, AuthMethod, CertificateClientConfig, CertificateTemplate,
    CertificateValidationConfig, ClientConfig, ConsumerConfig, PrincipalExtractionConfig,
    ProducerConfig, SecurityConfig, TlsConfig, TlsMode,
};
pub use connection::Connection;
pub use consumer::{Consumer, ConsumerBuilder};
pub use error::{ClientError, Result};
pub use message::{Message, MessageBuilder};
pub use producer::{Producer, ProducerBuilder};
pub use security::{
    AclCache, CertificateInfo, CertificateValidator, PermissionSet, Principal, PrincipalExtractor,
    SecurityContext, SecurityManager, SecurityMetrics,
};
pub use stream::{MessageStream, StreamConfig};
pub use types::*;

/// Initialize rustls crypto provider for integration tests
///
/// This function must be called before creating any QUIC connections when using the
/// rustmq-client SDK in integration tests. It initializes the rustls 0.23 crypto provider
/// with aws_lc_rs backend.
///
/// This function is idempotent and can be safely called multiple times - only the first
/// call will perform initialization.
///
/// # Example
/// ```no_run
/// rustmq_client::init_crypto_provider();
/// // Now safe to create RustMqClient instances
/// ```
pub fn init_crypto_provider() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_client_config_validation() {
        // Test client configuration validation (no network calls)
        let config = ClientConfig::default();

        // Verify default configuration values
        assert_eq!(config.brokers, vec!["localhost:9092".to_string()]);
        assert_eq!(config.client_id, None);
        assert_eq!(config.enable_tls, false);
        assert_eq!(config.max_connections, 10);

        // Test custom configuration creation
        let custom_config = ClientConfig {
            brokers: vec!["broker1:9092".to_string(), "broker2:9092".to_string()],
            client_id: Some("test-client".to_string()),
            enable_tls: true,
            max_connections: 20,
            ..Default::default()
        };

        assert_eq!(custom_config.brokers.len(), 2);
        assert_eq!(custom_config.client_id, Some("test-client".to_string()));
        assert_eq!(custom_config.enable_tls, true);
        assert_eq!(custom_config.max_connections, 20);
    }
}
