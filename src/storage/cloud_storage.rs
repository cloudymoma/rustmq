use crate::{Result, storage::traits::*};
use async_trait::async_trait;
use bytes::Bytes;
use futures::TryStreamExt;
use futures::stream::StreamExt;
use object_store::buffered::BufWriter;
use object_store::{ObjectStore, PutPayload, path::Path};
use std::ops::Range;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

use tokio_util::io::StreamReader;

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() || key.contains("..") || key.starts_with('/') || key.contains('\0') {
        return Err(crate::error::RustMqError::Storage(format!(
            "Invalid object key: '{}'",
            key
        )));
    }
    Ok(())
}

use std::time::Duration;
use tokio::time::sleep;

const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 500;

pub struct CloudObjectStorage {
    store: Arc<dyn ObjectStore>,
}

impl CloudObjectStorage {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ObjectStorage for CloudObjectStorage {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        validate_key(key)?;

        let mut attempt = 0;
        loop {
            let path_clone = Path::from(key);
            let payload: PutPayload = data.clone().into();
            match self.store.put(&path_clone, payload).await {
                Ok(_) => break,
                Err(e) => {
                    attempt += 1;
                    if attempt > 3 {
                        tracing::error!("GCS put failed after 3 attempts: {}", e);
                        let mapped_err = match e {
                            object_store::Error::NotFound { path, .. } => {
                                crate::error::RustMqError::NotFound(format!(
                                    "Object not found: {}",
                                    path
                                ))
                            }
                            _ => crate::error::RustMqError::Storage(e.to_string()),
                        };
                        return Err(mapped_err);
                    }
                    let delay = std::time::Duration::from_millis(500 * (1 << (attempt - 1)));
                    tokio::time::sleep(delay).await;
                }
            }
        }

        Ok(())
    }

    async fn put_if(&self, key: &str, data: Bytes, expect: Precondition) -> Result<PutOutcome> {
        validate_key(key)?;

        let mode = match expect {
            Precondition::Create => object_store::PutMode::Create,
            Precondition::Match(v) => object_store::PutMode::Update(object_store::UpdateVersion {
                e_tag: v.e_tag,
                version: v.version,
            }),
        };
        let opts = object_store::PutOptions {
            mode,
            ..Default::default()
        };
        let payload: PutPayload = data.into();

        // No retry loop: a precondition failure is a definitive "someone else won", and
        // retrying a conditional write blindly would defeat the fence.
        match self.store.put_opts(&Path::from(key), payload, opts).await {
            Ok(res) => Ok(PutOutcome::Written(ObjectVersion {
                e_tag: res.e_tag,
                version: res.version,
            })),
            Err(object_store::Error::Precondition { .. })
            | Err(object_store::Error::AlreadyExists { .. })
            // A `Match` (Update) against an object deleted between read and write is
            // semantically a precondition failure: the object no longer matches the
            // expected version. (Create never returns NotFound, so this only affects
            // Update.) Lets the reservation CAS loop re-read and fall through to Create.
            | Err(object_store::Error::NotFound { .. }) => Ok(PutOutcome::PreconditionFailed),
            Err(e) => Err(crate::error::RustMqError::Storage(e.to_string())),
        }
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        validate_key(key)?;

        let mut attempt = 0;
        let result = loop {
            let path_clone = Path::from(key);
            match self.store.get(&path_clone).await {
                Ok(val) => break val,
                Err(e) => {
                    attempt += 1;
                    if attempt > 3 {
                        tracing::error!("GCS get failed after 3 attempts: {}", e);
                        let mapped_err = match e {
                            object_store::Error::NotFound { path, .. } => {
                                crate::error::RustMqError::NotFound(format!(
                                    "Object not found: {}",
                                    path
                                ))
                            }
                            _ => crate::error::RustMqError::Storage(e.to_string()),
                        };
                        return Err(mapped_err);
                    }
                    let delay = std::time::Duration::from_millis(500 * (1 << (attempt - 1)));
                    tokio::time::sleep(delay).await;
                }
            }
        };

        let bytes = result.bytes().await.map_err(|e| match e {
            object_store::Error::NotFound { path, .. } => {
                crate::error::RustMqError::NotFound(format!("Object not found: {}", path))
            }
            e => crate::error::RustMqError::Storage(e.to_string()),
        })?;

        Ok(bytes)
    }

