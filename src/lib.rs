pub mod broker;
pub mod config;
pub mod error;
pub mod storage;
pub mod replication;
pub mod network;
pub mod controller;
pub mod etl;
pub mod admin;
pub mod metrics;
pub mod types;
pub mod subscribers;
pub mod scaling;
pub mod operations;
pub mod security;
pub mod proto;
pub mod proto_convert;

pub use error::{RustMqError, Result};
pub use config::Config;

// Re-export protobuf types and services for convenience
pub use proto::common;
pub use proto::broker as proto_broker;
pub use proto::controller as proto_controller;

// Re-export protobuf service implementations
pub use network::grpc_server::BrokerReplicationServiceImpl;

// Re-export conversion utilities
pub use proto_convert::{ConversionError, error_to_code, error_to_message, error_is_retryable};