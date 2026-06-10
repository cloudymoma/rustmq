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
//! `[u32 LE body_len][bincode(ManifestRecord) body][u32 LE crc32(body)]`. Each frame is
//! one [`ManifestRecord`]: a [`Register`](ManifestRecord::Register) of a freshly tiered
//! range, or a [`Compact`](ManifestRecord::Compact) recording that several adjacent ranges
//! were merged into one object.
//! [`register`](ColdIndexManifest::register) and
//! [`record_compaction`](ColdIndexManifest::record_compaction) each append one frame and
//! `fsync` before returning, so the caller can safely mutate/evict the in-memory copy
//! afterwards.
//! [`load`](ColdIndexManifest::load) replays frames up to the first torn or corrupt one
//! (a partially-written tail from a crash), **truncates that tail off** so later appends
//! are not stranded behind it, and **folds** the log into the final live set (applying
//! each `Compact` by replacing the merged range's source entries) plus the set of orphaned
//! source-object keys to garbage-collect. A `register`/`record_compaction` that did not
//! fully persist leaves its source state intact, so the next cycle re-runs idempotently —
//! no data or index loss. Replay is idempotent; duplicate frames are harmless.

use crate::Result;
use crate::types::{Offset, TopicPartition};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// A tiered, contiguous offset range `[start_offset, end_offset)` of one partition,
/// stored as a single object. Mirrors the in-memory cold entry, plus the partition
/// (the manifest is flat across all of this broker's partitions). `size_bytes` is the
/// uploaded object's size, used by compaction to cheaply identify small objects without
/// downloading them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColdSegment {
    pub topic_partition: TopicPartition,
    pub start_offset: Offset,
    pub end_offset: Offset,
    pub object_key: String,
    pub size_bytes: u64,
}

/// One durable entry in the manifest log. Tagged so the append-only log can express both
/// new tiered ranges and compactions (N ranges → 1 object) without rewriting history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManifestRecord {
    /// A freshly tiered offset range and its object.
    Register(ColdSegment),
    /// Several adjacent ranges were merged into `merged`'s single object; `sources` are
    /// the now-superseded object keys to garbage-collect. Replaying this replaces every
    /// live entry within `[merged.start_offset, merged.end_offset)` with `merged`.
    Compact {
        merged: ColdSegment,
        sources: Vec<String>,
    },
}

