use crate::{Result, types::*};
use async_trait::async_trait;
use bytes::Bytes;
use std::any::Any;
use std::ops::Range;
use tokio::io::{AsyncRead, AsyncWrite};

#[async_trait]
pub trait WriteAheadLog: Send + Sync {
    /// Append a record to the WAL (now accepts reference to avoid cloning)
    async fn append(&self, record: &WalRecord) -> Result<u64>;
    async fn read(&self, offset: u64, max_bytes: usize) -> Result<Vec<WalRecord>>;

    /// Read records within a specific offset range.
    /// This is more efficient than read() + filtering when only a small range is needed.
    async fn read_range(&self, start_offset: u64, end_offset: u64) -> Result<Vec<WalRecord>>;

    async fn sync(&self) -> Result<()>;
    async fn truncate(&self, offset: u64) -> Result<()>;
    async fn get_end_offset(&self) -> Result<u64>;
    fn register_upload_callback(&self, callback: Box<dyn Fn(u64, u64) + Send + Sync>);
}

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
}

#[async_trait]
pub trait CompactionManager: Send + Sync {
    async fn compact_segments(&self, segments: Vec<String>) -> Result<String>;
    async fn schedule_compaction(&self, topic_partition: TopicPartition) -> Result<()>;
    async fn get_compaction_status(
        &self,
        topic_partition: &TopicPartition,
    ) -> Result<CompactionStatus>;
}

#[derive(Debug, Clone)]
pub enum CompactionStatus {
    NotScheduled,
    Scheduled,
    InProgress,
    Completed,
    Failed(String),
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
