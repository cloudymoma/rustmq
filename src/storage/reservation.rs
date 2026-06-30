//! Per-partition epoch reservation for object-level (split-brain) fencing.
//!
//! Each partition has a small reservation object in object storage holding the current
//! leader's node id and epoch. A new leader bumps the epoch with a conditional
//! (compare-and-swap) write; the tiering loop verifies it still holds the reservation
//! before uploading a segment and self-fences on mismatch. This prevents a stale leader
//! (e.g. after a network partition) from clobbering the real leader's cold data.

use crate::storage::traits::{ObjectStorage, Precondition, PutOutcome};
use crate::types::TopicPartition;
use crate::{Result, error::RustMqError};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Magic prefix identifying a reservation object ("RMQR").
const RESERVATION_MAGIC: u32 = 0x524D_5152;
/// Bounded retries when a concurrent writer keeps winning the CAS race.
const MAX_CAS_RETRIES: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Reservation {
    magic: u32,
    node_id: String,
    epoch: u64,
    /// Monotonic takeover counter (incremented on each epoch bump); diagnostic.
    failover: u64,
}

/// Outcome of attempting to acquire/refresh a partition reservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireOutcome {
    /// We now hold the reservation at the requested epoch.
    Owned,
    /// A newer leader (higher epoch, or equal epoch held by another node) owns it; this
    /// node must not act as leader for the partition.
    Fenced { current_epoch: u64 },
}

/// Owns the conditional-write protocol over the reservation objects. Stateless beyond
/// the object storage handle: `verify` always re-reads so a takeover is observed.
pub struct ReservationService {
    storage: Arc<dyn ObjectStorage>,
}

impl ReservationService {
    pub fn new(storage: Arc<dyn ObjectStorage>) -> Self {
        Self { storage }
    }

    fn key(tp: &TopicPartition) -> String {
        format!("topics/{}/{}/_epoch", tp.topic, tp.partition)
    }

    /// Acquire or bump the reservation for `tp` at `epoch` on behalf of `node_id`:
    /// - no reservation yet → create it;
    /// - stored epoch `<` `epoch` → CAS-bump to us;
    /// - stored epoch `==` `epoch` and same node → idempotent re-acquire;
    /// - otherwise (stored epoch `>` `epoch`, or `==` held by another node) → `Fenced`.
    pub async fn acquire(
        &self,
        tp: &TopicPartition,
        node_id: &str,
        epoch: u64,
    ) -> Result<AcquireOutcome> {
        let key = Self::key(tp);
        for _ in 0..MAX_CAS_RETRIES {
            match self.storage.get_versioned(&key).await {
                Err(RustMqError::NotFound(_)) => {
                    // No reservation yet: create one (succeeds only if still absent).
                    let body = encode(node_id, epoch, 0)?;
                    match self
                        .storage
                        .put_if(&key, body, Precondition::Create)
                        .await?
                    {
                        PutOutcome::Written(_) => return Ok(AcquireOutcome::Owned),
                        // Someone created it first; loop to read and re-evaluate.
                        PutOutcome::PreconditionFailed => continue,
                    }
                }
                Ok((bytes, version)) => {
                    let current = decode(&bytes)?;
                    if current.epoch > epoch
                        || (current.epoch == epoch && current.node_id != node_id)
                    {
                        return Ok(AcquireOutcome::Fenced {
                            current_epoch: current.epoch,
                        });
                    }
                    if current.epoch == epoch && current.node_id == node_id {
                        return Ok(AcquireOutcome::Owned); // already ours at this epoch
                    }
                    // current.epoch < epoch → bump via CAS against the version we read.
                    let body = encode(node_id, epoch, current.failover + 1)?;
                    match self
                        .storage
                        .put_if(&key, body, Precondition::Match(version))
                        .await?
                    {
                        PutOutcome::Written(_) => return Ok(AcquireOutcome::Owned),
                        // Lost the race to another writer; loop to re-read.
                        PutOutcome::PreconditionFailed => continue,
                    }
                }
                Err(e) => return Err(e),
            }
        }
        // Reservation is churning under us: never report success. Report the latest epoch
        // we can see so the caller knows it is fenced.
        let current_epoch = match self.storage.get_versioned(&key).await {
            Ok((bytes, _)) => decode(&bytes).map(|r| r.epoch).unwrap_or(epoch),
            _ => epoch,
        };
        Ok(AcquireOutcome::Fenced { current_epoch })
    }

