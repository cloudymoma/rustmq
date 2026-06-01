//! Segmented write-ahead log.
//!
//! A single shared, sequential WAL split into segment files `wal-{seq}.log`. The
//! log is **physical and partition-blind**: it frames and appends record bytes and
//! returns a [`PhysicalLocation`]; mapping per-partition logical offsets to records
//! is the job of the partition index ([`crate::storage::partition_index`]). Tiering
//! (demux + upload) and reclamation are driven by `PartitionStore`, not the WAL.
//!
//! On-disk layout (O_DIRECT-compatible — see the block writer below):
//! ```text
//! [ 4096-byte block-aligned header: magic | version | seq | created_ms | zero pad ]
//! [ frame: u64 LE body_len | body (bincode WalRecord) ]*
//! [ zero padding to the next block boundary ]            (written as part of the tail block)
//! ```
//! Recovery frame-walks from the header; a `body_len == 0` marks end-of-data within
//! a block (so the padded O_DIRECT format recovers with the same code).
//!
//! ## O_DIRECT writer
//! Phase 3 made the active segment **write-only** (consumer reads are served from the
//! in-memory hot tier, never the WAL), so the active segment can be opened with
//! `O_DIRECT` to bypass the page cache. O_DIRECT requires every write's buffer,
//! file offset, and length to be block-aligned, so the writer batches frames into a
//! 4 KiB-block-aligned tail: complete blocks are written and dropped from the tail
//! (advancing `base`); the trailing partial block is written zero-padded on every
//! sync/seal and **rewritten in place** as more frames arrive (`base` only advances
//! across complete blocks). The same aligned path runs whether or not O_DIRECT is
//! enabled, so a runtime probe simply selects the open flag — on filesystems without
//! O_DIRECT support (e.g. tmpfs) it falls back to a buffered open, identical logic.

use super::aligned_buf::{AlignedBuf, BLOCK_SIZE};
use crate::storage::traits::PhysicalLocation;
use crate::{Result, config::WalConfig, error::RustMqError, types::WalRecord};
use async_trait::async_trait;
use std::fs::File as SyncFile;
use std::os::unix::fs::FileExt; // write_all_at (pwrite) — required for O_DIRECT
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::time::Duration;

const HEADER_SIZE: u64 = 4096;
const SEGMENT_MAGIC: u32 = 0x524D_5147; // "RMQG"
const SEGMENT_VERSION: u16 = 1;
const FRAME_LEN_PREFIX: u64 = 8; // u64 body length prefix
const DEFAULT_WAL_CHANNEL_CAPACITY: usize = 10_000;

/// The physical, segmented WAL interface used by `PartitionStore`.
#[async_trait]
pub trait SegmentedLog: Send + Sync {
    /// Frame and append a record to the active segment; returns where its body landed.
    async fn append(&self, record: &WalRecord) -> Result<PhysicalLocation>;
    /// Read `len` raw body bytes at `file_offset` within segment `segment_seq`.
    async fn read_at(&self, segment_seq: u64, file_offset: u64, len: u32) -> Result<Vec<u8>>;
    /// Flush the active segment to disk.
    async fn sync(&self) -> Result<()>;
    /// Seal+rotate the active segment if it exceeds the size/age threshold. Returns the
    /// sealed segment seq if a rotation happened.
    async fn maybe_seal(&self) -> Result<Option<u64>>;
    /// Seal+rotate the active segment if it holds any data, regardless of size/age.
    /// Used to make recent appends tierable under memory-budget pressure.
    async fn force_seal(&self) -> Result<Option<u64>>;
    /// Sealed, not-yet-deleted segment seqs, oldest first.
    async fn sealed_segments(&self) -> Result<Vec<u64>>;
    /// Deserialize all records in a (sealed) segment, in order. Used for tiering demux.
    async fn read_segment_records(&self, segment_seq: u64) -> Result<Vec<WalRecord>>;
    /// Delete a fully-tiered segment file (reclamation).
    async fn delete_segment(&self, segment_seq: u64) -> Result<()>;
    /// Re-read all on-disk (un-reclaimed) segments and return every record with its
    /// physical location, oldest first. Used to rebuild the partition index on startup.
    async fn recover(&self) -> Result<Vec<(WalRecord, PhysicalLocation)>>;
    /// Total frames appended this process (liveness/health probe).
    async fn get_end_offset(&self) -> Result<u64>;
}

