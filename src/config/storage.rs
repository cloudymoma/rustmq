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
    /// Budget (bytes) for the in-memory hot serving tier — every un-tiered record is
    /// held here for read-your-writes. When full, appends apply backpressure
    /// (force-seal + tier) so memory stays bounded.
    pub write_cache_size_bytes: u64,
    /// Size (bytes) of the cold-segment download cache that accelerates repeated reads
    /// of tiered (object-storage) data.
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

#[derive(Clone, Serialize, Deserialize)]
pub struct ObjectStorageConfig {
    pub storage_type: StorageType,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub service_account_path: Option<String>,
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

impl ObjectStorageConfig {
    /// Validate storage configuration for security issues.
    pub fn validate(&self) -> crate::Result<()> {
        // Reject static credentials for GCS in production (mandate Workload Identity)
        if matches!(self.storage_type, StorageType::Gcs)
            && (self.access_key.as_ref().is_some_and(|k| !k.is_empty())
                || self.secret_key.as_ref().is_some_and(|k| !k.is_empty()))
        {
            return Err(crate::error::RustMqError::InvalidConfig(
                "object_storage.access_key/secret_key are strictly forbidden with GCS storage type.                      On GKE, use Workload Identity (DefaultCredentialProvider).".to_string()
            ));
        }

        // Validate S3 credentials are present when using S3
        if matches!(self.storage_type, StorageType::S3) {
            let has_access = self.access_key.as_ref().is_some_and(|k| !k.is_empty());
            let has_secret = self.secret_key.as_ref().is_some_and(|k| !k.is_empty());
            if has_access != has_secret {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "object_storage: access_key and secret_key must both be set or both be empty for S3".to_string(),
                ));
            }
        }

        if self.multipart_threshold == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "object_storage.multipart_threshold must be greater than 0".to_string(),
            ));
        }

        if self.max_concurrent_uploads == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "object_storage.max_concurrent_uploads must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

impl std::fmt::Debug for ObjectStorageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectStorageConfig")
            .field("storage_type", &self.storage_type)
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field(
                "access_key",
                &self.access_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "secret_key",
                &self.secret_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("service_account_path", &self.service_account_path)
            .field("multipart_threshold", &self.multipart_threshold)
            .field("max_concurrent_uploads", &self.max_concurrent_uploads)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub interval_ms: u64,
    pub small_threshold_bytes: u64,
    pub target_bytes: u64,
    pub max_sources: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_ms: 30_000,                     // 30 seconds
            small_threshold_bytes: 10 * 1024 * 1024, // 10MB
            target_bytes: 64 * 1024 * 1024,          // 64MB
            max_sources: 10,
        }
    }
}
