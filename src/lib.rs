pub mod admin;
pub mod broker;
pub mod config;
pub mod controller;
pub mod error;
pub mod etl;
pub mod health;
pub mod metrics;
pub mod network;
pub mod operations;
pub mod panic_handler;
pub mod proto;
pub mod proto_convert;
pub mod replication;
pub mod scaling;
pub mod security;
pub mod storage;
pub mod subscribers;
pub mod types;

pub use config::Config;
pub use error::{Result, RustMqError};

// Re-export protobuf types and services for convenience
pub use proto::broker as proto_broker;
pub use proto::common;
pub use proto::controller as proto_controller;

// Re-export protobuf service implementations
pub use network::grpc_server::BrokerReplicationServiceImpl;

// Re-export conversion utilities
pub use proto_convert::{ConversionError, error_is_retryable, error_to_code, error_to_message};