// Commands to the dedicated file task that owns the active segment file.
enum WalCommand {
    Append {
        framed: Vec<u8>,
        response: oneshot::Sender<Result<PhysicalLocation>>,
        _permit: OwnedSemaphorePermit,
    },
    Sync {
        response: oneshot::Sender<Result<()>>,
    },
    MaybeSeal {
        force: bool,
        response: oneshot::Sender<Result<Option<u64>>>,
    },
    /// Periodic durability flush (only sent when `fsync_on_write` is false).
    FlushTick,
    Shutdown,
}

pub struct SegmentedWal {
    dir: PathBuf,
    write_tx: mpsc::Sender<WalCommand>,
    write_semaphore: Arc<Semaphore>,
    active_seq: Arc<AtomicU64>,
    total_appends: Arc<AtomicU64>,
}

fn segment_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("wal-{seq:020}.log"))
}

/// Parse the segment seq out of a `wal-{seq}.log` filename.
fn parse_segment_seq(name: &str) -> Option<u64> {
    name.strip_prefix("wal-")
        .and_then(|s| s.strip_suffix(".log"))
        .and_then(|s| s.parse::<u64>().ok())
}

fn build_header(seq: u64) -> Vec<u8> {
    let mut hdr = vec![0u8; HEADER_SIZE as usize];
    hdr[0..4].copy_from_slice(&SEGMENT_MAGIC.to_le_bytes());
    hdr[4..6].copy_from_slice(&SEGMENT_VERSION.to_le_bytes());
    hdr[8..16].copy_from_slice(&seq.to_le_bytes());
    let created_ms = chrono::Utc::now().timestamp_millis();
    hdr[16..24].copy_from_slice(&created_ms.to_le_bytes());
    hdr
}

/// Round `n` up to the next multiple of [`BLOCK_SIZE`].
fn round_up_block(n: usize) -> usize {
    n.next_multiple_of(BLOCK_SIZE)
}

/// Write `data` to `file` at block-aligned `offset`, zero-padded up to a block multiple,
/// through the reusable block-aligned `scratch` buffer, via `pwrite`. Satisfies
/// O_DIRECT's buffer/offset/length alignment requirements; correct for buffered files
/// too. Synchronous on purpose: O_DIRECT needs a real aligned buffer handed to the
/// `write` syscall, which `tokio::fs::File` does not provide (it copies through an
/// unaligned internal buffer), so the writer runs on a dedicated OS thread.
fn write_blocks(file: &SyncFile, scratch: &mut AlignedBuf, offset: u64, data: &[u8]) -> Result<()> {
    debug_assert_eq!(
        offset % BLOCK_SIZE as u64,
        0,
        "write offset must be block-aligned"
    );
    let rounded = round_up_block(data.len());
    if scratch.capacity() < rounded {
        *scratch = AlignedBuf::new(rounded);
    }
    let buf = scratch.as_mut_slice();
    buf[..data.len()].copy_from_slice(data);
    buf[data.len()..rounded].fill(0); // zero the padding (scratch is reused)
    file.write_all_at(&scratch.as_slice()[..rounded], offset)?;
    Ok(())
}

/// Probe whether `dir`'s filesystem supports `O_DIRECT`. Linux-only; everywhere else
/// (and on filesystems like tmpfs that reject `O_DIRECT`) returns false → buffered open.
fn probe_o_direct(dir: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let probe = dir.join(".odirect-probe");
        let ok = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .custom_flags(libc::O_DIRECT)
            .open(&probe)
            .is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = dir;
        false
    }
}

/// Create a new segment file, opened with `O_DIRECT` when `o_direct`, and write its
/// block-aligned header. Synchronous (runs on the dedicated writer thread / at startup).
fn open_new_segment(dir: &Path, seq: u64, o_direct: bool) -> Result<SyncFile> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).read(true).truncate(true);
    #[cfg(target_os = "linux")]
    if o_direct {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECT);
    }
    let file = options.open(segment_path(dir, seq))?;

    // Write the header as one aligned block (O_DIRECT needs an aligned buffer too).
    let hdr = build_header(seq);
    let mut scratch = AlignedBuf::new(HEADER_SIZE as usize);
    write_blocks(&file, &mut scratch, 0, &hdr)?;
    file.sync_all()?;
    Ok(file)
}

