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

pub use client::RustMqClient;
pub use config::{ClientConfig, ProducerConfig, ConsumerConfig};
pub use error::{ClientError, Result};
pub use message::{Message, MessageBuilder};
pub use producer::{Producer, ProducerBuilder};
pub use consumer::{Consumer, ConsumerBuilder};
pub use stream::{MessageStream, StreamConfig};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_basic_client_creation() {
        let config = ClientConfig::default();
        let client = RustMqClient::new(config).await;
        assert!(client.is_ok());
    }
}