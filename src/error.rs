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
    PartitionNotFound(u32),

    #[error("Broker not found: {0}")]
    BrokerNotFound(String),

    #[error("Operation timeout")]
    Timeout,

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
}