impl SegmentedWal {
    pub async fn new(config: WalConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path).await?;
        let dir = config.path.clone();

        // Open the active segment with O_DIRECT where the filesystem supports it.
        let o_direct = probe_o_direct(&dir);

        // Existing segments (from a prior run) all become sealed; start a fresh active
        // segment at max+1 so we never append into a partially-written tail.
        let existing = scan_segment_seqs(&dir).await?;
        let active_seq_val = existing.iter().max().map(|m| m + 1).unwrap_or(0);
        let active_file = open_new_segment(&dir, active_seq_val, o_direct)?;

        let (write_tx, write_rx) = mpsc::channel(DEFAULT_WAL_CHANNEL_CAPACITY);
        let write_semaphore = Arc::new(Semaphore::new(DEFAULT_WAL_CHANNEL_CAPACITY));
        let active_seq = Arc::new(AtomicU64::new(active_seq_val));
        let total_appends = Arc::new(AtomicU64::new(0));

        let task = FileTask {
            dir: dir.clone(),
            file: active_file,
            o_direct,
            base: HEADER_SIZE,
            tail: Vec::new(),
            scratch: AlignedBuf::new(BLOCK_SIZE),
            active_seq: active_seq_val,
            active_size: 0,
            active_start: Instant::now(),
            needs_flush: false,
            fsync_on_write: config.fsync_on_write,
            segment_size_bytes: config.segment_size_bytes,
            upload_interval_ms: config.upload_interval_ms,
            active_seq_shared: active_seq.clone(),
            total_appends: total_appends.clone(),
        };
        // The writer owns the active O_DIRECT file and does blocking aligned `pwrite`s on
        // its own OS thread (tokio's async File cannot satisfy O_DIRECT alignment).
        std::thread::Builder::new()
            .name("rustmq-wal-writer".to_string())
            .spawn(move || task.run(write_rx))
            .map_err(|e| RustMqError::Wal(format!("failed to spawn WAL writer thread: {e}")))?;

        // Periodic durability flush for the batched (non-fsync-on-write) mode.
        if !config.fsync_on_write {
            let tx = write_tx.clone();
            let interval_ms = config.flush_interval_ms.max(1);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(interval_ms));
                loop {
                    tick.tick().await;
                    if tx.send(WalCommand::FlushTick).await.is_err() {
                        break; // writer thread gone
                    }
                }
            });
        }

        Ok(Self {
            dir,
            write_tx,
            write_semaphore,
            active_seq,
            total_appends,
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.write_tx
            .send(WalCommand::Shutdown)
            .await
            .map_err(|_| RustMqError::Wal("WAL file task unavailable".to_string()))?;
        Ok(())
    }

    /// Ask the file task to seal+rotate the active segment. `force` ignores the
    /// size/age threshold (sealing whenever there is data).
    async fn seal(&self, force: bool) -> Result<Option<u64>> {
        let (tx, rx) = oneshot::channel();
        self.write_tx
            .send(WalCommand::MaybeSeal {
                force,
                response: tx,
            })
            .await
            .map_err(|_| RustMqError::Wal("WAL file task unavailable".to_string()))?;
        rx.await
            .map_err(|_| RustMqError::Wal("WAL file task response failed".to_string()))?
    }
}

async fn scan_segment_seqs(dir: &Path) -> Result<Vec<u64>> {
    let mut seqs = Vec::new();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(seqs),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if let Some(name) = entry.file_name().to_str() {
            if let Some(seq) = parse_segment_seq(name) {
                seqs.push(seq);
            }
        }
    }
    seqs.sort_unstable();
    Ok(seqs)
}

/// Frame-walk a whole segment buffer (including its header), yielding
/// `(body_offset, body_bytes)` for each record.
fn walk_frames(buf: &[u8]) -> Vec<(u64, &[u8])> {
    let mut out = Vec::new();
    let mut pos = HEADER_SIZE as usize;
    while pos + FRAME_LEN_PREFIX as usize <= buf.len() {
        let len_bytes: [u8; 8] = buf[pos..pos + 8].try_into().unwrap();
        let body_len = u64::from_le_bytes(len_bytes) as usize;
        if body_len == 0 {
            break; // zero padding / end of data
        }
        let body_start = pos + 8;
        let body_end = body_start + body_len;
        if body_end > buf.len() {
            break; // incomplete trailing record
        }
        out.push((body_start as u64, &buf[body_start..body_end]));
        pos = body_end;
    }
    out
}

