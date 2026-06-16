//! Per-partition offset index and in-memory hot serving tier.
//!
//! The broker uses a single shared WAL, but producers and consumers see
//! **per-partition** logical offsets. This index maps `(TopicPartition, offset)`
//! to the record — either still in memory (`hot`) or demultiplexed into an
//! object-storage segment (`cold`) — so reads can be resolved correctly **per
//! partition**. Resolving a read never consults another partition's entries,
//! which is the core fix for the partition-blind read bug.
//!
//! Following AutoMQ's serving model, the WAL is durability/recover-only: consumer
//! reads are served from the in-memory `hot` map (recent, un-tiered) and object
//! storage (`cold`), **never** from the WAL. A record stays in `hot` from append
//! until its segment is tiered, at which point `flip_to_cold` drops the in-memory
//! copy (the eviction) — so the hot map holding every un-tiered record is a
//! read-your-writes correctness requirement, bounded by a memory budget enforced
//! in [`crate::storage::partition_store`].

use crate::storage::cold_index::ColdSegment;
use crate::types::{Offset, TopicPartition, WalRecord};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A contiguous range of a partition's offsets that has been uploaded to object
/// storage as a single segment object. Covers `[start_offset, end_offset)`.
/// `size_bytes` is the object's uploaded size, used by compaction to identify small
/// objects without downloading them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRange {
    pub end_offset: Offset,
    pub object_key: String,
    pub size_bytes: u64,
}

/// An un-tiered record held in memory for serving. `size` is the record's encoded
/// body length (bincode `WalRecord`), used for both `max_bytes` read accounting and
/// the global memory budget.
#[derive(Debug, Clone)]
struct HotEntry {
    record: Arc<WalRecord>,
    size: u32,
}

/// Index entries for one partition.
#[derive(Debug, Default)]
struct PartitionEntries {
    /// In-memory records, one entry per record offset (the hot serving tier).
    hot: BTreeMap<Offset, HotEntry>,
    /// Uploaded ranges, keyed by start offset. Disjoint and contiguous.
    cold: BTreeMap<Offset, ObjectRange>,
    /// One past the highest indexed offset for this partition (the next offset to assign/expect).
    next_offset: Offset,
}

/// How to satisfy a read at a given `(partition, offset)`.
#[derive(Debug, Clone)]
pub enum ReadPlan {
    /// Records are in memory; return them directly (no WAL/disk read).
    Hot(Vec<Arc<WalRecord>>),
    /// Records are in an uploaded object; download it and return records `>= offset`.
    Cold {
        object_key: String,
        start_offset: Offset,
        end_offset: Offset,
    },
    /// Nothing at/after the requested offset (caught up, or unknown partition/gap).
    Empty,
}

pub struct PartitionIndex {
    inner: RwLock<HashMap<TopicPartition, PartitionEntries>>,
    /// Total bytes held across all partitions' `hot` maps. Maintained under the
    /// inner write lock on every insert/evict; read lock-free for budget checks.
    hot_bytes: AtomicU64,
}

