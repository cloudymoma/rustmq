use async_trait::async_trait;
use crate::error::Result;
use crate::types::BrokerId;

/// Network handler trait for broker-to-broker communication
#[async_trait]
pub trait NetworkHandler {
    /// Send a request to a specific broker
    async fn send_request(&self, broker_id: &BrokerId, request: Vec<u8>) -> Result<Vec<u8>>;
    
    /// Broadcast a message to multiple brokers
    async fn broadcast(&self, brokers: &[BrokerId], request: Vec<u8>) -> Result<()>;
}