impl Drop for SegmentedWal {
    fn drop(&mut self) {
        let _ = self.write_tx.try_send(WalCommand::Shutdown);
    }
}

// The single task that owns the active segment file and serializes all writes,
// rotations and flushes (so offset assignment and rotation can't race).
//
// Block-aligned write model: `base` is the block-aligned file offset of the current
// partial (incomplete) block; `tail` holds the data bytes from `base` onward that are
// not yet part of a written *complete* block. On append, complete blocks are flushed
// and dropped from `tail` (advancing `base`); the trailing partial is written
// zero-padded on sync/seal and rewritten in place as it grows.
struct FileTask {
    dir: PathBuf,
    file: SyncFile,
    o_direct: bool,
    /// Block-aligned file offset corresponding to `tail[0]`.
    base: u64,
    /// Unflushed-as-complete-block data bytes, starting at `base`.
    tail: Vec<u8>,
    /// Reusable block-aligned buffer for O_DIRECT-compatible writes.
    scratch: AlignedBuf,
    active_seq: u64,
    active_size: u64,
    active_start: Instant,
    needs_flush: bool,
    fsync_on_write: bool,
    segment_size_bytes: u64,
    upload_interval_ms: u64,
    active_seq_shared: Arc<AtomicU64>,
    total_appends: Arc<AtomicU64>,
}

impl FileTask {
    /// Blocking command loop on the dedicated writer thread. Serializes all writes,
    /// rotations and flushes so offset assignment and rotation can't race.
    fn run(mut self, mut rx: mpsc::Receiver<WalCommand>) {
        while let Some(cmd) = rx.blocking_recv() {
            match cmd {
                WalCommand::Append {
                    framed,
                    response,
                    _permit,
                } => {
                    let _ = response.send(self.handle_append(framed));
                }
                WalCommand::Sync { response } => {
                    let _ = response.send(self.flush());
                }
                WalCommand::MaybeSeal { force, response } => {
                    let _ = response.send(self.handle_maybe_seal(force));
                }
                WalCommand::FlushTick => {
                    if self.needs_flush {
                        let _ = self.flush();
                    }
                }
                WalCommand::Shutdown => {
                    if self.needs_flush {
                        let _ = self.flush();
                    }
                    return;
                }
            }
        }
        // Channel closed without an explicit shutdown: best-effort final flush.
        if self.needs_flush {
            let _ = self.flush();
        }
    }

    fn handle_append(&mut self, framed: Vec<u8>) -> Result<PhysicalLocation> {
        let body_len = (framed.len() as u64 - FRAME_LEN_PREFIX) as u32;
        // Absolute file offset of the body — stable regardless of later block draining,
        // since draining advances `base` and shrinks `tail` by the same amount.
        let body_offset = self.base + self.tail.len() as u64 + FRAME_LEN_PREFIX;
        self.tail.extend_from_slice(&framed);

        // Flush any now-complete blocks (keeps `tail` < one block + this frame), then
        // satisfy durability: fsync mode flushes the partial block now; otherwise the
        // partial block is flushed on the next sync/tick/seal.
        self.write_complete_blocks()?;
        if self.fsync_on_write {
            self.flush()?;
        } else {
            self.needs_flush = true;
        }

        self.active_size += framed.len() as u64;
        self.total_appends.fetch_add(1, Ordering::SeqCst);
        Ok(PhysicalLocation {
            wal_segment_seq: self.active_seq,
            file_offset: body_offset,
            frame_len: body_len,
        })
    }

    /// Write and drop every complete block from `tail`, advancing `base`. The trailing
    /// partial block (if any) stays in `tail` to be rewritten by `flush`.
    fn write_complete_blocks(&mut self) -> Result<()> {
        let full = self.tail.len() / BLOCK_SIZE * BLOCK_SIZE;
        if full == 0 {
            return Ok(());
        }
        write_blocks(&self.file, &mut self.scratch, self.base, &self.tail[..full])?;
        self.base += full as u64;
        self.tail.drain(..full);
        self.needs_flush = true;
        Ok(())
    }

