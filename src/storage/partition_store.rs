//! Partition-aware storage engine over the shared segmented WAL.
//!
//! `PartitionStore` is the single write path for every record (produce, follower
//! replication, consumer-group offset commits) and the single read path for the
//! broker. It owns the [`SegmentedLog`] and the [`PartitionIndex`], so reads are
//! always resolved **per partition** — fixing the partition-blind read bug.
//!
//! Serving model (AutoMQ-style): the WAL is **durability + recovery only**. Consumer
//! reads are served from the in-memory hot tier (the index's `hot` map) and object
//! storage (`cold`) — **never** from the WAL. A record lives in the hot tier from
//! append until its WAL segment is tiered, at which point `flip_to_cold` drops the
//! in-memory copy. The hot tier therefore holds every un-tiered record (a
//! read-your-writes requirement), bounded by `hot_cache_max_bytes` via append
//! backpressure.

use crate::storage::cold_index::ColdIndexManifest;
use crate::storage::partition_index::{PartitionIndex, ReadPlan};
use crate::storage::traits::{Cache, PhysicalLocation, RecordLog, UploadManager, WalSegment};
use crate::storage::wal::SegmentedLog;
use crate::types::{Offset, TopicPartition, WalRecord};
use crate::{Result, error::RustMqError};
use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// Cheap in-memory footprint estimate (bytes) for budget + `max_bytes` accounting.
/// Avoids a serialization on the append hot path; the budget bounds RAM held by the
/// hot tier, not the encoded size, so an estimate of the live payload is what matters.
fn mem_size(record: &WalRecord) -> u32 {
    let r = &record.record;
    let key = r.key.as_ref().map_or(0, |k| k.len());
    let headers: usize = r.headers.iter().map(|h| h.key.len() + h.value.len()).sum();
    (std::mem::size_of::<WalRecord>() + r.value.len() + key + headers) as u32
}

pub struct PartitionStore {
    wal: Arc<dyn SegmentedLog>,
    index: Arc<PartitionIndex>,
    upload_manager: Arc<dyn UploadManager>,
    /// Cold-segment download cache only. The hot tier is the index's in-memory map;
    /// this cache merely accelerates repeated cold reads.
    cache: Arc<dyn Cache>,
    /// Max bytes the in-memory hot tier may hold before appends apply backpressure.
    /// `0` disables the budget (unbounded — used in tests that never tier).
    hot_cache_max_bytes: u64,
    /// Durable, broker-local record of tiered segments. Written before a tiered range
    /// is evicted from the hot tier, and replayed on recovery so cold data stays
    /// readable after restart/failover (the WAL only recovers un-tiered records).
    cold_index: Arc<ColdIndexManifest>,
    /// append → tiering task: run a (possibly forced) cycle now.
    tiering_wakeup: Arc<Notify>,
    /// tiering task → blocked appends: memory was freed, re-check the budget.
    memory_available: Arc<Notify>,
}

impl PartitionStore {
    /// Construct the store and rebuild the index from both durable sources:
    /// - the **cold** ranges from the manifest (`cold_index`) — tiered data whose WAL
    ///   segments were reclaimed, so they exist only in object storage;
    /// - the **hot** records from any un-reclaimed WAL segments left on disk.
    ///
    /// Together these restore the full per-partition offset space (and thus
    /// `next_offset`/high-watermark) after a restart, so cold data is readable again.
    /// The two sources are disjoint (cold = tiered/older, hot = un-tiered/newer); if a
    /// crash left a range in both (registered but its WAL segment not yet reclaimed),
    /// reads prefer hot and the next tiering cycle reconciles it idempotently. With a
    /// budget set, recovery may transiently exceed it until tiering drains the backlog.
    pub async fn new(
        wal: Arc<dyn SegmentedLog>,
        upload_manager: Arc<dyn UploadManager>,
        cache: Arc<dyn Cache>,
        hot_cache_max_bytes: u64,
        cold_index: Arc<ColdIndexManifest>,
    ) -> Result<Arc<Self>> {
        let index = Arc::new(PartitionIndex::new());

        // Cold first: restore tiered ranges from the durable (folded) manifest.
        for seg in cold_index.load().await?.live {
            index.insert_cold(
                &seg.topic_partition,
                seg.start_offset,
                seg.end_offset,
                seg.object_key,
            );
        }

        // Hot: re-index un-tiered records still in the WAL.
        for (record, _loc) in wal.recover().await? {
            let size = mem_size(&record);
            let tp = record.topic_partition.clone();
            let offset = record.offset;
            index.insert_hot(&tp, offset, Arc::new(record), size);
        }

        Ok(Arc::new(Self {
            wal,
            index,
            upload_manager,
            cache,
            hot_cache_max_bytes,
            cold_index,
            tiering_wakeup: Arc::new(Notify::new()),
            memory_available: Arc::new(Notify::new()),
        }))
    }