impl PartitionIndex {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            hot_bytes: AtomicU64::new(0),
        }
    }

    /// Record a freshly-appended record for `tp` at `offset`, holding it in memory
    /// (the hot serving tier). `size` is the encoded body length, for accounting.
    pub fn insert_hot(
        &self,
        tp: &TopicPartition,
        offset: Offset,
        record: Arc<WalRecord>,
        size: u32,
    ) {
        let mut guard = self.inner.write();
        let entries = guard.entry(tp.clone()).or_default();
        if let Some(old) = entries.hot.insert(offset, HotEntry { record, size }) {
            // Replacing an existing offset (e.g. duplicate recovery re-index): fix accounting.
            self.hot_bytes.fetch_sub(old.size as u64, Ordering::Relaxed);
        }
        self.hot_bytes.fetch_add(size as u64, Ordering::Relaxed);
        entries.next_offset = entries.next_offset.max(offset + 1);
    }

    /// Drop a hot entry without tiering it. Used to roll back a hot insert when the
    /// subsequent durable WAL append fails (so an un-acked record is never served).
    /// Does not rewind `next_offset` — the reserved offset stays consumed (reads of
    /// the resulting gap return `Empty`).
    pub fn remove_hot(&self, tp: &TopicPartition, offset: Offset) {
        let mut guard = self.inner.write();
        if let Some(entries) = guard.get_mut(tp) {
            if let Some(old) = entries.hot.remove(&offset) {
                self.hot_bytes.fetch_sub(old.size as u64, Ordering::Relaxed);
            }
        }
    }

    /// Replace the hot entries covering `[start_offset, end_offset)` with a single cold
    /// range pointing at the uploaded object. This is the **eviction**: dropping the
    /// `Arc<WalRecord>`s frees their memory and decrements the byte budget. Idempotent:
    /// safe to call again after a crash mid-reclaim (removing absent hot keys and
    /// re-inserting the same range are no-ops).
    pub fn flip_to_cold(
        &self,
        tp: &TopicPartition,
        start_offset: Offset,
        end_offset: Offset,
        object_key: String,
        size_bytes: u64,
    ) {
        let mut guard = self.inner.write();
        let entries = guard.entry(tp.clone()).or_default();
        // Evict hot entries in [start, end), reclaiming their bytes.
        let to_remove: Vec<Offset> = entries
            .hot
            .range(start_offset..end_offset)
            .map(|(&k, _)| k)
            .collect();
        let mut freed = 0u64;
        for k in to_remove {
            if let Some(old) = entries.hot.remove(&k) {
                freed += old.size as u64;
            }
        }
        if freed > 0 {
            self.hot_bytes.fetch_sub(freed, Ordering::Relaxed);
        }
        entries.cold.insert(
            start_offset,
            ObjectRange {
                end_offset,
                object_key,
                size_bytes,
            },
        );
        entries.next_offset = entries.next_offset.max(end_offset);
    }

    /// Insert a cold range learned during recovery from the durable manifest, without
    /// evicting any hot entries (there are none for an already-tiered range). Bumps
    /// `next_offset` so a partition whose recent data is entirely cold still reports the
    /// correct high-watermark. Idempotent: re-inserting the same range overwrites it.
    ///
    /// Distinct from [`flip_to_cold`](Self::flip_to_cold), which is the *live* tiering
    /// path and additionally evicts the now-tiered hot records.
    pub fn insert_cold(
        &self,
        tp: &TopicPartition,
        start_offset: Offset,
        end_offset: Offset,
        object_key: String,
        size_bytes: u64,
    ) {
        let mut guard = self.inner.write();
        let entries = guard.entry(tp.clone()).or_default();
        entries.cold.insert(
            start_offset,
            ObjectRange {
                end_offset,
                object_key,
                size_bytes,
            },
        );
        entries.next_offset = entries.next_offset.max(end_offset);
    }

    /// Replace the cold ranges within `[start_offset, end_offset)` with a single merged
    /// range pointing at the compacted object. This is the in-memory half of compaction:
    /// it removes the subsumed source ranges and inserts the merged one. The hot tier is
    /// untouched (a compacted range is below the cold frontier, so it has no hot copy).
    ///
    /// Range-based and idempotent: re-running with the same `[start, end)` removes the
    /// just-inserted merged entry and re-inserts it (a no-op), so a crash-retried
    /// compaction converges. Mirrors the manifest's fold so the in-memory and durable
    /// views stay identical.
    pub fn replace_cold(
        &self,
        tp: &TopicPartition,
        start_offset: Offset,
        end_offset: Offset,
        object_key: String,
        size_bytes: u64,
    ) {
        let mut guard = self.inner.write();
        let entries = guard.entry(tp.clone()).or_default();
        let subsumed: Vec<Offset> = entries
            .cold
            .range(start_offset..end_offset)
            .map(|(&k, _)| k)
            .collect();
        for k in subsumed {
            entries.cold.remove(&k);
        }
        entries.cold.insert(
            start_offset,
            ObjectRange {
                end_offset,
                object_key,
                size_bytes,
            },
        );
        entries.next_offset = entries.next_offset.max(end_offset);
    }

    /// Total bytes currently held in the in-memory hot tier (across all partitions).
    pub fn hot_bytes(&self) -> u64 {
        self.hot_bytes.load(Ordering::Relaxed)
    }

    /// One past the highest indexed offset for `tp` (0 if unknown). Used to rebuild
    /// the partition high-watermark on recovery and to compute follower lag.
    pub fn next_offset(&self, tp: &TopicPartition) -> Offset {
        self.inner
            .read()
            .get(tp)
            .map(|e| e.next_offset)
            .unwrap_or(0)
    }

    /// Resolve how to read `tp` starting at `offset`, returning up to ~`max_bytes`
    /// worth of contiguous hot records, or a single covering cold object, or Empty.
    ///
    /// At least one record is always included if available, even if it exceeds
    /// `max_bytes`, to avoid stalling a consumer on an oversized record.
    pub fn read_plan(&self, tp: &TopicPartition, offset: Offset, max_bytes: usize) -> ReadPlan {
        let guard = self.inner.read();
        let Some(entries) = guard.get(tp) else {
            return ReadPlan::Empty;
        };
        if offset >= entries.next_offset {
            return ReadPlan::Empty; // caught up
        }

        // Hot path: contiguous run of in-memory records starting exactly at `offset`.
        if entries.hot.contains_key(&offset) {
            let mut records = Vec::new();
            let mut bytes = 0usize;
            let mut cur = offset;
            while let Some(entry) = entries.hot.get(&cur) {
                let next_bytes = bytes + entry.size as usize;
                if !records.is_empty() && next_bytes > max_bytes {
                    break;
                }
                records.push(Arc::clone(&entry.record));
                bytes = next_bytes;
                cur += 1;
            }
            return ReadPlan::Hot(records);
        }

        // Cold path: floor-lookup the uploaded range covering `offset`.
        if let Some((&start, range)) = entries.cold.range(..=offset).next_back() {
            if offset < range.end_offset {
                return ReadPlan::Cold {
                    object_key: range.object_key.clone(),
                    start_offset: start,
                    end_offset: range.end_offset,
                };
            }
        }

        ReadPlan::Empty
    }

    pub fn get_partitions(&self) -> Vec<TopicPartition> {
        let guard = self.inner.read();
        guard.keys().cloned().collect()
    }

    pub fn get_cold_segments(&self, tp: &TopicPartition) -> Vec<ColdSegment> {
        let guard = self.inner.read();
        if let Some(entries) = guard.get(tp) {
            entries
                .cold
                .iter()
                .map(|(&start, range)| ColdSegment {
                    topic_partition: tp.clone(),
                    start_offset: start,
                    end_offset: range.end_offset,
                    object_key: range.object_key.clone(),
                    size_bytes: range.size_bytes,
                })
                .collect()
        } else {
            vec![]
        }
    }
}

