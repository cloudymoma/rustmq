use crate::consumer_group::coordinator::ConsumerGroupState;
use crate::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs::{File, create_dir_all, read_dir};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct SnapshotManager {
    base_dir: PathBuf,
}

impl SnapshotManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Generates a snapshot file for a given partition
    pub async fn save_snapshot(
        &self,
        partition: u32,
        offset: u64,
        state: &HashMap<String, ConsumerGroupState>,
    ) -> Result<()> {
        let partition_dir = self
            .base_dir
            .join("__consumer_offsets")
            .join(partition.to_string());
        create_dir_all(&partition_dir).await?;

        let snapshot_path = partition_dir.join(format!("snapshot-{}.bin", offset));
        let data = bincode::serialize(state)?;

        // Write to temporary file first, then rename for atomic creation
        let tmp_path = snapshot_path.with_extension("tmp");
        let mut file = File::create(&tmp_path).await?;
        file.write_all(&data).await?;
        file.sync_all().await?;

        tokio::fs::rename(tmp_path, snapshot_path).await?;

        Ok(())
    }

    /// Loads the latest snapshot for a partition, returning the state and the offset it was taken at
    pub async fn load_latest_snapshot(
        &self,
        partition: u32,
    ) -> Result<Option<(u64, HashMap<String, ConsumerGroupState>)>> {
        let partition_dir = self
            .base_dir
            .join("__consumer_offsets")
            .join(partition.to_string());
        if !partition_dir.exists() {
            return Ok(None);
        }

        let mut entries = read_dir(&partition_dir).await?;
        let mut latest_offset = 0;
        let mut latest_file = None;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("snapshot-") && name.ends_with(".bin") {
                        let offset_str = &name["snapshot-".len()..name.len() - 4];
                        if let Ok(offset) = offset_str.parse::<u64>() {
                            if offset >= latest_offset {
                                latest_offset = offset;
                                latest_file = Some(path);
                            }
                        }
                    }
                }
            }
        }

        if let Some(path) = latest_file {
            let mut file = File::open(&path).await?;
            let mut data = Vec::new();
            file.read_to_end(&mut data).await?;
            let state: HashMap<String, ConsumerGroupState> = bincode::deserialize(&data)?;
            return Ok(Some((latest_offset, state)));
        }

        Ok(None)
    }

    pub async fn cleanup_old_snapshots(&self, partition_count: u32, keep: usize) {
        for partition in 0..partition_count {
            let partition_dir = self
                .base_dir
                .join("__consumer_offsets")
                .join(partition.to_string());
            if !partition_dir.exists() {
                continue;
            }

            let mut entries = match read_dir(&partition_dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut snapshots: Vec<(u64, std::path::PathBuf)> = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("snapshot-") && name.ends_with(".bin") {
                        let offset_str = &name["snapshot-".len()..name.len() - 4];
                        if let Ok(offset) = offset_str.parse::<u64>() {
                            snapshots.push((offset, path));
                        }
                    }
                }
            }

            if snapshots.len() <= keep {
                continue;
            }

            snapshots.sort_by(|a, b| b.0.cmp(&a.0));
            for (_, path) in snapshots.into_iter().skip(keep) {
                let _ = tokio::fs::remove_file(path).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_snapshot_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SnapshotManager::new(temp_dir.path());
        let mut state = HashMap::new();
        state.insert(
            "g".into(),
            ConsumerGroupState {
                group_id: "g".into(),
                generation_id: 1,
                members: HashMap::new(),
                state: crate::consumer_group::coordinator::GroupState::Stable,
                assignment: HashMap::new(),
                committed_offsets: HashMap::new(),
            },
        );

        manager.save_snapshot(0, 42, &state).await.unwrap();
        let (offset, loaded) = manager.load_latest_snapshot(0).await.unwrap().unwrap();
        assert_eq!(offset, 42);
        assert_eq!(loaded.get("g").unwrap().group_id, "g");
    }

    #[tokio::test]
    async fn test_snapshot_loads_latest() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SnapshotManager::new(temp_dir.path());
        let state = HashMap::new();
        manager.save_snapshot(0, 10, &state).await.unwrap();
        manager.save_snapshot(0, 42, &state).await.unwrap();
        manager.save_snapshot(0, 5, &state).await.unwrap(); // Out of order still works

        let (offset, _) = manager.load_latest_snapshot(0).await.unwrap().unwrap();
        assert_eq!(offset, 42);
    }
}