/// The folded result of replaying the manifest: the live cold segments (one per current
/// object), and the object keys that compaction superseded and that should be deleted from
/// object storage (idempotently) during recovery.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoadResult {
    pub live: Vec<ColdSegment>,
    pub orphan_object_keys: Vec<String>,
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
        size_bytes: u64,
    ) -> Result<()> {
        let seg = ColdSegment {
            topic_partition: tp.clone(),
            start_offset,
            end_offset,
            object_key: object_key.to_string(),
            size_bytes,
        };
        self.append_record(&ManifestRecord::Register(seg)).await
    }

    /// Durably record that `sources` were merged into `merged`'s single object. Returns
    /// only after the frame is `fsync`ed, so the caller may then flip the in-memory index
    /// to the merged object and delete the source objects.
    pub async fn record_compaction(
        &self,
        merged: ColdSegment,
        sources: Vec<String>,
    ) -> Result<()> {
        self.append_record(&ManifestRecord::Compact { merged, sources })
            .await
    }

    /// Append one length-prefixed, CRC-checked frame and `fsync`. The frame is assembled
    /// into a single buffer so one `write_all` emits it whole (no partial-frame
    /// interleaving with a concurrent writer holding the same lock).
    async fn append_record(&self, record: &ManifestRecord) -> Result<()> {
        let body = bincode::serialize(record)?;
        let crc = crc32fast::hash(&body);

        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        frame.extend_from_slice(&crc.to_le_bytes());

        let mut file = self.file.lock().await;
        file.write_all(&frame).await?;
        file.sync_all().await?;
        Ok(())
    }

    /// Replay all durably-written frames in append order and **fold** them into the final
    /// state: the live cold segments and the orphaned source-object keys to garbage-collect
    /// (see [`LoadResult`]). Stops at the first torn or CRC-mismatched frame (a
    /// partially-written tail from a crash) and **truncates that torn tail off the file**.
    ///
    /// Truncation is essential, not cosmetic: recovery after a crash re-tiers the
    /// orphaned WAL segment and appends a new frame *after* whatever was at the tail. If a
    /// torn frame were merely skipped (not removed), the new entry would sit behind it and
    /// the next `load` would stop at the torn frame and lose it — making cold data written
    /// after the crash unreadable. Removing the torn tail here (before any append)
    /// guarantees later appends land in a clean file.
    ///
    /// Folding: a `Register` inserts/overwrites its range; a `Compact` replaces every live
    /// entry within `[merged.start, merged.end)` with `merged` and adds its `sources` to
    /// the orphan set. Applying in append order is deterministic and idempotent (a
    /// re-applied `Compact` is a no-op once its sources are gone). The live set is returned
    /// sorted by `(topic, partition, start_offset)`.
    ///
    /// Must be called once at startup, before any append — it mutates the file and assumes
    /// no concurrent writers.
    pub async fn load(&self) -> Result<LoadResult> {
        let bytes = match tokio::fs::read(&self.path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LoadResult::default()),
            Err(e) => return Err(e.into()),
        };

        let mut records = Vec::new();
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
            match bincode::deserialize::<ManifestRecord>(body) {
                Ok(rec) => records.push(rec),
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

        Ok(Self::fold(records))
    }

    /// Fold an in-append-order list of records into the final live set + orphan keys.
    /// Pure (no I/O) so it is trivially unit-testable.
    fn fold(records: Vec<ManifestRecord>) -> LoadResult {
        // Per partition, live segments keyed by start offset (disjoint, contiguous).
        let mut live: HashMap<TopicPartition, BTreeMap<Offset, ColdSegment>> = HashMap::new();
        let mut orphan_object_keys = Vec::new();

        for record in records {
            match record {
                ManifestRecord::Register(seg) => {
                    live.entry(seg.topic_partition.clone())
                        .or_default()
                        .insert(seg.start_offset, seg);
                }
                ManifestRecord::Compact { merged, sources } => {
                    let entries = live.entry(merged.topic_partition.clone()).or_default();
                    // Replace every entry the merge subsumed.
                    let subsumed: Vec<Offset> = entries
                        .range(merged.start_offset..merged.end_offset)
                        .map(|(&k, _)| k)
                        .collect();
                    for k in subsumed {
                        entries.remove(&k);
                    }
                    entries.insert(merged.start_offset, merged);
                    orphan_object_keys.extend(sources);
                }
            }
        }

        let mut out: Vec<ColdSegment> = live
            .into_values()
            .flat_map(|m| m.into_values())
            .collect();
        out.sort_by(|a, b| {
            (
                &a.topic_partition.topic,
                a.topic_partition.partition,
                a.start_offset,
            )
                .cmp(&(
                    &b.topic_partition.topic,
                    b.topic_partition.partition,
                    b.start_offset,
                ))
        });

        LoadResult {
            live: out,
            orphan_object_keys,
        }
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
        m.register(&tp("t", 0), 0, 5, "topics/t/0/0_5", 100)
            .await
            .unwrap();
        m.register(&tp("t", 1), 0, 3, "topics/t/1/0_3", 60)
            .await
            .unwrap();
        m.register(&tp("t", 0), 5, 9, "topics/t/0/5_9", 80)
            .await
            .unwrap();

        // load() returns the live set sorted by (topic, partition, start_offset).
        let segs = m.load().await.unwrap().live;
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].object_key, "topics/t/0/0_5");
        assert_eq!(segs[0].size_bytes, 100);
        assert_eq!(segs[1].object_key, "topics/t/0/5_9");
        assert_eq!(segs[1].start_offset, 5);
        assert_eq!(segs[1].end_offset, 9);
        assert_eq!(segs[2].topic_partition, tp("t", 1));
    }

    #[tokio::test]
    async fn entries_survive_reopen() {
        // The durability guarantee: a fresh handle on the same path sees prior writes.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cold.manifest");
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            m.register(&tp("t", 0), 0, 5, "k0", 50).await.unwrap();
            m.register(&tp("t", 0), 5, 8, "k1", 50).await.unwrap();
        }
        let m2 = ColdIndexManifest::open(&path).await.unwrap();
        let segs = m2.load().await.unwrap().live;
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
        let result = m.load().await.unwrap();
        assert!(result.live.is_empty());
        assert!(result.orphan_object_keys.is_empty());
    }

    #[tokio::test]
    async fn torn_trailing_frame_is_ignored() {
        // A crash mid-append leaves a partial frame; load must return the valid prefix
        // and drop the torn tail (which was never acked, so its segment wasn't reclaimed).
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cold.manifest");
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            m.register(&tp("t", 0), 0, 5, "good0", 50).await.unwrap();
            m.register(&tp("t", 0), 5, 9, "good1", 50).await.unwrap();
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
        let segs = m.load().await.unwrap().live;
        assert_eq!(segs.len(), 2, "torn tail must be dropped, valid prefix kept");
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
            m.register(&tp("t", 0), 0, 5, "k0", 50).await.unwrap();
            m.register(&tp("t", 0), 5, 9, "k1", 50).await.unwrap();
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
            assert_eq!(m.load().await.unwrap().live.len(), 2);
            m.register(&tp("t", 0), 9, 13, "k2", 50).await.unwrap();
        }

        // Restart #2: the post-recovery entry must survive. (Without truncation, load()
        // would still stop at the torn frame and return only 2.)
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            let segs = m.load().await.unwrap().live;
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
            m.register(&tp("t", 0), 0, 5, "k0", 50).await.unwrap();
            m.register(&tp("t", 0), 5, 9, "k1", 50).await.unwrap();
        }
        // Flip a byte inside the first frame's body (offset 4 = first body byte).
        let mut raw = std::fs::read(&path).unwrap();
        raw[4] ^= 0xFF;
        std::fs::write(&path, &raw).unwrap();

        let m = ColdIndexManifest::open(&path).await.unwrap();
        let segs = m.load().await.unwrap().live;
        assert!(
            segs.is_empty(),
            "corruption in the first frame stops replay there"
        );
    }

    fn seg(topic: &str, partition: u32, start: Offset, end: Offset, key: &str) -> ColdSegment {
        ColdSegment {
            topic_partition: tp(topic, partition),
            start_offset: start,
            end_offset: end,
            object_key: key.to_string(),
            size_bytes: (end - start) * 10,
        }
    }

    #[test]
    fn fold_collapses_compacted_sources_into_merged_range() {
        // Three registered ranges, then a Compact merging the first two into one object.
        // The fold must drop the two sources and expose the merged range plus the
        // untouched third range; the sources become orphan keys to GC.
        let records = vec![
            ManifestRecord::Register(seg("t", 0, 0, 5, "k0_5")),
            ManifestRecord::Register(seg("t", 0, 5, 9, "k5_9")),
            ManifestRecord::Register(seg("t", 0, 9, 12, "k9_12")),
            ManifestRecord::Compact {
                merged: seg("t", 0, 0, 9, "k0_9"),
                sources: vec!["k0_5".into(), "k5_9".into()],
            },
        ];
        let result = ColdIndexManifest::fold(records);
        let keys: Vec<&str> = result.live.iter().map(|s| s.object_key.as_str()).collect();
        assert_eq!(keys, vec!["k0_9", "k9_12"], "sources replaced by merged");
        assert_eq!(result.live[0].start_offset, 0);
        assert_eq!(result.live[0].end_offset, 9);
        assert_eq!(
            result.orphan_object_keys,
            vec!["k0_5".to_string(), "k5_9".to_string()],
            "compacted sources are reported for GC"
        );
    }

    #[test]
    fn fold_is_idempotent_under_replayed_compaction() {
        // Replaying the same Compact twice (a duplicate frame after a crash-retry) yields
        // the same live set and does not resurrect the sources.
        let base = vec![
            ManifestRecord::Register(seg("t", 0, 0, 5, "k0_5")),
            ManifestRecord::Register(seg("t", 0, 5, 9, "k5_9")),
            ManifestRecord::Compact {
                merged: seg("t", 0, 0, 9, "k0_9"),
                sources: vec!["k0_5".into(), "k5_9".into()],
            },
        ];
        let mut twice = base.clone();
        twice.push(ManifestRecord::Compact {
            merged: seg("t", 0, 0, 9, "k0_9"),
            sources: vec!["k0_5".into(), "k5_9".into()],
        });

        let once = ColdIndexManifest::fold(base);
        let again = ColdIndexManifest::fold(twice);
        assert_eq!(once.live, again.live, "duplicate Compact is a no-op on live set");
        assert_eq!(once.live.len(), 1);
        assert_eq!(once.live[0].object_key, "k0_9");
    }

    #[test]
    fn fold_chained_compactions_track_all_orphans() {
        // A merged object can itself become a source of a later, larger merge. Every
        // superseded key (including the intermediate merged one) must be reported.
        let records = vec![
            ManifestRecord::Register(seg("t", 0, 0, 5, "k0_5")),
            ManifestRecord::Register(seg("t", 0, 5, 9, "k5_9")),
            ManifestRecord::Compact {
                merged: seg("t", 0, 0, 9, "k0_9"),
                sources: vec!["k0_5".into(), "k5_9".into()],
            },
            ManifestRecord::Register(seg("t", 0, 9, 14, "k9_14")),
            ManifestRecord::Compact {
                merged: seg("t", 0, 0, 14, "k0_14"),
                sources: vec!["k0_9".into(), "k9_14".into()],
            },
        ];
        let result = ColdIndexManifest::fold(records);
        assert_eq!(result.live.len(), 1);
        assert_eq!(result.live[0].object_key, "k0_14");
        assert_eq!(
            result.orphan_object_keys,
            vec![
                "k0_5".to_string(),
                "k5_9".to_string(),
                "k0_9".to_string(),
                "k9_14".to_string()
            ]
        );
        // No live key is ever an orphan (a deleted source can never resurface live).
        for live in &result.live {
            assert!(!result.orphan_object_keys.contains(&live.object_key));
        }
    }

    #[tokio::test]
    async fn record_compaction_survives_reopen_and_folds() {
        // End-to-end durability: register two ranges, record a compaction, reopen, and the
        // folded load reflects the merge with the sources reported as orphans.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cold.manifest");
        {
            let m = ColdIndexManifest::open(&path).await.unwrap();
            m.register(&tp("t", 0), 0, 5, "k0_5", 50).await.unwrap();
            m.register(&tp("t", 0), 5, 9, "k5_9", 40).await.unwrap();
            m.record_compaction(seg("t", 0, 0, 9, "k0_9"), vec!["k0_5".into(), "k5_9".into()])
                .await
                .unwrap();
        }
        let m2 = ColdIndexManifest::open(&path).await.unwrap();
        let result = m2.load().await.unwrap();
        assert_eq!(result.live.len(), 1);
        assert_eq!(result.live[0].object_key, "k0_9");
        assert_eq!(
            result.orphan_object_keys,
            vec!["k0_5".to_string(), "k5_9".to_string()]
        );
    }
}
