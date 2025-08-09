//! ETL (Extract, Transform, Load) processing module
//! 
//! This module provides a high-performance, priority-based ETL pipeline system
//! for real-time message transformation using WebAssembly modules.
//!
//! ## Key Components
//!
//! - **Processor**: Main ETL processor with backward compatibility
//! - **Orchestrator**: Priority-based pipeline orchestrator with multi-stage execution
//! - **Filter**: Optimized topic filtering engine with multiple pattern types
//! - **Instance Pool**: High-performance WASM instance pooling with LRU eviction
//!
//! ## Features
//!
//! - Priority-based execution (lower numbers execute first)
//! - Multiple modules per priority level with parallel execution support
//! - Advanced topic filtering (exact, wildcard, regex, prefix, suffix, contains)
//! - Conditional rules based on message headers and payload
//! - WASM instance pooling for optimal performance
//! - Zero-copy operations where possible
//! - Comprehensive error handling strategies
//! - Real-time configuration updates

pub mod processor;
pub mod orchestrator;
pub mod filter;
pub mod instance_pool;

#[cfg(test)]
mod tests;

pub use processor::{
    EtlProcessor, EtlPipeline, MockEtlProcessor,
    EtlModule, ModuleConfig, DataFormat,
    EtlRequest, EtlResponse, ProcessingContext,
    ProcessedRecord, ProcessingMetadata,
    ModuleInfo, ModuleStats, MemoryUsageStats,
    EtlMetrics
};

pub use orchestrator::{
    EtlPipelineOrchestrator, PipelineExecutionContext, PipelineExecutionResult,
    ExecutionMetadata, StageExecutionInfo, ModuleExecutionInfo,
    OrchestratorStats, PipelineStats, ModuleExecutionStats
};

pub use filter::{
    TopicFilterEngine, ConditionalRuleEngine, FilterResult,
    CompiledGlobFilter, CompiledRegexFilter, StringFilter,
    CompiledConditionalRule, FilterEngineStats
};

pub use instance_pool::{
    WasmInstancePool, PooledInstance, WasmContext,
    InstanceCheckout, InstanceReturnHandle,
    InstancePoolStats, ModulePoolStats
};