impl Default for PartitionIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Record;

    fn tp(topic: &str, partition: u32) -> TopicPartition {
        TopicPartition {
            topic: topic.to_string(),
            partition,
        }
    }

    /// Build a hot record carrying `value` so reads can be verified by content, and a
    /// caller-controlled `size` so `max_bytes`/budget accounting can be tested precisely.
    fn hot(tp: &TopicPartition, offset: u64, value: &[u8], size: u32) -> (Arc<WalRecord>, u32) {
        let record = Arc::new(WalRecord {
            topic_partition: tp.clone(),
            offset,
            record: Record::new(None, value.to_vec(), vec![], 0),
            crc32: 0,
        });
        (record, size)
    }

    #[test]
    fn hot_only_contiguous_run_respects_max_bytes() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        for i in 0..5u64 {
            let (r, s) = hot(&a, i, b"x", 10);
            idx.insert_hot(&a, i, r, s);
        }
        // max_bytes=25 -> 10+10 fits, +10 would exceed -> 2 records (but at least 1 guaranteed).
        match idx.read_plan(&a, 0, 25) {
            ReadPlan::Hot(recs) => assert_eq!(recs.len(), 2),
            other => panic!("expected Hot, got {other:?}"),
        }
        // From offset 2, large budget -> remaining 3 records.
        match idx.read_plan(&a, 2, 10_000) {
            ReadPlan::Hot(recs) => assert_eq!(recs.len(), 3),
            other => panic!("expected Hot, got {other:?}"),
        }
    }

    #[test]
    fn hot_read_returns_actual_record_bytes() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        for i in 0..3u64 {
            let (r, s) = hot(&a, i, format!("v{i}").as_bytes(), 8);
            idx.insert_hot(&a, i, r, s);
        }
        match idx.read_plan(&a, 0, 10_000) {
            ReadPlan::Hot(recs) => {
                let values: Vec<&[u8]> = recs.iter().map(|r| r.record.value.as_ref()).collect();
                assert_eq!(values, vec![b"v0".as_ref(), b"v1".as_ref(), b"v2".as_ref()]);
            }
            other => panic!("expected Hot, got {other:?}"),
        }
    }

    #[test]
    fn oversized_first_record_still_returned() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        let (r, s) = hot(&a, 0, b"big", 9999);
        idx.insert_hot(&a, 0, r, s);
        match idx.read_plan(&a, 0, 10) {
            ReadPlan::Hot(recs) => assert_eq!(recs.len(), 1),
            other => panic!("expected Hot, got {other:?}"),
        }
    }

    #[test]
    fn caught_up_returns_empty() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        let (r, s) = hot(&a, 0, b"x", 10);
        idx.insert_hot(&a, 0, r, s);
        assert!(matches!(idx.read_plan(&a, 1, 1000), ReadPlan::Empty));
        assert_eq!(idx.next_offset(&a), 1);
    }

    #[test]
    fn unknown_partition_returns_empty() {
        let idx = PartitionIndex::new();
        assert!(matches!(
            idx.read_plan(&tp("ghost", 7), 0, 1000),
            ReadPlan::Empty
        ));
        assert_eq!(idx.next_offset(&tp("ghost", 7)), 0);
    }

    #[test]
    fn hot_bytes_accounts_inserts_and_evictions() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        for i in 0..10u64 {
            let (r, s) = hot(&a, i, b"x", 50);
            idx.insert_hot(&a, i, r, s);
        }
        assert_eq!(idx.hot_bytes(), 500);
        // Flipping 0..5 to cold evicts those 5 records, freeing their bytes.
        idx.flip_to_cold(&a, 0, 5, "k".to_string(), 100);
        assert_eq!(idx.hot_bytes(), 250);
    }

    #[test]
    fn flip_to_cold_moves_range_and_resolves_cold() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        for i in 0..10u64 {
            let (r, s) = hot(&a, i, b"x", 50);
            idx.insert_hot(&a, i, r, s);
        }
        idx.flip_to_cold(&a, 0, 5, "topics/a/0/seg-0-5".to_string(), 250);

        // Offsets 0..5 now resolve cold.
        match idx.read_plan(&a, 2, 1000) {
            ReadPlan::Cold {
                object_key,
                start_offset,
                end_offset,
            } => {
                assert_eq!(object_key, "topics/a/0/seg-0-5");
                assert_eq!(start_offset, 0);
                assert_eq!(end_offset, 5);
            }
            other => panic!("expected Cold, got {other:?}"),
        }
        // Offsets 5..10 still hot.
        match idx.read_plan(&a, 5, 1000) {
            ReadPlan::Hot(recs) => assert_eq!(recs.len(), 5),
            other => panic!("expected Hot, got {other:?}"),
        }
        assert_eq!(idx.next_offset(&a), 10);
    }

    #[test]
    fn flip_to_cold_is_idempotent() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        for i in 0..5u64 {
            let (r, s) = hot(&a, i, b"x", 50);
            idx.insert_hot(&a, i, r, s);
        }
        idx.flip_to_cold(&a, 0, 5, "k".to_string(), 100);
        idx.flip_to_cold(&a, 0, 5, "k".to_string(), 100); // second call: no panic, same result
        assert_eq!(idx.hot_bytes(), 0); // no double-subtraction
        match idx.read_plan(&a, 0, 1000) {
            ReadPlan::Cold { object_key, .. } => assert_eq!(object_key, "k"),
            other => panic!("expected Cold, got {other:?}"),
        }
    }

    #[test]
    fn insert_cold_recovers_range_without_hot() {
        // Recovery path: a fully-tiered partition (no hot records, WAL segment gone) is
        // restored from the manifest. The range resolves cold and next_offset is set.
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        idx.insert_cold(&a, 0, 5, "topics/a/0/0_5".to_string(), 50);
        idx.insert_cold(&a, 5, 9, "topics/a/0/5_9".to_string(), 40);

        assert_eq!(idx.hot_bytes(), 0);
        assert_eq!(
            idx.next_offset(&a),
            9,
            "next_offset (HW source) restored from cold"
        );
        match idx.read_plan(&a, 2, 1000) {
            ReadPlan::Cold {
                object_key,
                start_offset,
                end_offset,
            } => {
                assert_eq!(object_key, "topics/a/0/0_5");
                assert_eq!(start_offset, 0);
                assert_eq!(end_offset, 5);
            }
            other => panic!("expected Cold, got {other:?}"),
        }
        match idx.read_plan(&a, 6, 1000) {
            ReadPlan::Cold { object_key, .. } => assert_eq!(object_key, "topics/a/0/5_9"),
            other => panic!("expected Cold, got {other:?}"),
        }
    }

    #[test]
    fn replace_cold_collapses_run_to_single_range() {
        // Compaction's in-memory half: two adjacent cold ranges merge into one object;
        // the untouched neighbor still resolves on its own.
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        idx.insert_cold(&a, 0, 5, "k0_5".to_string(), 50);
        idx.insert_cold(&a, 5, 9, "k5_9".to_string(), 40);
        idx.insert_cold(&a, 9, 12, "k9_12".to_string(), 30);

        idx.replace_cold(&a, 0, 9, "k0_9".to_string(), 90);

        match idx.read_plan(&a, 2, 1000) {
            ReadPlan::Cold {
                object_key,
                start_offset,
                end_offset,
            } => {
                assert_eq!(object_key, "k0_9");
                assert_eq!(start_offset, 0);
                assert_eq!(end_offset, 9);
            }
            other => panic!("expected Cold, got {other:?}"),
        }
        match idx.read_plan(&a, 10, 1000) {
            ReadPlan::Cold { object_key, .. } => assert_eq!(object_key, "k9_12"),
            other => panic!("expected Cold, got {other:?}"),
        }
        assert_eq!(idx.next_offset(&a), 12);
    }

    #[test]
    fn replace_cold_is_idempotent() {
        // A crash-retried compaction may apply the same replace twice; the second is a
        // no-op (remove the just-inserted merged entry, re-insert it).
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        idx.insert_cold(&a, 0, 5, "k0_5".to_string(), 50);
        idx.insert_cold(&a, 5, 9, "k5_9".to_string(), 40);
        idx.replace_cold(&a, 0, 9, "k0_9".to_string(), 90);
        idx.replace_cold(&a, 0, 9, "k0_9".to_string(), 90);
        match idx.read_plan(&a, 0, 1000) {
            ReadPlan::Cold {
                object_key,
                start_offset,
                end_offset,
            } => {
                assert_eq!(object_key, "k0_9");
                assert_eq!(start_offset, 0);
                assert_eq!(end_offset, 9);
            }
            other => panic!("expected Cold, got {other:?}"),
        }
    }

    #[test]
    fn replace_cold_leaves_hot_untouched() {
        // Compaction only touches cold ranges; un-tiered hot records above the cold
        // frontier must remain served from memory with their bytes still accounted.
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        idx.insert_cold(&a, 0, 5, "k0_5".to_string(), 50);
        idx.insert_cold(&a, 5, 9, "k5_9".to_string(), 40);
        for i in 9..12u64 {
            let (r, s) = hot(&a, i, b"x", 10);
            idx.insert_hot(&a, i, r, s);
        }
        let hot_before = idx.hot_bytes();

        idx.replace_cold(&a, 0, 9, "k0_9".to_string(), 90);

        assert_eq!(
            idx.hot_bytes(),
            hot_before,
            "replace_cold must not touch the hot tier"
        );
        match idx.read_plan(&a, 9, 1000) {
            ReadPlan::Hot(recs) => assert_eq!(recs.len(), 3),
            other => panic!("expected Hot, got {other:?}"),
        }
        match idx.read_plan(&a, 0, 1000) {
            ReadPlan::Cold { object_key, .. } => assert_eq!(object_key, "k0_9"),
            other => panic!("expected Cold, got {other:?}"),
        }
    }

    #[test]
    fn remove_hot_rolls_back_insert() {
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        let (r, s) = hot(&a, 0, b"x", 50);
        idx.insert_hot(&a, 0, r, s);
        assert_eq!(idx.hot_bytes(), 50);
        idx.remove_hot(&a, 0);
        assert_eq!(idx.hot_bytes(), 0);
        // The reserved offset stays consumed; the gap reads as Empty.
        assert!(matches!(idx.read_plan(&a, 0, 1000), ReadPlan::Empty));
    }

    #[test]
    fn partitions_are_isolated_no_cross_leakage() {
        // The regression guard for the original bug: same offset in two partitions
        // must resolve to that partition's own record, never the other's.
        let idx = PartitionIndex::new();
        let a = tp("topic", 0);
        let b = tp("topic", 1);
        let (ra, sa) = hot(&a, 0, b"A-data", 10);
        let (rb, sb) = hot(&b, 0, b"B-data", 20);
        idx.insert_hot(&a, 0, ra, sa);
        idx.insert_hot(&b, 0, rb, sb);

        match idx.read_plan(&a, 0, 1000) {
            ReadPlan::Hot(recs) => {
                assert_eq!(recs.len(), 1);
                assert_eq!(recs[0].record.value.as_ref(), b"A-data");
            }
            other => panic!("expected Hot, got {other:?}"),
        }
        match idx.read_plan(&b, 0, 1000) {
            ReadPlan::Hot(recs) => {
                assert_eq!(recs.len(), 1);
                assert_eq!(recs[0].record.value.as_ref(), b"B-data");
            }
            other => panic!("expected Hot, got {other:?}"),
        }
    }

    #[test]
    fn gap_below_hot_without_cold_is_empty() {
        // If an offset falls below the hot range but no cold range covers it, Empty.
        let idx = PartitionIndex::new();
        let a = tp("a", 0);
        for i in 5..8u64 {
            let (r, s) = hot(&a, i, b"x", 10);
            idx.insert_hot(&a, i, r, s);
        }
        // offset 2 < next_offset(8), not in hot (starts at 5), no cold -> Empty.
        assert!(matches!(idx.read_plan(&a, 2, 1000), ReadPlan::Empty));
    }
}