    async fn get_versioned(&self, key: &str) -> Result<(Bytes, ObjectVersion)> {
        validate_key(key)?;
        let result = self
            .store
            .get(&Path::from(key))
            .await
            .map_err(|e| match e {
                object_store::Error::NotFound { path, .. } => {
                    crate::error::RustMqError::NotFound(format!("Object not found: {}", path))
                }
                e => crate::error::RustMqError::Storage(e.to_string()),
            })?;
        let version = ObjectVersion {
            e_tag: result.meta.e_tag.clone(),
            version: result.meta.version.clone(),
        };
        let bytes = result.bytes().await.map_err(|e| match e {
            object_store::Error::NotFound { path, .. } => {
                crate::error::RustMqError::NotFound(format!("Object not found: {}", path))
            }
            e => crate::error::RustMqError::Storage(e.to_string()),
        })?;
        Ok((bytes, version))
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        validate_key(key)?;
        let start = usize::try_from(range.start).map_err(|_| {
            crate::error::RustMqError::Storage(format!(
                "Range start {} exceeds platform capacity",
                range.start
            ))
        })?;
        let end = usize::try_from(range.end).map_err(|_| {
            crate::error::RustMqError::Storage(format!(
                "Range end {} exceeds platform capacity",
                range.end
            ))
        })?;

        let options = object_store::GetOptions {
            range: Some(object_store::GetRange::Bounded(start..end)),
            ..Default::default()
        };
        let result = self
            .store
            .get_opts(&Path::from(key), options)
            .await
            .map_err(|e| match e {
                object_store::Error::NotFound { path, .. } => {
                    crate::error::RustMqError::NotFound(format!("Object not found: {}", path))
                }
                e => crate::error::RustMqError::Storage(e.to_string()),
            })?;
        let bytes = result.bytes().await.map_err(|e| match e {
            object_store::Error::NotFound { path, .. } => {
                crate::error::RustMqError::NotFound(format!("Object not found: {}", path))
            }
            e => crate::error::RustMqError::Storage(e.to_string()),
        })?;
        Ok(bytes)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        validate_key(key)?;

        let mut attempt = 0;
        loop {
            let path_clone = Path::from(key);
            match self.store.delete(&path_clone).await {
                Ok(_) => break,
                Err(e) => {
                    attempt += 1;
                    if attempt > 3 {
                        tracing::error!("GCS delete failed after 3 attempts: {}", e);
                        let mapped_err = match e {
                            object_store::Error::NotFound { path, .. } => {
                                crate::error::RustMqError::NotFound(format!(
                                    "Object not found: {}",
                                    path
                                ))
                            }
                            _ => crate::error::RustMqError::Storage(e.to_string()),
                        };
                        return Err(mapped_err);
                    }
                    let delay = std::time::Duration::from_millis(500 * (1 << (attempt - 1)));
                    tokio::time::sleep(delay).await;
                }
            }
        }
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        validate_key(prefix)?;
        let mut stream = self.store.list(Some(&Path::from(prefix)));
        let mut results = Vec::new();
        const MAX_LIST_RESULTS: usize = 10_000;

        while let Some(meta) = stream.next().await {
            if results.len() >= MAX_LIST_RESULTS {
                return Err(crate::error::RustMqError::Storage(format!(
                    "List results exceeded maximum limit of {}",
                    MAX_LIST_RESULTS
                )));
            }
            match meta {
                Ok(m) => results.push(m.location.to_string()),
                Err(e) => return Err(crate::error::RustMqError::Storage(e.to_string())),
            }
        }
        Ok(results)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        validate_key(key)?;
        match self.store.head(&Path::from(key)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(crate::error::RustMqError::Storage(e.to_string())),
        }
    }

    async fn open_read_stream(&self, key: &str) -> Result<Box<dyn AsyncRead + Send + Unpin>> {
        validate_key(key)?;
        let get_result = self
            .store
            .get(&Path::from(key))
            .await
            .map_err(|e| match e {
                object_store::Error::NotFound { path, .. } => {
                    crate::error::RustMqError::NotFound(format!("Object not found: {}", path))
                }
                e => crate::error::RustMqError::Storage(e.to_string()),
            })?;
        let stream = get_result
            .into_stream()
            .map_err(|e| std::io::Error::other(e));
        Ok(Box::new(StreamReader::new(stream)))
    }

