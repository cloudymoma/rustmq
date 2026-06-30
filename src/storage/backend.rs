use crate::{Result, storage::traits::*};
use async_trait::async_trait;
use bytes::Bytes;
use std::ops::Range;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::storage::LocalObjectStorage;
use crate::storage::cloud_storage::CloudObjectStorage;

pub enum StorageBackend {
    Local(LocalObjectStorage),
    Cloud(CloudObjectStorage),
}

#[async_trait]
impl ObjectStorage for StorageBackend {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        match self {
            Self::Local(s) => s.put(key, data).await,
            Self::Cloud(s) => s.put(key, data).await,
        }
    }
    async fn put_if(&self, key: &str, data: Bytes, expect: Precondition) -> Result<PutOutcome> {
        match self {
            Self::Local(s) => s.put_if(key, data, expect).await,
            Self::Cloud(s) => s.put_if(key, data, expect).await,
        }
    }
    async fn get(&self, key: &str) -> Result<Bytes> {
        match self {
            Self::Local(s) => s.get(key).await,
            Self::Cloud(s) => s.get(key).await,
        }
    }
    async fn get_versioned(&self, key: &str) -> Result<(Bytes, ObjectVersion)> {
        match self {
            Self::Local(s) => s.get_versioned(key).await,
            Self::Cloud(s) => s.get_versioned(key).await,
        }
    }
    async fn get_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        match self {
            Self::Local(s) => s.get_range(key, range).await,
            Self::Cloud(s) => s.get_range(key, range).await,
        }
    }
    async fn delete(&self, key: &str) -> Result<()> {
        match self {
            Self::Local(s) => s.delete(key).await,
            Self::Cloud(s) => s.delete(key).await,
        }
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        match self {
            Self::Local(s) => s.list(prefix).await,
            Self::Cloud(s) => s.list(prefix).await,
        }
    }
    async fn exists(&self, key: &str) -> Result<bool> {
        match self {
            Self::Local(s) => s.exists(key).await,
            Self::Cloud(s) => s.exists(key).await,
        }
    }
    async fn open_read_stream(&self, key: &str) -> Result<Box<dyn AsyncRead + Send + Unpin>> {
        match self {
            Self::Local(s) => s.open_read_stream(key).await,
            Self::Cloud(s) => s.open_read_stream(key).await,
        }
    }
    async fn open_write_stream(&self, key: &str) -> Result<Box<dyn AsyncWrite + Send + Unpin>> {
        match self {
            Self::Local(s) => s.open_write_stream(key).await,
            Self::Cloud(s) => s.open_write_stream(key).await,
        }
    }
}
