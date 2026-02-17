pub mod raft_operations; // Raft operations abstraction for ACL (breaks circular dependency)
pub mod service;

// Production-ready OpenRaft implementations for RustMQ - fully compatible with OpenRaft 0.9.21
pub mod openraft_compaction; // Log compaction and snapshot management
pub mod openraft_manager; // Complete cluster management with consensus operations
pub mod openraft_network; // gRPC-based networking with connection pooling and retries
pub mod openraft_performance; // Performance optimizations and caching
pub mod openraft_storage; // Production storage with WAL, crash recovery, and high-throughput

#[cfg(test)]
pub mod tests;

pub use raft_operations::ControllerRaftOperations;
pub use service::*;

pub use openraft_storage::{
    NodeId, PartitionAssignment, RustMqAppData, RustMqAppDataResponse, RustMqLogStorage,
    RustMqNode, RustMqResponder, RustMqSnapshotBuilder, RustMqSnapshotData, RustMqStateMachine,
    RustMqStorageConfig, RustMqTypeConfig,
};

pub use openraft_manager::{
    CompactionMetrics, HealthCheckResult, ManagerState, RaftManager, RaftManagerConfig,
    RaftManagerMetrics, StorageMetrics,
};

pub use openraft_network::{
    ConnectionPoolStats, NetworkMetrics, RustMqNetwork, RustMqNetworkConfig, RustMqNetworkFactory,
};

pub use openraft_compaction::{
    CompactionConfig, CompactionStats, LogCompactionManager, SnapshotMetadata, SnapshotType,
};

pub use openraft_performance::{
    HighPerformanceCache, PerformanceConfig, PerformanceMetrics, PerformanceSnapshot,
    RaftPerformanceOptimizer, ZeroCopyBuffer,
};