    /// Persist all appended data durably: write the trailing partial block (zero-padded
    /// to a full block) at `base`, then fsync. `base` does not advance — the partial
    /// block is rewritten in place as more frames arrive.
    fn flush(&mut self) -> Result<()> {
        if !self.tail.is_empty() {
            write_blocks(&self.file, &mut self.scratch, self.base, &self.tail)?;
        }
        self.file.sync_data()?;
        self.needs_flush = false;
        Ok(())
    }

    fn handle_maybe_seal(&mut self, force: bool) -> Result<Option<u64>> {
        let has_data = self.active_size > 0;
        let should_seal = has_data
            && (force
                || self.active_size >= self.segment_size_bytes
                || self.active_start.elapsed() >= Duration::from_millis(self.upload_interval_ms));
        if !should_seal {
            return Ok(None);
        }

        // Flush all data durably, then seal the current active segment.
        self.flush()?;
        let sealed_seq = self.active_seq;

        // Rotate to a fresh active segment.
        let next_seq = self.active_seq + 1;
        self.file = open_new_segment(&self.dir, next_seq, self.o_direct)?;
        self.active_seq = next_seq;
        self.base = HEADER_SIZE;
        self.tail.clear();
        self.active_size = 0;
        self.active_start = Instant::now();
        self.active_seq_shared.store(next_seq, Ordering::SeqCst);

        Ok(Some(sealed_seq))
    }
}

#[async_trait]
impl SegmentedLog for SegmentedWal {
    async fn append(&self, record: &WalRecord) -> Result<PhysicalLocation> {
        let body = bincode::serialize(record)?;
        let mut framed = Vec::with_capacity(body.len() + FRAME_LEN_PREFIX as usize);
        framed.extend_from_slice(&(body.len() as u64).to_le_bytes());
        framed.extend_from_slice(&body);

        let permit = self
            .write_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| RustMqError::Wal("WAL semaphore closed".to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.write_tx
            .send(WalCommand::Append {
                framed,
                response: tx,
                _permit: permit,
            })
            .await
            .map_err(|_| RustMqError::Wal("WAL file task unavailable".to_string()))?;
        rx.await
            .map_err(|_| RustMqError::Wal("WAL file task response failed".to_string()))?
    }

    async fn read_at(&self, segment_seq: u64, file_offset: u64, len: u32) -> Result<Vec<u8>> {
        // `read_at` is off the consumer serving path (Phase 3: reads come from the hot
        // tier / object storage). It is used by recovery-adjacent diagnostics and tests.
        // Flush first so the active segment's in-memory tail block is on disk, then read
        // through a buffered handle (the active segment may be O_DIRECT for writes only).
        self.sync().await?;
        let mut file = File::open(segment_path(&self.dir, segment_seq)).await?;
        file.seek(SeekFrom::Start(file_offset)).await?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf).await?;
        Ok(buf)
    }

    async fn sync(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.write_tx
            .send(WalCommand::Sync { response: tx })
            .await
            .map_err(|_| RustMqError::Wal("WAL file task unavailable".to_string()))?;
        rx.await
            .map_err(|_| RustMqError::Wal("WAL file task response failed".to_string()))?
    }

    async fn maybe_seal(&self) -> Result<Option<u64>> {
        self.seal(false).await
    }

    async fn force_seal(&self) -> Result<Option<u64>> {
        self.seal(true).await
    }

    async fn sealed_segments(&self) -> Result<Vec<u64>> {
        let active = self.active_seq.load(Ordering::SeqCst);
        let seqs = scan_segment_seqs(&self.dir).await?;
        Ok(seqs.into_iter().filter(|&s| s != active).collect())
    }

    async fn read_segment_records(&self, segment_seq: u64) -> Result<Vec<WalRecord>> {
        let buf = tokio::fs::read(segment_path(&self.dir, segment_seq)).await?;
        let mut records = Vec::new();
        for (_off, body) in walk_frames(&buf) {
            records.push(bincode::deserialize::<WalRecord>(body)?);
        }
        Ok(records)
    }

