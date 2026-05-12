use crate::error::Result;
use crate::storage::traits::WriteAheadLog;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupState {
    Empty,
    Stable,
    PreparingRebalance,
    CompletingRebalance,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberState {
    pub member_id: String,
    pub subscribed_topics: Vec<String>,
    #[serde(skip)]
    pub last_heartbeat: Option<Instant>,
    pub session_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroupState {
    pub group_id: String,
    pub generation_id: u64,
    pub members: HashMap<String, MemberState>,
    pub state: GroupState,
    pub assignment: HashMap<String, Vec<TopicPartition>>,
    pub committed_offsets: HashMap<TopicPartition, Offset>,
}

pub struct GroupCoordinatorManager {
    groups: Arc<RwLock<HashMap<String, ConsumerGroupState>>>,
    owned_partitions: Arc<RwLock<HashSet<u32>>>,
    wal: Arc<dyn WriteAheadLog>,
    offsets_partition_count: u32,
    topic_partitions: Arc<RwLock<HashMap<String, u32>>>,
    rebalance_count: Arc<AtomicU64>,
    commit_count: Arc<AtomicU64>,
}

impl GroupCoordinatorManager {
    pub fn new(wal: Arc<dyn WriteAheadLog>, offsets_partition_count: u32) -> Self {
        Self {
            groups: Arc::new(RwLock::new(HashMap::new())),
            owned_partitions: Arc::new(RwLock::new(HashSet::new())),
            wal,
            offsets_partition_count,
            topic_partitions: Arc::new(RwLock::new(HashMap::new())),
            rebalance_count: Arc::new(AtomicU64::new(0)),
            commit_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn recover_from_snapshots(&self, snapshot_dir: &std::path::Path) {
        let snapshot_mgr = crate::consumer_group::snapshot::SnapshotManager::new(snapshot_dir);
        let mut recovered = 0usize;
        for partition in 0..self.offsets_partition_count {
            match snapshot_mgr.load_latest_snapshot(partition).await {
                Ok(Some((_offset, state))) => {
                    let mut groups = self.groups.write().await;
                    for (group_id, group_state) in state {
                        tracing::info!("Recovered consumer group '{}' from snapshot (partition {})", group_id, partition);
                        groups.insert(group_id, group_state);
                        recovered += 1;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Failed to load snapshot for partition {}: {}", partition, e);
                }
            }
        }
        if recovered > 0 {
            tracing::info!("Recovered {} consumer groups from snapshots", recovered);
        }
    }

    pub async fn update_topic_partitions(&self, topic: &str, partition_count: u32) {
        let mut tp = self.topic_partitions.write().await;
        tp.insert(topic.to_string(), partition_count);
    }

    pub fn start_snapshot_task(&self, snapshot_dir: std::path::PathBuf) {
        let groups = self.groups.clone();
        let commit_count = self.commit_count.clone();
        let partition_count = self.offsets_partition_count;

        tokio::spawn(async move {
            let snapshot_mgr = crate::consumer_group::snapshot::SnapshotManager::new(&snapshot_dir);
            let mut last_snapshot_commit = 0u64;
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;
                let current_commits = commit_count.load(Ordering::Relaxed);
                let commits_since = current_commits.saturating_sub(last_snapshot_commit);

                if commits_since < 10_000 {
                    continue;
                }

                let all_groups = groups.read().await;
                if all_groups.is_empty() {
                    continue;
                }

                for partition in 0..partition_count {
                    let partition_groups: HashMap<String, ConsumerGroupState> = all_groups
                        .iter()
                        .filter(|(group_id, _)| {
                            use std::hash::{Hash, Hasher};
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            group_id.hash(&mut hasher);
                            (hasher.finish() % partition_count as u64) as u32 == partition
                        })
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();

                    if !partition_groups.is_empty() {
                        if let Err(e) = snapshot_mgr
                            .save_snapshot(partition, current_commits, &partition_groups)
                            .await
                        {
                            tracing::error!("Failed to save snapshot for partition {}: {}", partition, e);
                        }
                    }
                }
                drop(all_groups);

                snapshot_mgr.cleanup_old_snapshots(partition_count, 3).await;
                last_snapshot_commit = current_commits;
                tracing::info!("Consumer group snapshot saved at commit count {}", current_commits);
            }
        });
    }

    pub fn start_reaper_task(&self) {
        let groups = self.groups.clone();
        let rebalance_count = self.rebalance_count.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let mut groups = groups.write().await;
                for state in groups.values_mut() {
                    if state.state == GroupState::Dead || state.state == GroupState::Empty {
                        continue;
                    }
                    let expired: Vec<String> = state
                        .members
                        .iter()
                        .filter(|(_, m)| {
                            m.last_heartbeat
                                .map(|hb| hb.elapsed() > Duration::from_millis(m.session_timeout_ms))
                                .unwrap_or(true)
                        })
                        .map(|(id, _)| id.clone())
                        .collect();

                    for member_id in &expired {
                        tracing::warn!("Evicting stale member {} from group {}", member_id, state.group_id);
                        state.members.remove(member_id);
                    }

                    if !expired.is_empty() {
                        if state.members.is_empty() {
                            state.state = GroupState::Empty;
                        } else {
                            state.state = GroupState::PreparingRebalance;
                            state.generation_id += 1;
                            state.assignment.clear();
                            rebalance_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        });
    }

    pub async fn handle_join_group(&self, request: JoinGroupRequest) -> Result<JoinGroupResponse> {
        let mut groups = self.groups.write().await;

        let state = groups
            .entry(request.group_id.clone())
            .or_insert_with(|| ConsumerGroupState {
                group_id: request.group_id.clone(),
                generation_id: 0,
                members: HashMap::new(),
                state: GroupState::Empty,
                assignment: HashMap::new(),
                committed_offsets: HashMap::new(),
            });

        if state.state == GroupState::Empty || state.state == GroupState::Stable {
            state.state = GroupState::PreparingRebalance;
            state.generation_id += 1;
            state.assignment.clear();
            self.rebalance_count.fetch_add(1, Ordering::Relaxed);
        }

        let member_id = if request.member_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            request.member_id.clone()
        };

        state.members.insert(
            member_id.clone(),
            MemberState {
                member_id: member_id.clone(),
                subscribed_topics: request.topics.clone(),
                last_heartbeat: Some(Instant::now()),
                session_timeout_ms: request.session_timeout_ms as u64,
            },
        );

        // Compute partition assignments for all current members (eager rebalance)
        let all_topics: HashSet<String> = state
            .members
            .values()
            .flat_map(|m| m.subscribed_topics.iter().cloned())
            .collect();

        let topic_partitions = self.topic_partitions.read().await;
        let mut all_partitions = Vec::new();
        for topic in &all_topics {
            let count = topic_partitions.get(topic).copied().unwrap_or(1);
            for p in 0..count {
                all_partitions.push(TopicPartition { topic: topic.clone(), partition: p });
            }
        }
        drop(topic_partitions);

        let member_ids: Vec<String> = state.members.keys().cloned().collect();
        let mut assignments: HashMap<String, Vec<TopicPartition>> = HashMap::new();
        for (i, tp) in all_partitions.iter().enumerate() {
            let member = &member_ids[i % member_ids.len()];
            assignments.entry(member.clone()).or_default().push(tp.clone());
        }

        state.assignment = assignments.clone();
        state.state = GroupState::Stable;

        Ok(JoinGroupResponse {
            generation_id: state.generation_id,
            member_id,
            leader_id: member_ids.first().cloned().unwrap_or_default(),
            assigned_partitions: assignments,
            error_code: 0,
        })
    }

    pub async fn handle_heartbeat(&self, request: ConsumerHeartbeatRequest) -> Result<ConsumerHeartbeatResponse> {
        let mut groups = self.groups.write().await;

        if let Some(state) = groups.get_mut(&request.group_id) {
            if state.state == GroupState::PreparingRebalance {
                return Ok(ConsumerHeartbeatResponse { error_code: 27 });
            }
            if state.generation_id != request.generation_id {
                return Ok(ConsumerHeartbeatResponse { error_code: 22 });
            }
            if let Some(member) = state.members.get_mut(&request.member_id) {
                member.last_heartbeat = Some(Instant::now());
                return Ok(ConsumerHeartbeatResponse { error_code: 0 });
            }
            return Ok(ConsumerHeartbeatResponse { error_code: 25 });
        }

        Ok(ConsumerHeartbeatResponse { error_code: 16 })
    }

    pub async fn handle_leave_group(&self, request: LeaveGroupRequest) -> Result<LeaveGroupResponse> {
        let mut groups = self.groups.write().await;

        if let Some(state) = groups.get_mut(&request.group_id) {
            state.members.remove(&request.member_id);
            if state.members.is_empty() {
                state.state = GroupState::Empty;
            } else {
                state.state = GroupState::PreparingRebalance;
                state.generation_id += 1;
                state.assignment.clear();
                self.rebalance_count.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(LeaveGroupResponse { error_code: 0 });
        }

        Ok(LeaveGroupResponse { error_code: 16 })
    }

    pub async fn handle_commit_offset(&self, request: CommitOffsetRequest) -> Result<CommitOffsetResponse> {
        let mut groups = self.groups.write().await;

        if let Some(state) = groups.get_mut(&request.group_id) {
            if state.generation_id != request.generation_id {
                return Ok(CommitOffsetResponse { error_code: 22 });
            }
            let partition = self.partition_for(&request.group_id);
            for (tp, offset) in &request.offsets {
                let key = format!("offset:{}:{}:{}", request.group_id, tp.topic, tp.partition);
                let value = bincode::serialize(offset).unwrap_or_default();
                let serialized = bincode::serialize(&value).unwrap_or_default();
                let crc = crc32fast::hash(&serialized);
                let record = WalRecord {
                    topic_partition: TopicPartition {
                        topic: "__consumer_offsets".to_string(),
                        partition,
                    },
                    offset: 0,
                    record: Record {
                        key: Some(bytes::Bytes::from(key.into_bytes())),
                        value: bytes::Bytes::from(value),
                        headers: smallvec::SmallVec::new(),
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    },
                    crc32: crc,
                };
                if let Err(e) = self.wal.append(&record).await {
                    tracing::error!("Failed to persist offset to WAL: {}", e);
                    return Ok(CommitOffsetResponse { error_code: 1 });
                }
                state.committed_offsets.insert(tp.clone(), *offset);
            }
            self.commit_count.fetch_add(request.offsets.len() as u64, Ordering::Relaxed);
            return Ok(CommitOffsetResponse { error_code: 0 });
        }

        Ok(CommitOffsetResponse { error_code: 16 })
    }

    pub async fn handle_fetch_offset(&self, request: FetchOffsetRequest) -> Result<FetchOffsetResponse> {
        let groups = self.groups.read().await;

        if let Some(state) = groups.get(&request.group_id) {
            let mut result = HashMap::new();
            for tp in request.topic_partitions {
                if let Some(&offset) = state.committed_offsets.get(&tp) {
                    result.insert(tp, offset);
                }
            }
            return Ok(FetchOffsetResponse { offsets: result, error_code: 0 });
        }

        Ok(FetchOffsetResponse { offsets: HashMap::new(), error_code: 16 })
    }

    pub async fn get_metrics(&self) -> (Vec<(String, TopicPartition, u64)>, Vec<(String, usize)>, Vec<(String, u64)>) {
        let groups = self.groups.read().await;
        let mut committed_offsets = Vec::new();
        let mut group_members = Vec::new();
        let mut group_generations = Vec::new();

        for (group_id, state) in groups.iter() {
            group_members.push((group_id.clone(), state.members.len()));
            group_generations.push((group_id.clone(), state.generation_id));
            for (tp, &offset) in state.committed_offsets.iter() {
                committed_offsets.push((group_id.clone(), tp.clone(), offset));
            }
        }

        (committed_offsets, group_members, group_generations)
    }

    pub fn get_rebalance_count(&self) -> u64 {
        self.rebalance_count.load(Ordering::Relaxed)
    }

    pub fn partition_for(&self, group_id: &str) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        group_id.hash(&mut hasher);
        (hasher.finish() % self.offsets_partition_count as u64) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockWal {
        records: tokio::sync::Mutex<Vec<WalRecord>>,
    }

    impl MockWal {
        fn new() -> Self { Self { records: tokio::sync::Mutex::new(Vec::new()) } }
    }

    #[async_trait]
    impl WriteAheadLog for MockWal {
        async fn append(&self, record: &WalRecord) -> crate::error::Result<u64> {
            let mut records = self.records.lock().await;
            records.push(record.clone());
            Ok(records.len() as u64)
        }
        async fn read(&self, _offset: u64, _max_bytes: usize) -> crate::error::Result<Vec<WalRecord>> { Ok(vec![]) }
        async fn read_range(&self, _start: u64, _end: u64) -> crate::error::Result<Vec<WalRecord>> { Ok(vec![]) }
        async fn sync(&self) -> crate::error::Result<()> { Ok(()) }
        async fn truncate(&self, _offset: u64) -> crate::error::Result<()> { Ok(()) }
        async fn get_end_offset(&self) -> crate::error::Result<u64> { Ok(0) }
        fn register_upload_callback(&self, _cb: Box<dyn Fn(u64, u64) + Send + Sync>) {}
    }

    fn make_coordinator() -> (GroupCoordinatorManager, Arc<MockWal>) {
        let wal = Arc::new(MockWal::new());
        let coord = GroupCoordinatorManager::new(wal.clone(), 50);
        (coord, wal)
    }

    #[tokio::test]
    async fn test_join_group_creates_and_assigns() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 3).await;

        let resp = coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        assert_eq!(resp.error_code, 0);
        assert_eq!(resp.generation_id, 1);
        assert!(!resp.member_id.is_empty());
        let parts = resp.assigned_partitions.get(&resp.member_id).unwrap();
        assert_eq!(parts.len(), 3);
    }

    #[tokio::test]
    async fn test_two_members_split_partitions() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 4).await;

        coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m1".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        let r2 = coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m2".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        let m1 = r2.assigned_partitions.get("m1").map(|v| v.len()).unwrap_or(0);
        let m2 = r2.assigned_partitions.get("m2").map(|v| v.len()).unwrap_or(0);
        assert_eq!(m1 + m2, 4);
    }

    #[tokio::test]
    async fn test_heartbeat_rebalance_in_progress() {
        let (coord, _wal) = make_coordinator();
        {
            let mut groups = coord.groups.write().await;
            groups.insert("g".into(), ConsumerGroupState {
                group_id: "g".into(), generation_id: 1,
                members: HashMap::new(), state: GroupState::PreparingRebalance,
                assignment: HashMap::new(), committed_offsets: HashMap::new(),
            });
        }
        let resp = coord.handle_heartbeat(ConsumerHeartbeatRequest {
            group_id: "g".into(), member_id: "m".into(), generation_id: 1,
        }).await.unwrap();
        assert_eq!(resp.error_code, 27);
    }

    #[tokio::test]
    async fn test_heartbeat_illegal_generation() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 1).await;
        let resp = coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        let hb = coord.handle_heartbeat(ConsumerHeartbeatRequest {
            group_id: "g".into(), member_id: "m".into(), generation_id: resp.generation_id + 99,
        }).await.unwrap();
        assert_eq!(hb.error_code, 22);
    }

    #[tokio::test]
    async fn test_heartbeat_unknown_member() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 1).await;
        coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m1".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        let hb = coord.handle_heartbeat(ConsumerHeartbeatRequest {
            group_id: "g".into(), member_id: "nonexistent".into(), generation_id: 1,
        }).await.unwrap();
        assert_eq!(hb.error_code, 25);
    }

    #[tokio::test]
    async fn test_heartbeat_unknown_group() {
        let (coord, _wal) = make_coordinator();
        let hb = coord.handle_heartbeat(ConsumerHeartbeatRequest {
            group_id: "nonexistent".into(), member_id: "m".into(), generation_id: 1,
        }).await.unwrap();
        assert_eq!(hb.error_code, 16);
    }

    #[tokio::test]
    async fn test_leave_triggers_rebalance() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 2).await;
        coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m1".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();
        let r2 = coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m2".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        coord.handle_leave_group(LeaveGroupRequest {
            group_id: "g".into(), member_id: "m1".into(),
        }).await.unwrap();

        let groups = coord.groups.read().await;
        let state = groups.get("g").unwrap();
        assert_eq!(state.state, GroupState::PreparingRebalance);
        assert!(state.generation_id > r2.generation_id);
    }

    #[tokio::test]
    async fn test_leave_last_member_empties() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 1).await;
        coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();
        coord.handle_leave_group(LeaveGroupRequest {
            group_id: "g".into(), member_id: "m".into(),
        }).await.unwrap();

        let groups = coord.groups.read().await;
        assert_eq!(groups.get("g").unwrap().state, GroupState::Empty);
    }

    #[tokio::test]
    async fn test_commit_and_fetch_offset() {
        let (coord, wal) = make_coordinator();
        coord.update_topic_partitions("t", 1).await;
        let resp = coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        let tp = TopicPartition { topic: "t".into(), partition: 0 };
        let mut offsets = HashMap::new();
        offsets.insert(tp.clone(), 42);

        let c = coord.handle_commit_offset(CommitOffsetRequest {
            group_id: "g".into(), member_id: "m".into(),
            generation_id: resp.generation_id, offsets,
        }).await.unwrap();
        assert_eq!(c.error_code, 0);

        let f = coord.handle_fetch_offset(FetchOffsetRequest {
            group_id: "g".into(), topic_partitions: vec![tp.clone()],
        }).await.unwrap();
        assert_eq!(*f.offsets.get(&tp).unwrap(), 42);

        let records = wal.records.lock().await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].topic_partition.topic, "__consumer_offsets");
        let key = std::str::from_utf8(records[0].record.key.as_ref().unwrap()).unwrap();
        assert!(key.starts_with("offset:g:t:0"));
    }

    #[tokio::test]
    async fn test_partition_for_deterministic() {
        let (coord, _wal) = make_coordinator();
        assert_eq!(coord.partition_for("g"), coord.partition_for("g"));
        assert!(coord.partition_for("g") < 50);
    }

    #[tokio::test]
    async fn test_rebalance_count() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 1).await;
        assert_eq!(coord.get_rebalance_count(), 0);
        coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();
        assert_eq!(coord.get_rebalance_count(), 1);
    }

    #[tokio::test]
    async fn test_snapshot_recovery() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let snapshot_dir = temp_dir.path();

        let snapshot_mgr = crate::consumer_group::snapshot::SnapshotManager::new(snapshot_dir);
        let mut state = HashMap::new();
        let mut committed = HashMap::new();
        committed.insert(TopicPartition { topic: "t".into(), partition: 0 }, 100u64);
        state.insert("recovered-group".to_string(), ConsumerGroupState {
            group_id: "recovered-group".into(),
            generation_id: 5,
            members: HashMap::new(),
            state: GroupState::Stable,
            assignment: HashMap::new(),
            committed_offsets: committed,
        });
        snapshot_mgr.save_snapshot(0, 999, &state).await.unwrap();

        let (coord, _wal) = make_coordinator();
        coord.recover_from_snapshots(snapshot_dir).await;

        let f = coord.handle_fetch_offset(FetchOffsetRequest {
            group_id: "recovered-group".into(),
            topic_partitions: vec![TopicPartition { topic: "t".into(), partition: 0 }],
        }).await.unwrap();
        assert_eq!(*f.offsets.get(&TopicPartition { topic: "t".into(), partition: 0 }).unwrap(), 100);
    }

    #[tokio::test]
    async fn test_commit_increments_commit_count() {
        let (coord, _wal) = make_coordinator();
        coord.update_topic_partitions("t", 1).await;
        let resp = coord.handle_join_group(JoinGroupRequest {
            group_id: "g".into(), member_id: "m".into(),
            session_timeout_ms: 30000, topics: vec!["t".into()],
        }).await.unwrap();

        let mut offsets = HashMap::new();
        offsets.insert(TopicPartition { topic: "t".into(), partition: 0 }, 10);
        offsets.insert(TopicPartition { topic: "t".into(), partition: 1 }, 20);

        coord.handle_commit_offset(CommitOffsetRequest {
            group_id: "g".into(), member_id: "m".into(),
            generation_id: resp.generation_id, offsets,
        }).await.unwrap();

        assert_eq!(coord.commit_count.load(Ordering::Relaxed), 2);
    }
}
