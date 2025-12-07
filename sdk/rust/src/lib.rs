//! RustMQ Client Library
//! 
//! High-performance async client for RustMQ message queue system with QUIC transport.

pub mod client;
pub mod config;
pub mod connection;
pub mod error;
pub mod message;
pub mod producer;
pub mod consumer;
pub mod stream;
pub mod types;
pub mod security;
pub mod admin;

pub use client::RustMqClient;
pub use config::{
    ClientConfig, ProducerConfig, ConsumerConfig, TlsConfig, TlsMode,
    AuthConfig, AuthMethod, SecurityConfig, CertificateValidationConfig,
    PrincipalExtractionConfig, AclClientConfig, CertificateClientConfig,
    CertificateTemplate,
};
pub use error::{ClientError, Result};
pub use message::{Message, MessageBuilder};
pub use producer::{Producer, ProducerBuilder};
pub use consumer::{Consumer, ConsumerBuilder};
pub use stream::{MessageStream, StreamConfig};
pub use types::*;
pub use connection::Connection;
pub use security::{
    SecurityManager, SecurityContext, CertificateInfo, PermissionSet,
    Principal, PrincipalExtractor, CertificateValidator, AclCache,
    SecurityMetrics,
};
pub use admin::{
    AdminClient, AdminConfig, HealthResponse, BrokerStatus, ClusterStatus,
    TopicSummary, CreateTopicRequest, TopicDetail, GenerateCaRequest,
    CaInfo, IssueCertificateRequest, CertificateListItem, CertificateDetail,
    RevokeCertificateRequest, CreateAclRuleRequest, AclRuleResponse,
    AclEvaluationRequest, AclEvaluationResponse, AuditLogEntry,
    AuditLogRequest, AuditLogResponse,
};

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