    async fn delete_segment(&self, segment_seq: u64) -> Result<()> {
        match tokio::fs::remove_file(segment_path(&self.dir, segment_seq)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn recover(&self) -> Result<Vec<(WalRecord, PhysicalLocation)>> {
        let active = self.active_seq.load(Ordering::SeqCst);
        let mut out = Vec::new();
        // Recover every on-disk segment except the freshly-created active one.
        for seq in scan_segment_seqs(&self.dir).await? {
            if seq == active {
                continue;
            }
            let buf = tokio::fs::read(segment_path(&self.dir, seq)).await?;
            for (body_off, body) in walk_frames(&buf) {
                let record: WalRecord = bincode::deserialize(body)?;
                out.push((
                    record,
                    PhysicalLocation {
                        wal_segment_seq: seq,
                        file_offset: body_off,
                        frame_len: body.len() as u32,
                    },
                ));
            }
        }
        Ok(out)
    }

    async fn get_end_offset(&self) -> Result<u64> {
        Ok(self.total_appends.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Record, TopicPartition};
    use tempfile::TempDir;

    fn test_config(dir: &Path, segment_size_bytes: u64) -> WalConfig {
        WalConfig {
            path: dir.to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes,
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

    #[tokio::test]
    async fn append_then_read_at_round_trips() {
        let dir = TempDir::new().unwrap();
        let wal = SegmentedWal::new(test_config(dir.path(), 64 * 1024))
            .await
            .unwrap();

        let r = rec("t", 0, 0, b"hello");
        let loc = wal.append(&r).await.unwrap();
        let bytes = wal
            .read_at(loc.wal_segment_seq, loc.file_offset, loc.frame_len)
            .await
            .unwrap();
        let got: WalRecord = bincode::deserialize(&bytes).unwrap();
        assert_eq!(got.record.value.as_ref(), b"hello");
        assert_eq!(got.topic_partition.topic, "t");
    }

    #[tokio::test]
    async fn maybe_seal_rotates_and_lists_sealed() {
        let dir = TempDir::new().unwrap();
        // Tiny segment so a couple of appends exceed it.
        let wal = SegmentedWal::new(test_config(dir.path(), 64))
            .await
            .unwrap();

        for i in 0..3u64 {
            wal.append(&rec("t", 0, i, &[0u8; 64])).await.unwrap();
        }
        let sealed = wal.maybe_seal().await.unwrap();
        assert!(sealed.is_some(), "should have sealed an over-size segment");
        let sealed_seq = sealed.unwrap();

        let listed = wal.sealed_segments().await.unwrap();
        assert!(listed.contains(&sealed_seq));
        // New appends land in the fresh active segment (a different seq).
        let loc = wal.append(&rec("t", 0, 3, b"x")).await.unwrap();
        assert_ne!(loc.wal_segment_seq, sealed_seq);
    }

    #[tokio::test]
    async fn read_segment_records_and_delete() {
        let dir = TempDir::new().unwrap();
        let wal = SegmentedWal::new(test_config(dir.path(), 64))
            .await
            .unwrap();
        for i in 0..3u64 {
            wal.append(&rec("t", 0, i, &[7u8; 64])).await.unwrap();
        }
        let sealed_seq = wal.maybe_seal().await.unwrap().unwrap();

        let recs = wal.read_segment_records(sealed_seq).await.unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].offset, 0);
        assert_eq!(recs[2].offset, 2);

        wal.delete_segment(sealed_seq).await.unwrap();
        assert!(!wal.sealed_segments().await.unwrap().contains(&sealed_seq));
    }

    #[tokio::test]
    async fn recover_reenumerates_unreclaimed_segments() {
        let dir = TempDir::new().unwrap();
        {
            let wal = SegmentedWal::new(test_config(dir.path(), 64))
                .await
                .unwrap();
            for i in 0..5u64 {
                wal.append(&rec("t", 0, i, &[1u8; 64])).await.unwrap();
            }
            wal.maybe_seal().await.unwrap();
            wal.sync().await.unwrap();
            // drop wal (shutdown) — segments remain on disk
        }
        // Re-open: a new active segment is created; recover() reads the old ones.
        let wal2 = SegmentedWal::new(test_config(dir.path(), 64))
            .await
            .unwrap();
        let recovered = wal2.recover().await.unwrap();
        assert_eq!(recovered.len(), 5);
        for (i, (record, loc)) in recovered.iter().enumerate() {
            assert_eq!(record.offset, i as u64);
            // Locations must point at a real segment and be re-readable.
            let bytes = wal2
                .read_at(loc.wal_segment_seq, loc.file_offset, loc.frame_len)
                .await
                .unwrap();
            let got: WalRecord = bincode::deserialize(&bytes).unwrap();
            assert_eq!(got.offset, i as u64);
        }
    }

    #[tokio::test]
    async fn get_end_offset_counts_appends() {
        let dir = TempDir::new().unwrap();
        let wal = SegmentedWal::new(test_config(dir.path(), 64 * 1024))
            .await
            .unwrap();
        for i in 0..4u64 {
            wal.append(&rec("t", 0, i, b"v")).await.unwrap();
        }
        assert_eq!(wal.get_end_offset().await.unwrap(), 4);
    }

    #[tokio::test]
    async fn segment_files_are_block_aligned_and_multiblock_records_roundtrip() {
        // The block-aligned writer always writes whole blocks (header + frames + zero
        // padding), so every segment file is a whole number of blocks — the on-disk
        // invariant O_DIRECT requires. A record larger than one block exercises frames
        // that span block boundaries. Holds in both O_DIRECT and buffered-fallback mode.
        let dir = TempDir::new().unwrap();
        let wal = SegmentedWal::new(test_config(dir.path(), 1024 * 1024))
            .await
            .unwrap();

        let big = vec![0xABu8; 10_000]; // > 2 blocks
        wal.append(&rec("t", 0, 0, b"small")).await.unwrap();
        wal.append(&rec("t", 0, 1, &big)).await.unwrap();
        wal.append(&rec("t", 0, 2, b"after-big")).await.unwrap();
        let sealed = wal.force_seal().await.unwrap().unwrap();

        let len = std::fs::metadata(segment_path(dir.path(), sealed))
            .unwrap()
            .len();
        assert_eq!(
            len % BLOCK_SIZE as u64,
            0,
            "segment file not block-aligned: {len}"
        );

        let recs = wal.read_segment_records(sealed).await.unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].record.value.as_ref(), b"small");
        assert_eq!(recs[1].record.value.as_ref(), big.as_slice());
        assert_eq!(recs[2].record.value.as_ref(), b"after-big");
    }

    #[tokio::test]
    async fn o_direct_path_roundtrips_on_capable_filesystem() {
        // Exercise the real O_DIRECT path on a capable filesystem by placing the WAL on
        // the crate's build disk (`target/`) rather than the default tmpdir (often tmpfs,
        // which rejects O_DIRECT). If the probe still reports no support (e.g. CI overlay
        // fs), the same code runs buffered and the assertions still hold — either way the
        // aligned writer must round-trip without EINVAL.
        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/target");
        let _ = std::fs::create_dir_all(base);
        let dir = tempfile::Builder::new()
            .prefix("rustmq-odirect-test-")
            .tempdir_in(base)
            .unwrap();

        let o_direct = probe_o_direct(dir.path());
        let mut cfg = test_config(dir.path(), 1024 * 1024);
        cfg.fsync_on_write = true; // force the synchronous write+flush path per append
        let wal = SegmentedWal::new(cfg).await.unwrap();

        // Mixed sizes including a multi-block record, with fsync on every append.
        let big = vec![0xCDu8; 9000];
        wal.append(&rec("t", 0, 0, b"a")).await.unwrap();
        wal.append(&rec("t", 0, 1, &big)).await.unwrap();
        wal.append(&rec("t", 0, 2, b"c")).await.unwrap();
        let sealed = wal.force_seal().await.unwrap().unwrap();

        let recs = wal.read_segment_records(sealed).await.unwrap();
        assert_eq!(recs.len(), 3, "o_direct_enabled={o_direct}");
        assert_eq!(recs[0].record.value.as_ref(), b"a");
        assert_eq!(recs[1].record.value.as_ref(), big.as_slice());
        assert_eq!(recs[2].record.value.as_ref(), b"c");

        // Recovery (sequential read of the padded, block-aligned segment) also round-trips.
        let recovered = wal.recover().await.unwrap();
        assert_eq!(recovered.len(), 3);
        assert_eq!(recovered[1].0.record.value.as_ref(), big.as_slice());
    }
}
