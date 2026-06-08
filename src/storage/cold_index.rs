//! Broker-local durable cold-index manifest.
//!
//! The in-memory cold index (`PartitionIndex.cold`) maps a partition's tiered
//! offset ranges to their object-storage keys. It is rebuilt from the WAL only for
//! *un-tiered* (hot) records; once a segment is tiered its WAL file is reclaimed, so
//! without a durable record the offset→object mapping is lost on restart and the
//! cold data becomes unreadable. This manifest is that durable record.
//!
//! ## Why broker-local (not the controller's Raft)
//! Each replica receives records via replication and runs its **own** tiering, which
//! produces the **same** deterministic object keys. So a broker-local manifest makes
//! cold data readable again after (a) a same-broker restart and (b) failover to an
//! in-sync replica that tiered independently. (Backfilling a brand-new replica that
//! never saw the old segments needs a shared store / catch-up transfer — an existing
//! limitation, out of scope here.)
//!
//! ## Format & crash-safety
//! Append-only log of length-prefixed, CRC-checked frames:
//! `[u32 LE body_len][bincode(ColdSegment) body][u32 LE crc32(body)]`.
//! [`register`](ColdIndexManifest::register) appends one frame and `fsync`s before
//! returning, so the caller can safely evict the in-memory hot copy afterwards.
//! [`load`](ColdIndexManifest::load) replays frames up to the first torn or corrupt one
//! (a partially-written tail from a crash) and **truncates that tail off** so later
//! appends are not stranded behind it. A `register` that did not fully persist means its
//! WAL segment was never reclaimed, so recovery re-indexes it hot and the next tiering
//! cycle re-uploads + re-registers it idempotently — no data or index loss. Duplicate
//! frames are harmless: replay is idempotent.

use crate::Result;
use crate::types::{Offset, TopicPartition};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// A tiered, contiguous offset range `[start_offset, end_offset)` of one partition,
/// stored as a single object. Mirrors the in-memory cold entry, plus the partition
/// (the manifest is flat across all of this broker's partitions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColdSegment {
    pub topic_partition: TopicPartition,
    pub start_offset: Offset,
    pub end_offset: Offset,
    pub object_key: String,
}

/// Append-only, fsync-durable manifest of this broker's tiered segments.
pub struct ColdIndexManifest {
    path: PathBuf,
    /// Append handle, held open across `register`s. A `Mutex` serializes concurrent
    /// tiering registrations so frames are never interleaved.
    file: Mutex<File>,
}

impl ColdIndexManifest {
    /// Open (creating if absent) the manifest at `path`. The parent directory must
    /// already exist (it is the broker's WAL/data directory).
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .await?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    /// Durably append one tiered-segment record. Returns only after the frame is
    /// `fsync`ed, so the caller may then evict the in-memory hot copy of the range.
    pub async fn register(
        &self,
        tp: &TopicPartition,
        start_offset: Offset,
        end_offset: Offset,
        object_key: &str,
    ) -> Result<()> {
        let seg = ColdSegment {
            topic_partition: tp.clone(),
            start_offset,
            end_offset,
            object_key: object_key.to_string(),
        };
        let body = bincode::serialize(&seg)?;
        let crc = crc32fast::hash(&body);

        // Frame: [len][body][crc]. Assembled into one buffer so a single write_all
        // emits the whole frame (no partial-frame interleaving with other writers).
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        frame.extend_from_slice(&crc.to_le_bytes());

        let mut file = self.file.lock().await;
        file.write_all(&frame).await?;
        file.sync_all().await?;
        Ok(())
    }

