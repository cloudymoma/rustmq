pub mod service;
pub mod openraft_integration;
pub mod openraft_simple_working; // Legacy simplified implementation (kept for reference)
pub mod raft_operations;  // Raft operations abstraction for ACL (breaks circular dependency)

// Production-ready OpenRaft implementations for RustMQ - fully compatible with OpenRaft 0.9.21
pub mod openraft_storage;     // Production storage with WAL, crash recovery, and high-throughput
pub mod openraft_network;     // gRPC-based networking with connection pooling and retries
pub mod openraft_compaction;  // Log compaction and snapshot management
pub mod openraft_performance; // Performance optimizations and caching
pub mod openraft_manager;     // Complete cluster management with consensus operations

// pub mod acl_service;  // Temporarily disabled due to compilation issues

#[cfg(test)]
pub mod tests;

pub use service::*;
pub use raft_operations::ControllerRaftOperations;

// Legacy simple implementation exports (kept for backward compatibility during transition)
pub use openraft_simple_working::{
    WorkingNodeId,
    WorkingAppData,
    WorkingAppDataResponse,
    WorkingSnapshotData,
    WorkingPartitionAssignment,
    WorkingStateMachine,
    WorkingRaftManager,
    WorkingRaftIntegrationHelper,
    WorkingRaftConfig,
    WorkingOpenRaftManager,
};

// 🚀 Production-ready OpenRaft implementation exports - fully functional with OpenRaft 0.9.21

// Primary production types (use these for new deployments)
pub use openraft_storage::{
    RustMqTypeConfig,
    RustMqAppData, 
    RustMqAppDataResponse,
    RustMqSnapshotData,
    RustMqLogStorage,
    RustMqStateMachine,
    RustMqStorageConfig,
    RustMqNode,
    PartitionAssignment,
    NodeId,
    RustMqResponder,
    RustMqSnapshotBuilder,
};

pub use openraft_manager::{
    RaftManager,
    RaftManagerConfig,
    RaftManagerMetrics,
    ManagerState,
    HealthCheckResult,
    StorageMetrics,
    CompactionMetrics,
};

pub use openraft_network::{
    RustMqNetwork, 
    RustMqNetworkConfig, 
    RustMqNetworkFactory,
    NetworkMetrics,
    ConnectionPoolStats,
};

// Advanced OpenRaft features - production-ready for high-performance deployments
pub use openraft_compaction::{
    LogCompactionManager,
    CompactionConfig,
    SnapshotMetadata,
    SnapshotType,
    CompactionStats,
};

pub use openraft_performance::{
    RaftPerformanceOptimizer,
    PerformanceConfig,
    PerformanceMetrics,
    PerformanceSnapshot,
    HighPerformanceCache,
    ZeroCopyBuffer,
};

// pub use acl_service::*;