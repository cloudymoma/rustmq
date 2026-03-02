use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalConfig {
    pub path: PathBuf,
    pub capacity_bytes: u64,
    pub fsync_on_write: bool,
    pub segment_size_bytes: u64,
    pub buffer_size: usize,
    /// Time interval in milliseconds after which WAL segments are uploaded regardless of size
    pub upload_interval_ms: u64,
    /// Flush interval in milliseconds when fsync_on_write is false
    pub flush_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub write_cache_size_bytes: u64,
    pub read_cache_size_bytes: u64,
    pub eviction_policy: EvictionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionPolicy {
    Lru,
    Lfu,
    Random,
    #[cfg(feature = "moka-cache")]
    Moka,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStorageConfig {
    pub storage_type: StorageType,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub multipart_threshold: u64,
    pub max_concurrent_uploads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageType {
    S3,
    Gcs,
    Azure,
    Local { path: PathBuf },
}
