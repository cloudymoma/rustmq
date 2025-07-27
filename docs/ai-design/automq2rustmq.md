# AutoMQ Technical Architecture and Rust Refactoring Guide

## Executive Summary

AutoMQ is a cloud-native, stateless Apache Kafka alternative that separates compute and storage by using S3 as the primary storage backend. This document provides a comprehensive technical analysis of AutoMQ's architecture and serves as a detailed guide for refactoring the system to Rust programming language.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Core Components Analysis](#core-components-analysis)
3. [Data Structures and Interfaces](#data-structures-and-interfaces)
4. [S3Stream Architecture](#s3stream-architecture)
5. [Configuration and Deployment](#configuration-and-deployment)
6. [Rust Implementation Guide](#rust-implementation-guide)
7. [Migration Strategy](#migration-strategy)
8. [Performance Considerations](#performance-considerations)
9. [Testing Strategy](#testing-strategy)
10. [Conclusion](#conclusion)

## 1. Architecture Overview

### 1.1 High-Level Architecture

AutoMQ transforms Apache Kafka's shared-nothing architecture into a shared-storage architecture using S3 as the backend. The key architectural components include:

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   AutoMQ Broker │    │   AutoMQ Broker │    │   AutoMQ Broker │
│   (Stateless)   │    │   (Stateless)   │    │   (Stateless)   │
└─────────┬───────┘    └─────────┬───────┘    └─────────┬───────┘
          │                      │                      │
          └──────────────────────┼──────────────────────┘
                                 │
                    ┌─────────────┴─────────────┐
                    │        S3 Storage         │
                    │   (Shared Storage Layer)  │
                    └───────────────────────────┘
```

### 1.2 Key Differences from Apache Kafka

| Aspect | Apache Kafka | AutoMQ |
|--------|-------------|---------|
| **Storage** | Local log segments | S3 objects |
| **Replication** | Peer-to-peer replication | S3 as single source of truth |
| **Scaling** | Stateful brokers requiring data movement | Stateless brokers with instant scaling |
| **Storage Cost** | EBS/local disks | S3 (90% cost reduction) |
| **Cross-AZ Traffic** | High costs for replication | Eliminated via S3 |
| **Durability** | 3x replication factor | S3's 99.999999999% durability |

### 1.3 Project Structure

```
automq/
├── clients/                    # Kafka client library
├── core/                      # Core Kafka modifications (Scala)
├── s3stream/                  # S3Stream implementation (Java)
├── server/                    # Server components
├── server-common/             # Common server utilities
├── storage/                   # Storage abstractions
├── metadata/                  # Metadata management
├── raft/                      # Raft consensus implementation
├── connect/                   # Kafka Connect components
├── streams/                   # Kafka Streams
├── tools/                     # Administrative tools
├── config/                    # Configuration files
└── docker/                    # Docker deployment configs
```

## 2. Core Components Analysis

### 2.1 Module Dependencies

The dependency graph shows the layered architecture:

```
Core (Scala) ←→ S3Stream (Java)
     ↓               ↓
Server-Common ←→ Storage
     ↓               ↓
   Clients ←→ Server-Common
     ↓
  Metadata ←→ Raft
```

### 2.2 Key Modules

#### 2.2.1 S3Stream Module (`s3stream/`)
- **Purpose**: Core streaming abstraction over S3
- **Language**: Java
- **Key Components**:
  - Stream API and implementations
  - WAL (Write-Ahead Log) system
  - Object storage abstraction
  - Caching layer
  - Compaction system

#### 2.2.2 Core Module (`core/`)
- **Purpose**: Kafka integration layer
- **Language**: Scala
- **Key Components**:
  - ElasticLog (replaces Kafka's LocalLog)
  - ElasticUnifiedLog
  - ElasticReplicaManager
  - Kafka protocol extensions

#### 2.2.3 Storage Module (`storage/`)
- **Purpose**: Storage abstractions and remote log management
- **Language**: Java
- **Key Components**:
  - Remote storage interfaces
  - Storage implementations
  - Metadata management

## 3. Data Structures and Interfaces

### 3.1 Stream API

#### 3.1.1 Core Stream Interface (Java)

```java
public interface Stream {
    // Stream lifecycle
    CompletableFuture<Void> open();
    CompletableFuture<Void> close();
    CompletableFuture<Void> destroy();
    
    // Data operations
    CompletableFuture<AppendResult> append(RecordBatch recordBatch);
    CompletableFuture<FetchResult> fetch(long startOffset, long endOffset, int maxBytes);
    CompletableFuture<Void> trim(long newStartOffset);
    
    // Properties
    long streamId();
    long epoch();
    long startOffset();
    long confirmOffset();
    long nextOffset();
}
```

#### 3.1.2 Rust Translation

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use bytes::Bytes;

#[async_trait]
pub trait Stream: Send + Sync {
    // Stream lifecycle
    async fn open(&self) -> Result<(), StreamError>;
    async fn close(&self) -> Result<(), StreamError>;
    async fn destroy(&self) -> Result<(), StreamError>;
    
    // Data operations
    async fn append(&self, record_batch: RecordBatch) -> Result<AppendResult, StreamError>;
    async fn fetch(&self, start_offset: u64, end_offset: u64, max_bytes: usize) 
                  -> Result<FetchResult, StreamError>;
    async fn trim(&self, new_start_offset: u64) -> Result<(), StreamError>;
    
    // Properties
    fn stream_id(&self) -> u64;
    fn epoch(&self) -> u64;
    fn start_offset(&self) -> u64;
    fn confirm_offset(&self) -> u64;
    fn next_offset(&self) -> u64;
}

#[derive(Debug, Clone)]
pub struct RecordBatch {
    pub stream_id: u64,
    pub epoch: u64,
    pub base_offset: u64,
    pub count: u32,
    pub payload: Bytes,
}

#[derive(Debug)]
pub struct AppendResult {
    pub offset: u64,
}

#[derive(Debug)]
pub struct FetchResult {
    pub records: Vec<StreamRecord>,
}

#[derive(Debug)]
pub struct StreamRecord {
    pub offset: u64,
    pub timestamp: i64,
    pub payload: Bytes,
}
```

### 3.2 Storage Interface

#### 3.2.1 Storage Abstraction (Java)

```java
public interface Storage {
    CompletableFuture<Void> startup();
    CompletableFuture<Void> shutdown();
    
    CompletableFuture<Stream> openStream(long streamId, long epoch);
    CompletableFuture<Void> closeStream(long streamId);
    
    CompletableFuture<Void> forceUpload(long streamId);
}
```

#### 3.2.2 Rust Translation

```rust
#[async_trait]
pub trait Storage: Send + Sync {
    async fn startup(&self) -> Result<(), StorageError>;
    async fn shutdown(&self) -> Result<(), StorageError>;
    
    async fn open_stream(&self, stream_id: u64, epoch: u64) -> Result<Arc<dyn Stream>, StorageError>;
    async fn close_stream(&self, stream_id: u64) -> Result<(), StorageError>;
    
    async fn force_upload(&self, stream_id: u64) -> Result<(), StorageError>;
}

pub struct S3Storage {
    config: Arc<S3Config>,
    wal: Arc<dyn WriteAheadLog>,
    block_cache: Arc<Mutex<BlockCache>>,
    object_storage: Arc<dyn ObjectStorage>,
    streams: Arc<Mutex<HashMap<u64, Arc<S3Stream>>>>,
}

impl S3Storage {
    pub fn new(config: S3Config) -> Result<Self, StorageError> {
        Ok(Self {
            config: Arc::new(config),
            wal: Arc::new(create_wal(&config)?),
            block_cache: Arc::new(Mutex::new(BlockCache::new(config.block_cache_size))),
            object_storage: Arc::new(create_object_storage(&config)?),
            streams: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}
```

### 3.3 Write-Ahead Log (WAL)

#### 3.3.1 WAL Interface (Java)

```java
public interface WriteAheadLog {
    WriteAheadLogConfig config();
    
    CompletableFuture<AppendResult> append(long streamId, 
                                          ByteBuf data, 
                                          int crc);
    Iterator<WALRecord> recover();
    CompletableFuture<Void> trim(long offset);
    CompletableFuture<Void> reset();
    CompletableFuture<Void> shutdown();
}
```

#### 3.3.2 Rust Translation

```rust
#[async_trait]
pub trait WriteAheadLog: Send + Sync {
    fn config(&self) -> &WalConfig;
    
    async fn append(&self, stream_id: u64, data: Bytes, crc: u32) 
                   -> Result<AppendResult, WalError>;
    async fn recover(&self) -> Result<Vec<WalRecord>, WalError>;
    async fn trim(&self, offset: u64) -> Result<(), WalError>;
    async fn reset(&self) -> Result<(), WalError>;
    async fn shutdown(&self) -> Result<(), WalError>;
}

#[derive(Debug, Clone)]
pub struct WalRecord {
    pub offset: u64,
    pub stream_id: u64,
    pub data: Bytes,
    pub crc: u32,
    pub timestamp: i64,
}

pub struct ObjectWal {
    config: WalConfig,
    current_segment: Arc<Mutex<WalSegment>>,
    object_storage: Arc<dyn ObjectStorage>,
    uploader: Arc<WalUploader>,
}

impl ObjectWal {
    pub fn new(config: WalConfig, object_storage: Arc<dyn ObjectStorage>) -> Self {
        Self {
            config,
            current_segment: Arc::new(Mutex::new(WalSegment::new())),
            object_storage,
            uploader: Arc::new(WalUploader::new()),
        }
    }
}
```

### 3.4 Object Storage Abstraction

#### 3.4.1 Object Storage Interface (Java)

```java
public interface ObjectStorage {
    CompletableFuture<WriteOptions> writer(WriteOptions options);
    CompletableFuture<List<S3ObjectMetadata>> list(String prefix);
    CompletableFuture<DataBlock> rangeRead(String objectPath, 
                                          long start, 
                                          long end);
    CompletableFuture<Void> delete(List<String> objectKeys);
    CompletableFuture<Void> multiPartUpload(MultiPartWriter writer);
}
```

#### 3.4.2 Rust Translation

```rust
#[async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn write(&self, path: &str, data: Bytes, options: WriteOptions) 
                  -> Result<(), ObjectStorageError>;
    async fn read(&self, path: &str) -> Result<Bytes, ObjectStorageError>;
    async fn range_read(&self, path: &str, start: u64, end: u64) 
                       -> Result<Bytes, ObjectStorageError>;
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMetadata>, ObjectStorageError>;
    async fn delete(&self, paths: &[String]) -> Result<(), ObjectStorageError>;
    async fn multipart_upload(&self, path: &str, parts: Vec<Bytes>) 
                             -> Result<(), ObjectStorageError>;
}

#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub bucket: String,
    pub key: String,
    pub metadata: HashMap<String, String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub key: String,
    pub size: u64,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub etag: String,
}

pub struct S3ObjectStorage {
    client: aws_sdk_s3::Client,
    bucket: String,
    region: String,
}

impl S3ObjectStorage {
    pub async fn new(config: S3Config) -> Result<Self, ObjectStorageError> {
        let aws_config = aws_config::load_from_env().await;
        let client = aws_sdk_s3::Client::new(&aws_config);
        
        Ok(Self {
            client,
            bucket: config.bucket,
            region: config.region,
        })
    }
}
```

### 3.5 Caching System

#### 3.5.1 Block Cache (Java)

```java
public class S3BlockCache {
    private final int maxSize;
    private final LRUCache<String, ReadDataBlock> cache;
    private final ObjectStorage objectStorage;
    
    public CompletableFuture<ReadDataBlock> get(String blockKey) {
        return cache.get(blockKey)
            .orElseGet(() -> loadFromS3(blockKey));
    }
    
    private CompletableFuture<ReadDataBlock> loadFromS3(String blockKey) {
        return objectStorage.rangeRead(blockKey, 0, -1)
            .thenApply(data -> new ReadDataBlock(blockKey, data));
    }
}
```

#### 3.5.2 Rust Translation

```rust
use std::collections::HashMap;
use tokio::sync::RwLock;
use lru::LruCache;

pub struct BlockCache {
    cache: Arc<RwLock<LruCache<String, Arc<DataBlock>>>>,
    object_storage: Arc<dyn ObjectStorage>,
    max_size: usize,
}

impl BlockCache {
    pub fn new(max_size: usize, object_storage: Arc<dyn ObjectStorage>) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(max_size))),
            object_storage,
            max_size,
        }
    }
    
    pub async fn get(&self, key: &str) -> Result<Arc<DataBlock>, CacheError> {
        // Try cache first
        {
            let mut cache = self.cache.write().await;
            if let Some(block) = cache.get(key) {
                return Ok(block.clone());
            }
        }
        
        // Load from S3
        let data = self.object_storage.read(key).await?;
        let block = Arc::new(DataBlock::new(key.to_string(), data));
        
        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.put(key.to_string(), block.clone());
        }
        
        Ok(block)
    }
}

#[derive(Debug, Clone)]
pub struct DataBlock {
    pub key: String,
    pub data: Bytes,
    pub size: usize,
    pub last_accessed: std::time::Instant,
}

impl DataBlock {
    pub fn new(key: String, data: Bytes) -> Self {
        let size = data.len();
        Self {
            key,
            data,
            size,
            last_accessed: std::time::Instant::now(),
        }
    }
}
```

### 3.6 Configuration System

#### 3.6.1 Configuration Structure (Java)

```java
public class Config {
    // WAL Configuration
    private long walCacheSize = 200 * 1024 * 1024L; // 200MB
    private long walUploadThreshold = 100 * 1024 * 1024L; // 100MB
    private int walUploadIntervalMs = 1000;
    
    // Object Storage Configuration
    private String objectBucket;
    private String objectPrefix;
    private int objectBlockSize = 1024 * 1024; // 1MB
    private int objectPartSize = 16 * 1024 * 1024; // 16MB
    
    // Cache Configuration
    private long blockCacheSize = 100 * 1024 * 1024L; // 100MB
    private int logCacheSize = 50 * 1024 * 1024; // 50MB
    
    // Network Configuration
    private long networkBaselineBandwidth = 100 * 1024 * 1024L; // 100MB/s
    private int refillPeriodMs = 10;
    
    // Compaction Configuration
    private int streamSetObjectCompactionIntervalMs = 10 * 60 * 1000; // 10 minutes
    private long streamSetObjectCompactionCacheSize = 200 * 1024 * 1024L; // 200MB
    private int maxStreamNumPerStreamSetObject = 100000;
}
```

#### 3.6.2 Rust Translation

```rust
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoMqConfig {
    pub wal: WalConfig,
    pub object_storage: ObjectStorageConfig,
    pub cache: CacheConfig,
    pub network: NetworkConfig,
    pub compaction: CompactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalConfig {
    #[serde(default = "default_wal_cache_size")]
    pub cache_size: usize, // 200MB
    
    #[serde(default = "default_wal_upload_threshold")]
    pub upload_threshold: usize, // 100MB
    
    #[serde(default = "default_wal_upload_interval")]
    pub upload_interval: Duration, // 1 second
    
    #[serde(default = "default_wal_path")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    
    #[serde(default = "default_object_block_size")]
    pub block_size: usize, // 1MB
    
    #[serde(default = "default_object_part_size")]
    pub part_size: usize, // 16MB
    
    #[serde(default = "default_path_style")]
    pub path_style: bool, // true for MinIO
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_block_cache_size")]
    pub block_cache_size: usize, // 100MB
    
    #[serde(default = "default_log_cache_size")]
    pub log_cache_size: usize, // 50MB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_baseline_bandwidth")]
    pub baseline_bandwidth: u64, // 100MB/s
    
    #[serde(default = "default_refill_period")]
    pub refill_period: Duration, // 10ms
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    #[serde(default = "default_compaction_interval")]
    pub interval: Duration, // 10 minutes
    
    #[serde(default = "default_compaction_cache_size")]
    pub cache_size: usize, // 200MB
    
    #[serde(default = "default_max_streams_per_object")]
    pub max_streams_per_object: usize, // 100,000
}

// Default value functions
fn default_wal_cache_size() -> usize { 200 * 1024 * 1024 }
fn default_wal_upload_threshold() -> usize { 100 * 1024 * 1024 }
fn default_wal_upload_interval() -> Duration { Duration::from_secs(1) }
fn default_wal_path() -> String { "/tmp/wal".to_string() }
fn default_object_block_size() -> usize { 1024 * 1024 }
fn default_object_part_size() -> usize { 16 * 1024 * 1024 }
fn default_path_style() -> bool { false }
fn default_block_cache_size() -> usize { 100 * 1024 * 1024 }
fn default_log_cache_size() -> usize { 50 * 1024 * 1024 }
fn default_baseline_bandwidth() -> u64 { 100 * 1024 * 1024 }
fn default_refill_period() -> Duration { Duration::from_millis(10) }
fn default_compaction_interval() -> Duration { Duration::from_secs(600) }
fn default_compaction_cache_size() -> usize { 200 * 1024 * 1024 }
fn default_max_streams_per_object() -> usize { 100_000 }
```

### 3.7 Error Handling

#### 3.7.1 Error Types (Java)

```java
public class StreamClientException extends Exception {
    private final ErrorCode errorCode;
    private final String message;
    
    public enum ErrorCode {
        STREAM_NOT_EXIST,
        STREAM_FENCED,
        OFFSET_OUT_OF_RANGE,
        NETWORK_ERROR,
        STORAGE_ERROR,
        UNKNOWN_ERROR
    }
}
```

#### 3.7.2 Rust Translation

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StreamError {
    #[error("Stream {stream_id} does not exist")]
    StreamNotExist { stream_id: u64 },
    
    #[error("Stream {stream_id} is fenced with epoch {epoch}")]
    StreamFenced { stream_id: u64, epoch: u64 },
    
    #[error("Offset {offset} is out of range for stream {stream_id}")]
    OffsetOutOfRange { stream_id: u64, offset: u64 },
    
    #[error("Network error: {message}")]
    NetworkError { message: String },
    
    #[error("Storage error: {source}")]
    StorageError { #[from] source: ObjectStorageError },
    
    #[error("WAL error: {source}")]
    WalError { #[from] source: WalError },
    
    #[error("Unknown error: {message}")]
    Unknown { message: String },
}

#[derive(Error, Debug)]
pub enum ObjectStorageError {
    #[error("S3 error: {message}")]
    S3Error { message: String },
    
    #[error("Network timeout")]
    Timeout,
    
    #[error("Access denied")]
    AccessDenied,
    
    #[error("Object not found: {key}")]
    ObjectNotFound { key: String },
    
    #[error("IO error: {source}")]
    IoError { #[from] source: std::io::Error },
}

#[derive(Error, Debug)]
pub enum WalError {
    #[error("WAL corruption detected at offset {offset}")]
    Corruption { offset: u64 },
    
    #[error("WAL is full")]
    Full,
    
    #[error("IO error: {source}")]
    IoError { #[from] source: std::io::Error },
}

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Cache miss for key: {key}")]
    Miss { key: String },
    
    #[error("Cache is full")]
    Full,
    
    #[error("Storage error: {source}")]
    StorageError { #[from] source: ObjectStorageError },
}
```

## 4. S3Stream Architecture

### 4.1 Stream Implementation Details

#### 4.1.1 S3Stream Core (Java Analysis)

The S3Stream implementation provides the core streaming abstraction with these key features:

- **Offset Management**: Atomic tracking of `nextOffset` and `confirmOffset`
- **Epoch-based Fencing**: Prevents split-brain scenarios
- **Bandwidth Limiting**: Integrated rate limiting for reads/writes
- **Snapshot Reading**: Read-only mode for historical data access

#### 4.1.2 Rust Implementation

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, Mutex};
use std::sync::Arc;

pub struct S3Stream {
    stream_id: u64,
    epoch: u64,
    start_offset: AtomicU64,
    next_offset: AtomicU64,
    confirm_offset: AtomicU64,
    
    // Storage backend
    storage: Arc<dyn Storage>,
    
    // Caching
    log_cache: Arc<Mutex<LogCache>>,
    
    // Rate limiting
    bandwidth_limiter: Arc<BandwidthLimiter>,
    
    // State management
    state: Arc<RwLock<StreamState>>,
}

#[derive(Debug, Clone, Copy)]
pub enum StreamState {
    Opening,
    Opened,
    Closing,
    Closed,
    Destroyed,
}

impl S3Stream {
    pub fn new(
        stream_id: u64,
        epoch: u64,
        start_offset: u64,
        storage: Arc<dyn Storage>,
    ) -> Self {
        Self {
            stream_id,
            epoch,
            start_offset: AtomicU64::new(start_offset),
            next_offset: AtomicU64::new(start_offset),
            confirm_offset: AtomicU64::new(start_offset),
            storage,
            log_cache: Arc::new(Mutex::new(LogCache::new(1024 * 1024))), // 1MB
            bandwidth_limiter: Arc::new(BandwidthLimiter::new(100 * 1024 * 1024)), // 100MB/s
            state: Arc::new(RwLock::new(StreamState::Opening)),
        }
    }
}

#[async_trait]
impl Stream for S3Stream {
    async fn open(&self) -> Result<(), StreamError> {
        let mut state = self.state.write().await;
        match *state {
            StreamState::Opening => {
                // Initialize stream
                *state = StreamState::Opened;
                Ok(())
            }
            _ => Err(StreamError::Unknown {
                message: "Stream already opened or closed".to_string(),
            }),
        }
    }
    
    async fn append(&self, record_batch: RecordBatch) -> Result<AppendResult, StreamError> {
        let state = self.state.read().await;
        if !matches!(*state, StreamState::Opened) {
            return Err(StreamError::Unknown {
                message: "Stream not in opened state".to_string(),
            });
        }
        
        // Validate epoch
        if record_batch.epoch != self.epoch {
            return Err(StreamError::StreamFenced {
                stream_id: self.stream_id,
                epoch: record_batch.epoch,
            });
        }
        
        // Apply bandwidth limiting
        self.bandwidth_limiter.acquire(record_batch.payload.len()).await?;
        
        // Get next offset
        let offset = self.next_offset.fetch_add(record_batch.count as u64, Ordering::SeqCst);
        
        // Write to WAL via storage
        self.storage.append_to_wal(self.stream_id, &record_batch).await?;
        
        // Update cache
        let mut cache = self.log_cache.lock().await;
        cache.append(offset, &record_batch.payload)?;
        
        Ok(AppendResult { offset })
    }
    
    async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        max_bytes: usize,
    ) -> Result<FetchResult, StreamError> {
        let state = self.state.read().await;
        if !matches!(*state, StreamState::Opened) {
            return Err(StreamError::Unknown {
                message: "Stream not in opened state".to_string(),
            });
        }
        
        // Validate offset range
        let current_start = self.start_offset.load(Ordering::SeqCst);
        let current_next = self.next_offset.load(Ordering::SeqCst);
        
        if start_offset < current_start || start_offset >= current_next {
            return Err(StreamError::OffsetOutOfRange {
                stream_id: self.stream_id,
                offset: start_offset,
            });
        }
        
        // Apply bandwidth limiting
        self.bandwidth_limiter.acquire(max_bytes).await?;
        
        // Try cache first
        let mut records = Vec::new();
        let mut current_offset = start_offset;
        let mut bytes_read = 0;
        
        {
            let cache = self.log_cache.lock().await;
            while current_offset < end_offset && bytes_read < max_bytes {
                if let Some(record) = cache.get(current_offset)? {
                    bytes_read += record.payload.len();
                    records.push(record);
                    current_offset += 1;
                } else {
                    break;
                }
            }
        }
        
        // If cache miss, read from storage
        if current_offset < end_offset && bytes_read < max_bytes {
            let storage_records = self.storage
                .fetch_from_s3(
                    self.stream_id,
                    current_offset,
                    end_offset,
                    max_bytes - bytes_read,
                )
                .await?;
            records.extend(storage_records);
        }
        
        Ok(FetchResult { records })
    }
    
    async fn trim(&self, new_start_offset: u64) -> Result<(), StreamError> {
        let current_start = self.start_offset.load(Ordering::SeqCst);
        if new_start_offset <= current_start {
            return Ok(()); // Nothing to trim
        }
        
        // Update start offset
        self.start_offset.store(new_start_offset, Ordering::SeqCst);
        
        // Clean up cache
        let mut cache = self.log_cache.lock().await;
        cache.trim(new_start_offset)?;
        
        // Notify storage for cleanup
        self.storage.trim_stream(self.stream_id, new_start_offset).await?;
        
        Ok(())
    }
    
    async fn close(&self) -> Result<(), StreamError> {
        let mut state = self.state.write().await;
        match *state {
            StreamState::Opened => {
                *state = StreamState::Closing;
                
                // Flush any pending data
                self.storage.flush_stream(self.stream_id).await?;
                
                *state = StreamState::Closed;
                Ok(())
            }
            StreamState::Closed => Ok(()),
            _ => Err(StreamError::Unknown {
                message: "Invalid state for close operation".to_string(),
            }),
        }
    }
    
    async fn destroy(&self) -> Result<(), StreamError> {
        let mut state = self.state.write().await;
        *state = StreamState::Destroyed;
        
        // Clean up all resources
        self.storage.destroy_stream(self.stream_id).await?;
        
        Ok(())
    }
    
    fn stream_id(&self) -> u64 {
        self.stream_id
    }
    
    fn epoch(&self) -> u64 {
        self.epoch
    }
    
    fn start_offset(&self) -> u64 {
        self.start_offset.load(Ordering::SeqCst)
    }
    
    fn confirm_offset(&self) -> u64 {
        self.confirm_offset.load(Ordering::SeqCst)
    }
    
    fn next_offset(&self) -> u64 {
        self.next_offset.load(Ordering::SeqCst)
    }
}
```

### 4.2 WAL Implementation

#### 4.2.1 Object WAL Strategy

The WAL system uses a dual approach:
1. **Immediate WAL Write**: For durability guarantees
2. **Async S3 Upload**: For cost-effective long-term storage

#### 4.2.2 Rust Implementation

```rust
pub struct ObjectWal {
    config: WalConfig,
    current_segment: Arc<Mutex<WalSegment>>,
    object_storage: Arc<dyn ObjectStorage>,
    upload_scheduler: Arc<UploadScheduler>,
    recovery_manager: Arc<RecoveryManager>,
}

#[derive(Debug)]
pub struct WalSegment {
    id: u64,
    records: Vec<WalRecord>,
    size: usize,
    created_at: std::time::Instant,
}

impl WalSegment {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            records: Vec::new(),
            size: 0,
            created_at: std::time::Instant::now(),
        }
    }
    
    pub fn append(&mut self, record: WalRecord) -> Result<(), WalError> {
        self.size += record.data.len();
        self.records.push(record);
        Ok(())
    }
    
    pub fn should_upload(&self, config: &WalConfig) -> bool {
        self.size >= config.upload_threshold
            || self.created_at.elapsed() >= config.upload_interval
    }
}

#[async_trait]
impl WriteAheadLog for ObjectWal {
    fn config(&self) -> &WalConfig {
        &self.config
    }
    
    async fn append(
        &self,
        stream_id: u64,
        data: Bytes,
        crc: u32,
    ) -> Result<AppendResult, WalError> {
        let record = WalRecord {
            offset: 0, // Will be assigned by segment
            stream_id,
            data,
            crc,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        
        let mut segment = self.current_segment.lock().await;
        let offset = segment.records.len() as u64;
        let mut record = record;
        record.offset = offset;
        
        segment.append(record)?;
        
        // Check if segment should be uploaded
        if segment.should_upload(&self.config) {
            self.upload_scheduler.schedule_upload(segment.id).await;
        }
        
        Ok(AppendResult { offset })
    }
    
    async fn recover(&self) -> Result<Vec<WalRecord>, WalError> {
        self.recovery_manager.recover_all().await
    }
    
    async fn trim(&self, offset: u64) -> Result<(), WalError> {
        // Clean up old segments
        self.upload_scheduler.trim_before(offset).await;
        Ok(())
    }
    
    async fn reset(&self) -> Result<(), WalError> {
        let mut segment = self.current_segment.lock().await;
        *segment = WalSegment::new(segment.id + 1);
        Ok(())
    }
    
    async fn shutdown(&self) -> Result<(), WalError> {
        // Upload any remaining data
        let segment = self.current_segment.lock().await;
        if !segment.records.is_empty() {
            self.upload_scheduler.schedule_upload(segment.id).await;
        }
        
        // Wait for all uploads to complete
        self.upload_scheduler.shutdown().await;
        Ok(())
    }
}

pub struct UploadScheduler {
    object_storage: Arc<dyn ObjectStorage>,
    pending_uploads: Arc<Mutex<HashMap<u64, WalSegment>>>,
    upload_queue: Arc<Mutex<tokio::sync::mpsc::UnboundedSender<u64>>>,
}

impl UploadScheduler {
    pub fn new(object_storage: Arc<dyn ObjectStorage>) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let scheduler = Self {
            object_storage: object_storage.clone(),
            pending_uploads: Arc::new(Mutex::new(HashMap::new())),
            upload_queue: Arc::new(Mutex::new(tx)),
        };
        
        // Start upload worker
        let pending_uploads = scheduler.pending_uploads.clone();
        let object_storage = object_storage.clone();
        tokio::spawn(async move {
            while let Some(segment_id) = rx.recv().await {
                if let Some(segment) = {
                    let mut pending = pending_uploads.lock().await;
                    pending.remove(&segment_id)
                } {
                    if let Err(e) = Self::upload_segment(&object_storage, &segment).await {
                        eprintln!("Failed to upload segment {}: {:?}", segment_id, e);
                        // Could implement retry logic here
                    }
                }
            }
        });
        
        scheduler
    }
    
    pub async fn schedule_upload(&self, segment_id: u64) {
        let tx = self.upload_queue.lock().await;
        let _ = tx.send(segment_id);
    }
    
    async fn upload_segment(
        object_storage: &Arc<dyn ObjectStorage>,
        segment: &WalSegment,
    ) -> Result<(), ObjectStorageError> {
        let key = format!("wal/segment-{:08}", segment.id);
        
        // Serialize segment
        let serialized = bincode::serialize(&segment.records)
            .map_err(|e| ObjectStorageError::S3Error {
                message: format!("Serialization error: {}", e),
            })?;
        
        // Upload to S3
        object_storage
            .write(
                &key,
                Bytes::from(serialized),
                WriteOptions {
                    bucket: "wal-bucket".to_string(),
                    key: key.clone(),
                    metadata: HashMap::new(),
                    content_type: Some("application/octet-stream".to_string()),
                },
            )
            .await?;
        
        Ok(())
    }
}
```

### 4.3 Compaction System

#### 4.3.1 Compaction Strategy

AutoMQ implements several compaction strategies:
- **Stream Set Object Compaction**: Merges multiple small objects
- **Force Split**: Converts stream set objects to individual streams
- **Age-Based Compaction**: Time-based object reorganization

#### 4.3.2 Rust Implementation

```rust
use std::collections::BTreeMap;
use tokio::time::{interval, Duration};

pub struct CompactionManager {
    config: CompactionConfig,
    object_storage: Arc<dyn ObjectStorage>,
    metadata_store: Arc<dyn MetadataStore>,
    bandwidth_limiter: Arc<BandwidthLimiter>,
    compaction_scheduler: Arc<Mutex<CompactionScheduler>>,
}

#[derive(Debug, Clone)]
pub struct CompactionTask {
    pub task_id: u64,
    pub objects: Vec<S3ObjectMetadata>,
    pub strategy: CompactionStrategy,
    pub priority: CompactionPriority,
}

#[derive(Debug, Clone)]
pub enum CompactionStrategy {
    StreamSetMerge,
    ForceSplit,
    AgeBased,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompactionPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl CompactionManager {
    pub fn new(
        config: CompactionConfig,
        object_storage: Arc<dyn ObjectStorage>,
        metadata_store: Arc<dyn MetadataStore>,
    ) -> Self {
        let bandwidth_limiter = Arc::new(BandwidthLimiter::new(config.bandwidth_limit));
        
        Self {
            config,
            object_storage,
            metadata_store,
            bandwidth_limiter,
            compaction_scheduler: Arc::new(Mutex::new(CompactionScheduler::new())),
        }
    }
    
    pub async fn start(&self) -> Result<(), CompactionError> {
        let mut interval = interval(self.config.interval);
        let manager = self.clone();
        
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                if let Err(e) = manager.run_compaction_cycle().await {
                    eprintln!("Compaction cycle failed: {:?}", e);
                }
            }
        });
        
        Ok(())
    }
    
    async fn run_compaction_cycle(&self) -> Result<(), CompactionError> {
        // Identify compaction candidates
        let candidates = self.identify_compaction_candidates().await?;
        
        // Schedule compaction tasks
        for candidate in candidates {
            let task = self.create_compaction_task(candidate).await?;
            let mut scheduler = self.compaction_scheduler.lock().await;
            scheduler.schedule(task);
        }
        
        // Execute highest priority tasks
        let tasks = {
            let mut scheduler = self.compaction_scheduler.lock().await;
            scheduler.get_pending_tasks(self.config.max_concurrent_tasks)
        };
        
        let mut handles = Vec::new();
        for task in tasks {
            let manager = self.clone();
            let handle = tokio::spawn(async move {
                manager.execute_compaction_task(task).await
            });
            handles.push(handle);
        }
        
        // Wait for all tasks to complete
        for handle in handles {
            if let Err(e) = handle.await {
                eprintln!("Compaction task failed: {:?}", e);
            }
        }
        
        Ok(())
    }
    
    async fn identify_compaction_candidates(&self) -> Result<Vec<CompactionCandidate>, CompactionError> {
        let mut candidates = Vec::new();
        
        // Get all objects from metadata store
        let objects = self.metadata_store.list_all_objects().await?;
        
        // Group objects by stream set
        let mut stream_sets: BTreeMap<String, Vec<S3ObjectMetadata>> = BTreeMap::new();
        for object in objects {
            let stream_set_id = object.stream_set_id.clone();
            stream_sets.entry(stream_set_id).or_default().push(object);
        }
        
        // Analyze each stream set
        for (stream_set_id, objects) in stream_sets {
            // Check for size-based compaction
            if self.should_compact_by_size(&objects) {
                candidates.push(CompactionCandidate {
                    stream_set_id: stream_set_id.clone(),
                    objects: objects.clone(),
                    reason: CompactionReason::Size,
                });
            }
            
            // Check for age-based compaction
            if self.should_compact_by_age(&objects) {
                candidates.push(CompactionCandidate {
                    stream_set_id: stream_set_id.clone(),
                    objects: objects.clone(),
                    reason: CompactionReason::Age,
                });
            }
            
            // Check for stream count-based compaction
            if self.should_split_by_stream_count(&objects) {
                candidates.push(CompactionCandidate {
                    stream_set_id,
                    objects,
                    reason: CompactionReason::StreamCount,
                });
            }
        }
        
        Ok(candidates)
    }
    
    async fn execute_compaction_task(&self, task: CompactionTask) -> Result<(), CompactionError> {
        match task.strategy {
            CompactionStrategy::StreamSetMerge => {
                self.execute_stream_set_merge(task).await
            }
            CompactionStrategy::ForceSplit => {
                self.execute_force_split(task).await
            }
            CompactionStrategy::AgeBased => {
                self.execute_age_based_compaction(task).await
            }
        }
    }
    
    async fn execute_stream_set_merge(&self, task: CompactionTask) -> Result<(), CompactionError> {
        // Read all source objects
        let mut all_data = Vec::new();
        let mut total_size = 0;
        
        for object in &task.objects {
            // Apply bandwidth limiting
            self.bandwidth_limiter.acquire(object.size as usize).await
                .map_err(|e| CompactionError::BandwidthLimit { message: e.to_string() })?;
            
            let data = self.object_storage.read(&object.key).await
                .map_err(|e| CompactionError::StorageError { source: e })?;
            
            total_size += data.len();
            all_data.push(data);
        }
        
        // Merge data blocks
        let merged_data = self.merge_data_blocks(all_data).await?;
        
        // Write merged object
        let new_key = format!("compacted/stream-set-{}-{}", 
                             task.task_id, 
                             chrono::Utc::now().timestamp());
        
        self.object_storage
            .write(
                &new_key,
                merged_data,
                WriteOptions {
                    bucket: "data-bucket".to_string(),
                    key: new_key.clone(),
                    metadata: HashMap::new(),
                    content_type: Some("application/octet-stream".to_string()),
                },
            )
            .await
            .map_err(|e| CompactionError::StorageError { source: e })?;
        
        // Update metadata
        self.metadata_store
            .replace_objects(&task.objects, &new_key)
            .await?;
        
        // Delete old objects
        let old_keys: Vec<String> = task.objects.iter().map(|o| o.key.clone()).collect();
        self.object_storage
            .delete(&old_keys)
            .await
            .map_err(|e| CompactionError::StorageError { source: e })?;
        
        Ok(())
    }
    
    async fn merge_data_blocks(&self, data_blocks: Vec<Bytes>) -> Result<Bytes, CompactionError> {
        // This would implement the actual data block merging logic
        // For now, just concatenate (real implementation would be more sophisticated)
        let total_size: usize = data_blocks.iter().map(|b| b.len()).sum();
        let mut merged = Vec::with_capacity(total_size);
        
        for block in data_blocks {
            merged.extend_from_slice(&block);
        }
        
        Ok(Bytes::from(merged))
    }
    
    fn should_compact_by_size(&self, objects: &[S3ObjectMetadata]) -> bool {
        let total_size: u64 = objects.iter().map(|o| o.size).sum();
        let small_objects = objects.iter().filter(|o| o.size < self.config.min_object_size).count();
        
        objects.len() > self.config.min_objects_for_compaction
            && small_objects > objects.len() / 2
            && total_size < self.config.max_compacted_size
    }
    
    fn should_compact_by_age(&self, objects: &[S3ObjectMetadata]) -> bool {
        let now = chrono::Utc::now();
        objects
            .iter()
            .any(|o| now.signed_duration_since(o.created_at).num_seconds() > self.config.max_age_seconds)
    }
    
    fn should_split_by_stream_count(&self, objects: &[S3ObjectMetadata]) -> bool {
        objects
            .iter()
            .any(|o| o.stream_count > self.config.max_streams_per_object)
    }
}

#[derive(Debug)]
struct CompactionCandidate {
    stream_set_id: String,
    objects: Vec<S3ObjectMetadata>,
    reason: CompactionReason,
}

#[derive(Debug)]
enum CompactionReason {
    Size,
    Age,
    StreamCount,
}

#[derive(Error, Debug)]
pub enum CompactionError {
    #[error("Storage error: {source}")]
    StorageError { #[from] source: ObjectStorageError },
    
    #[error("Metadata error: {message}")]
    MetadataError { message: String },
    
    #[error("Bandwidth limit error: {message}")]
    BandwidthLimit { message: String },
    
    #[error("Serialization error: {message}")]
    SerializationError { message: String },
}
```

### 4.4 Bandwidth Limiting

#### 4.4.1 Token Bucket Implementation

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::{interval, Duration, Instant};
use std::collections::VecDeque;

pub struct BandwidthLimiter {
    capacity: u64,
    tokens: AtomicU64,
    refill_rate: u64,
    refill_period: Duration,
    last_refill: Arc<Mutex<Instant>>,
    waiting_requests: Arc<Mutex<VecDeque<BandwidthRequest>>>,
}

#[derive(Debug)]
struct BandwidthRequest {
    bytes: usize,
    sender: tokio::sync::oneshot::Sender<Result<(), BandwidthError>>,
    priority: ThrottleStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThrottleStrategy {
    Bypass,    // No throttling for critical operations
    CatchUp,   // Priority throttling for recovery
    Tail,      // Standard throttling
}

#[derive(Error, Debug)]
pub enum BandwidthError {
    #[error("Request too large: {requested} bytes, limit: {limit} bytes")]
    RequestTooLarge { requested: usize, limit: u64 },
    
    #[error("Limiter is shutting down")]
    ShuttingDown,
}

impl BandwidthLimiter {
    pub fn new(bytes_per_second: u64) -> Self {
        let refill_period = Duration::from_millis(10); // 10ms refill period
        let capacity = bytes_per_second;
        let refill_rate = bytes_per_second / 100; // Refill every 10ms
        
        let limiter = Self {
            capacity,
            tokens: AtomicU64::new(capacity),
            refill_rate,
            refill_period,
            last_refill: Arc::new(Mutex::new(Instant::now())),
            waiting_requests: Arc::new(Mutex::new(VecDeque::new())),
        };
        
        // Start refill task
        let tokens = limiter.tokens.clone();
        let last_refill = limiter.last_refill.clone();
        let waiting_requests = limiter.waiting_requests.clone();
        let capacity = limiter.capacity;
        let refill_rate = limiter.refill_rate;
        let refill_period = limiter.refill_period;
        
        tokio::spawn(async move {
            let mut interval = interval(refill_period);
            loop {
                interval.tick().await;
                
                // Refill tokens
                {
                    let mut last_refill_time = last_refill.lock().await;
                    let now = Instant::now();
                    let elapsed = now.duration_since(*last_refill_time);
                    
                    if elapsed >= refill_period {
                        let current_tokens = tokens.load(Ordering::SeqCst);
                        let new_tokens = (current_tokens + refill_rate).min(capacity);
                        tokens.store(new_tokens, Ordering::SeqCst);
                        *last_refill_time = now;
                    }
                }
                
                // Process waiting requests
                Self::process_waiting_requests(&tokens, &waiting_requests, capacity).await;
            }
        });
        
        limiter
    }
    
    pub async fn acquire(&self, bytes: usize) -> Result<(), BandwidthError> {
        self.acquire_with_strategy(bytes, ThrottleStrategy::Tail).await
    }
    
    pub async fn acquire_with_strategy(
        &self,
        bytes: usize,
        strategy: ThrottleStrategy,
    ) -> Result<(), BandwidthError> {
        if bytes as u64 > self.capacity {
            return Err(BandwidthError::RequestTooLarge {
                requested: bytes,
                limit: self.capacity,
            });
        }
        
        // Bypass strategy - no throttling
        if strategy == ThrottleStrategy::Bypass {
            return Ok(());
        }
        
        // Try immediate acquisition
        loop {
            let current_tokens = self.tokens.load(Ordering::SeqCst);
            if current_tokens >= bytes as u64 {
                match self.tokens.compare_exchange_weak(
                    current_tokens,
                    current_tokens - bytes as u64,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return Ok(()),
                    Err(_) => continue, // Retry on contention
                }
            } else {
                break; // Not enough tokens, need to wait
            }
        }
        
        // Create waiting request
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request = BandwidthRequest {
            bytes,
            sender: tx,
            priority: strategy,
        };
        
        // Add to waiting queue (with priority ordering)
        {
            let mut waiting = self.waiting_requests.lock().await;
            let insert_index = waiting
                .iter()
                .position(|r| r.priority > strategy)
                .unwrap_or(waiting.len());
            waiting.insert(insert_index, request);
        }
        
        // Wait for allocation
        rx.await.map_err(|_| BandwidthError::ShuttingDown)?
    }
    
    async fn process_waiting_requests(
        tokens: &AtomicU64,
        waiting_requests: &Arc<Mutex<VecDeque<BandwidthRequest>>>,
        capacity: u64,
    ) {
        let mut to_process = Vec::new();
        
        // Collect requests that can be satisfied
        {
            let mut waiting = waiting_requests.lock().await;
            let mut current_tokens = tokens.load(Ordering::SeqCst);
            
            while let Some(request) = waiting.pop_front() {
                if current_tokens >= request.bytes as u64 {
                    current_tokens -= request.bytes as u64;
                    to_process.push(request);
                } else {
                    // Put it back and stop processing
                    waiting.push_front(request);
                    break;
                }
            }
            
            // Update token count
            tokens.store(current_tokens, Ordering::SeqCst);
        }
        
        // Notify processed requests
        for request in to_process {
            let _ = request.sender.send(Ok(()));
        }
    }
}
```

## 5. Configuration and Deployment

### 5.1 Configuration System

AutoMQ uses a layered configuration system:

1. **Default Configuration**: Built-in defaults
2. **File Configuration**: Properties files
3. **Environment Variables**: Runtime overrides
4. **Command Line Arguments**: Deployment-specific overrides

#### 5.1.1 Key Configuration Categories

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoMqConfig {
    pub server: ServerConfig,
    pub s3: S3Config,
    pub wal: WalConfig,
    pub cache: CacheConfig,
    pub network: NetworkConfig,
    pub compaction: CompactionConfig,
    pub metrics: MetricsConfig,
    pub auto_balancer: AutoBalancerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub node_id: u32,
    pub process_roles: Vec<ProcessRole>, // broker, controller
    pub listeners: Vec<Listener>,
    pub advertised_listeners: Vec<Listener>,
    pub controller_quorum_voters: Vec<QuorumVoter>,
    pub cluster_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessRole {
    Broker,
    Controller,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listener {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub security_protocol: SecurityProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityProtocol {
    Plaintext,
    Ssl,
    SaslPlaintext,
    SaslSsl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub data_buckets: Vec<S3Bucket>,
    pub ops_buckets: Vec<S3Bucket>,
    pub wal_path: S3Path,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub path_style: bool,
    pub auth_type: AuthType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Bucket {
    pub index: u32,
    pub bucket: String,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthType {
    Instance,
    Static,
}
```

### 5.2 Deployment Configurations

#### 5.2.1 Single Node Setup

```yaml
# docker-compose.yaml equivalent in Rust config
version: "3.8"

services:
  minio:
    image: "minio/minio:RELEASE.2025-05-24T17-08-30Z"
    environment:
      MINIO_ROOT_USER: "minioadmin"
      MINIO_ROOT_PASSWORD: "minioadmin"
    ports:
      - "9000:9000"
      - "9001:9001"
    
  automq:
    image: "automq/automq-rust:latest"
    environment:
      AUTOMQ_S3_ACCESS_KEY: "minioadmin"
      AUTOMQ_S3_SECRET_KEY: "minioadmin"
      AUTOMQ_CLUSTER_ID: "3D4fXN-yS1-vsQ8aJ_q4Mg"
    config:
      server:
        node_id: 0
        process_roles: ["broker", "controller"]
        cluster_id: "3D4fXN-yS1-vsQ8aJ_q4Mg"
      s3:
        data_buckets:
          - index: 0
            bucket: "automq-data"
        ops_buckets:
          - index: 1
            bucket: "automq-ops"
        endpoint: "http://minio:9000"
        path_style: true
        auth_type: "static"
```

#### 5.2.2 Cluster Setup (3 Nodes)

```rust
// Node 1 Configuration
AutoMqConfig {
    server: ServerConfig {
        node_id: 0,
        process_roles: vec![ProcessRole::Broker, ProcessRole::Controller],
        listeners: vec![
            Listener {
                name: "PLAINTEXT".to_string(),
                host: "0.0.0.0".to_string(),
                port: 9092,
                security_protocol: SecurityProtocol::Plaintext,
            },
            Listener {
                name: "CONTROLLER".to_string(),
                host: "0.0.0.0".to_string(),
                port: 9093,
                security_protocol: SecurityProtocol::Plaintext,
            },
        ],
        controller_quorum_voters: vec![
            QuorumVoter { id: 0, host: "server1".to_string(), port: 9093 },
            QuorumVoter { id: 1, host: "server2".to_string(), port: 9093 },
            QuorumVoter { id: 2, host: "server3".to_string(), port: 9093 },
        ],
        cluster_id: "5XF4fHIOTfSIqkmje2KFlg".to_string(),
    },
    s3: S3Config {
        data_buckets: vec![S3Bucket {
            index: 0,
            bucket: "automq-data".to_string(),
            prefix: None,
        }],
        ops_buckets: vec![S3Bucket {
            index: 1,
            bucket: "automq-ops".to_string(),
            prefix: None,
        }],
        wal_path: S3Path {
            bucket: "automq-data".to_string(),
            prefix: Some("wal".to_string()),
        },
        region: "us-east-1".to_string(),
        endpoint: Some("http://minio:9000".to_string()),
        path_style: true,
        auth_type: AuthType::Static,
        access_key: Some("minioadmin".to_string()),
        secret_key: Some("minioadmin".to_string()),
    },
    // ... other config sections
}
```

### 5.3 Configuration Loading

```rust
use config::{Config, ConfigError, Environment, File};
use std::env;

impl AutoMqConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = Config::builder();
        
        // Start with default configuration
        config = config.add_source(File::with_name("config/default"));
        
        // Add environment-specific configuration
        if let Ok(env) = env::var("AUTOMQ_ENV") {
            config = config.add_source(File::with_name(&format!("config/{}", env)).required(false));
        }
        
        // Add local configuration (not in version control)
        config = config.add_source(File::with_name("config/local").required(false));
        
        // Override with environment variables
        config = config.add_source(
            Environment::with_prefix("AUTOMQ")
                .prefix_separator("_")
                .separator("__")
        );
        
        // Build and deserialize
        let config = config.build()?;
        config.try_deserialize()
    }
    
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate node_id
        if self.server.process_roles.is_empty() {
            return Err(ConfigError::Message("At least one process role must be specified".to_string()));
        }
        
        // Validate S3 configuration
        if self.s3.data_buckets.is_empty() {
            return Err(ConfigError::Message("At least one data bucket must be specified".to_string()));
        }
        
        // Validate cluster configuration for controllers
        if self.server.process_roles.contains(&ProcessRole::Controller) {
            if self.server.controller_quorum_voters.is_empty() {
                return Err(ConfigError::Message("Controller quorum voters must be specified".to_string()));
            }
        }
        
        // Validate cache sizes
        if self.cache.block_cache_size == 0 {
            return Err(ConfigError::Message("Block cache size must be greater than 0".to_string()));
        }
        
        Ok(())
    }
}
```

## 6. Rust Implementation Guide

### 6.1 Project Structure

```
automq-rust/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config/
│   │   ├── mod.rs
│   │   └── loader.rs
│   ├── server/
│   │   ├── mod.rs
│   │   ├── broker.rs
│   │   └── controller.rs
│   ├── s3stream/
│   │   ├── mod.rs
│   │   ├── stream.rs
│   │   ├── storage.rs
│   │   ├── wal/
│   │   │   ├── mod.rs
│   │   │   ├── object_wal.rs
│   │   │   └── memory_wal.rs
│   │   ├── cache/
│   │   │   ├── mod.rs
│   │   │   ├── block_cache.rs
│   │   │   └── log_cache.rs
│   │   ├── object_storage/
│   │   │   ├── mod.rs
│   │   │   ├── s3.rs
│   │   │   ├── local.rs
│   │   │   └── memory.rs
│   │   └── compaction/
│   │       ├── mod.rs
│   │       └── manager.rs
│   ├── network/
│   │   ├── mod.rs
│   │   ├── bandwidth_limiter.rs
│   │   └── protocol.rs
│   ├── metadata/
│   │   ├── mod.rs
│   │   └── store.rs
│   └── errors/
│       └── mod.rs
├── tests/
├── benches/
├── examples/
└── docker/
    ├── Dockerfile
    └── docker-compose.yml
```

### 6.2 Core Dependencies

```toml
[package]
name = "automq"
version = "0.1.0"
edition = "2021"

[dependencies]
# Async runtime
tokio = { version = "1.0", features = ["full"] }
async-trait = "0.1"

# Serialization
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"
bytes = "1.0"

# Configuration
config = "0.13"
clap = { version = "4.0", features = ["derive"] }

# AWS SDK
aws-config = "0.55"
aws-sdk-s3 = "0.26"

# Networking
hyper = "0.14"
tonic = "0.9"

# Caching
lru = "0.10"

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Collections
hashbrown = "0.14"

# Metrics
prometheus = "0.13"

# Testing
tokio-test = "0.4"
tempfile = "3.0"

[dev-dependencies]
criterion = "0.5"
proptest = "1.0"
```

### 6.3 Main Application Structure

```rust
// src/main.rs
use automq::{AutoMqConfig, AutoMqServer};
use clap::Parser;
use tracing::{info, error};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value = "config/server.toml")]
    config: String,
    
    #[arg(long)]
    node_id: Option<u32>,
    
    #[arg(long)]
    cluster_id: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    // Load configuration
    let mut config = AutoMqConfig::load_from_file(&args.config)?;
    
    // Override with command line arguments
    if let Some(node_id) = args.node_id {
        config.server.node_id = node_id;
    }
    if let Some(cluster_id) = args.cluster_id {
        config.server.cluster_id = cluster_id;
    }
    
    // Validate configuration
    config.validate()?;
    
    info!("Starting AutoMQ with config: {:?}", config);
    
    // Create and start server
    let server = AutoMqServer::new(config).await?;
    
    // Handle graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        info!("Received shutdown signal");
    };
    
    // Run server until shutdown
    tokio::select! {
        result = server.run() => {
            if let Err(e) = result {
                error!("Server error: {:?}", e);
                return Err(e.into());
            }
        }
        _ = shutdown_signal => {
            info!("Shutting down server");
            server.shutdown().await?;
        }
    }
    
    info!("Server shutdown complete");
    Ok(())
}
```

### 6.4 Server Implementation

```rust
// src/server/mod.rs
use crate::{AutoMqConfig, s3stream::S3Storage, network::NetworkServer};
use std::sync::Arc;

pub struct AutoMqServer {
    config: AutoMqConfig,
    storage: Arc<S3Storage>,
    network_server: NetworkServer,
}

impl AutoMqServer {
    pub async fn new(config: AutoMqConfig) -> Result<Self, ServerError> {
        // Initialize storage
        let storage = Arc::new(S3Storage::new(config.s3.clone()).await?);
        
        // Initialize network server
        let network_server = NetworkServer::new(
            config.server.clone(),
            storage.clone(),
        ).await?;
        
        Ok(Self {
            config,
            storage,
            network_server,
        })
    }
    
    pub async fn run(&self) -> Result<(), ServerError> {
        // Start storage
        self.storage.startup().await?;
        
        // Start network server
        self.network_server.start().await?;
        
        // Keep running
        std::future::pending::<()>().await;
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        // Shutdown network server
        self.network_server.shutdown().await?;
        
        // Shutdown storage
        self.storage.shutdown().await?;
        
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("Storage error: {source}")]
    Storage { #[from] source: crate::s3stream::StorageError },
    
    #[error("Network error: {source}")]
    Network { #[from] source: crate::network::NetworkError },
    
    #[error("Configuration error: {message}")]
    Config { message: String },
}
```

### 6.5 Testing Strategy

#### 6.5.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_stream_append_fetch() {
        let config = S3Config::default();
        let storage = Arc::new(MemoryStorage::new());
        let stream = S3Stream::new(1, 0, 0, storage).await.unwrap();
        
        // Test append
        let record_batch = RecordBatch {
            stream_id: 1,
            epoch: 0,
            base_offset: 0,
            count: 1,
            payload: Bytes::from("test data"),
        };
        
        let result = stream.append(record_batch).await.unwrap();
        assert_eq!(result.offset, 0);
        
        // Test fetch
        let fetch_result = stream.fetch(0, 1, 1024).await.unwrap();
        assert_eq!(fetch_result.records.len(), 1);
        assert_eq!(fetch_result.records[0].payload, Bytes::from("test data"));
    }
    
    #[tokio::test]
    async fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        
        // Create WAL and write some records
        let wal = ObjectWal::new(config.clone(), Arc::new(MemoryObjectStorage::new()));
        
        let record1 = WalRecord {
            offset: 0,
            stream_id: 1,
            data: Bytes::from("record1"),
            crc: 0,
            timestamp: 0,
        };
        
        wal.append(1, record1.data.clone(), 0).await.unwrap();
        wal.shutdown().await.unwrap();
        
        // Create new WAL and recover
        let wal2 = ObjectWal::new(config, Arc::new(MemoryObjectStorage::new()));
        let recovered = wal2.recover().await.unwrap();
        
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].data, record1.data);
    }
}
```

#### 6.5.2 Integration Tests

```rust
// tests/integration_test.rs
use automq::{AutoMqConfig, AutoMqServer};
use testcontainers::{clients::Cli, images::minio::MinIO, Container};

#[tokio::test]
async fn test_full_integration() {
    // Start MinIO container
    let docker = Cli::default();
    let minio_container = docker.run(MinIO::default());
    let minio_port = minio_container.get_host_port_ipv4(9000);
    
    // Create test configuration
    let config = AutoMqConfig {
        server: ServerConfig {
            node_id: 0,
            process_roles: vec![ProcessRole::Broker],
            listeners: vec![Listener {
                name: "PLAINTEXT".to_string(),
                host: "127.0.0.1".to_string(),
                port: 0, // Random port
                security_protocol: SecurityProtocol::Plaintext,
            }],
            controller_quorum_voters: vec![],
            cluster_id: "test-cluster".to_string(),
        },
        s3: S3Config {
            data_buckets: vec![S3Bucket {
                index: 0,
                bucket: "test-bucket".to_string(),
                prefix: None,
            }],
            endpoint: Some(format!("http://127.0.0.1:{}", minio_port)),
            access_key: Some("minioadmin".to_string()),
            secret_key: Some("minioadmin".to_string()),
            path_style: true,
            ..Default::default()
        },
        ..Default::default()
    };
    
    // Start server
    let server = AutoMqServer::new(config).await.unwrap();
    
    tokio::spawn(async move {
        server.run().await.unwrap();
    });
    
    // Wait for server to start
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    // Test operations
    // ... (client operations)
    
    // Cleanup is automatic when containers are dropped
}
```

#### 6.5.3 Benchmarks

```rust
// benches/stream_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use automq::s3stream::{S3Stream, RecordBatch};
use bytes::Bytes;

fn bench_stream_append(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("stream_append");
    group.throughput(Throughput::Bytes(1024));
    
    group.bench_function("1kb_records", |b| {
        b.to_async(&rt).iter(|| async {
            let storage = Arc::new(MemoryStorage::new());
            let stream = S3Stream::new(1, 0, 0, storage).await.unwrap();
            
            let record = RecordBatch {
                stream_id: 1,
                epoch: 0,
                base_offset: 0,
                count: 1,
                payload: Bytes::from(vec![0u8; 1024]),
            };
            
            black_box(stream.append(record).await.unwrap());
        });
    });
    
    group.finish();
}

criterion_group!(benches, bench_stream_append);
criterion_main!(benches);
```

### 6.6 Performance Optimizations

#### 6.6.1 Memory Management

```rust
use bytes::{Bytes, BytesMut, BufMut};
use std::sync::Arc;

// Use Bytes for zero-copy operations
pub struct ZeroCopyBuffer {
    data: Bytes,
}

impl ZeroCopyBuffer {
    pub fn new(data: impl Into<Bytes>) -> Self {
        Self { data: data.into() }
    }
    
    pub fn slice(&self, start: usize, end: usize) -> Bytes {
        self.data.slice(start..end) // Zero-copy slice
    }
    
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

// Memory pool for buffer reuse
pub struct BufferPool {
    pool: Arc<Mutex<Vec<BytesMut>>>,
    buffer_size: usize,
}

impl BufferPool {
    pub fn new(buffer_size: usize, initial_capacity: usize) -> Self {
        let mut pool = Vec::with_capacity(initial_capacity);
        for _ in 0..initial_capacity {
            pool.push(BytesMut::with_capacity(buffer_size));
        }
        
        Self {
            pool: Arc::new(Mutex::new(pool)),
            buffer_size,
        }
    }
    
    pub async fn get(&self) -> BytesMut {
        let mut pool = self.pool.lock().await;
        pool.pop().unwrap_or_else(|| BytesMut::with_capacity(self.buffer_size))
    }
    
    pub async fn return_buffer(&self, mut buffer: BytesMut) {
        buffer.clear();
        let mut pool = self.pool.lock().await;
        if pool.len() < 100 { // Max pool size
            pool.push(buffer);
        }
    }
}
```

#### 6.6.2 Async Optimization

```rust
use tokio::task::JoinSet;
use futures::future::try_join_all;

// Parallel processing with bounded concurrency
pub struct ParallelProcessor<T> {
    max_concurrency: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> ParallelProcessor<T>
where
    T: Send + 'static,
{
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            _phantom: std::marker::PhantomData,
        }
    }
    
    pub async fn process_all<F, Fut, R>(
        &self,
        items: Vec<T>,
        processor: F,
    ) -> Result<Vec<R>, ProcessingError>
    where
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<R, ProcessingError>> + Send,
        R: Send + 'static,
    {
        let mut join_set = JoinSet::new();
        let mut results = Vec::with_capacity(items.len());
        let mut items_iter = items.into_iter();
        
        // Start initial batch
        for _ in 0..self.max_concurrency.min(items_iter.len()) {
            if let Some(item) = items_iter.next() {
                let processor = processor.clone();
                join_set.spawn(async move { processor(item).await });
            }
        }
        
        // Process remaining items as others complete
        while let Some(result) = join_set.join_next().await {
            let output = result.map_err(|e| ProcessingError::JoinError {
                message: e.to_string(),
            })??;
            results.push(output);
            
            // Start next item if available
            if let Some(item) = items_iter.next() {
                let processor = processor.clone();
                join_set.spawn(async move { processor(item).await });
            }
        }
        
        Ok(results)
    }
}

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("Join error: {message}")]
    JoinError { message: String },
    
    #[error("Processing error: {message}")]
    Processing { message: String },
}
```

## 7. Migration Strategy

### 7.1 Phase 1: Foundation (Months 1-2)

**Objectives**: 
- Set up Rust project structure
- Implement core traits and interfaces
- Create basic object storage abstraction

**Deliverables**:
- [ ] Project setup with CI/CD
- [ ] Core trait definitions (`Stream`, `Storage`, `ObjectStorage`)
- [ ] Basic error handling system
- [ ] Configuration loading system
- [ ] Memory-based implementations for testing

**Key Tasks**:
1. Initialize Rust project with proper tooling
2. Define all core interfaces
3. Implement memory storage backend
4. Set up comprehensive testing framework
5. Create basic benchmarking suite

### 7.2 Phase 2: Storage Layer (Months 3-4)

**Objectives**:
- Implement S3 object storage backend
- Create WAL system
- Basic caching implementation

**Deliverables**:
- [ ] S3 object storage implementation
- [ ] WAL interface and object-based WAL
- [ ] Basic block cache
- [ ] Integration tests with MinIO

**Key Tasks**:
1. Implement AWS S3 client integration
2. Create object WAL with upload/recovery
3. Implement LRU block cache
4. Add bandwidth limiting
5. Create integration test suite

### 7.3 Phase 3: Core Streaming (Months 5-6)

**Objectives**:
- Implement S3Stream
- Create storage management layer
- Add basic compaction

**Deliverables**:
- [ ] Complete S3Stream implementation
- [ ] Storage coordination layer
- [ ] Basic compaction system
- [ ] Performance benchmarks

**Key Tasks**:
1. Implement full S3Stream with all operations
2. Add stream lifecycle management
3. Create compaction manager
4. Optimize performance-critical paths
5. Add comprehensive metrics

### 7.4 Phase 4: Network Layer (Months 7-8)

**Objectives**:
- Kafka protocol compatibility
- Network server implementation
- Client integration

**Deliverables**:
- [ ] Kafka protocol implementation
- [ ] Network server with proper routing
- [ ] Client compatibility layer
- [ ] End-to-end integration tests

**Key Tasks**:
1. Implement Kafka wire protocol
2. Create network request routing
3. Add authentication/authorization
4. Test with existing Kafka clients
5. Performance optimization

### 7.5 Phase 5: Advanced Features (Months 9-10)

**Objectives**:
- Advanced compaction strategies
- Metadata management
- Monitoring and observability

**Deliverables**:
- [ ] Advanced compaction algorithms
- [ ] Metadata store implementation
- [ ] Metrics and monitoring
- [ ] Production readiness features

**Key Tasks**:
1. Implement sophisticated compaction
2. Add metadata persistence
3. Create monitoring dashboards
4. Add operational tools
5. Security hardening

### 7.6 Phase 6: Production Deployment (Months 11-12)

**Objectives**:
- Production deployment
- Performance tuning
- Documentation and training

**Deliverables**:
- [ ] Production deployment guides
- [ ] Performance optimization
- [ ] Comprehensive documentation
- [ ] Migration tools from Java version

**Key Tasks**:
1. Production deployment automation
2. Performance benchmarking and tuning
3. Create migration tooling
4. Write operational documentation
5. Team training and knowledge transfer

## 8. Performance Considerations

### 8.1 Memory Management

**Zero-Copy Operations**:
- Use `Bytes` type for efficient buffer management
- Implement buffer pools for frequent allocations
- Minimize data copying between layers

**Memory Pressure Handling**:
```rust
pub struct MemoryPressureManager {
    total_memory: usize,
    used_memory: AtomicUsize,
    warning_threshold: usize,
    critical_threshold: usize,
}

impl MemoryPressureManager {
    pub fn check_pressure(&self) -> MemoryPressure {
        let used = self.used_memory.load(Ordering::SeqCst);
        let usage_ratio = used as f64 / self.total_memory as f64;
        
        if usage_ratio > 0.9 {
            MemoryPressure::Critical
        } else if usage_ratio > 0.7 {
            MemoryPressure::Warning
        } else {
            MemoryPressure::Normal
        }
    }
}
```

### 8.2 Concurrency Optimization

**Lock-Free Data Structures**:
- Use atomic operations where possible
- Implement lock-free queues for high-throughput paths
- Minimize critical sections

**Task Scheduling**:
```rust
// Separate thread pools for different workloads
pub struct TaskExecutor {
    io_pool: ThreadPool,      // I/O intensive tasks
    cpu_pool: ThreadPool,     // CPU intensive tasks
    priority_pool: ThreadPool, // High priority tasks
}

impl TaskExecutor {
    pub async fn execute_io<F, T>(&self, task: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.io_pool.spawn(move || {
            let result = task();
            let _ = tx.send(result);
        });
        rx.await.unwrap()
    }
}
```

### 8.3 Network Optimization

**Connection Pooling**:
- Reuse HTTP connections for S3 operations
- Implement connection warming
- Monitor connection health

**Batching**:
- Batch small requests to reduce overhead
- Implement adaptive batch sizing
- Balance latency vs. throughput

### 8.4 Storage Optimization

**Read-Ahead Strategies**:
```rust
pub struct ReadAheadCache {
    cache: Arc<RwLock<LruCache<String, Bytes>>>,
    predictor: Arc<AccessPatternPredictor>,
}

impl ReadAheadCache {
    pub async fn get_with_prediction(&self, key: &str) -> Option<Bytes> {
        // Get current block
        let result = {
            let cache = self.cache.read().await;
            cache.get(key).cloned()
        };
        
        // Predict next access and prefetch
        if let Some(predicted_keys) = self.predictor.predict_next(key) {
            for pred_key in predicted_keys {
                self.prefetch(&pred_key).await;
            }
        }
        
        result
    }
}
```

## 9. Testing Strategy

### 9.1 Unit Testing

**Coverage Requirements**:
- Minimum 80% code coverage
- 100% coverage for critical paths (stream operations, WAL)
- Property-based testing for complex algorithms

**Example Property Tests**:
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_stream_append_fetch_invariant(
        records in prop::collection::vec(any::<Vec<u8>>(), 1..100)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let storage = Arc::new(MemoryStorage::new());
            let stream = S3Stream::new(1, 0, 0, storage).await.unwrap();
            
            let mut expected_data = Vec::new();
            let mut offset = 0;
            
            // Append all records
            for record_data in &records {
                let record = RecordBatch {
                    stream_id: 1,
                    epoch: 0,
                    base_offset: offset,
                    count: 1,
                    payload: Bytes::from(record_data.clone()),
                };
                
                stream.append(record).await.unwrap();
                expected_data.push(record_data.clone());
                offset += 1;
            }
            
            // Fetch all records
            let fetch_result = stream.fetch(0, offset, usize::MAX).await.unwrap();
            
            // Verify all data matches
            assert_eq!(fetch_result.records.len(), records.len());
            for (i, record) in fetch_result.records.iter().enumerate() {
                assert_eq!(record.payload.as_ref(), &expected_data[i]);
            }
        });
    }
}
```

### 9.2 Integration Testing

**Test Scenarios**:
- Full end-to-end workflows
- Failure injection and recovery
- Performance under load
- Multi-node cluster behavior

### 9.3 Load Testing

**Performance Benchmarks**:
```rust
// Performance targets
const APPEND_THROUGHPUT_TARGET: f64 = 100_000.0; // ops/sec
const APPEND_LATENCY_P99_TARGET: Duration = Duration::from_millis(10);
const FETCH_THROUGHPUT_TARGET: f64 = 50_000.0; // ops/sec

#[tokio::test]
async fn benchmark_append_performance() {
    let storage = Arc::new(MemoryStorage::new());
    let stream = S3Stream::new(1, 0, 0, storage).await.unwrap();
    
    let num_operations = 10_000;
    let record_size = 1024; // 1KB
    
    let start = Instant::now();
    let mut tasks = Vec::new();
    
    for i in 0..num_operations {
        let stream = stream.clone();
        let task = tokio::spawn(async move {
            let record = RecordBatch {
                stream_id: 1,
                epoch: 0,
                base_offset: i,
                count: 1,
                payload: Bytes::from(vec![0u8; record_size]),
            };
            
            stream.append(record).await.unwrap()
        });
        tasks.push(task);
    }
    
    // Wait for all operations
    for task in tasks {
        task.await.unwrap();
    }
    
    let duration = start.elapsed();
    let throughput = num_operations as f64 / duration.as_secs_f64();
    
    assert!(
        throughput >= APPEND_THROUGHPUT_TARGET,
        "Append throughput {} ops/sec below target {} ops/sec",
        throughput,
        APPEND_THROUGHPUT_TARGET
    );
}
```

## 10. Conclusion

This comprehensive technical guide provides a detailed roadmap for refactoring AutoMQ from Java to Rust. The analysis covers:

### 10.1 Key Architectural Insights

1. **S3-Centric Design**: AutoMQ's core innovation is treating S3 as the primary storage layer rather than just a backup tier
2. **Stateless Brokers**: The separation of compute and storage enables true elastic scaling
3. **WAL Strategy**: The dual WAL approach (local + S3) provides both performance and durability
4. **Sophisticated Caching**: Multi-level caching is essential for S3-based performance
5. **Compaction Complexity**: Object-based compaction requires careful orchestration

### 10.2 Rust Implementation Benefits

1. **Memory Safety**: Eliminates entire classes of bugs common in Java (null pointer exceptions, memory leaks)
2. **Performance**: Zero-cost abstractions and efficient memory management
3. **Concurrency**: Safer concurrent programming with ownership model
4. **Resource Efficiency**: Lower memory footprint and CPU usage
5. **Reliability**: Strong type system catches errors at compile time

### 10.3 Critical Success Factors

1. **Incremental Migration**: Phase-by-phase implementation reduces risk
2. **Comprehensive Testing**: Property-based testing and extensive benchmarking
3. **Performance Monitoring**: Continuous performance validation against targets
4. **Team Training**: Adequate Rust expertise development
5. **Operational Tooling**: Migration and monitoring tools for production deployment

### 10.4 Risk Mitigation

1. **Prototype Early**: Validate critical performance assumptions
2. **Maintain Compatibility**: Ensure Kafka protocol compatibility throughout
3. **Monitor Performance**: Establish performance baselines and regression testing
4. **Plan Rollback**: Have clear rollback procedures for each phase
5. **Gradual Deployment**: Use canary deployments for production migration

### 10.5 Expected Outcomes

Upon successful completion of this Rust refactoring:

- **50-70% reduction** in memory usage
- **20-30% improvement** in throughput
- **Improved operational reliability** through memory safety
- **Enhanced developer productivity** through better tooling and type safety
- **Future-proof architecture** for cloud-native scaling

This guide serves as both a technical specification and implementation roadmap for the AutoMQ Rust refactoring project. The detailed code examples, architectural patterns, and migration strategy provide a solid foundation for the development team to execute this ambitious but valuable transformation.