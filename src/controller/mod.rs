pub mod openraft_integration;
pub mod openraft_simple_working; // Legacy simplified implementation (kept for reference)
pub mod raft_operations;
pub mod service; // Raft operations abstraction for ACL (breaks circular dependency)

// Production-ready OpenRaft implementations for RustMQ - fully compatible with OpenRaft 0.9.21
pub mod openraft_compaction; // Log compaction and snapshot management
pub mod openraft_manager;
pub mod openraft_network; // gRPC-based networking with connection pooling and retries
pub mod openraft_performance; // Performance optimizations and caching
pub mod openraft_storage; // Production storage with WAL, crash recovery, and high-throughput // Complete cluster management with consensus operations

// pub mod acl_service;  // Temporarily disabled due to compilation issues

#[cfg(test)]
pub mod tests;

pub use raft_operations::ControllerRaftOperations;
pub use service::*;

// Legacy simple implementation exports (kept for backward compatibility during transition)
pub use openraft_simple_working::{
    WorkingAppData, WorkingAppDataResponse, WorkingNodeId, WorkingOpenRaftManager,
    WorkingPartitionAssignment, WorkingRaftConfig, WorkingRaftIntegrationHelper,
    WorkingRaftManager, WorkingSnapshotData, WorkingStateMachine,
};

// 🚀 Production-ready OpenRaft implementation exports - fully functional with OpenRaft 0.9.21

// Primary production types (use these for new deployments)
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

// Advanced OpenRaft features - production-ready for high-performance deployments
pub use openraft_compaction::{
    CompactionConfig, CompactionStats, LogCompactionManager, SnapshotMetadata, SnapshotType,
};

pub use openraft_performance::{
    HighPerformanceCache, PerformanceConfig, PerformanceMetrics, PerformanceSnapshot,
    RaftPerformanceOptimizer, ZeroCopyBuffer,
};

// pub use acl_service::*;
