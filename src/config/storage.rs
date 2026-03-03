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

impl ObjectStorageConfig {
    /// Validate storage configuration for security issues.
    pub fn validate(&self) -> crate::Result<()> {
        // Warn if access_key/secret_key are set with GCS (should use Workload Identity)
        if matches!(self.storage_type, StorageType::Gcs) {
            if self.access_key.as_ref().is_some_and(|k| !k.is_empty()) {
                tracing::warn!(
                    "object_storage.access_key is set with GCS storage type. \
                     On GKE, use Workload Identity instead of static credentials."
                );
            }
            if self.secret_key.as_ref().is_some_and(|k| !k.is_empty()) {
                tracing::warn!(
                    "object_storage.secret_key is set with GCS storage type. \
                     On GKE, use Workload Identity instead of static credentials."
                );
            }
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