    /// Append a record (produce / replication / offset-commit), holding it in the hot
    /// tier for serving and writing it to the WAL for durability.
    pub async fn append(&self, record: &WalRecord) -> Result<PhysicalLocation> {
        let tp = &record.topic_partition;
        let offset = record.offset;
        let size = mem_size(record);

        // Backpressure: block until the hot tier has room (invariant 5: bounded memory).
        self.await_capacity(size as u64).await;

        // Index in memory FIRST: a record must be in the hot tier before it can become
        // tierable on disk. If we appended first, a concurrent tiering cycle could seal
        // and flip its range to cold before we indexed it, stranding the in-memory copy
        // (it would never be evicted). On WAL failure we roll the insert back below.
        self.index
            .insert_hot(tp, offset, Arc::new(record.clone()), size);

        match self.wal.append(record).await {
            Ok(loc) => {
                // Soft limit: nudge tiering to run ahead of the hard limit.
                if self.index.hot_bytes() >= self.soft_limit() {
                    self.tiering_wakeup.notify_one();
                }
                Ok(loc)
            }
            Err(e) => {
                // Durable append failed: drop the hot entry so an un-acked record is
                // never served.
                self.index.remove_hot(tp, offset);
                Err(e)
            }
        }
    }

    /// Block until the hot tier can accept `incoming` more bytes. Triggers tiering to
    /// free memory and waits for it (with a timeout safety-net against a missed notify).
    async fn await_capacity(&self, incoming: u64) {
        // Disabled budget, or a single record larger than the whole budget: allow it
        // through — blocking forever would deadlock, and memory stays bounded by the
        // max record size.
        if self.hot_cache_max_bytes == 0 || incoming >= self.hot_cache_max_bytes {
            return;
        }
        while self.index.hot_bytes() + incoming > self.hot_cache_max_bytes {
            self.tiering_wakeup.notify_one();
            let _ =
                tokio::time::timeout(Duration::from_millis(100), self.memory_available.notified())
                    .await;
        }
    }

    /// 80% of the budget; `u64::MAX` when the budget is disabled (so the soft path and
    /// forced sealing never trigger in unbounded mode).
    fn soft_limit(&self) -> u64 {
        if self.hot_cache_max_bytes == 0 {
            u64::MAX
        } else {
            self.hot_cache_max_bytes / 5 * 4
        }
    }

    /// One past the highest indexed offset for `tp` (0 if unknown). Source for
    /// rebuilding the partition high-watermark and computing follower lag.
    pub fn next_offset(&self, tp: &TopicPartition) -> Offset {
        self.index.next_offset(tp)
    }

    /// Flush the WAL.
    pub async fn sync(&self) -> Result<()> {
        self.wal.sync().await
    }

    /// Graceful shutdown: flush the WAL. The WAL writer task stops when the last
    /// `Arc` to the store (and thus the WAL) is dropped.
    pub async fn shutdown(&self) -> Result<()> {
        self.wal.sync().await
    }