    /// Replay all durably-written segments, in append order, returning everything up to
    /// the first torn or CRC-mismatched frame, then **truncate that torn tail off the
    /// file**.
    ///
    /// Truncation is essential, not cosmetic: recovery after a crash re-tiers the
    /// orphaned WAL segment and `register`s the result, which appends *after* whatever
    /// was at the tail. If a torn frame were merely skipped (not removed), the new entry
    /// would sit behind it and the next `load` would stop at the torn frame and lose the
    /// new entry — making cold data tiered after the crash unreadable. Removing the torn
    /// tail here (before any `register`) guarantees later appends land in a clean file.
    ///
    /// Must be called once at startup, before any `register` — it mutates the file and
    /// assumes no concurrent appends.
    pub async fn load(&self) -> Result<Vec<ColdSegment>> {
        let bytes = match tokio::fs::read(&self.path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let mut out = Vec::new();
        let mut pos = 0usize;
        while pos + 4 <= bytes.len() {
            let len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            let body_start = pos + 4;
            // `checked_add` guards against a corrupt length header overflowing `usize`
            // (only reachable on 32-bit, but a cheap correctness backstop).
            let Some(crc_start) = body_start.checked_add(len) else {
                break;
            };
            let Some(frame_end) = crc_start.checked_add(4) else {
                break;
            };
            if frame_end > bytes.len() {
                break; // torn tail: incomplete body or crc
            }
            let body = &bytes[body_start..crc_start];
            let stored_crc = u32::from_le_bytes(bytes[crc_start..frame_end].try_into().unwrap());
            if crc32fast::hash(body) != stored_crc {
                break; // corrupt/torn frame
            }
            match bincode::deserialize::<ColdSegment>(body) {
                Ok(seg) => out.push(seg),
                Err(_) => break, // unparseable frame
            }
            pos = frame_end;
        }

        // Remove any torn/corrupt tail so subsequent appends are not stranded behind it.
        // Fail loud if truncation fails: silently leaving the tail would resurrect the
        // "later appends lost on next restart" bug.
        if pos < bytes.len() {
            let file = self.file.lock().await;
            file.set_len(pos as u64).await?;
            file.sync_all().await?;
            tracing::warn!(
                "ColdIndexManifest: truncated {} byte(s) of torn/corrupt tail at offset {}",
                bytes.len() - pos,
                pos
            );
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tp(topic: &str, partition: u32) -> TopicPartition {
        TopicPartition {
            topic: topic.to_string(),
            partition,
        }
    }

    #[tokio::test]
    async fn register_then_load_roundtrips_in_order() {
        let dir = TempDir::new().unwrap();
        let m = ColdIndexManifest::open(dir.path().join("cold.manifest"))
            .await
            .unwrap();
        m.register(&tp("t", 0), 0, 5, "topics/t/0/0_5")
            .await
            .unwrap();
        m.register(&tp("t", 1), 0, 3, "topics/t/1/0_3")
            .await
            .unwrap();
        m.register(&tp("t", 0), 5, 9, "topics/t/0/5_9")
            .await
            .unwrap();

        let segs = m.load().await.unwrap();
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].object_key, "topics/t/0/0_5");
        assert_eq!(segs[1].topic_partition, tp("t", 1));
        assert_eq!(segs[2].start_offset, 5);
        assert_eq!(segs[2].end_offset, 9);
    }

    #[tokio::test]
    async fn entries_survive_reopen() {
        // The durability guarantee: a fresh handle on the same path sees prior writes.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cold.manifest");
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            m.register(&tp("t", 0), 0, 5, "k0").await.unwrap();
            m.register(&tp("t", 0), 5, 8, "k1").await.unwrap();
        }
        let m2 = ColdIndexManifest::open(&path).await.unwrap();
        let segs = m2.load().await.unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].object_key, "k1");
    }

    #[tokio::test]
    async fn load_on_missing_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let m = ColdIndexManifest::open(dir.path().join("absent.manifest"))
            .await
            .unwrap();
        // open() creates the file; remove it to simulate a never-written manifest.
        std::fs::remove_file(dir.path().join("absent.manifest")).unwrap();
        assert!(m.load().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn torn_trailing_frame_is_ignored() {
        // A crash mid-append leaves a partial frame; load must return the valid prefix
        // and drop the torn tail (which was never acked, so its segment wasn't reclaimed).
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cold.manifest");
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            m.register(&tp("t", 0), 0, 5, "good0").await.unwrap();
            m.register(&tp("t", 0), 5, 9, "good1").await.unwrap();
        }
        // Append a bogus partial frame: a length header promising 100 bytes, then only 3.
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            f.write_all(&100u32.to_le_bytes()).unwrap();
            f.write_all(b"abc").unwrap();
            f.sync_all().unwrap();
        }
        let m = ColdIndexManifest::open(&path).await.unwrap();
        let segs = m.load().await.unwrap();
        assert_eq!(
            segs.len(),
            2,
            "torn tail must be dropped, valid prefix kept"
        );
        assert_eq!(segs[1].object_key, "good1");
    }

    #[tokio::test]
    async fn torn_tail_truncated_so_post_recovery_appends_survive_restart() {
        // Regression for the crash-recovery bug: a torn tail must be *removed* during
        // load(), not merely skipped. Otherwise a register() after recovery appends
        // behind the torn frame, and the next load() stops at the torn frame and loses
        // the new entry — making cold data tiered after the first crash unreadable.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cold.manifest");

        // Crash state: two good frames followed by a torn partial frame.
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            m.register(&tp("t", 0), 0, 5, "k0").await.unwrap();
            m.register(&tp("t", 0), 5, 9, "k1").await.unwrap();
        }
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(&100u32.to_le_bytes()).unwrap(); // header promises 100 bytes
            f.write_all(b"xy").unwrap(); // only 2 delivered
            f.sync_all().unwrap();
        }

        // Restart #1: recovery loads the valid prefix and truncates the torn tail, then
        // re-tiering registers a new segment (which must land in a clean file).
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            assert_eq!(m.load().await.unwrap().len(), 2);
            m.register(&tp("t", 0), 9, 13, "k2").await.unwrap();
        }

        // Restart #2: the post-recovery entry must survive. (Without truncation, load()
        // would still stop at the torn frame and return only 2.)
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            let segs = m.load().await.unwrap();
            assert_eq!(
                segs.len(),
                3,
                "entry registered after recovery must survive the next restart"
            );
            assert_eq!(segs[2].object_key, "k2");
            assert_eq!(segs[2].start_offset, 9);
            assert_eq!(segs[2].end_offset, 13);
        }
    }

    #[tokio::test]
    async fn corrupt_frame_crc_stops_replay() {
        // A frame whose body was corrupted (bit-rot / partial overwrite) fails its CRC
        // and is dropped along with anything after it.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cold.manifest");
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            m.register(&tp("t", 0), 0, 5, "k0").await.unwrap();
            m.register(&tp("t", 0), 5, 9, "k1").await.unwrap();
        }
        // Flip a byte inside the first frame's body (offset 4 = first body byte).
        let mut raw = std::fs::read(&path).unwrap();
        raw[4] ^= 0xFF;
        std::fs::write(&path, &raw).unwrap();

        let m = ColdIndexManifest::open(&path).await.unwrap();
        let segs = m.load().await.unwrap();
        assert!(
            segs.is_empty(),
            "corruption in the first frame stops replay there"
        );
    }
}
