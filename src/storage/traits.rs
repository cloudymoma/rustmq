use crate::{Result, types::*};
use async_trait::async_trait;
use bytes::Bytes;
use std::any::Any;
use std::ops::Range;
use tokio::io::{AsyncRead, AsyncWrite};

#[async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn get_range(&self, key: &str, range: Range<u64>) -> Result<Bytes>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn exists(&self, key: &str) -> Result<bool>;

    /// Open a streaming reader for an object.
    /// Enables processing large objects without loading them entirely into memory.
    async fn open_read_stream(&self, key: &str) -> Result<Box<dyn AsyncRead + Send + Unpin>>;

    /// Open a streaming writer for an object.
    /// Enables writing large objects chunk by chunk to prevent OOM issues.
    async fn open_write_stream(&self, key: &str) -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
}

#[async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Bytes>>;
    async fn put(&self, key: &str, value: Bytes) -> Result<()>;
    async fn remove(&self, key: &str) -> Result<()>;
    async fn clear(&self) -> Result<()>;
    async fn size(&self) -> Result<usize>;
    fn as_any(&self) -> Option<&dyn Any>;
}

#[async_trait]
pub trait Stream: Send + Sync {
    async fn append(&self, record: Record) -> Result<Offset>;
    async fn read(&self, offset: Offset, max_bytes: usize) -> Result<Vec<Record>>;
    async fn get_high_watermark(&self) -> Result<Offset>;
    async fn get_low_watermark(&self) -> Result<Offset>;
}

pub trait BufferPool: Send + Sync {
    fn get_aligned_buffer(&self, size: usize) -> Result<Vec<u8>>;
    fn return_buffer(&self, buffer: Vec<u8>);
}

#[async_trait]
pub trait UploadManager: Send + Sync {
    async fn upload_segment(&self, segment: WalSegment) -> Result<String>;
    async fn download_segment(&self, object_key: &str) -> Result<WalSegment>;
    async fn verify_upload(&self, object_key: &str, expected_data: &[u8]) -> Result<bool>;
    async fn delete_object(&self, object_key: &str) -> Result<()>;
}

/// Append-oriented view of the storage engine used by subsystems that write to the
/// shared WAL but do not need partition-correct reads (replication, consumer-group
/// offset commits). Implemented by `PartitionStore`, which also updates the
/// partition index on every append so follower-replicated records are indexed.
#[async_trait]
pub trait RecordLog: Send + Sync {
    /// Append a record to the shared WAL and index it.
    async fn append(&self, record: &WalRecord) -> Result<()>;
    /// Flush the WAL to disk.
    async fn sync(&self) -> Result<()>;
    /// One past the highest indexed offset for `tp` (0 if unknown). Used for
    /// follower lag and next-write position.
    fn next_offset(&self, tp: &TopicPartition) -> Offset;
}

/// Physical location of a record body within the segmented WAL.
///
/// Returned by the segmented WAL on append and stored in the partition index so a
/// per-partition logical offset can be resolved to its exact bytes on disk.
/// `file_offset` points at the record body (after the 8-byte frame length prefix);
/// `frame_len` is the body length in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalLocation {
    pub wal_segment_seq: u64,
    pub file_offset: u64,
    pub frame_len: u32,
}

#[derive(Debug, Clone)]
pub struct WalSegment {
    pub start_offset: u64,
    pub end_offset: u64,
    pub size_bytes: u64,
    pub data: Bytes,
    pub topic_partition: TopicPartition,
}

impl WalSegment {
    pub fn size(&self) -> usize {
        self.data.len()
    }
}
