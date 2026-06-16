//! Object compaction planner and executor for cold storage.
//!
//! Merges small adjacent cold segments into larger ones to reduce request costs,
//! metadata size, and read latency, while maintaining strict contiguity.

use crate::Result;
use crate::config::CompactionConfig;
use crate::storage::cold_index::{ColdIndexManifest, ColdSegment};
use crate::storage::partition_index::PartitionIndex;
use crate::storage::traits::{UploadManager, WalSegment};
use crate::types::{Offset, TopicPartition, WalRecord};
use bytes::Bytes;
use std::sync::Arc;
use tracing::{error, info, warn};

/// A planned compaction run for a partition, specifying the source segments to merge.
#[derive(Debug, Clone)]
pub struct CompactionPlan {
    pub topic_partition: TopicPartition,
    pub sources: Vec<ColdSegment>,
}

/// Orchestrates planning and executing compaction of cold storage objects.
pub struct Compactor {
    index: Arc<PartitionIndex>,
    upload_manager: Arc<dyn UploadManager>,
    cold_index: Arc<ColdIndexManifest>,
    config: CompactionConfig,
}

impl Compactor {
    pub fn new(
        index: Arc<PartitionIndex>,
        upload_manager: Arc<dyn UploadManager>,
        cold_index: Arc<ColdIndexManifest>,
        config: CompactionConfig,
    ) -> Self {
        Self {
            index,
            upload_manager,
            cold_index,
            config,
        }
    }

    /// Run one cycle of compaction across all partitions.
    pub async fn run_compaction_cycle(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let plans = plan_compaction(&self.index, &self.config);
        for plan in plans {
            if let Err(e) = self.execute_plan(plan).await {
                error!("Failed to execute compaction plan: {:?}", e);
            }
        }
        Ok(())
    }

    async fn execute_plan(&self, plan: CompactionPlan) -> Result<()> {
        let tp = &plan.topic_partition;
        let sources = &plan.sources;
        if sources.len() < 2 {
            return Ok(()); // Sanity check
        }

        info!(
            "Starting compaction for {:?}: merging {} segments",
            tp,
            sources.len()
        );

        let start_offset = sources.first().unwrap().start_offset;
        let end_offset = sources.last().unwrap().end_offset;

        // 1. Download and merge records
        let mut merged_records = Vec::new();
        let mut expected_next_offset = start_offset;

        for source in sources {
            let wal_seg = self
                .upload_manager
                .download_segment(&source.object_key)
                .await?;
            let mut recs: Vec<WalRecord> = bincode::deserialize(&wal_seg.data).map_err(|e| {
                crate::error::RustMqError::Storage(format!(
                    "Failed to deserialize segment records: {}",
                    e
                ))
            })?;

            // Verify contiguity of records
            if !recs.is_empty() {
                let first_offset = recs.first().unwrap().offset;
                if first_offset != expected_next_offset {
                    return Err(crate::error::RustMqError::Storage(format!(
                        "Gaps detected during compaction for {:?}: expected start offset {}, got {}",
                        tp, expected_next_offset, first_offset
                    )));
                }
                expected_next_offset = recs.last().unwrap().offset + 1;
            }
            merged_records.append(&mut recs);
        }

        if expected_next_offset != end_offset {
            return Err(crate::error::RustMqError::Storage(format!(
                "End offset mismatch after merge for {:?}: expected {}, got {}",
                tp, end_offset, expected_next_offset
            )));
        }

        // 2. Serialize and Upload
        let merged_data = bincode::serialize(&merged_records)?;
        let size_bytes = merged_data.len() as u64;
        let merged_segment = WalSegment {
            start_offset,
            end_offset,
            size_bytes,
            data: Bytes::from(merged_data),
            topic_partition: tp.clone(),
        };

        let merged_key = self.upload_manager.upload_segment(merged_segment).await?;

        // 3. Record in manifest (Durable)
        let merged_seg = ColdSegment {
            topic_partition: tp.clone(),
            start_offset,
            end_offset,
            object_key: merged_key.clone(),
            size_bytes,
        };
        let source_keys: Vec<String> = sources.iter().map(|s| s.object_key.clone()).collect();

        self.cold_index
            .record_compaction(merged_seg, source_keys.clone())
            .await?;

        // 4. Update live index
        self.index
            .replace_cold(tp, start_offset, end_offset, merged_key, size_bytes);

        // 5. Delete source objects (Best effort)
        for key in source_keys {
            if let Err(e) = self.upload_manager.delete_object(&key).await {
                warn!("Failed to delete compacted source object {}: {:?}", key, e);
            }
        }

        info!(
            "Successfully compacted {:?}: range [{}, {}) merged",
            tp, start_offset, end_offset
        );

        Ok(())
    }
}

/// Plan compaction for all partitions in the index.
pub fn plan_compaction(index: &PartitionIndex, config: &CompactionConfig) -> Vec<CompactionPlan> {
    let mut plans = Vec::new();
    let partitions = index.get_partitions();
    for tp in partitions {
        let segments = index.get_cold_segments(&tp);
        let mut partition_plans = plan_partition_compaction(&tp, segments, config);
        plans.append(&mut partition_plans);
    }
    plans
}

fn plan_partition_compaction(
    tp: &TopicPartition,
    segments: Vec<ColdSegment>,
    config: &CompactionConfig,
) -> Vec<CompactionPlan> {
    if segments.is_empty() {
        return vec![];
    }

    let mut plans = Vec::new();
    let mut current_run = Vec::new();
    let mut current_run_size = 0u64;

    for seg in segments {
        let is_small = seg.size_bytes < config.small_threshold_bytes;
        let is_contiguous = current_run.last().map_or(true, |last: &ColdSegment| {
            last.end_offset == seg.start_offset
        });

        if !is_small || !is_contiguous {
            if current_run.len() >= 2 {
                plans.push(CompactionPlan {
                    topic_partition: tp.clone(),
                    sources: std::mem::take(&mut current_run),
                });
            }
            current_run = Vec::new();
            current_run_size = 0;
            if is_small {
                current_run.push(seg.clone());
                current_run_size = seg.size_bytes;
            }
            continue;
        }

        if current_run_size + seg.size_bytes > config.target_bytes && current_run.len() >= 2 {
            plans.push(CompactionPlan {
                topic_partition: tp.clone(),
                sources: std::mem::take(&mut current_run),
            });
            current_run = Vec::new();
            current_run.push(seg.clone());
            current_run_size = seg.size_bytes;
            continue;
        }

        current_run_size += seg.size_bytes;
        current_run.push(seg);

        if current_run.len() >= config.max_sources {
            plans.push(CompactionPlan {
                topic_partition: tp.clone(),
                sources: std::mem::take(&mut current_run),
            });
            current_run_size = 0;
        }
    }

    if current_run.len() >= 2 {
        plans.push(CompactionPlan {
            topic_partition: tp.clone(),
            sources: current_run,
        });
    }

    plans
}
