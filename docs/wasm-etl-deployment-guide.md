# WebAssembly ETL Deployment Guide

This comprehensive guide provides step-by-step instructions for deploying production-ready WebAssembly ETL modules in RustMQ. WebAssembly ETL enables secure, sandboxed real-time data transformation and processing with near-native performance, featuring advanced configuration, efficient binary serialization, and intelligent content processing.

## Table of Contents

1. [Overview](#overview)
2. [Architecture Overview](#architecture-overview)
3. [Prerequisites](#prerequisites)
4. [Step 1: Configure RustMQ for WASM ETL](#step-1-configure-rustmq-for-wasm-etl)
5. [Step 2: Create a Production WASM ETL Module](#step-2-create-a-production-wasm-etl-module)
6. [Step 3: Build and Optimize the WASM Module](#step-3-build-and-optimize-the-wasm-module)
7. [Step 4: Deploy the WASM Module](#step-4-deploy-the-wasm-module)
8. [Step 5: Configure Message Processing Pipelines](#step-5-configure-message-processing-pipelines)
9. [Step 6: Test and Validate the ETL Pipeline](#step-6-test-and-validate-the-etl-pipeline)
10. [Advanced Configuration and Features](#advanced-configuration-and-features)
11. [Performance Optimization](#performance-optimization)
12. [Security Considerations](#security-considerations)
13. [Monitoring and Observability](#monitoring-and-observability)
14. [Troubleshooting](#troubleshooting)
15. [Production Best Practices](#production-best-practices)

## Overview

RustMQ's WebAssembly ETL system provides a production-ready platform for real-time message processing with:

### Core Capabilities
- **Real-time message transformation**: Transform messages as they flow through topics with sub-millisecond latency
- **Advanced filtering and enrichment**: Apply sophisticated content-based filtering, geolocation enrichment, and language detection
- **Secure sandboxed execution**: Execute untrusted code safely with memory and CPU resource limits
- **Hot deployment**: Deploy and update ETL modules without broker restarts or downtime
- **Multi-format support**: Handle JSON, XML, plain text, and binary data with automatic type detection
- **Configurable processing pipelines**: Fine-tune behavior with comprehensive configuration options

### Performance Characteristics
- **High throughput**: Process 100,000+ messages per second per broker
- **Low latency**: Sub-millisecond processing times for typical transformations
- **Efficient serialization**: Binary serialization using `bincode` for optimal performance
- **Memory efficiency**: Optimized memory management with aligned buffer pools
- **Concurrent execution**: Semaphore-based concurrency control for optimal resource utilization

### Production Features
- **Comprehensive error handling**: Graceful error recovery with detailed error reporting
- **Resource monitoring**: Real-time metrics for memory usage, execution time, and throughput
- **Content analysis**: Automatic content type detection and language identification
- **Flexible configuration**: Runtime-configurable filter rules, enrichment settings, and transformation options

## Architecture Overview

### ETL Processing Pipeline

RustMQ's WASM ETL system now supports **priority-based multi-stage pipelines** with sophisticated topic filtering and conditional processing:

```
┌─────────────────────────────────────────────────────────────────┐
│                    ETL Pipeline Orchestrator                    │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │ Priority 0  │  │ Priority 1  │  │ Priority 2  │   ...        │
│  │ [Module A]  │→ │ [Module B]  │→ │ [Module C]  │              │
│  │ [Module D]  │  │ [Module E]  │  │             │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
├─────────────────────────────────────────────────────────────────┤
│                     Topic Filter Engine                         │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐    │
│  │ Exact Matches   │ │ Wildcard Rules  │ │ Regex Patterns  │    │
│  │ - topic-a       │ │ - logs.*        │ │ - ^sensor-\d+   │    │
│  │ - events        │ │ - *.critical    │ │ - .*error.*     │    │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘    │
├─────────────────────────────────────────────────────────────────┤
│                    WASM Instance Pool                           │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐    │
│  │ Module Cache    │ │ Instance Pool   │ │ Resource Limits │    │
│  │ - Hot modules   │ │ - Pre-warmed    │ │ - Memory caps   │    │
│  │ - LRU eviction  │ │ - Instance reuse│ │ - CPU limits    │    │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

**Enhanced Architecture**: The ETL system now supports **priority-ordered multi-stage pipelines** with:
- **Priority-based execution**: Lower numbers execute first (0 → 1 → 2)
- **Advanced topic filtering**: Exact match, wildcards, regex, prefix/suffix patterns
- **Conditional processing**: Rules based on message headers and payload content
- **Parallel execution**: Multiple modules can run concurrently within the same priority level
- **Instance pooling**: Pre-warmed WASM instances for optimal performance
- **Runtime configuration**: Hot-reload pipeline configurations without broker restart

**Processing Model**: Messages are still transformed **in-place** during broker flow, but now through sophisticated multi-stage pipelines that can apply complex transformation sequences based on topic patterns and message content.

### Processing Stages

1. **Message Interception**: Messages are intercepted during normal broker processing flow
2. **Deserialization**: Binary message data is efficiently deserialized using `bincode`
3. **Pipeline Execution**: Messages are processed through a sequential pipeline of WASM modules
4. **Filtering**: Messages are evaluated against configurable filter rules (spam detection, size limits, age restrictions)
   - Filtered messages are dropped from the pipeline (not forwarded)
5. **Transformation**: Content-specific transformations are applied in-place based on message type (JSON, XML, text)
6. **Enrichment**: Additional metadata is added to message headers (geolocation, language detection, content analysis)
7. **Serialization**: Processed messages are serialized back to binary format
8. **Continuation**: Transformed messages continue through the normal broker flow to consumers and storage

### Memory Management Model

```
┌─────────────────────────────────────────────────────────────┐
│                    WASM Linear Memory                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Stack     │  │    Heap     │  │  Message Buffers    │  │
│  │   (Fixed)   │  │ (Dynamic)   │  │   (Managed by       │  │
│  │             │  │             │  │    RustMQ)          │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Configuration Architecture

ETL modules support hierarchical configuration with runtime updates:

```yaml
EtlConfig:
  filter_rules:
    spam_keywords: [...]
    max_message_size: 1MB
    max_age_seconds: 3600
    required_headers: [...]
  
  enrichment_config:
    enable_geolocation: true
    enable_content_analysis: true
    enable_language_detection: true
  
  transformation_config:
    enable_temperature_conversion: true
    enable_email_normalization: true
    enable_coordinate_formatting: true
```

### Security Sandbox Model

RustMQ implements a multi-layered security model for WASM execution:

- **Resource Limits**: Memory (64MB default), CPU time (5s default), execution count (100 concurrent)
- **System Call Restrictions**: No file I/O, network access, or system call capabilities
- **Memory Isolation**: Each WASM instance operates in isolated linear memory
- **Capability-based Security**: Only explicitly granted capabilities are available

## Prerequisites

### System Requirements

Before starting, ensure you have:
- **RustMQ cluster**: Broker and controller services running (v1.0.0+)
- **Rust toolchain**: Rust 1.75+ with WebAssembly support
- **Development tools**: `wasm-pack`, `wasm-opt` for optimization
- **Admin access**: RustMQ cluster admin credentials
- **Network access**: Access to broker (port 9092) and controller (port 9094) APIs

### Hardware Requirements

**Development Environment:**
- CPU: 2+ cores
- Memory: 4GB+ RAM
- Storage: 1GB+ free space

**Production Environment:**
- CPU: 8+ cores (for concurrent WASM execution)
- Memory: 16GB+ RAM (64MB per concurrent WASM instance)
- Storage: 10GB+ for WASM module storage
- Network: 1Gbps+ for high-throughput message processing

### Install Required Tools

```bash
# Install Rust toolchain with WebAssembly support
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Add WebAssembly compilation target
rustup target add wasm32-unknown-unknown

# Install wasm-pack for WebAssembly builds
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Install wasm-opt for size optimization (via Binaryen)
# On Ubuntu/Debian:
sudo apt update && sudo apt install -y binaryen

# On macOS:
brew install binaryen

# On other systems, install via npm:
npm install -g wasm-opt

# Verify installations
rustc --version
wasm-pack --version
wasm-opt --version
```

### Verify RustMQ Installation

```bash
# Check broker status
curl -s http://localhost:9094/api/v1/cluster/health | jq '.'

# Verify ETL support is available
curl -s http://localhost:9094/api/v1/features | jq '.etl'

# Check broker configuration
curl -s http://localhost:9094/api/v1/config | jq '.etl'
```

## Step 1: Configure RustMQ for WASM ETL

### 1.1 Production Broker Configuration

Edit your broker configuration file (e.g., `config/broker.toml`) with comprehensive ETL settings:

```toml
[etl]
# Enable WebAssembly ETL processing
enabled = true

# Memory limit per WASM execution (64MB default, max 512MB)
memory_limit_bytes = 67108864  # 64MB

# Maximum execution time for ETL operations
execution_timeout_ms = 5000  # 5 seconds

# Maximum concurrent WASM executions per broker
max_concurrent_executions = 100

# Path to store WASM modules (persistent storage recommended)
modules_path = "/var/lib/rustmq/wasm"

# Enable performance monitoring
enable_metrics = true

# Garbage collection settings for WASM instances
gc_interval_ms = 60000  # 1 minute
max_idle_instances = 10

# Resource monitoring thresholds
memory_warning_threshold_percent = 80
cpu_warning_threshold_percent = 90

# Module caching settings
module_cache_size = 50  # Maximum cached modules
module_cache_ttl_seconds = 3600  # 1 hour

# Error handling configuration
enable_error_topic = true
error_topic_prefix = "etl-errors"
max_error_message_size = 1048576  # 1MB

# Security settings
enable_module_signature_verification = false  # Set to true in production
allowed_module_sources = ["*"]  # Restrict in production
sandbox_strict_mode = true

# Logging configuration
enable_debug_logging = false
log_execution_times = true
log_memory_usage = true

# Instance pooling configuration
[etl.instance_pool]
max_pool_size = 50              # Maximum instances per module
warmup_instances = 5            # Pre-warmed instances per module
creation_rate_limit = 10.0      # Max new instances per second
idle_timeout_seconds = 300      # Idle instance cleanup timeout
enable_lru_eviction = true      # Enable LRU-based eviction
enable_background_cleanup = true # Enable background cleanup task

# Priority-based pipeline configuration
[[etl.pipelines]]
pipeline_id = "default-enrichment"
name = "Default Message Enrichment Pipeline"
enabled = true
global_timeout_ms = 10000       # Total pipeline timeout
max_retries = 3                 # Retry attempts on failure
error_handling = "SkipModule"   # StopPipeline | SkipModule | RetryWithBackoff | SendToDeadLetter

# Stage 0: Input validation (highest priority)
[[etl.pipelines.stages]]
priority = 0
parallel_execution = false      # Sequential execution for validation
continue_on_error = true

[[etl.pipelines.stages.modules]]
module_id = "message-validator"
[[etl.pipelines.stages.modules.topic_filters]]
filter_type = "Wildcard"
pattern = "events.*"
case_sensitive = false
negate = false

# Stage 1: Content enrichment (parallel execution)
[[etl.pipelines.stages]]
priority = 1
parallel_execution = true       # Allow parallel execution
continue_on_error = true

[[etl.pipelines.stages.modules]]
module_id = "geolocation-enricher"
[[etl.pipelines.stages.modules.topic_filters]]
filter_type = "Regex"
pattern = "^(events|logs)\\.(mobile|web)\\."
case_sensitive = true
negate = false

[[etl.pipelines.stages.modules.conditional_rules]]
condition_type = "HeaderValue"
field_path = "client_ip"
operator = "Contains"
value = "."
negate = false

[[etl.pipelines.stages.modules]]
module_id = "content-analyzer"
[[etl.pipelines.stages.modules.topic_filters]]
filter_type = "Suffix" 
pattern = ".content"
case_sensitive = false
negate = false

# Stage 2: Final formatting (lowest priority)
[[etl.pipelines.stages]]
priority = 2
parallel_execution = false
continue_on_error = false

[[etl.pipelines.stages.modules]]
module_id = "message-formatter"
[[etl.pipelines.stages.modules.topic_filters]]
filter_type = "Exact"
pattern = "*"  # Matches all topics
case_sensitive = false
negate = false
```

### 1.2 Controller Configuration

Update your controller configuration for ETL pipeline management:

```toml
[etl_management]
# Enable ETL pipeline management APIs
enabled = true

# Pipeline metadata storage
pipeline_metadata_path = "/var/lib/rustmq/etl/pipelines"

# Default pipeline settings
default_batch_size = 1
default_timeout_ms = 5000
default_retry_attempts = 3

# Pipeline monitoring
health_check_interval_ms = 30000  # 30 seconds
metrics_collection_interval_ms = 10000  # 10 seconds

# Resource limits per pipeline
max_pipelines_per_broker = 50
max_total_pipelines = 1000
```

### 1.3 Apply Configuration Changes

```bash
# For systemd-managed services:
sudo systemctl reload rustmq-broker  # Graceful reload
sudo systemctl reload rustmq-controller

# For production zero-downtime updates:
curl -X POST http://localhost:9094/api/v1/config/reload \
  -H "Content-Type: application/json" \
  -d '{"component": "etl", "graceful": true}'

# For development/testing:
cargo run --bin rustmq-broker -- --config config/broker.toml &
cargo run --bin rustmq-controller -- --config config/controller.toml &
```

### 1.4 Verify ETL Configuration

```bash
# Comprehensive ETL status check
curl -s http://localhost:9094/api/v1/etl/status | jq '.'

# Expected response:
# {
#   "enabled": true,
#   "memory_limit_bytes": 67108864,
#   "execution_timeout_ms": 5000,
#   "max_concurrent_executions": 100,
#   "active_modules": 0,
#   "active_pipelines": 0,
#   "total_executions": 0,
#   "cache_hit_rate": 0.0,
#   "average_execution_time_ms": 0.0
# }

# Check ETL capabilities
curl -s http://localhost:9094/api/v1/etl/capabilities | jq '.'

# Verify module storage directory
curl -s http://localhost:9094/api/v1/etl/modules | jq '.storage_info'
```

## Step 2: Create a Production WASM ETL Module

### 2.1 Initialize a Production-Ready Rust Project

```bash
# Create a new Rust library project for ETL processing
cargo new --lib advanced-message-processor
cd advanced-message-processor

# Initialize git repository for version control
git init
echo "target/" >> .gitignore
echo "Cargo.lock" >> .gitignore
```

### 2.2 Configure Production Cargo.toml

Create a comprehensive `Cargo.toml` with all required dependencies:

```toml
[package]
name = "advanced-message-processor"
version = "2.0.0"
edition = "2021"
authors = ["Your Name <your.email@company.com>"]
description = "Production-ready WebAssembly ETL module for RustMQ"
license = "MIT"
repository = "https://github.com/your-org/advanced-message-processor"

[lib]
# WebAssembly C dynamic library
crate-type = ["cdylib"]

[dependencies]
# Serialization and data structures
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"  # Efficient binary serialization used by RustMQ
chrono = { version = "0.4", features = ["serde"] }

# Efficient string searching for content filtering
memchr = "2.7"

# Collections and utilities
indexmap = "2.0"  # Ordered maps for predictable iteration

# Optional: WebAssembly optimizations
[dependencies.getrandom]
version = "0.2"
features = ["js"]  # WebAssembly entropy source

# Development dependencies for comprehensive testing
[dev-dependencies]
serde_json = "1.0"  # For test data generation
proptest = "1.0"    # Property-based testing
criterion = "0.5"   # Benchmarking

# WebAssembly-specific test support
wasm-bindgen-test = "0.3"

# Optimize for size and performance in release builds
[profile.release]
opt-level = "s"        # Optimize for size
lto = true            # Link-time optimization
codegen-units = 1     # Single codegen unit for better optimization
panic = "abort"       # Reduce binary size
strip = "symbols"     # Remove debug symbols

# Development profile for debugging
[profile.dev]
opt-level = 0
debug = true
overflow-checks = true

# Custom profile for benchmarking
[profile.bench]
opt-level = 3
debug = false
lto = true

[features]
default = ["content-analysis", "geolocation", "language-detection"]

# Feature flags for conditional compilation
content-analysis = []
geolocation = []
language-detection = []
advanced-filtering = []
debugging = []

# Metadata for RustMQ integration
[package.metadata.rustmq]
min_rustmq_version = "1.0.0"
memory_requirement_mb = 64
cpu_intensive = false
network_access = false
filesystem_access = false
```

### 2.3 Create the Production ETL Module

Create `src/lib.rs` with production-ready message processing logic matching RustMQ's implementation:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use memchr::memmem;

/// Message structure passed from RustMQ to the ETL module
/// This exactly matches RustMQ's internal Message structure
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Message {
    pub key: Option<String>,
    pub value: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub timestamp: i64,
}

/// Result structure returned by the ETL module to RustMQ
/// Uses binary serialization for optimal performance
#[derive(Serialize, Deserialize, Debug)]
pub struct ProcessResult {
    pub transformed_messages: Vec<Message>,
    pub should_continue: bool,
    pub error: Option<String>,
}

/// WASM interface structure to return both pointer and length
/// This prevents memory leaks by providing explicit size information
#[repr(C)]
pub struct WasmResult {
    pub ptr: *mut u8,
    pub len: usize,
}

/// Configuration for the ETL pipeline (passed during initialization)
/// Supports runtime configuration updates and feature toggles
#[derive(Deserialize, Debug, Clone)]
pub struct EtlConfig {
    pub filter_rules: FilterConfig,
    pub enrichment_config: EnrichmentConfig,
    pub transformation_config: TransformationConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FilterConfig {
    pub spam_keywords: Vec<String>,
    pub max_message_size: usize,
    pub max_age_seconds: i64,
    pub required_headers: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EnrichmentConfig {
    pub enable_geolocation: bool,
    pub enable_content_analysis: bool,
    pub enable_language_detection: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TransformationConfig {
    pub enable_temperature_conversion: bool,
    pub enable_email_normalization: bool,
    pub enable_coordinate_formatting: bool,
}

impl Default for EtlConfig {
    fn default() -> Self {
        Self {
            filter_rules: FilterConfig {
                spam_keywords: vec!["spam".to_string(), "test-delete".to_string()],
                max_message_size: 1024 * 1024, // 1MB
                max_age_seconds: 3600, // 1 hour
                required_headers: vec!["source".to_string()],
            },
            enrichment_config: EnrichmentConfig {
                enable_geolocation: true,
                enable_content_analysis: true,
                enable_language_detection: true,
            },
            transformation_config: TransformationConfig {
                enable_temperature_conversion: true,
                enable_email_normalization: true,
                enable_coordinate_formatting: true,
            },
        }
    }
}

// Global configuration (initialized once)
static mut ETL_CONFIG: Option<EtlConfig> = None;

/// Initialize the ETL module with configuration
/// This should be called once when the module is loaded by RustMQ
#[no_mangle]
pub extern "C" fn init_module(config_ptr: *const u8, config_len: usize) -> bool {
    if config_len == 0 {
        // Use default configuration
        unsafe {
            ETL_CONFIG = Some(EtlConfig::default());
        }
        return true;
    }

    let config_slice = unsafe { std::slice::from_raw_parts(config_ptr, config_len) };
    
    match bincode::deserialize::<EtlConfig>(config_slice) {
        Ok(config) => {
            unsafe {
                ETL_CONFIG = Some(config);
            }
            true
        }
        Err(_) => {
            // Fallback to default on error
            unsafe {
                ETL_CONFIG = Some(EtlConfig::default());
            }
            false
        }
    }
}

/// Main entry point called by RustMQ for each message
/// Returns a pointer to WasmResult containing both data pointer and length
/// Uses efficient binary serialization with bincode for optimal performance
#[no_mangle]
pub extern "C" fn process_message(input_ptr: *const u8, input_len: usize) -> *mut WasmResult {
    let input_slice = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    
    // Parse message using efficient binary format (not JSON!)
    let message: Message = match bincode::deserialize(input_slice) {
        Ok(msg) => msg,
        Err(e) => {
            return create_wasm_result(ProcessResult {
                transformed_messages: vec![],
                should_continue: false,
                error: Some(format!("Failed to parse message: {}", e)),
            });
        }
    };

    // Ensure configuration is initialized
    if unsafe { ETL_CONFIG.is_none() } {
        unsafe {
            ETL_CONFIG = Some(EtlConfig::default());
        }
    }

    match transform_message(message) {
        Ok(result) => create_wasm_result(result),
        Err(e) => create_wasm_result(ProcessResult {
            transformed_messages: vec![],
            should_continue: false,
            error: Some(e),
        }),
    }
}

/// Core message transformation logic - refactored into clear pipeline
/// Follows the exact pattern used in RustMQ's production implementation
fn transform_message(message: Message) -> Result<ProcessResult, String> {
    let default_config = EtlConfig::default();
    let config = unsafe { 
        ETL_CONFIG.as_ref().unwrap_or(&default_config)
    };

    // Step 1: Apply filtering first (early exit if filtered)
    if should_filter_message(&message, &config.filter_rules) {
        return Ok(ProcessResult {
            transformed_messages: vec![],
            should_continue: true,
            error: None,
        });
    }

    // Step 2: Apply transformations
    let mut message = apply_transformations(message, &config.transformation_config)?;

    // Step 3: Apply enrichment
    message = apply_enrichment(message, &config.enrichment_config)?;

    Ok(ProcessResult {
        transformed_messages: vec![message],
        should_continue: true,
        error: None,
    })
}

/// Apply content-specific transformations based on configuration
fn apply_transformations(mut message: Message, config: &TransformationConfig) -> Result<Message, String> {
    // Add processing timestamp
    message.headers.insert(
        "processed_at".to_string(),
        chrono::Utc::now().timestamp().to_string(),
    );

    // Handle different message types
    if let Some(content_type) = message.headers.get("content-type") {
        match content_type.as_str() {
            "application/json" => message = transform_json_message(message, config)?,
            "text/plain" => message = transform_text_message(message)?,
            _ => {
                // Pass through unknown content types
                message.headers.insert("transformation".to_string(), "passthrough".to_string());
            }
        }
    }

    Ok(message)
}

/// Apply enrichment based on configuration
fn apply_enrichment(mut message: Message, config: &EnrichmentConfig) -> Result<Message, String> {
    // Add standard enrichment headers
    message.headers.insert("enriched".to_string(), "true".to_string());
    message.headers.insert("processor".to_string(), "message-processor-v2".to_string());
    message.headers.insert("processing_time".to_string(), 
        chrono::Utc::now().timestamp_millis().to_string());

    // Conditional enrichment based on configuration
    if config.enable_geolocation {
        if let Some(ip) = message.headers.get("client_ip") {
            if let Ok(geo_data) = lookup_geolocation(ip) {
                message.headers.insert("geo_country".to_string(), geo_data.country);
                message.headers.insert("geo_city".to_string(), geo_data.city);
                message.headers.insert("geo_region".to_string(), geo_data.region);
            }
        }
    }

    if config.enable_content_analysis {
        let content_analysis = analyze_content(&message.value);
        message.headers.insert("content_size".to_string(), message.value.len().to_string());
        message.headers.insert("content_type_detected".to_string(), content_analysis.detected_type);
        
        if config.enable_language_detection {
            message.headers.insert("content_language".to_string(), content_analysis.language);
        }
    }

    Ok(message)
}

/// Transform JSON messages with configurable features
fn transform_json_message(mut message: Message, config: &TransformationConfig) -> Result<Message, String> {
    let mut json_value: serde_json::Value = serde_json::from_slice(&message.value)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    if let serde_json::Value::Object(ref mut obj) = json_value {
        // Add processing metadata
        obj.insert("_processed".to_string(), serde_json::Value::Bool(true));
        obj.insert("_processor_version".to_string(), serde_json::Value::String("2.0.0".to_string()));
        
        // Transform temperature units (configurable)
        if config.enable_temperature_conversion {
            if let Some(temp_c) = obj.get("temperature_celsius").and_then(|v| v.as_f64()) {
                let temp_f = (temp_c * 9.0 / 5.0) + 32.0;
                // Handle potential NaN/Infinity values gracefully
                if let Some(temp_f_number) = serde_json::Number::from_f64(temp_f) {
                    obj.insert("temperature_fahrenheit".to_string(), temp_f_number.into());
                } else {
                    // Log error or skip field if temperature conversion resulted in invalid number
                    obj.insert("temperature_conversion_error".to_string(), 
                        "Invalid temperature value (NaN or Infinity)".into());
                }
            }
        }

        // Normalize email addresses (configurable)
        if config.enable_email_normalization {
            if let Some(email) = obj.get("email").and_then(|v| v.as_str()) {
                let normalized_email = email.to_lowercase();
                obj.insert("email".to_string(), normalized_email.into());
            }
        }

        // Add computed fields (configurable)
        if config.enable_coordinate_formatting {
            if let (Some(lat), Some(lon)) = (
                obj.get("latitude").and_then(|v| v.as_f64()),
                obj.get("longitude").and_then(|v| v.as_f64())
            ) {
                obj.insert("coordinate_string".to_string(), 
                    format!("{:.6},{:.6}", lat, lon).into());
            }
        }
    }

    message.value = serde_json::to_vec(&json_value)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

    Ok(message)
}

/// Transform plain text messages
fn transform_text_message(mut message: Message) -> Result<Message, String> {
    let text = std::str::from_utf8(&message.value)
        .map_err(|e| format!("Invalid UTF-8: {}", e))?;
    
    let original_length = text.len();
    
    // Convert to uppercase and add metadata
    let transformed_text = format!("[PROCESSED] {}", text.to_uppercase());
    message.value = transformed_text.into_bytes();
    
    message.headers.insert("transformation".to_string(), "text_uppercase".to_string());
    message.headers.insert("original_length".to_string(), original_length.to_string());

    Ok(message)
}

/// Determine if a message should be filtered out using efficient byte search
/// Uses memchr for high-performance string searching
fn should_filter_message(message: &Message, config: &FilterConfig) -> bool {
    // Filter messages without required headers
    for required_header in &config.required_headers {
        if !message.headers.contains_key(required_header) {
            return true;
        }
    }

    // Filter oversized messages
    if message.value.len() > config.max_message_size {
        return true;
    }

    // Filter based on spam keywords using efficient byte search
    for spam_keyword in &config.spam_keywords {
        if memmem::find(&message.value, spam_keyword.as_bytes()).is_some() {
            return true;
        }
        // Also check case-insensitive by converting to lowercase bytes
        let keyword_lower = spam_keyword.to_lowercase();
        if let Ok(text) = std::str::from_utf8(&message.value) {
            let text_lower = text.to_lowercase();
            if memmem::find(text_lower.as_bytes(), keyword_lower.as_bytes()).is_some() {
                return true;
            }
        }
    }

    // Filter old messages
    let current_time = chrono::Utc::now().timestamp();
    if message.timestamp < current_time - config.max_age_seconds {
        return true;
    }

    false
}

/// Enhanced geolocation lookup with regional data
struct GeoData {
    country: String,
    city: String,
    region: String,
}

fn lookup_geolocation(ip: &str) -> Result<GeoData, String> {
    // Enhanced IP-based geolocation (replace with actual service in production)
    if ip.starts_with("192.168.") || ip.starts_with("10.") || ip.starts_with("172.") {
        Ok(GeoData {
            country: "Local".to_string(),
            city: "Private Network".to_string(),
            region: "RFC1918".to_string(),
        })
    } else if ip.starts_with("8.8.") || ip.starts_with("8.4.") {
        Ok(GeoData {
            country: "US".to_string(),
            city: "Mountain View".to_string(),
            region: "California".to_string(),
        })
    } else if ip.starts_with("1.1.") {
        Ok(GeoData {
            country: "US".to_string(),
            city: "San Francisco".to_string(),
            region: "California".to_string(),
        })
    } else {
        Ok(GeoData {
            country: "Unknown".to_string(),
            city: "Unknown".to_string(),
            region: "Unknown".to_string(),
        })
    }
}

/// Content analysis structure
struct ContentAnalysis {
    detected_type: String,
    language: String,
}

/// Analyze message content to detect type and language
fn analyze_content(content: &[u8]) -> ContentAnalysis {
    if let Ok(text) = std::str::from_utf8(content) {
        // Try to detect JSON
        if text.trim_start().starts_with('{') && text.trim_end().ends_with('}') {
            if serde_json::from_str::<serde_json::Value>(text).is_ok() {
                return ContentAnalysis {
                    detected_type: "application/json".to_string(),
                    language: detect_language(text),
                };
            }
        }

        // Try to detect XML
        if text.trim_start().starts_with('<') {
            return ContentAnalysis {
                detected_type: "application/xml".to_string(),
                language: detect_language(text),
            };
        }

        // Default to plain text
        ContentAnalysis {
            detected_type: "text/plain".to_string(),
            language: detect_language(text),
        }
    } else {
        ContentAnalysis {
            detected_type: "application/octet-stream".to_string(),
            language: "unknown".to_string(),
        }
    }
}

/// Simple language detection based on common words
fn detect_language(text: &str) -> String {
    let text_lower = text.to_lowercase();
    
    // English indicators
    if text_lower.contains(" the ") || text_lower.contains(" and ") || text_lower.contains(" is ") {
        return "en".to_string();
    }
    
    // Spanish indicators
    if text_lower.contains(" el ") || text_lower.contains(" la ") || text_lower.contains(" es ") {
        return "es".to_string();
    }
    
    // French indicators
    if text_lower.contains(" le ") || text_lower.contains(" la ") || text_lower.contains(" est ") {
        return "fr".to_string();
    }
    
    "unknown".to_string()
}

/// Create WASM result with proper memory management
/// Uses binary serialization for optimal performance
fn create_wasm_result(result: ProcessResult) -> *mut WasmResult {
    // Serialize using efficient binary format (not JSON!)
    let serialized = match bincode::serialize(&result) {
        Ok(data) => data,
        Err(_) => {
            let error_result = ProcessResult {
                transformed_messages: vec![],
                should_continue: false,
                error: Some("Serialization failed".to_string()),
            };
            bincode::serialize(&error_result).unwrap_or_else(|_| vec![0])
        }
    };

    // Allocate buffer for the data
    let len = serialized.len();
    let ptr = alloc(len);
    
    // Copy data to allocated buffer
    unsafe {
        std::ptr::copy_nonoverlapping(serialized.as_ptr(), ptr, len);
    }

    // Create WasmResult structure
    let wasm_result = WasmResult { ptr, len };
    
    // Allocate and return pointer to WasmResult
    let result_ptr = alloc(std::mem::size_of::<WasmResult>()) as *mut WasmResult;
    unsafe {
        std::ptr::write(result_ptr, wasm_result);
    }
    
    result_ptr
}

/// Memory allocation for WASM - required by RustMQ
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

/// Memory deallocation for WASM - required by RustMQ
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    let layout = std::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_transformation() {
        let message = Message {
            key: Some("test-key".to_string()),
            value: r#"{"temperature_celsius": 25.0, "sensor_id": "temp001", "email": "TEST@EXAMPLE.COM"}"#.as_bytes().to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h.insert("source".to_string(), "sensor-network".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = transform_message(message).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);
        
        let transformed = &result.transformed_messages[0];
        let json: serde_json::Value = serde_json::from_slice(&transformed.value).unwrap();
        
        assert!(json.get("_processed").unwrap().as_bool().unwrap());
        assert_eq!(json.get("temperature_fahrenheit").unwrap().as_f64().unwrap(), 77.0);
        assert_eq!(json.get("email").unwrap().as_str().unwrap(), "test@example.com");
    }

    #[test]
    fn test_text_transformation() {
        let message = Message {
            key: Some("text-key".to_string()),
            value: b"hello world".to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "text/plain".to_string());
                h.insert("source".to_string(), "user-input".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = transform_message(message).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);
        
        let transformed = &result.transformed_messages[0];
        let text = std::str::from_utf8(&transformed.value).unwrap();
        assert!(text.contains("[PROCESSED] HELLO WORLD"));
        assert_eq!(transformed.headers.get("transformation").unwrap(), "text_uppercase");
    }

    #[test]
    fn test_message_filtering() {
        let filter_config = FilterConfig {
            spam_keywords: vec!["spam".to_string()],
            max_message_size: 1024 * 1024,
            max_age_seconds: 3600,
            required_headers: vec!["source".to_string()],
        };
        
        // Test spam filtering
        let spam_message = Message {
            key: Some("spam-key".to_string()),
            value: b"This is spam content".to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("source".to_string(), "unknown".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        assert!(should_filter_message(&spam_message, &filter_config));

        // Test missing source header
        let no_source_message = Message {
            key: Some("test-key".to_string()),
            value: b"valid content".to_vec(),
            headers: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        assert!(should_filter_message(&no_source_message, &filter_config));
    }

    #[test]
    fn test_geolocation_lookup() {
        let private_ip_geo = lookup_geolocation("192.168.1.1").unwrap();
        assert_eq!(private_ip_geo.country, "Local");

        let google_dns_geo = lookup_geolocation("8.8.8.8").unwrap();
        assert_eq!(google_dns_geo.country, "US");
        assert_eq!(google_dns_geo.city, "Mountain View");
    }

    #[test]
    fn test_content_analysis() {
        let json_analysis = analyze_content(br#"{"test": true}"#);
        assert_eq!(json_analysis.detected_type, "application/json");

        let xml_analysis = analyze_content(b"<root><test>value</test></root>");
        assert_eq!(xml_analysis.detected_type, "application/xml");

        let text_analysis = analyze_content(b"Hello world, this is a test");
        assert_eq!(text_analysis.detected_type, "text/plain");
        assert_eq!(text_analysis.language, "en");
    }
}
```

## Step 3: Build the WASM Module

### 3.1 Build for WebAssembly

```bash
# Build the WASM module
wasm-pack build --target web --out-dir pkg

# Or build directly with cargo
cargo build --target wasm32-unknown-unknown --release
```

### 3.2 Optimize the WASM Module (Optional)

```bash
# Install wasm-opt for size optimization
npm install -g wasm-opt

# Optimize the WASM file
wasm-opt --strip-debug --strip-producers -Oz \
  target/wasm32-unknown-unknown/release/message_processor.wasm \
  -o message_processor_optimized.wasm
```

## Step 4: Deploy the WASM Module

### 4.1 Upload the WASM Module

```bash
# Upload the WASM module to RustMQ
curl -X POST http://localhost:9094/api/v1/etl/modules \
  -H "Content-Type: application/octet-stream" \
  -H "X-Module-Name: message-processor" \
  -H "X-Module-Version: 1.0.0" \
  --data-binary @target/wasm32-unknown-unknown/release/message_processor.wasm

# Response should indicate successful upload
# {"status": "uploaded", "module_id": "message-processor-v1.0.0", "size_bytes": 12345}
```

### 4.2 Verify Module Upload

```bash
# List available WASM modules
curl -s http://localhost:9094/api/v1/etl/modules | jq '.'

# Get specific module info
curl -s http://localhost:9094/api/v1/etl/modules/message-processor-v1.0.0 | jq '.'
```

## Step 5: Configure Priority-Based ETL Pipelines

### 5.1 Understanding Pipeline Configuration

The new priority-based ETL system supports complex multi-stage pipelines with advanced filtering and conditional processing:

#### Pipeline Structure
- **Stages**: Ordered by priority (0 = highest priority, executed first)
- **Modules**: Multiple modules can exist in each stage
- **Filters**: Topic patterns that determine which messages each module processes
- **Conditions**: Rules based on message headers or payload content
- **Execution**: Sequential or parallel execution within each stage

### 5.2 Create Advanced ETL Pipeline Configuration

Create `etl-pipeline.json` for a sophisticated processing pipeline:

```json
{
  "pipeline_id": "iot-sensor-processing",
  "name": "IoT Sensor Data Processing Pipeline",
  "description": "Multi-stage processing for IoT sensor data with validation, enrichment, and formatting",
  "enabled": true,
  "global_timeout_ms": 15000,
  "max_retries": 3,
  "error_handling": "SkipModule",
  "stages": [
    {
      "priority": 0,
      "parallel_execution": false,
      "continue_on_error": true,
      "stage_timeout_ms": 2000,
      "modules": [
        {
          "module_id": "sensor-data-validator",
          "topic_filters": [
            {
              "filter_type": "Prefix",
              "pattern": "iot.sensors.",
              "case_sensitive": true,
              "negate": false
            },
            {
              "filter_type": "Regex",
              "pattern": "^sensor-\\d+\\.",
              "case_sensitive": true,
              "negate": false
            }
          ],
          "conditional_rules": [
            {
              "condition_type": "HeaderValue",
              "field_path": "content-type",
              "operator": "Equals",
              "value": "application/json",
              "negate": false
            },
            {
              "condition_type": "PayloadField",
              "field_path": "$.sensor_id",
              "operator": "Contains",
              "value": "sensor",
              "negate": false
            }
          ],
          "instance_config": {
            "memory_limit_bytes": 33554432,
            "execution_timeout_ms": 1000,
            "max_concurrent_instances": 10,
            "enable_caching": true,
            "cache_ttl_seconds": 300,
            "custom_config": {
              "strict_validation": true,
              "schema_version": "v2.1"
            }
          }
        }
      ]
    },
    {
      "priority": 1,
      "parallel_execution": true,
      "continue_on_error": true,
      "stage_timeout_ms": 5000,
      "modules": [
        {
          "module_id": "temperature-converter",
          "topic_filters": [
            {
              "filter_type": "Contains",
              "pattern": "temperature",
              "case_sensitive": false,
              "negate": false
            }
          ],
          "conditional_rules": [
            {
              "condition_type": "PayloadField",
              "field_path": "$.sensor_type",
              "operator": "Equals",
              "value": "temperature",
              "negate": false
            },
            {
              "condition_type": "PayloadField",
              "field_path": "$.unit",
              "operator": "Equals",
              "value": "celsius",
              "negate": false
            }
          ],
          "instance_config": {
            "memory_limit_bytes": 16777216,
            "execution_timeout_ms": 500,
            "max_concurrent_instances": 20,
            "enable_caching": false
          }
        },
        {
          "module_id": "geolocation-enricher",
          "topic_filters": [
            {
              "filter_type": "Wildcard",
              "pattern": "iot.sensors.*",
              "case_sensitive": true,
              "negate": false
            }
          ],
          "conditional_rules": [
            {
              "condition_type": "HeaderValue",
              "field_path": "location",
              "operator": "Contains",
              "value": "lat",
              "negate": false
            }
          ],
          "instance_config": {
            "memory_limit_bytes": 25165824,
            "execution_timeout_ms": 800,
            "max_concurrent_instances": 15,
            "enable_caching": true,
            "cache_ttl_seconds": 600
          }
        },
        {
          "module_id": "anomaly-detector",
          "topic_filters": [
            {
              "filter_type": "Suffix",
              "pattern": ".critical",
              "case_sensitive": false,
              "negate": false
            }
          ],
          "conditional_rules": [
            {
              "condition_type": "PayloadField",
              "field_path": "$.reading",
              "operator": "GreaterThan",
              "value": 100,
              "negate": false
            }
          ],
          "instance_config": {
            "memory_limit_bytes": 50331648,
            "execution_timeout_ms": 2000,
            "max_concurrent_instances": 5,
            "enable_caching": true,
            "cache_ttl_seconds": 120
          }
        }
      ]
    },
    {
      "priority": 2,
      "parallel_execution": false,
      "continue_on_error": false,
      "stage_timeout_ms": 3000,
      "modules": [
        {
          "module_id": "data-formatter",
          "topic_filters": [
            {
              "filter_type": "Exact",
              "pattern": "*",
              "case_sensitive": false,
              "negate": false
            }
          ],
          "conditional_rules": [],
          "instance_config": {
            "memory_limit_bytes": 8388608,
            "execution_timeout_ms": 300,
            "max_concurrent_instances": 30,
            "enable_caching": true,
            "cache_ttl_seconds": 60
          }
        }
      ]
    }
  ]
}
```

### 5.3 Filter Types and Patterns

#### Available Filter Types

1. **Exact**: Direct string comparison (fastest)
   ```json
   {"filter_type": "Exact", "pattern": "events.user.login"}
   ```

2. **Wildcard**: Glob patterns with `*` and `?`
   ```json
   {"filter_type": "Wildcard", "pattern": "logs.*.error"}
   ```

3. **Regex**: Full regular expression support
   ```json
   {"filter_type": "Regex", "pattern": "^sensor-\\d{3}-\\w+$"}
   ```

4. **Prefix**: String prefix matching
   ```json
   {"filter_type": "Prefix", "pattern": "iot.devices."}
   ```

5. **Suffix**: String suffix matching
   ```json
   {"filter_type": "Suffix", "pattern": ".critical"}
   ```

6. **Contains**: Substring matching
   ```json
   {"filter_type": "Contains", "pattern": "temperature"}
   ```

#### Conditional Rules

Rules evaluate message content for fine-grained processing control:

```json
{
  "condition_type": "HeaderValue",
  "field_path": "content-type",
  "operator": "Equals", 
  "value": "application/json",
  "negate": false
}
```

**Condition Types:**
- `HeaderValue`: Check message headers
- `PayloadField`: Check JSON payload fields (JSONPath syntax)
- `MessageSize`: Check total message size
- `MessageAge`: Check message timestamp

**Operators:**
- `Equals`, `NotEquals`
- `GreaterThan`, `LessThan`, `GreaterThanOrEqual`, `LessThanOrEqual`
- `Contains`, `StartsWith`, `EndsWith`
- `Matches` (regex)

### 5.2 Deploy the Pipeline

```bash
# Create the ETL pipeline
curl -X POST http://localhost:9094/api/v1/etl/pipelines \
  -H "Content-Type: application/json" \
  -d @etl-pipeline.json

# Response: {"pipeline_id": "pipeline-123", "status": "created"}
```

### 5.3 Start the Pipeline

```bash
# Start the ETL pipeline
curl -X POST http://localhost:9094/api/v1/etl/pipelines/pipeline-123/start

# Check pipeline status
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/status | jq '.'
```

## Step 6: Test the ETL Pipeline

### 6.1 Send Test Messages

```bash
# Send a test message to a topic configured for ETL processing
curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
  -H "Content-Type: application/json" \
  -d '{
    "key": "sensor-001",
    "value": "{\"temperature_celsius\": 23.5, \"humidity\": 65.2, \"sensor_id\": \"env001\"}",
    "headers": {
      "content-type": "application/json",
      "source": "iot-sensors",
      "client_ip": "192.168.1.100"
    }
  }'
```

### 6.2 Verify Processed Messages

```bash
# Consume messages from the same topic - messages will be transformed in-place
curl -s "http://localhost:9092/api/v1/topics/raw-events/messages?offset=0&max_messages=10" | jq '.'

# Expected output should show transformed message with:
# - temperature_fahrenheit field added to JSON payload
# - _processed: true in the message content
# - enrichment headers added to message headers
# - processing timestamps in headers
# - original message structure preserved but enhanced
```

### 6.3 Monitor Pipeline Metrics

```bash
# Get comprehensive ETL pipeline metrics
curl -s http://localhost:9094/api/v1/etl/pipelines/iot-sensor-processing/metrics | jq '.'

# Expected metrics:
# {
#   "pipeline_id": "iot-sensor-processing",
#   "total_messages_processed": 1542,
#   "messages_by_stage": {
#     "0": {"processed": 1542, "failed": 3, "avg_time_ms": 0.8},
#     "1": {"processed": 1539, "failed": 12, "avg_time_ms": 2.3},
#     "2": {"processed": 1527, "failed": 0, "avg_time_ms": 0.3}
#   },
#   "filter_performance": {
#     "exact_matches": 892,
#     "wildcard_matches": 435,
#     "regex_matches": 127,
#     "filter_evaluation_time_us": 45.2
#   },
#   "instance_pool_stats": {
#     "pool_hit_ratio": 0.94,
#     "instances_created": 23,
#     "instances_reused": 1519,
#     "avg_pool_utilization": 0.76
#   },
#   "parallel_execution_stats": {
#     "concurrent_modules_avg": 2.3,
#     "parallelization_efficiency": 0.89
#   },
#   "error_handling": {
#     "modules_skipped": 12,
#     "retries_attempted": 4,
#     "dead_letter_messages": 3
#   },
#   "throughput_msgs_per_sec": 156.7,
#   "avg_pipeline_latency_ms": 3.4
# }

# Get instance pool statistics
curl -s http://localhost:9094/api/v1/etl/instance-pool/stats | jq '.'

# Get filter engine performance
curl -s http://localhost:9094/api/v1/etl/filters/performance | jq '.'

# Monitor real-time pipeline execution
curl -s http://localhost:9094/api/v1/etl/pipelines/iot-sensor-processing/status | jq '.'
```

## Step 7: Pipeline Management and Runtime Operations

### 7.1 Runtime Pipeline Management

The priority-based ETL system supports hot configuration updates and runtime pipeline management:

```bash
# Add a new pipeline at runtime
curl -X POST http://localhost:9094/api/v1/etl/pipelines \
  -H "Content-Type: application/json" \
  -d @new-pipeline.json

# Update an existing pipeline configuration
curl -X PUT http://localhost:9094/api/v1/etl/pipelines/iot-sensor-processing \
  -H "Content-Type: application/json" \
  -d @updated-pipeline.json

# Enable/disable a pipeline without removal
curl -X PATCH http://localhost:9094/api/v1/etl/pipelines/iot-sensor-processing \
  -H "Content-Type: application/json" \
  -d '{"enabled": false}'

# Remove a pipeline completely
curl -X DELETE http://localhost:9094/api/v1/etl/pipelines/iot-sensor-processing

# List all configured pipelines
curl -s http://localhost:9094/api/v1/etl/pipelines | jq '.'
```

### 7.2 Instance Pool Management

Monitor and manage the WASM instance pool for optimal performance:

```bash
# Get detailed pool statistics
curl -s http://localhost:9094/api/v1/etl/instance-pool/stats | jq '.'

# Warm up instances for a specific module
curl -X POST http://localhost:9094/api/v1/etl/instance-pool/warmup \
  -H "Content-Type: application/json" \
  -d '{"module_id": "temperature-converter", "instance_count": 10}'

# Evict idle instances to free memory
curl -X POST http://localhost:9094/api/v1/etl/instance-pool/cleanup

# Resize pool for a module
curl -X PATCH http://localhost:9094/api/v1/etl/instance-pool/temperature-converter \
  -H "Content-Type: application/json" \
  -d '{"max_pool_size": 30, "warmup_instances": 8}'
```

### 7.3 Performance Tuning

#### Optimize Filter Performance

```bash
# Get filter performance statistics
curl -s http://localhost:9094/api/v1/etl/filters/performance | jq '.'

# Response shows evaluation times by filter type:
# {
#   "exact_match_time_ns": 45,
#   "wildcard_time_ns": 1250,
#   "regex_time_ns": 8900,
#   "prefix_suffix_time_ns": 180,
#   "contains_time_ns": 320,
#   "total_evaluations": 15432,
#   "cache_hit_ratio": 0.89
# }
```

#### Parallel Execution Optimization

```json
{
  "stages": [
    {
      "priority": 1,
      "parallel_execution": true,
      "max_concurrent_modules": 5,
      "execution_strategy": "WorkStealing",
      "modules": [
        // Multiple independent modules can run in parallel
      ]
    }
  ]
}
```

### 7.4 Error Handling Strategies

Configure different error handling approaches based on your requirements:

```json
{
  "error_handling": "RetryWithBackoff",
  "retry_config": {
    "max_retries": 3,
    "initial_delay_ms": 100,
    "max_delay_ms": 5000,
    "backoff_multiplier": 2.0,
    "jitter": true
  },
  "dead_letter_config": {
    "topic_prefix": "dlq.etl",
    "include_error_context": true,
    "max_message_size": 1048576
  }
}
```

**Error Handling Options:**
- `StopPipeline`: Halt entire pipeline on any module failure
- `SkipModule`: Continue with next module, log error
- `RetryWithBackoff`: Retry failed module with exponential backoff
- `SendToDeadLetter`: Route failed messages to error topic

### 7.5 Health Monitoring and Alerting

```bash
# Check overall ETL system health
curl -s http://localhost:9094/api/v1/etl/health | jq '.'

# Get pipeline health status
curl -s http://localhost:9094/api/v1/etl/pipelines/iot-sensor-processing/health | jq '.'

# Response includes health indicators:
# {
#   "status": "healthy",
#   "pipeline_id": "iot-sensor-processing",
#   "last_successful_execution": "2024-01-15T10:30:45Z",
#   "error_rate_percent": 0.8,
#   "avg_latency_ms": 3.2,
#   "instance_pool_health": "optimal",
#   "stages": [
#     {
#       "priority": 0,
#       "status": "healthy",
#       "modules_healthy": 1,
#       "modules_degraded": 0,
#       "modules_failed": 0
#     }
#   ]
# }
```

## Advanced Configuration

### Multiple Message Output

Your ETL module can output multiple messages from a single input (useful for message splitting):

```rust
fn transform_message(message: Message) -> Result<ProcessResult, String> {
    let mut results = vec![];
    
    // Split a batch message into individual messages
    if message.headers.get("type") == Some(&"batch".to_string()) {
        let batch_data: Vec<serde_json::Value> = serde_json::from_slice(&message.value)?;
        
        for (i, item) in batch_data.into_iter().enumerate() {
            let mut new_message = Message {
                key: Some(format!("{}-{}", message.key.unwrap_or_default(), i)),
                value: serde_json::to_vec(&item)?,
                headers: message.headers.clone(),
                timestamp: message.timestamp,
            };
            new_message.headers.insert("batch_index".to_string(), i.to_string());
            results.push(new_message);
        }
    } else {
        results.push(message);
    }
    
    Ok(ProcessResult {
        transformed_messages: results,
        should_continue: true,
        error: None,
    })
}
```

**Note**: Multiple messages returned by the ETL module replace the original message in the processing pipeline. To send messages to different topics, you would need to integrate the ETL processor with the broker's producer API (not currently implemented).

### Error Handling and Dead Letter Queue

```rust
fn transform_message(message: Message) -> Result<ProcessResult, String> {
    // Validate message structure
    if message.value.is_empty() {
        return Ok(ProcessResult {
            transformed_messages: vec![],
            should_continue: true,
            error: Some("Empty message body".to_string()),
        });
    }
    
    // Try processing with fallback
    match risky_processing(&message) {
        Ok(processed) => Ok(ProcessResult {
            transformed_messages: vec![processed],
            should_continue: true,
            error: None,
        }),
        Err(e) => {
            // Send to dead letter queue with error context
            let mut error_message = message.clone();
            error_message.headers.insert("error".to_string(), e.clone());
            error_message.headers.insert("failed_at".to_string(), 
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string());
            
            Ok(ProcessResult {
                transformed_messages: vec![error_message],
                should_continue: true,
                error: Some(format!("Processing failed: {}", e)),
            })
        }
    }
}
```

### State Management (Limited)

WASM modules are stateless between invocations, but you can use static variables for simple state:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static MESSAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn transform_message(mut message: Message) -> Result<ProcessResult, String> {
    let count = MESSAGE_COUNTER.fetch_add(1, Ordering::SeqCst);
    
    // Add sequence number to message
    message.headers.insert("sequence".to_string(), count.to_string());
    
    // Reset counter periodically
    if count % 10000 == 0 {
        MESSAGE_COUNTER.store(0, Ordering::SeqCst);
    }
    
    Ok(ProcessResult {
        transformed_messages: vec![message],
        should_continue: true,
        error: None,
    })
}
```

## Troubleshooting

### Common Issues

#### 1. Module Upload Fails

```bash
# Check module size (should be < 50MB)
ls -lh target/wasm32-unknown-unknown/release/message_processor.wasm

# Verify WASM format
file target/wasm32-unknown-unknown/release/message_processor.wasm
# Should output: "WebAssembly (wasm) binary module"

# Check broker logs
tail -f /var/log/rustmq/broker.log | grep -i "etl\|wasm"
```

#### 2. Pipeline Not Processing Messages

```bash
# Check pipeline status
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/status

# Verify source topic has messages
curl -s "http://localhost:9092/api/v1/topics/raw-events/messages?offset=0&max_messages=1"

# Check broker ETL metrics
curl -s http://localhost:9094/api/v1/metrics | jq '.etl'
```

#### 3. Processing Errors

```bash
# Check error logs
curl -s "http://localhost:9092/api/v1/topics/etl-errors/messages?offset=0&max_messages=10" | jq '.'

# Monitor processing metrics
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/metrics | jq '.messages_failed'

# Enable debug logging in broker config
[logging]
level = "debug"
targets = ["rustmq::etl"]
```

#### 4. Performance Issues

```bash
# Check WASM execution times
curl -s http://localhost:9094/api/v1/etl/pipelines/pipeline-123/metrics | jq '.avg_processing_time_ms'

# Monitor memory usage
curl -s http://localhost:9094/api/v1/metrics | jq '.etl.memory_usage_bytes'

# Adjust configuration for better performance
curl -X PATCH http://localhost:9094/api/v1/etl/pipelines/pipeline-123 \
  -H "Content-Type: application/json" \
  -d '{"configuration": {"timeout_ms": 500, "batch_size": 10}}'
```

## Best Practices

### 1. Performance Optimization

- **Keep modules small**: Target < 1MB for optimal loading
- **Minimize allocations**: Reuse data structures where possible
- **Avoid expensive operations**: No network calls, file I/O, or crypto
- **Use efficient serialization**: Consider binary formats for large messages

### 2. Error Handling

- **Always handle errors gracefully**: Return error information rather than panicking
- **Validate input data**: Check message structure before processing
- **Use timeouts**: Set reasonable execution time limits
- **Log meaningful errors**: Include context for debugging

### 3. Security

- **Validate all inputs**: Never trust message content
- **Limit resource usage**: Respect memory and CPU limits
- **Avoid unsafe operations**: Stick to safe Rust patterns
- **Test thoroughly**: Include edge cases and malformed data

### 4. Development Workflow

```bash
# Use a development script for rapid iteration
#!/bin/bash
# build-and-deploy.sh

set -e

echo "Building WASM module..."
cargo build --target wasm32-unknown-unknown --release

echo "Uploading module..."
curl -X POST http://localhost:9094/api/v1/etl/modules \
  -H "Content-Type: application/octet-stream" \
  -H "X-Module-Name: message-processor" \
  -H "X-Module-Version: $(date +%s)" \
  --data-binary @target/wasm32-unknown-unknown/release/message_processor.wasm

echo "Testing with sample message..."
curl -X POST http://localhost:9092/api/v1/topics/raw-events/messages \
  -H "Content-Type: application/json" \
  -d '{
    "key": "test",
    "value": "{\"test\": true}",
    "headers": {"content-type": "application/json", "source": "test"}
  }'

echo "Done!"
```

### 5. Monitoring and Observability

```rust
// Add metrics to your ETL module
static PROCESSED_COUNT: AtomicU64 = AtomicU64::new(0);
static ERROR_COUNT: AtomicU64 = AtomicU64::new(0);

fn transform_message(message: Message) -> Result<ProcessResult, String> {
    let start_time = std::time::Instant::now();
    
    let result = match do_transform(&message) {
        Ok(transformed) => {
            PROCESSED_COUNT.fetch_add(1, Ordering::SeqCst);
            ProcessResult {
                transformed_messages: vec![transformed],
                should_continue: true,
                error: None,
            }
        }
        Err(e) => {
            ERROR_COUNT.fetch_add(1, Ordering::SeqCst);
            ProcessResult {
                transformed_messages: vec![],
                should_continue: true,
                error: Some(e),
            }
        }
    };
    
    let duration = start_time.elapsed();
    // Log performance metrics (if logging is available)
    
    Ok(result)
}
```

## Conclusion

This guide provides a comprehensive foundation for deploying WebAssembly ETL modules in RustMQ. The combination of Rust's safety, WebAssembly's sandboxing, and RustMQ's performance enables powerful real-time message processing capabilities.

For additional examples and advanced patterns, see:
- [examples/wasm-etl/](../examples/wasm-etl/) - Complete example projects
- [API Reference](./api-reference.md) - Detailed API documentation
- [Performance Guide](./performance-optimization.md) - Advanced optimization techniques