    /// True iff this node still holds `tp`'s reservation at exactly `epoch`. Re-reads the
    /// object so a takeover that happened after `acquire` is detected (the fence check
    /// run before each upload).
    pub async fn verify(&self, tp: &TopicPartition, node_id: &str, epoch: u64) -> Result<bool> {
        let key = Self::key(tp);
        match self.storage.get_versioned(&key).await {
            Ok((bytes, _)) => {
                let current = decode(&bytes)?;
                Ok(current.node_id == node_id && current.epoch == epoch)
            }
            Err(RustMqError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

fn encode(node_id: &str, epoch: u64, failover: u64) -> Result<Bytes> {
    let r = Reservation {
        magic: RESERVATION_MAGIC,
        node_id: node_id.to_string(),
        epoch,
        failover,
    };
    let bytes = bincode::serialize(&r)
        .map_err(|e| RustMqError::Storage(format!("reservation encode: {e}")))?;
    Ok(Bytes::from(bytes))
}

fn decode(bytes: &[u8]) -> Result<Reservation> {
    let r: Reservation = bincode::deserialize(bytes)
        .map_err(|e| RustMqError::Storage(format!("reservation decode: {e}")))?;
    if r.magic != RESERVATION_MAGIC {
        return Err(RustMqError::Storage("reservation magic mismatch".into()));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::cloud_storage::CloudObjectStorage;
    use object_store::memory::InMemory;

    fn shared_storage() -> Arc<dyn ObjectStorage> {
        Arc::new(CloudObjectStorage::new(Arc::new(InMemory::new())))
    }

    fn tp() -> TopicPartition {
        TopicPartition {
            topic: "t".to_string(),
            partition: 0,
        }
    }

    #[tokio::test]
    async fn test_split_brain_fencing() {
        // Two nodes share one object store (simulating GCS).
        let store = shared_storage();
        let a = ReservationService::new(store.clone());
        let b = ReservationService::new(store.clone());
        let tp = tp();

        // A becomes leader at epoch 5.
        assert_eq!(
            a.acquire(&tp, "node-a", 5).await.unwrap(),
            AcquireOutcome::Owned
        );
        assert!(a.verify(&tp, "node-a", 5).await.unwrap());

        // B takes over at epoch 6 (CAS-bumps the reservation).
        assert_eq!(
            b.acquire(&tp, "node-b", 6).await.unwrap(),
            AcquireOutcome::Owned
        );
        assert!(b.verify(&tp, "node-b", 6).await.unwrap());

        // A is now fenced: its verify fails and a stale re-acquire is rejected.
        assert!(!a.verify(&tp, "node-a", 5).await.unwrap());
        assert_eq!(
            a.acquire(&tp, "node-a", 5).await.unwrap(),
            AcquireOutcome::Fenced { current_epoch: 6 }
        );
    }

    #[tokio::test]
    async fn test_reacquire_same_epoch_is_idempotent() {
        let store = shared_storage();
        let a = ReservationService::new(store);
        let tp = tp();
        assert_eq!(
            a.acquire(&tp, "node-a", 3).await.unwrap(),
            AcquireOutcome::Owned
        );
        // Same node, same epoch → still owned, no error.
        assert_eq!(
            a.acquire(&tp, "node-a", 3).await.unwrap(),
            AcquireOutcome::Owned
        );
    }
}