    /// Read records for `tp` starting at `offset`, up to ~`max_bytes`.
    /// Resolution is always scoped to `tp`: hot (memory) → cold (object storage).
    /// The WAL is never read here.
    pub async fn read(
        &self,
        tp: &TopicPartition,
        offset: Offset,
        max_bytes: usize,
    ) -> Result<Vec<WalRecord>> {
        match self.index.read_plan(tp, offset, max_bytes) {
            ReadPlan::Hot(records) => {
                // Served from RAM; clone out of the Arcs (payloads are ref-counted Bytes).
                Ok(records.iter().map(|r| (**r).clone()).collect())
            }
            ReadPlan::Cold { object_key, .. } => {
                let cache_key = format!("{tp}:{offset}");
                if let Some(bytes) = self.cache.get(&cache_key).await? {
                    if let Ok(records) = bincode::deserialize::<Vec<WalRecord>>(&bytes) {
                        return Ok(records);
                    }
                }

                let segment = self.upload_manager.download_segment(&object_key).await?;
                let all: Vec<WalRecord> = bincode::deserialize(&segment.data).map_err(|e| {
                    RustMqError::Storage(format!("failed to decode cold segment {object_key}: {e}"))
                })?;

                let mut out = Vec::new();
                let mut bytes_acc = 0usize;
                for record in all.into_iter().filter(|r| r.offset >= offset) {
                    let sz = mem_size(&record) as usize;
                    if !out.is_empty() && bytes_acc + sz > max_bytes {
                        break;
                    }
                    bytes_acc += sz;
                    out.push(record);
                }

                if let Ok(serialized) = bincode::serialize(&out) {
                    let _ = self.cache.put(&cache_key, Bytes::from(serialized)).await;
                }
                Ok(out)
            }
            ReadPlan::Empty => Ok(Vec::new()),
        }
    }

    /// Spawn the background tiering loop: periodically (and on demand under memory
    /// pressure) seal the active segment and move sealed segments to object storage,
    /// evicting the tiered records from the hot tier.
    pub fn spawn_tiering_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let store = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = tick.tick() => {}
                    _ = store.tiering_wakeup.notified() => {}
                }
                // Under memory pressure, force-seal so the active segment becomes tierable.
                let force = store.index.hot_bytes() >= store.soft_limit();
                if let Err(e) = store.tier_once(force).await {
                    tracing::error!("tiering cycle failed: {e}");
                }
            }
        })
    }

    /// One tiering pass with size/age-based sealing (the default, periodic path).
    pub async fn run_tiering_cycle(&self) -> Result<()> {
        self.tier_once(false).await
    }

    /// One tiering pass: seal (forced or threshold-based), then tier every sealed
    /// segment (oldest first), then wake any appends blocked on the memory budget.
    async fn tier_once(&self, force_seal: bool) -> Result<()> {
        if force_seal {
            self.wal.force_seal().await?;
        } else {
            self.wal.maybe_seal().await?;
        }
        for seq in self.wal.sealed_segments().await? {
            self.tier_segment(seq).await?;
        }
        // Evictions in tier_segment freed memory; let blocked appends re-check.
        self.memory_available.notify_waiters();
        Ok(())
    }

    /// Demultiplex one sealed WAL segment by partition, upload one object per
    /// `(partition, offset-range)`, flip the index to cold (which evicts those records
    /// from the hot tier), then reclaim the segment.
    ///
    /// Idempotent: re-uploading the same key, re-registering the same range in the
    /// manifest, and re-flipping the same range are all no-ops, so a crash anywhere in
    /// this sequence is safely retried after recovery re-indexes the segment as hot.
    async fn tier_segment(&self, seq: u64) -> Result<()> {
        let records = self.wal.read_segment_records(seq).await?;
        if records.is_empty() {
            // Nothing to upload (e.g. header-only segment); just reclaim.
            self.wal.delete_segment(seq).await?;
            return Ok(());
        }

        // Group by partition, preserving append (offset) order.
        let mut groups: HashMap<TopicPartition, Vec<WalRecord>> = HashMap::new();
        for record in records {
            groups
                .entry(record.topic_partition.clone())
                .or_default()
                .push(record);
        }

        for (tp, recs) in groups {
            // A partition's records within a single segment are a contiguous offset run.
            let start_offset = recs.first().unwrap().offset;
            let end_offset = recs.last().unwrap().offset + 1;
            let data = bincode::serialize(&recs)?;
            let size_bytes = data.len() as u64;
            let segment = WalSegment {
                start_offset,
                end_offset,
                size_bytes,
                data: Bytes::from(data),
                topic_partition: tp.clone(),
            };
            let object_key = self.upload_manager.upload_segment(segment).await?;
            // Durably record the tiered range BEFORE evicting its in-memory copy, so a
            // crash can never leave cold data without an offset→object mapping.
            self.cold_index
                .register(&tp, start_offset, end_offset, &object_key, size_bytes)
                .await?;
            self.index
                .flip_to_cold(&tp, start_offset, end_offset, object_key);
        }

        // All partitions uploaded+indexed; reclaim the WAL segment.
        self.wal.delete_segment(seq).await?;
        Ok(())
    }

    /// Test/diagnostic accessors.
    pub fn index(&self) -> &Arc<PartitionIndex> {
        &self.index
    }
    pub fn wal(&self) -> &Arc<dyn SegmentedLog> {
        &self.wal
    }
}

