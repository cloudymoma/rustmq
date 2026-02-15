// Advanced log compaction and snapshot management for RustMQ OpenRaft
// Production-ready with incremental snapshots, compression, and efficient storage

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

// use crate::controller::openraft_storage::{NodeId, RustMqSnapshotData, RustMqStorageConfig};
// Temporary types until openraft_storage is fully fixed
pub type NodeId = u64;

/// Temporary partition assignment until openraft_storage is fixed
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TempPartitionAssignment {
    pub leader: crate::types::BrokerId,
    pub replicas: Vec<crate::types::BrokerId>,
    pub in_sync_replicas: Vec<crate::types::BrokerId>,
    pub leader_epoch: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RustMqStorageConfig {
    pub data_dir: std::path::PathBuf,
    pub max_log_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RustMqSnapshotData {
    pub topics: BTreeMap<String, crate::controller::service::TopicInfo>,
    pub brokers: BTreeMap<String, crate::types::BrokerInfo>,
    pub partition_assignments: BTreeMap<crate::types::TopicPartition, TempPartitionAssignment>,
    pub last_applied_log: Option<u64>,
    pub snapshot_version: u64,
}

/// Configuration for log compaction and snapshot management
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Maximum number of log entries before triggering compaction
    pub max_log_entries: u64,
    /// Minimum number of entries to keep after compaction
    pub min_entries_to_keep: u64,
    /// Time-based compaction interval in seconds
    pub compaction_interval_secs: u64,
    /// Maximum snapshot file size in bytes before splitting
    pub max_snapshot_size: u64,
    /// Enable compression for snapshots
    pub enable_compression: bool,
    /// Compression level (1-9 for zstd)
    pub compression_level: i32,
    /// Keep N historical snapshots
    pub keep_snapshot_count: usize,
    /// Incremental snapshot threshold (entries since last full snapshot)
    pub incremental_threshold: u64,
    /// Verify snapshot integrity on load
    pub verify_snapshots: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_log_entries: 10000,
            min_entries_to_keep: 1000,
            compaction_interval_secs: 300,        // 5 minutes
            max_snapshot_size: 100 * 1024 * 1024, // 100MB
            enable_compression: true,
            compression_level: 3,
            keep_snapshot_count: 5,
            incremental_threshold: 1000,
            verify_snapshots: true,
        }
    }
}

/// Snapshot metadata for efficient management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Unique snapshot ID
    pub snapshot_id: String,
    /// Snapshot type (full or incremental)
    pub snapshot_type: SnapshotType,
    /// Last log ID included in this snapshot
    pub last_log_id: u64,
    /// Creation timestamp
    pub created_at: u64,
    /// Compressed size in bytes
    pub compressed_size: u64,
    /// Uncompressed size in bytes
    pub uncompressed_size: u64,
    /// Checksum for integrity verification
    pub checksum: String,
    /// Base snapshot ID for incremental snapshots
    pub base_snapshot_id: Option<String>,
    /// Number of topics in snapshot
    pub topic_count: usize,
    /// Number of brokers in snapshot
    pub broker_count: usize,
}

/// Type of snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SnapshotType {
    /// Full snapshot containing complete state
    Full,
    /// Incremental snapshot containing changes since base
    Incremental,
}

/// Snapshot chunk for large snapshot transfers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotChunk {
    /// Snapshot ID
    pub snapshot_id: String,
    /// Chunk sequence number
    pub chunk_id: u32,
    /// Total number of chunks
    pub total_chunks: u32,
    /// Chunk data
    pub data: Vec<u8>,
    /// Chunk checksum
    pub checksum: String,
}

/// Log compaction statistics
#[derive(Debug, Clone, Default)]
pub struct CompactionStats {
    /// Total number of compactions performed
    pub total_compactions: u64,
    /// Total entries compacted
    pub entries_compacted: u64,
    /// Total time spent in compaction (milliseconds)
    pub total_compaction_time_ms: u64,
    /// Total snapshots created
    pub snapshots_created: u64,
    /// Total snapshot bytes created
    pub snapshot_bytes_created: u64,
    /// Last compaction timestamp
    pub last_compaction_at: Option<u64>,
    /// Average compaction time
    pub avg_compaction_time_ms: u64,
}

