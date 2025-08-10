use thiserror::Error;

pub type Result<T> = std::result::Result<T, RustMqError>;

#[derive(Error, Debug)]
pub enum RustMqError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] Box<bincode::ErrorKind>),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Replication error: {0}")]
    Replication(String),

    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Wal error: {0}")]
    Wal(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Upload error: {0}")]
    Upload(String),

    #[error("ETL error: {0}")]
    Etl(String),

    #[error("Bandwidth error: {0}")]
    Bandwidth(String),

    #[error("Balance error: {0}")]
    Balance(String),

    #[error("Sandbox error: {0}")]
    Sandbox(String),

    #[error("Admin API error: {0}")]
    AdminApi(String),

    #[error("Buffer too small")]
    BufferTooSmall,

    #[error("Invalid offset: {0}")]
    InvalidOffset(u64),

    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    #[error("Partition not found: {0}")]
    PartitionNotFound(String),

    #[error("Broker not found: {0}")]
    BrokerNotFound(String),

    #[error("Operation timeout")]
    OperationTimeout,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),

    #[error("Certificate error: {0}")]
    Certificate(#[from] rcgen::RcgenError),

    #[error("QUIC error: {0}")]
    Quic(#[from] quinn::ConnectError),

    #[error("QUIC connection error: {0}")]
    QuicConnection(#[from] quinn::ConnectionError),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("No leader found")]
    NoLeader,

    #[error("Replication failed to broker {broker_id}: {error_message}")]
    ReplicationFailed {
        broker_id: String,
        error_message: String,
    },

    #[error("Stale leader epoch: request epoch {request_epoch}, current epoch {current_epoch}")]
    StaleLeaderEpoch {
        request_epoch: u64,
        current_epoch: u64,
    },

    #[error("ETL processing failed: {0}")]
    EtlProcessingFailed(String),

    #[error("ETL module not found: {0}")]
    EtlModuleNotFound(String),

    #[error("Topic already exists: {0}")]
    TopicAlreadyExists(String),

    #[error("Not leader for partition: {0}")]
    NotLeader(String),

    #[error("Offset out of range: {0}")]
    OffsetOutOfRange(String),

    #[error("Object not found: {0}")]
    ObjectNotFound(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Invalid URI: {0}")]
    InvalidUri(String),

    // Security-related errors
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authorization denied: principal '{principal}' lacks '{permission}' on '{resource}'")]
    AuthorizationDenied {
        principal: String,
        permission: String,
        resource: String,
    },

    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

    #[error("Certificate revoked: {subject}")]
    CertificateRevoked { subject: String },

    #[error("Certificate expired: {subject}")]
    CertificateExpired { subject: String },

    #[error("Invalid certificate: {reason}")]
    InvalidCertificate { reason: String },

    #[error("Certificate not found: {identifier}")]
    CertificateNotFound { identifier: String },

    #[error("Certificate generation failed: {reason}")]
    CertificateGeneration { reason: String },

    #[error("Certificate validation failed: {reason}")]
    CertificateValidation { reason: String },

    #[error("CA not available: {0}")]
    CaNotAvailable(String),

    #[error("Security configuration error: {0}")]
    SecurityConfig(String),

    #[error("Cache operation failed: {0}")]
    CacheOperation(String),

    #[error("Principal extraction failed: {0}")]
    PrincipalExtraction(String),

    #[error("ACL evaluation failed: {0}")]
    AclEvaluation(String),

    #[error("ACL rule not found: {rule_id}")]
    AclRuleNotFound { rule_id: String },

    #[error("ACL rule conflict: {reason}")]
    AclRuleConflict { reason: String },

    #[error("Invalid ACL pattern: {pattern}")]
    InvalidAclPattern { pattern: String },

    #[error("Security policy violation: {violation}")]
    SecurityPolicyViolation { violation: String },

    #[error("Rate limit exceeded for principal: {principal}")]
    RateLimitExceeded { principal: String },

    #[error("Session expired for principal: {principal}")]
    SessionExpired { principal: String },

    #[error("TLS handshake failed: {reason}")]
    TlsHandshakeFailed { reason: String },

    #[error("Signature verification failed: {reason}")]
    SignatureVerificationFailed { reason: String },

    #[error("Cryptographic operation failed: {operation}")]
    CryptographicFailure { operation: String },

    #[error("Revocation check failed: {reason}")]
    RevocationCheckFailed { reason: String },

    #[error("Certificate chain validation failed: {reason}")]
    CertificateChainValidation { reason: String },

    #[error("Security audit log error: {0}")]
    SecurityAuditLog(String),

    #[error("Insufficient privileges for operation: {operation}")]
    InsufficientPrivileges { operation: String },

    #[error("Timeout occurred: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    // OpenRaft specific errors
    #[error("Raft error: {0}")]
    RaftError(String),

    #[error("Raft storage error: {0}")]
    RaftStorage(String),

    #[error("Raft network error: {0}")]
    RaftNetwork(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Consensus timeout: {0}")]
    ConsensusTimeout(String),

    #[error("Log compaction error: {0}")]
    LogCompaction(String),

    #[error("Snapshot error: {0}")]
    SnapshotError(String),

    #[error("Leader election failed: {0}")]
    LeaderElection(String),

    #[error("Membership change failed: {0}")]
    MembershipChange(String),
}

// Add missing From implementations for tonic transport errors
impl From<tonic::transport::Error> for RustMqError {
    fn from(e: tonic::transport::Error) -> Self {
        RustMqError::Transport(e.to_string())
    }
}

impl From<warp::http::uri::InvalidUri> for RustMqError {
    fn from(e: warp::http::uri::InvalidUri) -> Self {
        RustMqError::InvalidUri(e.to_string())
    }
}

// Add From implementations for security-related errors
impl From<x509_parser::nom::Err<x509_parser::error::X509Error>> for RustMqError {
    fn from(e: x509_parser::nom::Err<x509_parser::error::X509Error>) -> Self {
        RustMqError::CertificateValidation {
            reason: format!("X.509 parsing error: {}", e),
        }
    }
}

impl From<x509_parser::error::X509Error> for RustMqError {
    fn from(e: x509_parser::error::X509Error) -> Self {
        RustMqError::CertificateValidation {
            reason: format!("X.509 error: {}", e),
        }
    }
}

impl From<webpki::Error> for RustMqError {
    fn from(e: webpki::Error) -> Self {
        RustMqError::CertificateValidation {
            reason: format!("WebPKI error: {:?}", e),
        }
    }
}

impl From<ring::error::Unspecified> for RustMqError {
    fn from(_e: ring::error::Unspecified) -> Self {
        RustMqError::CryptographicFailure {
            operation: "Ring cryptographic operation failed".to_string(),
        }
    }
}

impl From<ring::error::KeyRejected> for RustMqError {
    fn from(e: ring::error::KeyRejected) -> Self {
        RustMqError::CryptographicFailure {
            operation: format!("Ring key rejected: {}", e),
        }
    }
}

// Note: rustls_pemfile::Error removed as it's not available in this version

impl From<serde_json::Error> for RustMqError {
    fn from(e: serde_json::Error) -> Self {
        RustMqError::Config(format!("JSON serialization error: {}", e))
    }
}

impl From<hyper::Error> for RustMqError {
    fn from(e: hyper::Error) -> Self {
        RustMqError::Network(format!("HTTP client error: {}", e))
    }
}

impl From<hyper::http::Error> for RustMqError {
    fn from(e: hyper::http::Error) -> Self {
        RustMqError::Network(format!("HTTP request error: {}", e))
    }
}

impl From<Box<dyn std::error::Error>> for RustMqError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        RustMqError::Internal(format!("Internal error: {}", e))
    }
}