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