/// Append-side view for replication and consumer-group offset commits. The inherent
/// `append` (returning a `PhysicalLocation`) takes precedence for callers holding a
/// concrete `PartitionStore`; subsystems holding `Arc<dyn RecordLog>` get this `()` form.
#[async_trait]
impl RecordLog for PartitionStore {
    async fn append(&self, record: &WalRecord) -> Result<()> {
        PartitionStore::append(self, record).await.map(|_| ())
    }

    async fn sync(&self) -> Result<()> {
        PartitionStore::sync(self).await
    }

    fn next_offset(&self, tp: &TopicPartition) -> Offset {
        PartitionStore::next_offset(self, tp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WalConfig;
    use crate::storage::cache::LruCache;
    use crate::storage::traits::WalSegment;
    use crate::storage::wal::SegmentedWal;
    use crate::types::{Record, TopicPartition};
    use async_trait::async_trait;
    use tempfile::TempDir;

    // Hot-path tests never tier, so the upload manager must not be called.
    struct UnusedUploadManager;
    #[async_trait]
    impl UploadManager for UnusedUploadManager {
        async fn upload_segment(&self, _segment: WalSegment) -> Result<String> {
            panic!("upload_segment must not be called on the hot path");
        }
        async fn download_segment(&self, _object_key: &str) -> Result<WalSegment> {
            panic!("download_segment must not be called on the hot path");
        }
        async fn verify_upload(&self, _object_key: &str, _expected: &[u8]) -> Result<bool> {
            Ok(true)
        }
    }

    /// A WAL decorator whose `read_at` panics, proving the serving path never reads the
    /// WAL (invariant 3). All other methods forward to the wrapped WAL.
    struct PanicOnReadAtWal(Arc<dyn SegmentedLog>);
    #[async_trait]
    impl SegmentedLog for PanicOnReadAtWal {
        async fn append(&self, record: &WalRecord) -> Result<PhysicalLocation> {
            self.0.append(record).await
        }
        async fn read_at(&self, _seq: u64, _off: u64, _len: u32) -> Result<Vec<u8>> {
            panic!("serving path must not read the WAL");
        }
        async fn sync(&self) -> Result<()> {
            self.0.sync().await
        }
        async fn maybe_seal(&self) -> Result<Option<u64>> {
            self.0.maybe_seal().await
        }
        async fn force_seal(&self) -> Result<Option<u64>> {
            self.0.force_seal().await
        }
        async fn sealed_segments(&self) -> Result<Vec<u64>> {
            self.0.sealed_segments().await
        }
        async fn read_segment_records(&self, seq: u64) -> Result<Vec<WalRecord>> {
            self.0.read_segment_records(seq).await
        }
        async fn delete_segment(&self, seq: u64) -> Result<()> {
            self.0.delete_segment(seq).await
        }
        async fn recover(&self) -> Result<Vec<(WalRecord, PhysicalLocation)>> {
            self.0.recover().await
        }
        async fn get_end_offset(&self) -> Result<u64> {
            self.0.get_end_offset().await
        }
    }

    fn wal_config(dir: &std::path::Path) -> WalConfig {
        WalConfig {
            path: dir.to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        }
    }

    fn rec(topic: &str, partition: u32, offset: u64, value: &[u8]) -> WalRecord {
        WalRecord {
            topic_partition: TopicPartition {
                topic: topic.to_string(),
                partition,
            },
            offset,
            record: Record::new(None, value.to_vec(), vec![], 0),
            crc32: 0,
        }
    }

    async fn manifest(dir: &std::path::Path) -> Arc<ColdIndexManifest> {
        Arc::new(
            ColdIndexManifest::open(dir.join("cold.manifest"))
                .await
                .unwrap(),
        )
    }

    async fn make_store(dir: &std::path::Path) -> Arc<PartitionStore> {
        let wal: Arc<dyn SegmentedLog> =
            Arc::new(SegmentedWal::new(wal_config(dir)).await.unwrap());
        let upload: Arc<dyn UploadManager> = Arc::new(UnusedUploadManager);
        let cache: Arc<dyn Cache> = Arc::new(LruCache::new(1024 * 1024));
        // Unbounded hot tier (0): hot-path tests never tier.
        PartitionStore::new(wal, upload, cache, 0, manifest(dir).await)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn interleaved_multi_partition_reads_are_isolated() {
        // The correctness gate: the original bug returned the wrong partition's data
        // because reads weren't partition-scoped.
        let dir = TempDir::new().unwrap();
        let store = make_store(dir.path()).await;

        let a = TopicPartition {
            topic: "topic".into(),
            partition: 0,
        };
        let b = TopicPartition {
            topic: "topic".into(),
            partition: 1,
        };

        store.append(&rec("topic", 0, 0, b"A0")).await.unwrap();
        store.append(&rec("topic", 1, 0, b"B0")).await.unwrap();
        store.append(&rec("topic", 0, 1, b"A1")).await.unwrap();
        store.append(&rec("topic", 1, 1, b"B1")).await.unwrap();

        let a_records = store.read(&a, 0, 1_000_000).await.unwrap();
        let a_values: Vec<&[u8]> = a_records.iter().map(|r| r.record.value.as_ref()).collect();
        assert_eq!(a_values, vec![b"A0".as_ref(), b"A1".as_ref()]);

        let b_records = store.read(&b, 0, 1_000_000).await.unwrap();
        let b_values: Vec<&[u8]> = b_records.iter().map(|r| r.record.value.as_ref()).collect();
        assert_eq!(b_values, vec![b"B0".as_ref(), b"B1".as_ref()]);

        let a_tail = store.read(&a, 1, 1_000_000).await.unwrap();
        assert_eq!(a_tail.len(), 1);
        assert_eq!(a_tail[0].record.value.as_ref(), b"A1");

        assert_eq!(store.next_offset(&a), 2);
        assert_eq!(store.next_offset(&b), 2);
    }

    #[tokio::test]
    async fn read_your_writes_served_from_memory_not_wal() {
        // Invariant 3: un-tiered reads come from the in-memory hot tier, never the WAL.
        // The WAL panics if read_at is called on the serving path.
        let dir = TempDir::new().unwrap();
        let inner: Arc<dyn SegmentedLog> =
            Arc::new(SegmentedWal::new(wal_config(dir.path())).await.unwrap());
        let wal: Arc<dyn SegmentedLog> = Arc::new(PanicOnReadAtWal(inner));
        let upload: Arc<dyn UploadManager> = Arc::new(UnusedUploadManager);
        let cache: Arc<dyn Cache> = Arc::new(LruCache::new(1024 * 1024));
        let store = PartitionStore::new(wal, upload, cache, 0, manifest(dir.path()).await)
            .await
            .unwrap();

        let a = TopicPartition {
            topic: "topic".into(),
            partition: 0,
        };
        let b = TopicPartition {
            topic: "topic".into(),
            partition: 1,
        };
        for i in 0..4u64 {
            store
                .append(&rec("topic", 0, i, format!("A{i}").as_bytes()))
                .await
                .unwrap();
            store
                .append(&rec("topic", 1, i, format!("B{i}").as_bytes()))
                .await
                .unwrap();
        }

        // These reads would panic if they touched the WAL.
        let a_records = store.read(&a, 0, 1_000_000).await.unwrap();
        assert_eq!(a_records.len(), 4);
        assert_eq!(a_records[2].record.value.as_ref(), b"A2");
        let b_records = store.read(&b, 1, 1_000_000).await.unwrap();
        assert_eq!(b_records.first().unwrap().record.value.as_ref(), b"B1");
        assert!(store.index().hot_bytes() > 0);
    }

    #[tokio::test]
    async fn read_past_high_watermark_is_empty() {
        let dir = TempDir::new().unwrap();
        let store = make_store(dir.path()).await;
        let a = TopicPartition {
            topic: "t".into(),
            partition: 0,
        };
        store.append(&rec("t", 0, 0, b"x")).await.unwrap();
        assert!(store.read(&a, 5, 1000).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn recovery_repopulates_hot_cache_from_disk() {
        // Invariant 1 across restart: recovery rebuilds the in-memory hot tier from the
        // WAL so reads are again served from memory.
        let dir = TempDir::new().unwrap();
        {
            let store = make_store(dir.path()).await;
            for i in 0..3u64 {
                store
                    .append(&rec("t", 0, i, format!("v{i}").as_bytes()))
                    .await
                    .unwrap();
            }
            store.wal().maybe_seal().await.unwrap();
            store.sync().await.unwrap();
        }
        let store2 = make_store(dir.path()).await;
        let a = TopicPartition {
            topic: "t".into(),
            partition: 0,
        };
        assert_eq!(store2.next_offset(&a), 3);
        assert!(
            store2.index().hot_bytes() > 0,
            "recovery must repopulate the hot tier"
        );
        let records = store2.read(&a, 0, 1_000_000).await.unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].record.value.as_ref(), b"v1");
    }

    // ---- Tiering: real upload/download round-trip over LocalObjectStorage ----

    use crate::config::{ObjectStorageConfig, StorageType};
    use crate::storage::object_storage::{LocalObjectStorage, UploadManagerImpl};
    use crate::storage::partition_index::ReadPlan;
    use crate::storage::traits::ObjectStorage;

    fn obj_config(obj_dir: &std::path::Path) -> ObjectStorageConfig {
        ObjectStorageConfig {
            storage_type: StorageType::Local {
                path: obj_dir.to_path_buf(),
            },
            bucket: "test".into(),
            region: String::new(),
            endpoint: String::new(),
            access_key: None,
            secret_key: None,
            service_account_path: None,
            multipart_threshold: 100 * 1024 * 1024, // force simple (verified) uploads
            max_concurrent_uploads: 4,
        }
    }

    async fn make_tiered_store(
        wal_dir: &std::path::Path,
        obj_dir: &std::path::Path,
        segment_size_bytes: u64,
        hot_cache_max_bytes: u64,
    ) -> Arc<PartitionStore> {
        let mut cfg = wal_config(wal_dir);
        cfg.segment_size_bytes = segment_size_bytes;
        let wal: Arc<dyn SegmentedLog> = Arc::new(SegmentedWal::new(cfg).await.unwrap());
        let object_storage: Arc<dyn ObjectStorage> =
            Arc::new(LocalObjectStorage::new(obj_dir.to_path_buf()).unwrap());
        let upload: Arc<dyn UploadManager> =
            Arc::new(UploadManagerImpl::new(object_storage, obj_config(obj_dir)));
        let cache: Arc<dyn Cache> = Arc::new(LruCache::new(1024 * 1024));
        PartitionStore::new(
            wal,
            upload,
            cache,
            hot_cache_max_bytes,
            manifest(wal_dir).await,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn tiered_roundtrip_multi_partition() {
        let wal_dir = TempDir::new().unwrap();
        let obj_dir = TempDir::new().unwrap();
        // Small segment so the first tiering cycle seals + uploads everything.
        let store = make_tiered_store(wal_dir.path(), obj_dir.path(), 256, 0).await;

        let a = TopicPartition {
            topic: "t".into(),
            partition: 0,
        };
        let b = TopicPartition {
            topic: "t".into(),
            partition: 1,
        };

        for i in 0..6u64 {
            store
                .append(&rec("t", 0, i, format!("A{i}").as_bytes()))
                .await
                .unwrap();
            store
                .append(&rec("t", 1, i, format!("B{i}").as_bytes()))
                .await
                .unwrap();
        }
        assert!(store.index().hot_bytes() > 0);

        store.run_tiering_cycle().await.unwrap();

        // The sealed segment must be reclaimed (only the fresh active remains).
        assert!(store.wal().sealed_segments().await.unwrap().is_empty());
        // Eviction: tiered records are no longer held in memory.
        assert_eq!(
            store.index().hot_bytes(),
            0,
            "tiered records must be evicted from the hot tier"
        );

        // Offsets are now cold (served from object storage), per partition.
        assert!(matches!(
            store.index().read_plan(&a, 0, 1_000_000),
            ReadPlan::Cold { .. }
        ));

        let a_records = store.read(&a, 0, 1_000_000).await.unwrap();
        assert_eq!(a_records.len(), 6);
        for r in &a_records {
            assert!(
                r.record.value.starts_with(b"A"),
                "partition 0 must only see A* data"
            );
        }
        let b_records = store.read(&b, 0, 1_000_000).await.unwrap();
        assert_eq!(b_records.len(), 6);
        for r in &b_records {
            assert!(
                r.record.value.starts_with(b"B"),
                "partition 1 must only see B* data"
            );
        }
        let a_tail = store.read(&a, 3, 1_000_000).await.unwrap();
        assert_eq!(a_tail.first().unwrap().offset, 3);
        assert_eq!(a_tail.first().unwrap().record.value.as_ref(), b"A3");
    }

    #[tokio::test]
    async fn tier_after_restart_is_safe() {
        // Crash-before-reclaim safety: a sealed-but-untiered segment is re-indexed on
        // restart and tiered on the next cycle, with cold reads still correct.
        let wal_dir = TempDir::new().unwrap();
        let obj_dir = TempDir::new().unwrap();
        {
            let store = make_tiered_store(wal_dir.path(), obj_dir.path(), 256, 0).await;
            for i in 0..4u64 {
                store
                    .append(&rec("t", 0, i, format!("A{i}").as_bytes()))
                    .await
                    .unwrap();
            }
            store.wal().maybe_seal().await.unwrap(); // sealed, but NOT tiered (simulated crash)
            store.sync().await.unwrap();
        }
        let store2 = make_tiered_store(wal_dir.path(), obj_dir.path(), 256, 0).await;
        let a = TopicPartition {
            topic: "t".into(),
            partition: 0,
        };
        assert_eq!(store2.next_offset(&a), 4);
        store2.run_tiering_cycle().await.unwrap();
        assert!(matches!(
            store2.index().read_plan(&a, 0, 1_000_000),
            ReadPlan::Cold { .. }
        ));
        let records = store2.read(&a, 0, 1_000_000).await.unwrap();
        assert_eq!(records.len(), 4);
        assert_eq!(records[2].record.value.as_ref(), b"A2");
    }

    #[tokio::test]
    async fn cold_index_survives_restart() {
        // Phase 5 headline: after data is fully tiered (WAL segment uploaded AND
        // reclaimed), a restart must still serve it. The cold offset→object mapping
        // comes only from the durable manifest — the WAL no longer has these records.
        let wal_dir = TempDir::new().unwrap();
        let obj_dir = TempDir::new().unwrap();
        let a = TopicPartition {
            topic: "t".into(),
            partition: 0,
        };
        {
            let store = make_tiered_store(wal_dir.path(), obj_dir.path(), 256, 0).await;
            for i in 0..6u64 {
                store
                    .append(&rec("t", 0, i, format!("A{i}").as_bytes()))
                    .await
                    .unwrap();
            }
            store.run_tiering_cycle().await.unwrap();
            assert_eq!(
                store.index().hot_bytes(),
                0,
                "all data must be tiered (cold) before restart"
            );
            store.sync().await.unwrap();
        }

        // Restart: a fresh store over the same WAL dir (manifest lives here) + objects.
        let store2 = make_tiered_store(wal_dir.path(), obj_dir.path(), 256, 0).await;

        // High-watermark restored from the manifest alone (WAL has no hot records).
        assert_eq!(store2.next_offset(&a), 6);
        assert_eq!(store2.index().hot_bytes(), 0);
        // Reads resolve cold (object storage), not the WAL.
        assert!(matches!(
            store2.index().read_plan(&a, 0, 1_000_000),
            ReadPlan::Cold { .. }
        ));

        let records = store2.read(&a, 0, 1_000_000).await.unwrap();
        assert_eq!(records.len(), 6, "cold data must survive restart");
        for (i, r) in records.iter().enumerate() {
            assert_eq!(r.offset, i as u64);
            assert_eq!(r.record.value.as_ref(), format!("A{i}").as_bytes());
        }
    }

    #[tokio::test]
    async fn mixed_cold_and_hot_after_restart() {
        // A restart must stitch together cold ranges (manifest) and un-tiered hot
        // records (WAL) into one continuous offset space.
        let wal_dir = TempDir::new().unwrap();
        let obj_dir = TempDir::new().unwrap();
        let a = TopicPartition {
            topic: "t".into(),
            partition: 0,
        };
        {
            let store = make_tiered_store(wal_dir.path(), obj_dir.path(), 256, 0).await;
            for i in 0..6u64 {
                store
                    .append(&rec("t", 0, i, format!("A{i}").as_bytes()))
                    .await
                    .unwrap();
            }
            store.run_tiering_cycle().await.unwrap(); // 0..6 -> cold
            assert_eq!(store.index().hot_bytes(), 0);
            // More records that stay hot (sealed + synced, but not tiered).
            for i in 6..10u64 {
                store
                    .append(&rec("t", 0, i, format!("A{i}").as_bytes()))
                    .await
                    .unwrap();
            }
            store.wal().maybe_seal().await.unwrap();
            store.sync().await.unwrap();
        }

        let store2 = make_tiered_store(wal_dir.path(), obj_dir.path(), 256, 0).await;
        assert_eq!(store2.next_offset(&a), 10);
        // 0..6 cold (from manifest), 6..10 hot (from WAL).
        assert!(matches!(
            store2.index().read_plan(&a, 0, 1_000_000),
            ReadPlan::Cold { .. }
        ));
        assert!(matches!(
            store2.index().read_plan(&a, 6, 1_000_000),
            ReadPlan::Hot(_)
        ));

        // Read the whole partition back, hopping batches across the cold→hot boundary.
        let mut all = Vec::new();
        let mut off = 0u64;
        while off < 10 {
            let batch = store2.read(&a, off, 1_000_000).await.unwrap();
            assert!(!batch.is_empty(), "no records at offset {off}");
            off = batch.last().unwrap().offset + 1;
            all.extend(batch);
        }
        assert_eq!(all.len(), 10);
        for (i, r) in all.iter().enumerate() {
            assert_eq!(r.offset, i as u64);
            assert_eq!(r.record.value.as_ref(), format!("A{i}").as_bytes());
        }
    }

    #[tokio::test]
    async fn memory_budget_bounds_hot_cache_under_load() {
        // Invariant 5: with a small budget and a working upload, sustained appends keep
        // the hot tier bounded (backpressure + tiering eviction), and all data remains
        // readable.
        let wal_dir = TempDir::new().unwrap();
        let obj_dir = TempDir::new().unwrap();
        let budget = 8 * 1024;
        let store = make_tiered_store(wal_dir.path(), obj_dir.path(), 4096, budget).await;
        let _tiering = store.spawn_tiering_task();

        let a = TopicPartition {
            topic: "t".into(),
            partition: 0,
        };
        const N: u64 = 150;
        for i in 0..N {
            store.append(&rec("t", 0, i, &[b'x'; 200])).await.unwrap();
            // Hot tier must never grow toward the full dataset (~45 KB); it stays near
            // the budget. The slack covers the single in-flight append + estimate drift.
            assert!(
                store.index().hot_bytes() <= budget + 16 * 1024,
                "hot tier unbounded: {} bytes after {} appends",
                store.index().hot_bytes(),
                i
            );
        }

        // Drain remaining hot data, then read everything back in order.
        for _ in 0..50 {
            store.run_tiering_cycle().await.unwrap();
        }
        let mut all = Vec::new();
        let mut off = 0u64;
        while off < N {
            let batch = store.read(&a, off, 4096).await.unwrap();
            if batch.is_empty() {
                break;
            }
            off = batch.last().unwrap().offset + 1;
            all.extend(batch);
        }
        assert_eq!(all.len() as u64, N, "all appended records must be readable");
        for (i, r) in all.iter().enumerate() {
            assert_eq!(r.offset, i as u64);
        }
    }
}
