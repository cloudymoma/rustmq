use thiserror::Error;

/// Result type alias for RustMQ client operations
pub type Result<T> = std::result::Result<T, ClientError>;

/// Errors that can occur in the RustMQ client
#[derive(Error, Debug, Clone)]
pub enum ClientError {
    /// Connection-related errors
    #[error("Connection error: {0}")]
    Connection(String),

    /// No connections available
    #[error("No connections available")]
    NoConnectionsAvailable,

    /// Authentication failed
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Message serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Message deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Broker error
    #[error("Broker error: {0}")]
    Broker(String),

    /// Network timeout
    #[error("Operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Topic not found
    #[error("Topic not found: {topic}")]
    TopicNotFound { topic: String },

    /// Partition not found
    #[error("Partition not found: topic={topic}, partition={partition}")]
    PartitionNotFound { topic: String, partition: u32 },

    /// Producer errors
    #[error("Producer error: {0}")]
    Producer(String),

    /// Consumer errors
    #[error("Consumer error: {0}")]
    Consumer(String),

    /// Message too large
    #[error("Message too large: {size} bytes, max allowed: {max_size} bytes")]
    MessageTooLarge { size: usize, max_size: usize },

    /// Offset out of range
    #[error("Offset out of range: {offset}")]
    OffsetOutOfRange { offset: u64 },

    /// Consumer group errors
    #[error("Consumer group error: {0}")]
    ConsumerGroup(String),

    /// Rebalancing in progress
    #[error("Consumer group rebalancing in progress")]
    RebalancingInProgress,

    /// Invalid message format
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    /// Compression/decompression error
    #[error("Compression error: {0}")]
    Compression(String),

    /// TLS/Security related errors
    #[error("TLS error: {0}")]
    Tls(String),

    /// Stream processing error
    #[error("Stream error: {0}")]
    Stream(String),

    /// Resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },

    /// Internal client error
    #[error("Internal error: {0}")]
    Internal(String),

    /// QUIC transport error
    #[error("QUIC transport error: {0}")]
    QuicTransport(String),

    /// Protocol error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Broker not available
    #[error("Broker not available: {broker}")]
    BrokerNotAvailable { broker: String },

    /// Rate limiting error
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Security-related errors
    #[error("Authorization denied: {0}")]
    AuthorizationDenied(String),

    /// Invalid certificate
    #[error("Invalid certificate: {reason}")]
    InvalidCertificate { reason: String },

    /// Principal extraction error
    #[error("Principal extraction failed: {0}")]
    PrincipalExtraction(String),

    /// Unsupported authentication method
    #[error("Unsupported authentication method: {method}")]
    UnsupportedAuthMethod { method: String },

    /// ACL operation failed
    #[error("ACL operation failed: {0}")]
    AclOperation(String),

    /// Certificate management error
    #[error("Certificate management error: {0}")]
    CertificateManagement(String),
}

impl From<std::io::Error> for ClientError {
    fn from(err: std::io::Error) -> Self {
        ClientError::Connection(err.to_string())
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(err: serde_json::Error) -> Self {
        ClientError::Serialization(err.to_string())
    }
}

impl From<quinn::ConnectionError> for ClientError {
    fn from(err: quinn::ConnectionError) -> Self {
        ClientError::QuicTransport(err.to_string())
    }
}

impl From<quinn::ConnectError> for ClientError {
    fn from(err: quinn::ConnectError) -> Self {
        ClientError::Connection(err.to_string())
    }
}

impl From<tokio::time::error::Elapsed> for ClientError {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        ClientError::Timeout { timeout_ms: 0 }
    }
}

impl From<quinn::ReadExactError> for ClientError {
    fn from(err: quinn::ReadExactError) -> Self {
        ClientError::QuicTransport(format!("Read error: {}", err))
    }
}

impl From<quinn::WriteError> for ClientError {
    fn from(err: quinn::WriteError) -> Self {
        ClientError::QuicTransport(format!("Write error: {}", err))
    }
}

impl From<quinn::ClosedStream> for ClientError {
    fn from(err: quinn::ClosedStream) -> Self {
        ClientError::QuicTransport(format!("Stream closed: {}", err))
    }
}

/// Error categories for metrics and monitoring
impl ClientError {
    /// Get the error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            ClientError::Connection(_) | ClientError::NoConnectionsAvailable => "connection",
            ClientError::Authentication(_) => "authentication",
            ClientError::InvalidConfig(_) => "configuration",
            ClientError::Serialization(_) | ClientError::Deserialization(_) => "serialization",
            ClientError::Broker(_) => "broker",
            ClientError::Timeout { .. } => "timeout",
            ClientError::TopicNotFound { .. } | ClientError::PartitionNotFound { .. } => "not_found",
            ClientError::Producer(_) => "producer",
            ClientError::Consumer(_) | ClientError::ConsumerGroup(_) => "consumer",
            ClientError::MessageTooLarge { .. } => "message_size",
            ClientError::OffsetOutOfRange { .. } => "offset",
            ClientError::RebalancingInProgress => "rebalancing",
            ClientError::InvalidMessage(_) => "message_format",
            ClientError::Compression(_) => "compression",
            ClientError::Tls(_) => "tls",
            ClientError::Stream(_) => "stream",
            ClientError::ResourceExhausted { .. } => "resource_exhausted",
            ClientError::Internal(_) => "internal",
            ClientError::QuicTransport(_) => "transport",
            ClientError::Protocol(_) => "protocol",
            ClientError::BrokerNotAvailable { .. } => "broker_unavailable",
            ClientError::RateLimitExceeded(_) => "rate_limit",
            ClientError::InvalidOperation(_) => "invalid_operation",
            ClientError::AuthorizationDenied(_) => "authorization",
            ClientError::InvalidCertificate { .. } => "certificate",
            ClientError::PrincipalExtraction(_) => "principal",
            ClientError::UnsupportedAuthMethod { .. } => "auth_method",
            ClientError::AclOperation(_) => "acl",
            ClientError::CertificateManagement(_) => "cert_management",
        }
    }

    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            ClientError::Connection(_) 
            | ClientError::NoConnectionsAvailable
            | ClientError::Timeout { .. }
            | ClientError::BrokerNotAvailable { .. }
            | ClientError::ResourceExhausted { .. }
            | ClientError::QuicTransport(_)
            | ClientError::RebalancingInProgress
            | ClientError::Broker(_) => true,
            
            ClientError::Authentication(_)
            | ClientError::InvalidConfig(_)
            | ClientError::MessageTooLarge { .. }
            | ClientError::InvalidMessage(_)
            | ClientError::InvalidOperation(_)
            | ClientError::AuthorizationDenied(_)
            | ClientError::InvalidCertificate { .. }
            | ClientError::PrincipalExtraction(_)
            | ClientError::UnsupportedAuthMethod { .. } => false,
            
            _ => false,
        }
    }
}