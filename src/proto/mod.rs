//! Generated protobuf modules for RustMQ
//! 
//! This module contains all generated protobuf types and services.
//! Generated automatically by build.rs - do not edit manually.

#![allow(clippy::all)]
#![allow(warnings)]

#[path = "rustmq.broker.rs"]
pub mod broker;

#[path = "rustmq.common.rs"]
pub mod common;

#[path = "rustmq.controller.rs"]
pub mod controller;

// Re-export commonly used types for convenience
pub use common::*;

// Service re-exports
pub use broker::broker_replication_service_server::BrokerReplicationServiceServer;
pub use broker::broker_replication_service_client::BrokerReplicationServiceClient;
pub use controller::controller_raft_service_server::ControllerRaftServiceServer;
pub use controller::controller_raft_service_client::ControllerRaftServiceClient;