    async fn open_write_stream(&self, key: &str) -> Result<Box<dyn AsyncWrite + Send + Unpin>> {
        validate_key(key)?;
        let writer = BufWriter::new(self.store.clone(), Path::from(key));
        Ok(Box::new(writer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    #[tokio::test]
    async fn test_put_and_get() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        let data = Bytes::from("test data");
        storage.put("test/key", data.clone()).await.unwrap();
        let retrieved = storage.get("test/key").await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_put_if_cas() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);

        // Create-only succeeds once, then fails.
        let v = match storage
            .put_if("res/_epoch", Bytes::from("a"), Precondition::Create)
            .await
            .unwrap()
        {
            PutOutcome::Written(v) => v,
            other => panic!("expected Written, got {other:?}"),
        };
        assert_eq!(
            storage
                .put_if("res/_epoch", Bytes::from("b"), Precondition::Create)
                .await
                .unwrap(),
            PutOutcome::PreconditionFailed
        );

        // CAS with the current version succeeds; with the stale one it is fenced.
        let _v2 = match storage
            .put_if(
                "res/_epoch",
                Bytes::from("c"),
                Precondition::Match(v.clone()),
            )
            .await
            .unwrap()
        {
            PutOutcome::Written(v2) => v2,
            other => panic!("expected Written, got {other:?}"),
        };
        assert_eq!(
            storage
                .put_if("res/_epoch", Bytes::from("d"), Precondition::Match(v))
                .await
                .unwrap(),
            PutOutcome::PreconditionFailed
        );
        assert_eq!(storage.get("res/_epoch").await.unwrap(), Bytes::from("c"));
    }

    #[tokio::test]
    async fn test_put_if_match_on_deleted_object_is_precondition_failed() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);

        // Create the object and capture its version.
        let v = match storage
            .put_if("res/_epoch", Bytes::from("a"), Precondition::Create)
            .await
            .unwrap()
        {
            PutOutcome::Written(v) => v,
            other => panic!("expected Written, got {other:?}"),
        };

        // Delete it out from under a would-be CAS writer.
        storage.delete("res/_epoch").await.unwrap();

        // A CAS update against the now-missing object is a precondition failure, not an
        // I/O error — the object no longer matches the expected version. (InMemory maps
        // this to `Precondition`; some real backends return `NotFound` — both now fold to
        // `PreconditionFailed`.) This lets the reservation acquire loop re-read and create.
        assert_eq!(
            storage
                .put_if("res/_epoch", Bytes::from("b"), Precondition::Match(v))
                .await
                .unwrap(),
            PutOutcome::PreconditionFailed
        );
    }

    #[tokio::test]
    async fn test_exists_returns_false_for_missing() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        assert_eq!(storage.exists("nonexistent").await.unwrap(), false);
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_error() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        assert!(storage.get("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_get_range() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        storage
            .put("range/key", Bytes::from("hello world"))
            .await
            .unwrap();
        let result = storage.get_range("range/key", 0..5).await.unwrap();
        assert_eq!(result, Bytes::from("hello"));
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        storage.put("prefix/a.txt", Bytes::from("a")).await.unwrap();
        storage.put("prefix/b.txt", Bytes::from("b")).await.unwrap();
        storage.put("other/c.txt", Bytes::from("c")).await.unwrap();
        let results = storage.list("prefix").await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_delete() {
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        storage.put("del/key", Bytes::from("data")).await.unwrap();
        storage.delete("del/key").await.unwrap();
        assert_eq!(storage.exists("del/key").await.unwrap(), false);
    }

    #[tokio::test]
    async fn test_open_read_stream() {
        use tokio::io::AsyncReadExt;
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        storage
            .put("stream/key", Bytes::from("streamed"))
            .await
            .unwrap();
        let mut reader = storage.open_read_stream("stream/key").await.unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"streamed");
    }

    #[tokio::test]
    async fn test_open_write_stream() {
        use tokio::io::AsyncWriteExt;
        let store = Arc::new(InMemory::new());
        let storage = CloudObjectStorage::new(store);
        let mut writer = storage.open_write_stream("wstream/key").await.unwrap();
        writer.write_all(b"written via stream").await.unwrap();
        writer.shutdown().await.unwrap();
        let data = storage.get("wstream/key").await.unwrap();
        assert_eq!(data, Bytes::from("written via stream"));
    }
}
