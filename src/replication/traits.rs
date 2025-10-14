use crate::{Result, types::*};
use async_trait::async_trait;

#[async_trait]
pub trait ReplicationManager: Send + Sync {
    /// Replicate a record to followers (now accepts reference to avoid cloning)
    async fn replicate_record(&self, record: &WalRecord) -> Result<ReplicationResult>;
    async fn add_follower(&self, broker_id: BrokerId) -> Result<()>;
    async fn remove_follower(&self, broker_id: BrokerId) -> Result<()>;
    async fn get_follower_states(&self) -> Result<Vec<FollowerState>>;
    async fn update_high_watermark(&self, offset: Offset) -> Result<()>;
    async fn get_high_watermark(&self) -> Result<Offset>;
}