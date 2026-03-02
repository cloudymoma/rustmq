use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConfig {
    pub endpoints: Vec<String>,
    pub election_timeout_ms: u64,
    pub heartbeat_interval_ms: u64,
    /// Bind address for Raft cluster communication
    pub bind_addr: String,
    /// RPC port for cluster communication
    pub rpc_port: u16,
    /// Admin API port
    pub admin_port: u16,
    /// OpenRaft-specific configuration
    pub raft: RaftConfig,
}

/// OpenRaft-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// Maximum number of log entries per AppendEntries request
    pub max_payload_entries: u64,
    /// Enable or disable log compaction
    pub enable_tick: bool,
    /// Enable heartbeat
    pub enable_heartbeat: bool,
    /// Timeout for sending heartbeat messages
    pub heartbeat_timeout_ms: u64,
    /// Timeout for installing a snapshot
    pub install_snapshot_timeout_ms: u64,
    /// Maximum lag allowed for replication
    pub max_replication_lag: u64,
    /// Enable or disable response timeout
    pub enable_elect: bool,
    /// Snapshot policy configuration
    pub snapshot_policy: SnapshotPolicy,
    /// Maximum size of uncommitted logs before forcing a snapshot
    pub max_uncommitted_entries: u64,
    /// Network timeout for Raft operations
    pub network_timeout_ms: u64,
}

/// Snapshot policy for OpenRaft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPolicy {
    /// Number of log entries to trigger snapshot creation
    pub log_entries_since_last: u64,
    /// Enable periodic snapshots
    pub enable_periodic: bool,
    /// Interval for periodic snapshots in seconds
    pub periodic_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub min_in_sync_replicas: usize,
    pub ack_timeout_ms: u64,
    pub max_replication_lag: u64,
    /// Heartbeat timeout in milliseconds - how long to wait before considering a follower unresponsive
    pub heartbeat_timeout_ms: u64,
}
