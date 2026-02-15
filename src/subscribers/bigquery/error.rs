use thiserror::Error;

#[derive(Error, Debug)]
pub enum BigQueryError {
    #[error("BigQuery client error: {0}")]
    Client(#[from] gcp_bigquery_client::error::BQError),

    #[error("Google Cloud authentication error: {0}")]
    Auth(#[from] google_cloud_auth::error::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),

    #[error("Message transformation error: {0}")]
    Transformation(String),

    #[error("BigQuery quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("Table not found: {project}.{dataset}.{table}")]
    TableNotFound {
        project: String,
        dataset: String,
        table: String,
    },

    #[error("Streaming insert failed: {0}")]
    StreamingInsertFailed(String),

    #[error("Storage write API error: {0}")]
    StorageWriteError(String),

    #[error("RustMQ error: {0}")]
    RustMq(#[from] crate::error::RustMqError),
}

pub type Result<T> = std::result::Result<T, BigQueryError>;