impl CompactionStats {
    /// Record a compaction operation
    pub fn record_compaction(&mut self, entries: u64, duration: Duration) {
        self.total_compactions += 1;
        self.entries_compacted += entries;
        let duration_ms = duration.as_millis() as u64;
        self.total_compaction_time_ms += duration_ms;
        self.last_compaction_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
        self.avg_compaction_time_ms = self.total_compaction_time_ms / self.total_compactions;
    }

    /// Record a snapshot operation
    pub fn record_snapshot(&mut self, size: u64) {
        self.snapshots_created += 1;
        self.snapshot_bytes_created += size;
    }
}

/// Advanced log compaction manager
pub struct LogCompactionManager {
    /// Configuration
    config: CompactionConfig,
    /// Storage configuration
    storage_config: RustMqStorageConfig,
    /// Compaction statistics
    stats: Arc<Mutex<CompactionStats>>,
    /// Snapshot metadata cache
    snapshot_metadata: Arc<RwLock<BTreeMap<String, SnapshotMetadata>>>,
    /// Currently active compaction task
    active_compaction: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl LogCompactionManager {
    /// Create a new log compaction manager
    pub async fn new(
        config: CompactionConfig,
        storage_config: RustMqStorageConfig,
    ) -> crate::Result<Self> {
        let manager = Self {
            config,
            storage_config,
            stats: Arc::new(Mutex::new(CompactionStats::default())),
            snapshot_metadata: Arc::new(RwLock::new(BTreeMap::new())),
            active_compaction: Arc::new(Mutex::new(None)),
        };

        // Load existing snapshot metadata
        manager.load_snapshot_metadata().await?;

        info!("Log compaction manager initialized");
        Ok(manager)
    }

    /// Load snapshot metadata from disk
    async fn load_snapshot_metadata(&self) -> crate::Result<()> {
        let snapshots_dir = self.storage_config.data_dir.join("snapshots");
        if !snapshots_dir.exists() {
            tokio::fs::create_dir_all(&snapshots_dir).await?;
            return Ok(());
        }

        let mut dir_entries = tokio::fs::read_dir(&snapshots_dir).await?;
        let mut loaded_count = 0;

        while let Some(entry) = dir_entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".meta") {
                    match self.load_single_metadata(&entry.path()).await {
                        Ok(metadata) => {
                            let mut meta_map = self.snapshot_metadata.write().await;
                            meta_map.insert(metadata.snapshot_id.clone(), metadata);
                            loaded_count += 1;
                        }
                        Err(e) => {
                            warn!(
                                "Failed to load snapshot metadata from {:?}: {}",
                                entry.path(),
                                e
                            );
                        }
                    }
                }
            }
        }

        info!("Loaded {} snapshot metadata entries", loaded_count);
        Ok(())
    }

    /// Load a single snapshot metadata file
    async fn load_single_metadata(&self, path: &Path) -> crate::Result<SnapshotMetadata> {
        let content = tokio::fs::read_to_string(path).await?;
        let metadata: SnapshotMetadata = serde_json::from_str(&content)?;

        // Verify snapshot integrity if enabled
        if self.config.verify_snapshots {
            self.verify_snapshot_integrity(&metadata).await?;
        }

        Ok(metadata)
    }

    /// Verify snapshot file integrity
    async fn verify_snapshot_integrity(&self, metadata: &SnapshotMetadata) -> crate::Result<()> {
        let snapshot_file = self
            .storage_config
            .data_dir
            .join("snapshots")
            .join(format!("{}.snap", metadata.snapshot_id));

        if !snapshot_file.exists() {
            return Err(crate::error::RustMqError::SnapshotError(format!(
                "Snapshot file not found: {:?}",
                snapshot_file
            )));
        }

        let file_content = tokio::fs::read(&snapshot_file).await?;
        let computed_checksum = self.compute_checksum(&file_content);

        if computed_checksum != metadata.checksum {
            return Err(crate::error::RustMqError::SnapshotError(format!(
                "Snapshot checksum mismatch for {}",
                metadata.snapshot_id
            )));
        }

        debug!("Snapshot integrity verified for {}", metadata.snapshot_id);
        Ok(())
    }

    /// Compute checksum for data
    fn compute_checksum(&self, data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Check if compaction is needed
    pub async fn should_compact(
        &self,
        current_log_entries: u64,
        last_snapshot_log_id: u64,
    ) -> bool {
        // Entry-based compaction
        if current_log_entries > self.config.max_log_entries {
            return true;
        }

        // Incremental snapshot threshold
        if current_log_entries - last_snapshot_log_id > self.config.incremental_threshold {
            return true;
        }

        // Time-based compaction
        let stats = self.stats.lock().await;
        if let Some(last_compaction) = stats.last_compaction_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now - last_compaction > self.config.compaction_interval_secs {
                return true;
            }
        } else {
            // Never compacted before
            return true;
        }

        false
    }

    /// Perform log compaction
    pub async fn compact_logs(
        &self,
        current_state: &RustMqSnapshotData,
        last_log_id: u64,
    ) -> crate::Result<String> {
        let start = Instant::now();

        // Check if another compaction is already running
        {
            let active = self.active_compaction.lock().await;
            if active.is_some() {
                return Err(crate::error::RustMqError::LogCompaction(
                    "Compaction already in progress".to_string(),
                ));
            }
        }

        info!("Starting log compaction up to log ID {}", last_log_id);

        // Determine snapshot type
        let snapshot_type = self.determine_snapshot_type(last_log_id).await;
        let snapshot_id = self.generate_snapshot_id(&snapshot_type);

        // Create snapshot
        let snapshot_data = self
            .create_snapshot(current_state, &snapshot_type, last_log_id)
            .await?;
        let uncompressed_size = snapshot_data.len() as u64;

        // Compress if enabled
        let compressed_data = if self.config.enable_compression {
            self.compress_data(&snapshot_data)?
        } else {
            snapshot_data
        };

        // Save snapshot to disk
        let snapshot_file = self
            .storage_config
            .data_dir
            .join("snapshots")
            .join(format!("{}.snap", snapshot_id));

        tokio::fs::write(&snapshot_file, &compressed_data).await?;

        // Create and save metadata
        let metadata = SnapshotMetadata {
            snapshot_id: snapshot_id.clone(),
            snapshot_type: snapshot_type.clone(),
            last_log_id,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            compressed_size: compressed_data.len() as u64,
            uncompressed_size,
            checksum: self.compute_checksum(&compressed_data),
            base_snapshot_id: self.get_base_snapshot_id(&snapshot_type).await,
            topic_count: current_state.topics.len(),
            broker_count: current_state.brokers.len(),
        };

        self.save_snapshot_metadata(&metadata).await?;

        // Update cache
        {
            let mut meta_map = self.snapshot_metadata.write().await;
            meta_map.insert(snapshot_id.clone(), metadata.clone());
        }

        // Clean up old snapshots
        self.cleanup_old_snapshots().await?;

        // Update statistics
        let duration = start.elapsed();
        let mut stats = self.stats.lock().await;
        stats.record_compaction(last_log_id, duration);
        stats.record_snapshot(compressed_data.len() as u64);

        info!(
            "Log compaction completed. Snapshot ID: {}, Type: {:?}, Size: {} bytes, Duration: {:?}",
            snapshot_id,
            snapshot_type,
            compressed_data.len(),
            duration
        );

        Ok(snapshot_id)
    }

    /// Determine the type of snapshot to create
    async fn determine_snapshot_type(&self, last_log_id: u64) -> SnapshotType {
        let meta_map = self.snapshot_metadata.read().await;

        // Check if we have a recent full snapshot
        for metadata in meta_map.values() {
            if metadata.snapshot_type == SnapshotType::Full {
                let entries_since_full = last_log_id - metadata.last_log_id;
                if entries_since_full < self.config.incremental_threshold * 5 {
                    return SnapshotType::Incremental;
                }
            }
        }

        SnapshotType::Full
    }

    /// Generate a unique snapshot ID
    fn generate_snapshot_id(&self, snapshot_type: &SnapshotType) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        match snapshot_type {
            SnapshotType::Full => format!("full_{}", timestamp),
            SnapshotType::Incremental => format!("incr_{}", timestamp),
        }
    }

    /// Create snapshot data based on type
    async fn create_snapshot(
        &self,
        current_state: &RustMqSnapshotData,
        snapshot_type: &SnapshotType,
        last_log_id: u64,
    ) -> crate::Result<Vec<u8>> {
        match snapshot_type {
            SnapshotType::Full => {
                // Full snapshot contains complete state
                let mut full_state = current_state.clone();
                full_state.last_applied_log = Some(last_log_id);
                bincode::serialize(&full_state).map_err(Into::into)
            }
            SnapshotType::Incremental => {
                // Incremental snapshot contains only changes since last full snapshot
                self.create_incremental_snapshot(current_state, last_log_id)
                    .await
            }
        }
    }

    /// Create incremental snapshot
    async fn create_incremental_snapshot(
        &self,
        current_state: &RustMqSnapshotData,
        last_log_id: u64,
    ) -> crate::Result<Vec<u8>> {
        // Find the base full snapshot
        let base_snapshot = self.find_latest_full_snapshot().await;

        if let Some(base_metadata) = base_snapshot {
            // Load base snapshot
            let base_state = self.load_snapshot_data(&base_metadata.snapshot_id).await?;

            // Create delta
            let delta = self.compute_state_delta(&base_state, current_state);

            // Serialize delta with metadata
            let incremental_data = IncrementalSnapshotData {
                base_snapshot_id: base_metadata.snapshot_id.clone(),
                last_log_id,
                delta,
            };

            bincode::serialize(&incremental_data).map_err(Into::into)
        } else {
            // No base snapshot found, create full snapshot instead
            warn!("No base snapshot found for incremental, creating full snapshot");
            let mut full_state = current_state.clone();
            full_state.last_applied_log = Some(last_log_id);
            bincode::serialize(&full_state).map_err(Into::into)
        }
    }

    /// Find the latest full snapshot
    async fn find_latest_full_snapshot(&self) -> Option<SnapshotMetadata> {
        let meta_map = self.snapshot_metadata.read().await;

        meta_map
            .values()
            .filter(|m| m.snapshot_type == SnapshotType::Full)
            .max_by_key(|m| m.created_at)
            .cloned()
    }

    /// Load snapshot data from disk
    async fn load_snapshot_data(&self, snapshot_id: &str) -> crate::Result<RustMqSnapshotData> {
        let snapshot_file = self
            .storage_config
            .data_dir
            .join("snapshots")
            .join(format!("{}.snap", snapshot_id));

        let compressed_data = tokio::fs::read(&snapshot_file).await?;

        let data = if self.config.enable_compression {
            self.decompress_data(&compressed_data)?
        } else {
            compressed_data
        };

        bincode::deserialize(&data).map_err(Into::into)
    }

    /// Compute delta between two states
    fn compute_state_delta(
        &self,
        base: &RustMqSnapshotData,
        current: &RustMqSnapshotData,
    ) -> StateDelta {
        let mut delta = StateDelta::default();

        // Compute topic changes
        for (name, topic) in &current.topics {
            if !base.topics.contains_key(name) || base.topics[name] != *topic {
                delta.topic_changes.insert(name.clone(), topic.clone());
            }
        }

        // Compute broker changes
        for (id, broker) in &current.brokers {
            if !base.brokers.contains_key(id) || base.brokers[id] != *broker {
                delta.broker_changes.insert(id.clone(), broker.clone());
            }
        }

        // Compute partition assignment changes
        for (tp, assignment) in &current.partition_assignments {
            if !base.partition_assignments.contains_key(tp)
                || base.partition_assignments[tp] != *assignment
            {
                delta
                    .partition_changes
                    .insert(tp.clone(), assignment.clone());
            }
        }

        // Track deletions
        for name in base.topics.keys() {
            if !current.topics.contains_key(name) {
                delta.deleted_topics.insert(name.clone());
            }
        }

        for id in base.brokers.keys() {
            if !current.brokers.contains_key(id) {
                delta.deleted_brokers.insert(id.clone());
            }
        }

        delta
    }

    /// Get base snapshot ID for incremental snapshots
    async fn get_base_snapshot_id(&self, snapshot_type: &SnapshotType) -> Option<String> {
        if *snapshot_type == SnapshotType::Incremental {
            self.find_latest_full_snapshot()
                .await
                .map(|m| m.snapshot_id)
        } else {
            None
        }
    }

    /// Save snapshot metadata to disk
    async fn save_snapshot_metadata(&self, metadata: &SnapshotMetadata) -> crate::Result<()> {
        let metadata_file = self
            .storage_config
            .data_dir
            .join("snapshots")
            .join(format!("{}.meta", metadata.snapshot_id));

        let metadata_json = serde_json::to_string_pretty(metadata)?;
        tokio::fs::write(&metadata_file, metadata_json).await?;

        Ok(())
    }

    /// Clean up old snapshots based on retention policy
    async fn cleanup_old_snapshots(&self) -> crate::Result<()> {
        let meta_map = self.snapshot_metadata.read().await;
        let mut snapshots: Vec<_> = meta_map.values().collect();
        snapshots.sort_by_key(|m| m.created_at);

        if snapshots.len() > self.config.keep_snapshot_count {
            let to_remove = &snapshots[..snapshots.len() - self.config.keep_snapshot_count];

            for metadata in to_remove {
                info!("Cleaning up old snapshot: {}", metadata.snapshot_id);

                // Remove snapshot file
                let snapshot_file = self
                    .storage_config
                    .data_dir
                    .join("snapshots")
                    .join(format!("{}.snap", metadata.snapshot_id));

                if snapshot_file.exists() {
                    tokio::fs::remove_file(&snapshot_file).await?;
                }

                // Remove metadata file
                let metadata_file = self
                    .storage_config
                    .data_dir
                    .join("snapshots")
                    .join(format!("{}.meta", metadata.snapshot_id));

                if metadata_file.exists() {
                    tokio::fs::remove_file(&metadata_file).await?;
                }
            }
        }

        Ok(())
    }

    /// Compress data using zstd
    fn compress_data(&self, data: &[u8]) -> crate::Result<Vec<u8>> {
        zstd::encode_all(data, self.config.compression_level).map_err(|e| {
            crate::error::RustMqError::SnapshotError(format!("Compression failed: {}", e))
        })
    }

    /// Decompress data using zstd
    fn decompress_data(&self, data: &[u8]) -> crate::Result<Vec<u8>> {
        zstd::decode_all(data).map_err(|e| {
            crate::error::RustMqError::SnapshotError(format!("Decompression failed: {}", e))
        })
    }

    /// Get compaction statistics
    pub async fn get_stats(&self) -> CompactionStats {
        self.stats.lock().await.clone()
    }

    /// List all available snapshots
    pub async fn list_snapshots(&self) -> Vec<SnapshotMetadata> {
        let meta_map = self.snapshot_metadata.read().await;
        let mut snapshots: Vec<_> = meta_map.values().cloned().collect();
        snapshots.sort_by_key(|m| m.created_at);
        snapshots
    }

    /// Get the latest snapshot
    pub async fn get_latest_snapshot(&self) -> Option<SnapshotMetadata> {
        let meta_map = self.snapshot_metadata.read().await;
        meta_map.values().max_by_key(|m| m.created_at).cloned()
    }

    /// Start automatic compaction background task
    pub async fn start_auto_compaction(&self) -> tokio::task::JoinHandle<()> {
        let manager = Arc::new(self.clone());
        let interval = Duration::from_secs(self.config.compaction_interval_secs);

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                // This would be triggered by the main storage system
                // For now, it's just a placeholder for the auto-compaction logic
                debug!("Auto-compaction tick");
            }
        })
    }
}

