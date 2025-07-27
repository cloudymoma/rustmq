# RustMQ: Cloud-Native Distributed Message Queue System - Final Architecture Design

## Executive Summary

RustMQ is a next-generation, cloud-native distributed message queue system designed to combine the high-performance characteristics of Apache Kafka with the cost-effectiveness and operational simplicity of modern cloud architectures. Built from the ground up in Rust, RustMQ leverages a shared-storage architecture that decouples compute from storage, enabling unprecedented elasticity, cost savings, and operational efficiency.

This document synthesizes comprehensive architectural analysis from multiple design perspectives to present a unified, implementable design that addresses the core limitations of traditional messaging systems in cloud environments.

## Table of Contents

1. [Product Vision and Value Proposition](#1-product-vision-and-value-proposition)
2. [Core Architecture Overview](#2-core-architecture-overview)
3. [Tiered Storage Engine](#3-tiered-storage-engine)
4. [Data Replication and High Availability](#4-data-replication-and-high-availability)
5. [Network Protocols and APIs](#5-network-protocols-and-apis)
6. [Performance Optimization Strategies](#6-performance-optimization-strategies)
7. [WebAssembly-Based ETL Processing](#7-webassembly-based-etl-processing)
8. [Dynamic Scalability and Load Balancing](#8-dynamic-scalability-and-load-balancing)
9. [Administrative and Operational Framework](#9-administrative-and-operational-framework)
10. [Implementation Roadmap](#10-implementation-roadmap)
11. [Configuration and Deployment](#11-configuration-and-deployment)

## 1. Product Vision and Value Proposition

### 1.1 Core Vision

RustMQ transforms distributed messaging by engineering a cloud-native platform that delivers the powerful, durable semantics of Apache Kafka while fundamentally re-architecting its storage and compute layers to achieve the elasticity, performance, and cost-efficiency that modern cloud environments demand.

### 1.2 Key Value Propositions

#### **10x Cost Reduction**
- **90% Storage Cost Savings**: Single-copy storage in object storage vs. traditional 3x replication
- **Zero Cross-AZ Traffic**: Eliminates expensive inter-availability zone replication costs
- **Spot Instance Compatibility**: Stateless brokers enable usage of 60-90% cheaper spot instances
- **Independent Scaling**: Scale compute and storage resources independently based on actual needs

#### **100x Elasticity Improvement**
- **Instant Scaling**: Add/remove brokers in seconds without data movement
- **Stateless Architecture**: Brokers can be replaced instantly without service disruption
- **Metadata-Only Operations**: Partition reassignment requires only metadata updates
- **Auto-Balancing**: Continuous load distribution optimization without manual intervention

#### **Single-Digit Millisecond Latency**
- **Optimized Write Path**: Local NVMe WAL with Direct I/O for sub-millisecond persistence
- **Zero-Copy Data Movement**: Eliminate unnecessary memory copies in hot paths
- **QUIC Protocol**: Reduced connection overhead and head-of-line blocking elimination
- **Workload Isolation**: Hot and cold reads use separate cache systems

### 1.3 Target Use Cases

- **Real-Time Analytics & Data Lakehouse**: High-throughput ingestion with indefinite retention
- **Event-Driven Microservices**: Elastic message bus handling spiky traffic patterns
- **IoT and Telemetry**: Massive scale ingestion from millions of endpoints
- **Financial Services**: Ultra-low latency with uncompromising durability guarantees

## 2. Core Architecture Overview

### 2.1 Architectural Principles

RustMQ is built on four foundational principles:

1. **Storage-Compute Separation**: Brokers are ephemeral, stateless processing units
2. **Stateless Data Plane**: No permanent partition ownership by individual brokers
3. **Durability Delegation**: Leverage cloud infrastructure's built-in durability guarantees
4. **Performance through Specialization**: Each component optimized for a single task

### 2.2 System Components

```ascii
┌─────────────────────────────────────────────────────────────────┐
│                        Client Layer                             │
├─────────────────┬─────────────────┬─────────────────────────────┤
│   Producers     │   Consumers     │     Admin Tools             │
└─────────────────┴─────────────────┴─────────────────────────────┘
                           │
                    ┌──────┴──────┐
                    │Load Balancer│
                    │(QUIC/HTTP3) │
                    └──────┬──────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                 │                 │
┌───────▼──────┐ ┌────────▼──────┐ ┌───────▼──────┐
│  Broker 1    │ │   Broker 2    │ │   Broker N   │
│ (Stateless)  │ │  (Stateless)  │ │ (Stateless)  │
│              │ │               │ │              │
│┌───────────┐ │ │┌────────────┐ │ │┌───────────┐ │
││Local WAL  │ │ ││ Local WAL  │ │ ││Local WAL  │ │
││& Cache    │ │ ││ & Cache    │ │ ││& Cache    │ │
│└───────────┘ │ │└────────────┘ │ │└───────────┘ │
└───────┬──────┘ └────────┬──────┘ └───────┬──────┘
        │                 │                │
        └─────────────────┼────────────────┘
                          │
            ┌─────────────┴─────────────┐
            │   Controller Cluster      │
            │   (Raft Consensus)        │
            │  ┌─────────────────────┐  │
            │  │     Metastore       │  │
            │  │   (etcd/FDB)        │  │
            │  └─────────────────────┘  │
            └─────────────┬─────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        │                 │                 │
┌───────▼──────┐ ┌────────▼──────┐ ┌───────▼──────┐
│  S3 Bucket   │ │  GCS Bucket   │ │Object Storage│
│   Region A   │ │   Region B    │ │   Region N   │
└──────────────┘ └───────────────┘ └──────────────┘
```

#### **Broker Nodes (Stateless)**
- Handle client connections via QUIC/HTTP3
- Manage local WAL and cache for performance
- Coordinate with object storage for data persistence
- Execute WebAssembly-based ETL operations
- Participate in leader-follower replication without data ownership

#### **Controller Cluster**
- Raft-based consensus for cluster coordination
- Partition leadership assignment and management
- Continuous load balancing and auto-scaling decisions
- Broker health monitoring and failure detection
- Metadata consistency management

#### **External Metastore**
- Distributed key-value store (etcd or FoundationDB)
- Source of truth for all cluster metadata
- Transactional updates for consistency
- Watch-based change notifications

### 2.3 Metadata Schema

| Key Path | Value Schema | Description |
|----------|--------------|-------------|
| `/brokers/registered/{broker_id}` | `{"host": "10.0.1.23", "port_quic": 9092, "port_rpc": 9093, "rack_id": "us-east-1a"}` | Broker endpoint and availability zone information |
| `/topics/{topic_name}/config` | `{"partitions": 64, "retention_ms": -1, "wasm_module_id": "pii_scrubber_v1"}` | Topic configuration including ETL modules |
| `/topics/{topic_name}/partitions/{partition_id}/leader` | `{"broker_id": "broker-007", "leader_epoch": 142}` | Current partition leader and epoch |
| `/topics/{topic_name}/partitions/{partition_id}/segments/{segment_start_offset}` | `{"status": "IN_OBJECT_STORE", "object_key": "topics/my-topic/0/000...123.seg", "size_bytes": 1073741824}` | Segment location and status metadata |
| `/consumer_groups/{group_id}/offsets/{topic_name}/{partition_id}` | `{"offset": 45678, "metadata": "client-specific-data"}` | Consumer group offset tracking |

## 3. Tiered Storage Engine

### 3.1 Architecture Overview

The storage engine implements a sophisticated two-tier system designed for both low-latency writes and cost-effective infinite storage:

```rust
pub struct TieredStorageEngine {
    // Local high-performance tier
    wal: Arc<WriteAheadLog>,
    write_cache: Arc<WriteCache>,
    read_cache: Arc<ReadCache>,
    
    // Object storage tier
    object_storage: Arc<dyn ObjectStorage>,
    upload_manager: Arc<UploadManager>,
    
    // Coordination
    metadata_store: Arc<MetadataStore>,
    compaction_manager: Arc<CompactionManager>,
}
```

### 3.2 Local Tier: Write-Ahead Log

#### **Direct I/O Implementation**
```rust
use tokio_uring::{fs::File, BufResult};

pub struct DirectIOWal {
    file: File,
    buffer_pool: Arc<BufferPool>,
    current_offset: AtomicU64,
    config: WalConfig,
}

impl DirectIOWal {
    pub async fn append(&self, record: WalRecord) -> Result<u64, WalError> {
        // 1. Get aligned buffer from pool
        let buffer = self.buffer_pool.get_aligned_buffer(record.size())?;
        
        // 2. Serialize record with zero-copy
        let serialized = record.serialize_to_buffer(&buffer)?;
        
        // 3. Direct I/O write bypassing OS cache
        let offset = self.current_offset.fetch_add(serialized.len() as u64, Ordering::SeqCst);
        let (result, _buf) = self.file.write_at(serialized, offset).await;
        result?;
        
        // 4. Optional fsync for durability
        if self.config.fsync_on_write {
            self.file.sync_data().await?;
        }
        
        Ok(offset)
    }
}
```

#### **Circular Buffer Design**
The WAL uses a circular buffer structure where all partitions for which a broker is the leader append to a single shared log file. This design maximizes sequential write performance on block devices.

### 3.3 Object Storage Tier

#### **Upload Strategy**
```rust
pub struct UploadManager {
    storage: Arc<dyn ObjectStorage>,
    upload_queue: Arc<SegmentQueue<UploadTask>>,
    retry_policy: ExponentialBackoff,
    bandwidth_limiter: Arc<TokenBucket>,
}

impl UploadManager {
    pub async fn upload_segment(&self, segment: WalSegment) -> Result<ObjectKey, UploadError> {
        // 1. Apply bandwidth limiting
        self.bandwidth_limiter.acquire(segment.size()).await?;
        
        // 2. Compress if configured
        let compressed = if self.config.compression_enabled {
            self.compress_segment(&segment).await?
        } else {
            segment.data.clone()
        };
        
        // 3. Choose upload strategy based on size
        let object_key = if compressed.len() > self.config.multipart_threshold {
            self.multipart_upload(&compressed).await?
        } else {
            self.simple_upload(&compressed).await?
        };
        
        // 4. Verify upload integrity
        self.verify_upload(&object_key, &compressed).await?;
        
        Ok(object_key)
    }
}
```

#### **Object Management Strategy**
- **Stream Objects**: Large, monolithic objects (1GB+) for high-throughput partitions
- **Stream Set Objects**: Aggregated objects combining multiple low-volume partitions
- **Intelligent Compaction**: Background processes optimize object layout for cost and performance

### 3.4 Cache Management

#### **Workload Isolation**
```rust
pub struct CacheManager {
    // Hot data cache for tailing consumers
    write_cache: Arc<WriteCache>,
    
    // Cold data cache for historical consumers  
    read_cache: Arc<ReadCache>,
    
    // Cache policies
    write_cache_policy: Arc<LruPolicy>,
    read_cache_policy: Arc<LruPolicy>,
}

impl CacheManager {
    pub async fn serve_read(&self, offset: u64) -> Result<Bytes, CacheError> {
        // Try hot cache first (for tailing reads)
        if let Some(data) = self.write_cache.get(offset).await {
            return Ok(data);
        }
        
        // Check cold cache (for historical reads)
        if let Some(data) = self.read_cache.get(offset).await {
            return Ok(data);
        }
        
        // Fetch from object storage
        let data = self.fetch_from_object_storage(offset).await?;
        
        // Cache in read cache (not write cache)
        self.read_cache.put(offset, data.clone()).await?;
        
        Ok(data)
    }
}
```

## 4. Data Replication and High Availability

### 4.1 Leader-Based Replication

RustMQ implements a simplified replication model that leverages the shared storage architecture:

```rust
pub struct ReplicationManager {
    stream_id: u64,
    current_leader: Option<BrokerId>,
    replica_set: Vec<BrokerId>,
    
    // State tracking
    log_end_offset: AtomicU64,
    high_watermark: AtomicU64,
    follower_states: Arc<RwLock<HashMap<BrokerId, FollowerState>>>,
    
    // Configuration  
    min_in_sync_replicas: usize,
    ack_timeout: Duration,
}

impl ReplicationManager {
    pub async fn replicate_record(&self, record: WalRecord) -> Result<ReplicationResult, ReplicationError> {
        // 1. Write to local WAL first
        let local_offset = self.append_to_local_wal(record.clone()).await?;
        
        // 2. Replicate to followers in parallel
        let replication_futures = self.replica_set
            .iter()
            .filter(|&broker_id| *broker_id != self.current_leader.unwrap())
            .map(|&broker_id| {
                let record_clone = record.clone();
                async move {
                    self.send_to_follower(broker_id, record_clone).await
                }
            });
        
        // 3. Wait for minimum acknowledgments
        let acks = futures::future::join_all(replication_futures).await;
        let successful_acks = acks.iter().filter(|r| r.is_ok()).count();
        
        if successful_acks >= self.min_in_sync_replicas - 1 {
            self.high_watermark.store(local_offset, Ordering::SeqCst);
            Ok(ReplicationResult {
                offset: local_offset,
                durability: DurabilityLevel::Durable,
            })
        } else {
            Ok(ReplicationResult {
                offset: local_offset,
                durability: DurabilityLevel::LocalOnly,
            })
        }
    }
}
```

### 4.2 Acknowledgment Levels

```rust
pub enum AcknowledgmentLevel {
    None,        // Fire-and-forget
    Leader,      // Leader WAL only
    Majority,    // Majority of replicas
    All,         // All in-sync replicas
    Custom(usize), // Custom count
}
```

### 4.3 Failover Model

The failover process is dramatically simplified compared to traditional systems:

1. **Failure Detection**: Controller detects missed heartbeats
2. **Leader Re-election**: Controller selects new leader from healthy brokers
3. **Metadata Update**: Atomic update of leader information in metastore
4. **Client Redirection**: Clients discover new leader on next metadata refresh

No data movement is required since all data resides in shared storage accessible to all brokers.

## 5. Network Protocols and APIs

### 5.1 Client-Broker Protocol: QUIC/HTTP3

#### **Justification for QUIC**
- **0-RTT/1-RTT Connection Setup**: Faster than TCP+TLS handshakes
- **Head-of-Line Blocking Elimination**: Independent stream processing
- **Connection Migration**: Seamless network transitions for mobile/IoT
- **Built-in Encryption**: TLS 1.3 mandatory, simplifying security

#### **Implementation**
```rust
use quinn::{Endpoint, ServerConfig, ClientConfig};
use h3::{server::RequestStream, client::SendRequest};

pub struct QuicServer {
    endpoint: Endpoint,
    connection_pool: Arc<ConnectionPool>,
    request_router: Arc<RequestRouter>,
}

impl QuicServer {
    pub async fn handle_produce_request(
        &self,
        mut request_stream: RequestStream<bytes::Bytes>,
    ) -> Result<(), QuicError> {
        // Read request with zero-copy
        let request_data = request_stream.recv_data().await?;
        let produce_request: ProduceRequest = bincode::deserialize(&request_data)?;
        
        // Process request
        let response = self.process_produce(produce_request).await?;
        
        // Send response
        let response_data = bincode::serialize(&response)?;
        request_stream.send_data(response_data.into()).await?;
        request_stream.finish().await?;
        
        Ok(())
    }
}
```

#### **API Specification**
| Method | Request | Response | Description |
|--------|---------|----------|-------------|
| Produce | `{"topic": "...", "partition_id": ..., "records": [...]}` | `{"offset": ..., "error_code": ...}` | Write records to partition |
| Fetch | `{"topic": "...", "partition_id": ..., "fetch_offset": ..., "max_bytes": ...}` | `{"records": [...], "high_watermark": ...}` | Read records from offset |
| GetMetadata | `{"topics": ["..."]}` | `{"brokers": [...], "topics_metadata": [...]}` | Discover cluster topology |
| CommitOffset | `{"group_id": "...", "topic": "...", "partition_id": ..., "offset": ...}` | `{"error_code": ...}` | Commit consumer offset |

### 5.2 Internal Communication: gRPC

For internal broker-to-broker and controller-to-broker communication:

```protobuf
service BrokerService {
    // Replication operations
    rpc ReplicateRecords(ReplicationRequest) returns (ReplicationResponse);
    rpc FetchRecords(FetchRequest) returns (stream FetchResponse);
    
    // Cluster coordination
    rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
    rpc BecomeLeader(LeadershipRequest) returns (LeadershipResponse);
    rpc ResignLeader(LeadershipRequest) returns (LeadershipResponse);
}

message ReplicationRequest {
    uint64 stream_id = 1;
    repeated Record records = 2;
    uint64 leader_epoch = 3;
    uint64 high_watermark = 4;
}
```

## 6. Performance Optimization Strategies

### 6.1 Zero-Copy Implementation

#### **Memory Management**
```rust
use bytes::{Bytes, BytesMut, Buf, BufMut};

pub struct ZeroCopyPipeline {
    buffer_pool: Arc<BufferPool>,
    io_uring: Option<IoUring>,
}

impl ZeroCopyPipeline {
    pub async fn transfer_data<R, W>(&self, reader: R, writer: W, size: usize) -> Result<usize, IoError>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        // Use splice for zero-copy data movement on Linux
        #[cfg(target_os = "linux")]
        {
            self.splice_transfer(reader, writer, size).await
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            self.buffered_transfer(reader, writer, size).await
        }
    }
    
    #[cfg(target_os = "linux")]
    async fn splice_transfer<R, W>(&self, reader: R, writer: W, size: usize) -> Result<usize, IoError> {
        // Use io_uring splice operation for kernel-level zero-copy
        let splice_op = opcode::Splice::new(
            types::Fd(reader.as_raw_fd()),
            0,
            types::Fd(writer.as_raw_fd()),
            0,
            size as u32,
        );
        
        self.io_uring.as_ref().unwrap().submit_and_wait(splice_op).await
    }
}
```

#### **Vectored I/O**
```rust
pub async fn vectored_write<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message_parts: &[Bytes],
) -> Result<usize, io::Error> {
    let io_slices: Vec<IoSlice> = message_parts
        .iter()
        .map(|bytes| IoSlice::new(bytes))
        .collect();
    
    writer.write_vectored(&io_slices).await
}
```

### 6.2 Bandwidth Limiting

#### **Token Bucket Implementation**
```rust
pub struct BandwidthLimiter {
    capacity: u64,
    tokens: AtomicU64,
    refill_rate: u64,
    last_refill: Arc<Mutex<Instant>>,
}

impl BandwidthLimiter {
    pub async fn acquire(&self, bytes: usize) -> Result<(), BandwidthError> {
        loop {
            let current_tokens = self.tokens.load(Ordering::SeqCst);
            if current_tokens >= bytes as u64 {
                match self.tokens.compare_exchange_weak(
                    current_tokens,
                    current_tokens - bytes as u64,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return Ok(()),
                    Err(_) => continue, // Retry on contention
                }
            } else {
                // Wait for token refill
                tokio::time::sleep(Duration::from_millis(10)).await;
                self.refill_tokens().await;
            }
        }
    }
    
    async fn refill_tokens(&self) {
        let mut last_refill = self.last_refill.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);
        
        if elapsed >= Duration::from_millis(10) {
            let tokens_to_add = (elapsed.as_millis() as u64 * self.refill_rate) / 1000;
            let current = self.tokens.load(Ordering::SeqCst);
            let new_tokens = (current + tokens_to_add).min(self.capacity);
            self.tokens.store(new_tokens, Ordering::SeqCst);
            *last_refill = now;
        }
    }
}
```

## 7. WebAssembly-Based ETL Processing

### 7.1 Runtime Integration

```rust
use wasmer::{Store, Module, Instance, Function, Memory};

pub struct WasmETLProcessor {
    store: Store,
    modules: HashMap<String, Module>,
    instance_pool: Arc<SegmentQueue<Instance>>,
    memory_limiter: Arc<MemoryLimiter>,
    execution_timeout: Duration,
}

impl WasmETLProcessor {
    pub async fn process_message(
        &self,
        message: &[u8],
        module_name: &str,
    ) -> Result<Vec<u8>, ETLError> {
        // Get WASM instance from pool
        let instance = self.instance_pool.pop().await?;
        
        // Execute with timeout and resource limits
        let result = tokio::time::timeout(
            self.execution_timeout,
            self.execute_transform(&instance, message)
        ).await??;
        
        // Return instance to pool
        self.instance_pool.push(instance).await;
        
        Ok(result)
    }
    
    async fn execute_transform(&self, instance: &Instance, message: &[u8]) -> Result<Vec<u8>, ETLError> {
        // Get the transform function
        let transform_fn: Function = instance.exports.get_function("transform")?;
        
        // Allocate memory in WASM
        let memory = instance.exports.get_memory("memory")?;
        let input_ptr = self.allocate_in_wasm(instance, message.len())?;
        
        // Copy input data
        let memory_view = memory.view(&self.store);
        // ... copy message to WASM memory ...
        
        // Call transform function
        let result_ptr = transform_fn.call(&self.store, &[Value::I32(input_ptr as i32)])?;
        
        // Read result from WASM memory
        let output = self.read_wasm_output(instance, result_ptr)?;
        
        Ok(output)
    }
}
```

### 7.2 Security and Sandboxing

```rust
pub struct WasmSandbox {
    memory_limit: usize,
    cpu_time_limit: Duration,
    allowed_imports: HashSet<String>,
    resource_monitor: Arc<ResourceMonitor>,
}

impl WasmSandbox {
    pub fn create_secure_instance(&self, module: &Module) -> Result<Instance, SandboxError> {
        // Create restricted import object
        let import_object = self.create_restricted_imports()?;
        
        // Configure memory limits
        let memory_type = MemoryType::new(
            1,  // Minimum pages (64KB)
            Some(self.memory_limit / PAGE_SIZE), // Maximum pages
        );
        
        // Create instance with restrictions
        let instance = Instance::new(&self.store, module, &import_object)?;
        
        // Install resource monitor
        self.resource_monitor.monitor_instance(&instance)?;
        
        Ok(instance)
    }
}
```

## 8. Dynamic Scalability and Load Balancing

### 8.1 Auto-Balancing Algorithm

```rust
pub struct LoadBalancer {
    brokers: Arc<RwLock<HashMap<BrokerId, BrokerInfo>>>,
    partitions: Arc<RwLock<HashMap<PartitionId, PartitionInfo>>>,
    metrics_collector: Arc<MetricsCollector>,
    config: BalancingConfig,
}

impl LoadBalancer {
    pub async fn run_balancing_cycle(&self) -> Result<(), BalanceError> {
        // 1. Collect current load metrics
        let broker_loads = self.collect_broker_loads().await?;
        let partition_loads = self.collect_partition_loads().await?;
        
        // 2. Calculate load variance
        let load_variance = self.calculate_load_variance(&broker_loads);
        
        // 3. Check if rebalancing is needed
        if !self.should_rebalance(load_variance, &broker_loads).await {
            return Ok(());
        }
        
        // 4. Generate rebalancing plan
        let plan = self.generate_rebalancing_plan(&broker_loads, &partition_loads).await?;
        
        // 5. Execute rebalancing
        self.execute_rebalancing_plan(plan).await?;
        
        Ok(())
    }
    
    async fn generate_rebalancing_plan(
        &self,
        broker_loads: &[BrokerLoad],
        partition_loads: &[PartitionLoad],
    ) -> Result<RebalancePlan, BalanceError> {
        let mut plan = RebalancePlan::new();
        
        // Sort brokers by current load
        let mut sorted_brokers = broker_loads.to_vec();
        sorted_brokers.sort_by(|a, b| a.total_load().partial_cmp(&b.total_load()).unwrap());
        
        // Identify overloaded and underloaded brokers
        let overloaded_brokers = sorted_brokers
            .iter()
            .filter(|b| b.is_overloaded(&self.config))
            .collect::<Vec<_>>();
        
        let underloaded_brokers = sorted_brokers
            .iter()
            .filter(|b| b.is_underloaded(&self.config))
            .collect::<Vec<_>>();
        
        // Move partitions from overloaded to underloaded brokers
        for overloaded in overloaded_brokers {
            let partitions_to_move = self.select_partitions_to_move(overloaded, partition_loads)?;
            
            for partition in partitions_to_move {
                if let Some(target_broker) = self.find_best_target_broker(&underloaded_brokers, &partition) {
                    plan.add_move(RebalanceMove {
                        partition_id: partition.id,
                        from_broker: overloaded.broker_id,
                        to_broker: target_broker.broker_id,
                        estimated_cost: self.calculate_move_cost(&partition, target_broker),
                    });
                }
            }
        }
        
        Ok(plan)
    }
}
```

### 8.2 Cost Function

```rust
pub struct CostCalculator {
    config: CostConfig,
    network_topology: Arc<NetworkTopology>,
    historical_metrics: Arc<MetricsHistory>,
}

impl CostCalculator {
    pub fn calculate_placement_cost(
        &self,
        partition: &PartitionInfo,
        broker: &BrokerInfo,
    ) -> PlacementCost {
        let mut cost = 0.0;
        
        // Latency cost based on network topology
        let latency_cost = self.calculate_latency_cost(partition, broker);
        cost += latency_cost * self.config.latency_weight;
        
        // Bandwidth cost based on partition throughput
        let bandwidth_cost = self.calculate_bandwidth_cost(partition, broker);
        cost += bandwidth_cost * self.config.bandwidth_weight;
        
        // Resource utilization costs
        let cpu_cost = self.calculate_cpu_cost(partition, broker);
        cost += cpu_cost * self.config.cpu_weight;
        
        let memory_cost = self.calculate_memory_cost(partition, broker);
        cost += memory_cost * self.config.memory_weight;
        
        // Geographic penalties
        if !self.network_topology.same_zone(partition.current_broker, broker.id) {
            cost *= self.config.cross_zone_penalty;
        }
        
        PlacementCost {
            total: cost,
            latency: latency_cost,
            bandwidth: bandwidth_cost,
            cpu: cpu_cost,
            memory: memory_cost,
        }
    }
}
```

## 9. Administrative and Operational Framework

### 9.1 RESTful Admin API

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put, delete},
    Router,
};

pub fn create_admin_api_router() -> Router<AdminApiState> {
    Router::new()
        // Cluster endpoints
        .route("/api/v1/cluster", get(get_cluster_info))
        .route("/api/v1/cluster/health", get(get_cluster_health))
        
        // Broker endpoints
        .route("/api/v1/brokers", get(list_brokers).post(register_broker))
        .route("/api/v1/brokers/:broker_id", 
               get(get_broker_details).delete(decommission_broker))
        
        // Topic endpoints
        .route("/api/v1/topics", get(list_topics).post(create_topic))
        .route("/api/v1/topics/:topic_name", 
               get(get_topic_details).put(update_topic).delete(delete_topic))
        
        // WASM module endpoints
        .route("/api/v1/wasm/modules", get(list_wasm_modules).post(deploy_wasm_module))
        .route("/api/v1/wasm/modules/:module_name", 
               get(get_wasm_module).put(update_wasm_module).delete(remove_wasm_module))
        
        // Metrics endpoints
        .route("/api/v1/metrics", get(get_metrics))
        .route("/api/v1/metrics/stream", get(stream_metrics))
}

// Handler implementations
async fn create_topic(
    State(state): State<AdminApiState>,
    Json(request): Json<TopicCreationRequest>,
) -> Result<Json<TopicCreationResponse>, AdminApiError> {
    // Validate request
    request.validate()?;
    
    // Check if topic already exists
    if state.topic_manager.topic_exists(&request.name).await? {
        return Err(AdminApiError::TopicAlreadyExists(request.name));
    }
    
    // Create topic configuration
    let topic_config = TopicConfig {
        name: request.name.clone(),
        partition_count: request.partition_count,
        replication_factor: request.replication_factor,
        retention_policy: request.retention_policy,
        compression: request.compression,
        etl_modules: request.etl_modules.unwrap_or_default(),
    };
    
    // Start topic creation operation
    let operation_id = state.topic_manager.create_topic(topic_config).await?;
    
    Ok(Json(TopicCreationResponse {
        topic_name: request.name,
        operation_id,
        status: "pending".to_string(),
    }))
}
```

### 9.2 gRPC Service Definitions

```protobuf
service AdminService {
    // Cluster operations
    rpc GetClusterInfo(GetClusterInfoRequest) returns (ClusterInfo);
    rpc StreamClusterHealth(StreamHealthRequest) returns (stream HealthUpdate);
    
    // Topic operations
    rpc CreateTopic(CreateTopicRequest) returns (CreateTopicResponse);
    rpc UpdateTopicConfig(UpdateTopicConfigRequest) returns (UpdateTopicConfigResponse);
    
    // Partition operations
    rpc RebalancePartitions(RebalanceRequest) returns (stream RebalanceUpdate);
    rpc MigratePartition(MigratePartitionRequest) returns (stream MigrationUpdate);
    
    // Metrics streaming
    rpc StreamMetrics(MetricsStreamRequest) returns (stream MetricsUpdate);
}

message CreateTopicRequest {
    string name = 1;
    uint32 partition_count = 2;
    uint32 replication_factor = 3;
    TopicConfig config = 4;
    repeated string etl_modules = 5;
}

message RebalanceRequest {
    repeated string topic_names = 1;
    RebalanceStrategy strategy = 2;
    uint32 max_concurrent_moves = 3;
    google.protobuf.Duration timeout = 4;
}

enum RebalanceStrategy {
    MINIMIZE_MOVEMENT = 0;
    BALANCE_LOAD = 1;
    BALANCE_LEADERSHIP = 2;
    OPTIMIZE_LATENCY = 3;
}
```

## 10. Implementation Roadmap

### Phase 1: Foundation (Months 1-2)
**Objectives**: Core infrastructure and interfaces
**Deliverables**:
- [ ] Project setup with CI/CD pipeline
- [ ] Core trait definitions (Stream, Storage, ObjectStorage)
- [ ] Basic error handling and configuration systems
- [ ] Memory-based implementations for testing
- [ ] Comprehensive testing framework

### Phase 2: Storage Layer (Months 3-4)
**Objectives**: Tiered storage implementation
**Deliverables**:
- [ ] Direct I/O WAL implementation
- [ ] S3/GCS object storage backend
- [ ] Basic caching system with LRU eviction
- [ ] Upload/download pipeline with retry logic
- [ ] Integration tests with MinIO

### Phase 3: Core Messaging (Months 5-6)
**Objectives**: Message processing and replication
**Deliverables**:
- [ ] Complete message lifecycle implementation
- [ ] Leader-follower replication system
- [ ] Partition management and metadata tracking
- [ ] Basic load balancing logic
- [ ] Performance benchmarks

### Phase 4: Network Layer (Months 7-8)
**Objectives**: Client protocols and APIs
**Deliverables**:
- [ ] QUIC/HTTP3 client protocol implementation
- [ ] gRPC internal communication
- [ ] Message routing and client coordination
- [ ] Authentication and authorization framework
- [ ] Client compatibility testing

### Phase 5: Advanced Features (Months 9-10)
**Objectives**: Extensibility and optimization
**Deliverables**:
- [ ] WebAssembly ETL runtime
- [ ] Advanced compaction strategies
- [ ] Comprehensive monitoring and metrics
- [ ] Hot module reloading for WASM
- [ ] Performance optimization and tuning

### Phase 6: Production Readiness (Months 11-12)
**Objectives**: Deployment and operations
**Deliverables**:
- [ ] Production deployment automation
- [ ] Comprehensive admin APIs and tooling
- [ ] Migration tools and documentation
- [ ] Security hardening and compliance features
- [ ] Operational runbooks and training materials

## 11. Configuration and Deployment

### 11.1 Broker Configuration

```toml
[broker]
id = "broker-001"
rack_id = "us-east-1a"

[network]
quic_listen = "0.0.0.0:9092"
rpc_listen = "0.0.0.0:9093"

[wal]
path = "/dev/nvme1n1"
capacity_bytes = 10737418240  # 10GB
fsync_on_write = true

[cache]
write_cache_size_bytes = 1073741824   # 1GB
read_cache_size_bytes = 2147483648    # 2GB

[object_storage]
type = "s3"
bucket = "rustmq-data"
region = "us-east-1"
endpoint = "https://s3.us-east-1.amazonaws.com"

[controller]
endpoints = ["controller-1:9094", "controller-2:9094", "controller-3:9094"]
```

### 11.2 Controller Configuration

```toml
[controller]
node_id = "controller-1"
raft_listen = "0.0.0.0:9095"
rpc_listen = "0.0.0.0:9094"
http_listen = "0.0.0.0:9642"

[raft]
peers = [
    "controller-1@controller-1:9095",
    "controller-2@controller-2:9095", 
    "controller-3@controller-3:9095"
]

[metastore]
type = "etcd"
endpoints = ["etcd-1:2379", "etcd-2:2379", "etcd-3:2379"]

[autobalancer]
enabled = true
cpu_threshold = 0.80
memory_threshold = 0.75
cooldown_seconds = 300
```

### 11.3 Deployment Architecture

#### **Kubernetes Deployment**
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: rustmq-broker
spec:
  serviceName: rustmq-broker
  replicas: 3
  template:
    spec:
      containers:
      - name: broker
        image: rustmq/broker:latest
        resources:
          requests:
            memory: "4Gi"
            cpu: "2"
          limits:
            memory: "8Gi"
            cpu: "4"
        volumeMounts:
        - name: wal-storage
          mountPath: /var/lib/rustmq/wal
        env:
        - name: RUSTMQ_BROKER_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
  volumeClaimTemplates:
  - metadata:
      name: wal-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 50Gi
      storageClassName: fast-ssd
```

### 11.4 Production Considerations

#### **Security**
- TLS 1.3 everywhere with automatic certificate management
- Fine-grained RBAC for topics and operations
- Comprehensive audit logging
- Network policies and encryption in transit

#### **Monitoring**
- OpenTelemetry integration for distributed tracing
- Prometheus metrics with Grafana dashboards
- Custom alerting rules for critical system events
- Real-time performance monitoring

#### **Backup and Recovery**
- Object storage provides built-in durability (11 9's for S3)
- Metadata backup strategies for etcd/FoundationDB
- Point-in-time recovery capabilities
- Cross-region replication for disaster recovery

## Conclusion

RustMQ represents a fundamental reimagining of distributed messaging systems for the cloud era. By combining Rust's performance and safety characteristics with a cloud-native shared-storage architecture, RustMQ delivers unprecedented cost savings, operational simplicity, and elastic scalability while maintaining the high-performance characteristics required for mission-critical workloads.

The architecture synthesizes lessons learned from Apache Kafka, AutoMQ, and modern cloud-native systems to create a platform that is not just incrementally better, but represents a step-change in what's possible for messaging infrastructure. With its emphasis on stateless compute, intelligent caching, and seamless cloud integration, RustMQ is positioned to become the messaging backbone for the next generation of cloud-native applications.

This design provides a complete technical specification for implementation, balancing innovative architectural choices with practical engineering considerations to deliver a system that is both cutting-edge and production-ready.