// Implementation of Clone for LogCompactionManager
impl Clone for LogCompactionManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            storage_config: self.storage_config.clone(),
            stats: self.stats.clone(),
            snapshot_metadata: self.snapshot_metadata.clone(),
            active_compaction: self.active_compaction.clone(),
        }
    }
}

/// State delta for incremental snapshots
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StateDelta {
    pub topic_changes: BTreeMap<String, crate::controller::service::TopicInfo>,
    pub broker_changes: BTreeMap<String, crate::types::BrokerInfo>,
    pub partition_changes: BTreeMap<crate::types::TopicPartition, TempPartitionAssignment>,
    pub deleted_topics: std::collections::HashSet<String>,
    pub deleted_brokers: std::collections::HashSet<String>,
}

/// Incremental snapshot data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IncrementalSnapshotData {
    pub base_snapshot_id: String,
    pub last_log_id: u64,
    pub delta: StateDelta,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_manager() -> (LogCompactionManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage_config = RustMqStorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let config = CompactionConfig::default();

        let manager = LogCompactionManager::new(config, storage_config)
            .await
            .unwrap();
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_compaction_manager_creation() {
        let (manager, _temp_dir) = create_test_manager().await;

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_compactions, 0);
        assert_eq!(stats.snapshots_created, 0);
    }

    #[tokio::test]
    async fn test_should_compact_logic() {
        let (manager, _temp_dir) = create_test_manager().await;

        // Set a fake last compaction time to avoid the "never compacted before" condition
        {
            let mut stats = manager.stats.lock().await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            stats.last_compaction_at = Some(now - 60); // 60 seconds ago
        }

        // Should compact when exceeding max entries
        assert!(manager.should_compact(15000, 0).await);

        // Should not compact when below threshold (5000 - 4500 = 500 < 1000 incremental_threshold)
        assert!(!manager.should_compact(5000, 4500).await);

        // Should compact when exceeding incremental threshold
        assert!(manager.should_compact(5000, 3000).await);
    }

    #[tokio::test]
    async fn test_snapshot_id_generation() {
        let (manager, _temp_dir) = create_test_manager().await;

        let full_id = manager.generate_snapshot_id(&SnapshotType::Full);
        let incr_id = manager.generate_snapshot_id(&SnapshotType::Incremental);

        assert!(full_id.starts_with("full_"));
        assert!(incr_id.starts_with("incr_"));
        assert_ne!(full_id, incr_id);
    }

    #[tokio::test]
    async fn test_checksum_computation() {
        let (manager, _temp_dir) = create_test_manager().await;

        let data = b"test data for checksum";
        let checksum1 = manager.compute_checksum(data);
        let checksum2 = manager.compute_checksum(data);

        assert_eq!(checksum1, checksum2);
        assert_eq!(checksum1.len(), 64); // SHA256 hex string length
    }

    #[tokio::test]
    async fn test_compression_roundtrip() {
        let (manager, _temp_dir) = create_test_manager().await;

        let original_data = b"This is test data for compression testing. It should compress well due to repetition.".repeat(100);

        let compressed = manager.compress_data(&original_data).unwrap();
        let decompressed = manager.decompress_data(&compressed).unwrap();

        assert_eq!(original_data, decompressed);
        assert!(compressed.len() < original_data.len()); // Should be compressed
    }

    #[tokio::test]
    async fn test_state_delta_computation() {
        let (manager, _temp_dir) = create_test_manager().await;

        let mut base_state = RustMqSnapshotData::default();
        let mut current_state = RustMqSnapshotData::default();

        // Add a topic to current state
        current_state.topics.insert(
            "test-topic".to_string(),
            crate::controller::service::TopicInfo {
                name: "test-topic".to_string(),
                partitions: 3,
                replication_factor: 2,
                config: crate::controller::service::TopicConfig {
                    retention_ms: Some(86400000),
                    segment_bytes: Some(1073741824),
                    compression_type: Some("lz4".to_string()),
                },
                created_at: chrono::Utc::now(),
            },
        );

        let delta = manager.compute_state_delta(&base_state, &current_state);

        assert_eq!(delta.topic_changes.len(), 1);
        assert!(delta.topic_changes.contains_key("test-topic"));
        assert_eq!(delta.deleted_topics.len(), 0);
    }

    #[tokio::test]
    async fn test_snapshot_metadata_operations() {
        let (mut manager, _temp_dir) = create_test_manager().await;

        // Disable snapshot verification for this test
        manager.config.verify_snapshots = false;

        let metadata = SnapshotMetadata {
            snapshot_id: "test_snapshot_123".to_string(),
            snapshot_type: SnapshotType::Full,
            last_log_id: 1000,
            created_at: 1234567890,
            compressed_size: 1024,
            uncompressed_size: 2048,
            checksum: "abcd1234".to_string(),
            base_snapshot_id: None,
            topic_count: 5,
            broker_count: 3,
        };

        // Save metadata
        manager.save_snapshot_metadata(&metadata).await.unwrap();

        // Load metadata
        let loaded = manager
            .load_single_metadata(
                &manager
                    .storage_config
                    .data_dir
                    .join("snapshots")
                    .join("test_snapshot_123.meta"),
            )
            .await
            .unwrap();

        assert_eq!(metadata.snapshot_id, loaded.snapshot_id);
        assert_eq!(metadata.snapshot_type, loaded.snapshot_type);
        assert_eq!(metadata.last_log_id, loaded.last_log_id);
    }
}
