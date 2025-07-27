# RustMQ: A Cloud-Native, High-Performance Messaging System

## Part 1: Product Design

### 1.1. Vision and Value Proposition

#### Core Vision

The vision for RustMQ is to engineer a distributed messaging and event streaming platform built from first principles for the cloud-native era. RustMQ will provide the powerful, durable, and time-tested log-based semantics of Apache Kafka, while fundamentally re-architecting its storage and compute layers to achieve the elasticity, performance, and cost-efficiency that modern cloud environments demand. It is designed to be the definitive high-performance messaging backbone for applications deployed on cloud infrastructure, eliminating the architectural compromises and operational burdens inherent in legacy systems.

#### Value Proposition

RustMQ's primary value proposition is to deliver extreme throughput and predictable, single-digit millisecond P99 latency at a fraction of the total cost of ownership (TCO) of traditional Apache Kafka deployments. This is achieved by strategically decoupling compute and storage.

The platform's design directly addresses the core limitations of Kafka in the cloud, such as the tight coupling of processing power and storage capacity. By adopting a shared storage architecture, RustMQ leverages low-cost object storage (e.g., Amazon S3, Google Cloud Storage) as its primary data repository. This enables near-infinite, affordable data retention and independent scaling of stateless compute brokers.

Furthermore, RustMQ eliminates one of the most significant costs of running Kafka in a multi-availability zone (multi-AZ) configuration: cross-AZ data replication traffic. By delegating data durability to the underlying cloud storage services, RustMQ removes the need for synchronous, application-level replication between brokers. This architectural shift not only slashes network costs but also simplifies the system, boosts write throughput, and enables unprecedented operational agility.

### 1.2. Core Features and Capabilities

RustMQ is designed to deliver a superior experience across four key pillars: performance, elasticity, cost-effectiveness, and developer-centric operations.

#### Performance

*   **Single-Digit Millisecond Latency:** The write path is optimized for ultra-low latency using a Write-Ahead Log (WAL) on high-performance block storage (e.g., local NVMe) and employing Direct I/O.
*   **Extreme Throughput:** Throughput scales linearly with the addition of broker nodes. The elimination of in-band data replication means that a broker's network bandwidth is dedicated entirely to client traffic.
*   **Zero-Copy Data Path:** For consumers, RustMQ will leverage zero-copy principles extensively, using memory-mapped files and system calls like `sendfile` to transfer data from the broker's cache directly to the network socket, minimizing CPU overhead.
*   **Workload Isolation:** Read and write workloads are completely isolated. Historical "catch-up" reads from object storage will not impact the performance of real-time "tailing" reads or new message ingestion, preventing page cache pollution.

#### Elasticity & Scalability

*   **Stateless Brokers and Instant Scaling:** Broker nodes in RustMQ are stateless. This allows new brokers to be added to a cluster and become productive in seconds, as there is no data to copy.
*   **Continuous Auto-Balancing:** A built-in, controller-driven process monitors broker load and automatically reassigns partition leadership to eliminate hotspots and evenly distribute traffic.
*   **Independent Scaling of Compute and Storage:** The architecture fundamentally separates compute resources (brokers) from storage resources (object store), allowing them to be scaled independently.

#### Cost-Effectiveness

*   **Object Storage as Primary Tier:** The vast majority of data will reside in low-cost, pay-as-you-go object storage, making long-term data retention economically feasible.
*   **Elimination of Replication Traffic:** The architecture completely eliminates cross-AZ data replication traffic, leading to massive cost savings on networking.
*   **Spot Instance Compatibility:** Because brokers are stateless, they are ideal candidates for running on significantly cheaper cloud Spot or Preemptible instances.

#### Developer Experience & Operations

*   **Kafka API Compatibility:** RustMQ will be 100% compatible with the Apache Kafka producer and consumer protocol, ensuring seamless migration for existing applications.
*   **Embeddable Stream Processing (Wasm):** The platform will allow developers to embed custom data transformation logic directly into the message broker using WebAssembly (Wasm), written in languages like Rust.
*   **Comprehensive Admin APIs:** A modern, RESTful and gRPC Admin API will provide programmatic control over every aspect of the cluster.
*   **Simplified Day-2 Operations:** Automated balancing, rapid scaling, and self-healing capabilities drastically reduce operational complexity.

### 1.3. Target Use Cases

*   **Real-time Analytics & Data Lakehouse Architectures:** Act as the high-throughput, low-latency ingestion layer for data lakes and lakehouses, serving as the "streaming source of truth."
*   **Event-Driven Architectures & Microservices:** Provide a highly available and resilient message bus that can gracefully handle "spiky" traffic patterns common in microservice environments.
*   **Internet of Things (IoT) & Telemetry:** Efficiently handle the high-volume, high-cardinality firehose of telemetry data from millions of IoT endpoints.
*   **Financial Services:** Meet the stringent requirements for processing market data feeds, trade orders, and transaction logs, which demand both ultra-low latency and uncompromising data durability.

### 1.4. Architectural Comparison

| Feature / Aspect      | Apache Kafka (Shared-Nothing)                                                              | RustMQ (Proposed Design)                                                                                                                                      |
| :-------------------- | :----------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Storage Model**     | Tightly coupled compute and local disk storage.                                            | **Fundamentally Decoupled:** Local NVMe for a high-performance WAL, with Object Storage (S3/GCS) as the primary, infinite log storage tier.                     |
| **Data Replication**  | Synchronous, in-band replication between broker replicas, generating costly cross-AZ traffic. | **No Inter-Broker Replication:** Durability is delegated entirely to the cloud storage layers. Cross-AZ replication traffic costs are eliminated.                |
| **Elasticity & Scaling** | Slow, complex, and disruptive. Scaling out requires copying massive amounts of partition data. | **Instantaneous Elasticity:** Stateless brokers scale in seconds. An integrated, continuous auto-balancer shifts traffic via metadata-only partition reassignments. |
| **Primary Cost Drivers** | Over-provisioned compute, expensive replicated disks, and high cross-AZ network fees.       | **Optimized Cloud Spend:** On-demand or Spot compute, minimal local NVMe for the WAL, and cheap object storage for the main log.                               |
| **Failure Recovery**  | Controller-led leader re-election. Subsequent data re-replication is slow and resource-intensive. | **Metadata-Driven Failover:** The controller detects failure and reassigns partition leadership to any available broker in seconds. Recovery is near-instantaneous. |

---

## Part 2: Technical Architecture and Implementation Blueprint

### 2.1. System Architecture Overview

RustMQ implements a cloud-native messaging architecture that separates compute from storage, enabling unprecedented scalability and cost efficiency. The architecture is organized into a **Control Plane** (for cluster state and coordination) and a **Data Plane** (for high-performance message transport).

```mermaid
graph TD
    subgraph "Client Layer"
        Producer[Producer App]
        Consumer[Consumer App]
        Admin[Admin CLI/UI]
    end

    subgraph "RustMQ Cluster (Data Plane)"
        B1[Broker 1<br>(Stateless)]
        B2[Broker 2<br>(Stateless)]
        B3[Broker N<br>(Stateless)]
    end

    subgraph "RustMQ Cluster (Control Plane)"
        Controller[Controller Quorum<br>(Raft Consensus)]
    end

    subgraph "Persistence Layer"
        Metastore[(External Metastore <br> e.g., etcd)]
        ObjectStore[Cloud Object Storage <br> (S3 / GCS)]
        WAL_Storage[High-Performance Block Storage <br> (Local NVMe / EBS)]
    end

    Producer -- "Produce (QUIC)" --> B1
    Consumer -- "Fetch (QUIC)" --> B2
    Admin -- "Manage (gRPC/REST)" --> Controller

    B1 <-->|Internal RPC (gRPC)| B2
    B2 <-->|Internal RPC (gRPC)| B3

    B1 -- "Append" --> WAL_Storage
    B2 -- "Append" --> WAL_Storage
    B3 -- "Append" --> WAL_Storage

    B1 -- "Upload/Fetch Segments" --> ObjectStore
    B2 -- "Upload/Fetch Segments" --> ObjectStore
    B3 -- "Upload/Fetch Segments" --> ObjectStore

    Controller -- "Read/Write Metadata" --> Metastore
    B1 -- "Heartbeat/Watch" --> Controller
    B2 -- "Heartbeat/Watch" --> Controller
    B3 -- "Heartbeat/Watch" --> Controller
```

### 2.2. Cluster Architecture and Node Roles

#### Broker Nodes (Rust)

*   **Responsibilities:** The Broker is the workhorse of the data plane. It handles all client network connections, manages the high-performance write path to the WAL, orchestrates the asynchronous offloading of data to object storage, and serves all read requests.
*   **Stateless Nature:** A broker holds no permanent state associated with any specific partition. Any broker can assume leadership for any partition at any time.
*   **Implementation:** A multi-threaded, asynchronous Rust application leveraging the `tokio` runtime.

#### Controller Quorum (Rust)

*   **Responsibilities:** The Controller Quorum is the brain of the cluster. It tracks broker health, manages all cluster metadata, assigns partition leadership, and runs the continuous load-balancing algorithm.
*   **Consensus:** To ensure fault tolerance, controllers operate as a quorum (3 or 5 nodes) using the Raft consensus algorithm (`raft-rs` crate) to replicate their state and elect a leader.

### 2.3. Metadata Management

#### External Metastore: etcd

RustMQ will use an external, strongly consistent, distributed key-value store like **etcd** to persist all cluster metadata. This provides a clean separation of concerns and allows the metastore to be managed and scaled independently.

#### Metadata Schema

The following schema will be used to store all critical cluster state in etcd. Values will be serialized using Protobuf.

| Key Path (etcd)                                                      | Value (Serialized)                                                                                             | Description                                                                                                                            |
| :------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------- |
| `/brokers/registered/{broker_id}`                                    | `{ "host": "10.0.1.23", "port_quic": 9092, "port_rpc": 9093, "rack_id": "us-central1-a" }`                       | Contains network endpoint and availability zone for each live broker.                                                                  |
| `/topics/{topic_name}/config`                                        | `{ "partitions": 64, "retention_ms": -1, "wasm_module_id": "pii_scrubber_v1" }`                                  | Defines the configuration for a topic.                                                                                                 |
| `/topics/{topic_name}/partitions/{partition_id}/leader`              | `{ "broker_id": "broker-007", "leader_epoch": 142 }`                                                           | The current leader broker and the monotonically increasing leader epoch for a specific partition.                                      |
| `/topics/{topic_name}/partitions/{partition_id}/segments/{start_offset}` | `{ "status": "IN_WAL" | "UPLOADING" | "IN_OBJECT_STORE", "object_key": "...", "size_bytes": ... }` | Tracks the state and location of every log segment, enabling brokers to find data in the WAL or object storage.                  |
| `/consumer_groups/{group_id}/offsets/{topic_name}/{partition_id}`    | `{ "offset": 45678, "metadata": "..." }`                                                                       | Stores the last committed offset for each partition consumed by a consumer group.                                                      |
| `/wasm/modules/{module_id}`                                          | `{ "name": "pii_scrubber", "version": "v1", "object_key": "wasm/pii_scrubber_v1.wasm" }`                         | Metadata for registered WebAssembly modules, including their location in object storage.                                               |

### 2.4. The RustMQ Storage Engine

The storage engine is a two-tier system designed for both low-latency writes and cost-effective, infinite storage.

#### Tier 1: The Low-Latency Write-Ahead Log (WAL)

*   **Storage Medium:** Each broker node will be provisioned with a dedicated, high-performance block storage device (e.g., local NVMe SSD).
*   **Implementation Strategy:** The WAL will be implemented using **Direct I/O** to bypass the OS page cache, preventing cache pollution from cold reads. The implementation will leverage the `tokio-uring` crate, a safe abstraction over the Linux kernel's `io_uring` interface, to maximize IOPS.
*   **Data Structure:** The WAL on the block device will be a single, large circular buffer. All partitions for which a broker is the current leader will append their records into this one shared log file, which is highly efficient for block devices.
*   **Durability Guarantee:** A producer `acks=all` request is only acknowledged after the WAL's underlying block device confirms the write has been durably persisted.

#### Tier 2: The Infinite Log on Object Storage

*   **Offloading Process:** Background tasks on each broker monitor the WAL, read committed data segments, batch them intelligently, and upload them to the configured object storage bucket.
*   **Object Management Strategy:** To avoid the high cost and latency of many small file uploads, RustMQ will adopt a sophisticated object compaction strategy:
    *   **Stream Objects:** For high-throughput topics, data from a single partition is aggregated into large (e.g., 1 GB) "Stream Objects."
    *   **Stream Set Objects:** For low-throughput topics, data from multiple partitions is merged into a single "Stream Set Object" to reduce the number of PUT requests and lower API costs.
*   **Transactional Metadata Update:** After a segment is successfully uploaded, the broker issues an RPC to the controller, which transactionally updates the segment's entry in etcd, changing its status from `IN_WAL` to `IN_OBJECT_STORE`. Only then is the space in the local WAL marked as reusable.

### 2.5. Data Flow, Caching, and High Availability

#### Write Path (Producer `acks=all`)

1.  A client sends a `ProduceRequest` to the leader broker for the target partition.
2.  The leader appends the record batch to its shared WAL on the local block device using an `io_uring` operation and concurrently writes to an in-memory `WriteCache`.
3.  The block storage device confirms the write is durable.
4.  The broker immediately sends a `ProduceResponse` to the producer.
5.  **Asynchronously**, a background task offloads the data from the WAL to object storage.

#### Read Path and Workload Isolation

RustMQ implements two distinct, non-interfering cache systems within each broker to prevent page cache pollution:

*   **Hot Read (Tailing Consumer):** A consumer fetching recent data is served directly from the in-memory **WriteCache**. This is extremely fast and involves no disk I/O.
*   **Cold Read (Catch-up Consumer):** A consumer requesting old data triggers a fetch from the object store. The data is streamed back, passed to the consumer, and simultaneously written into a separate, dedicated **ReadCache** (with an LRU eviction policy). This operation does not touch the `WriteCache`, guaranteeing stable low latency for real-time workloads.

#### High Availability & Failover Model

The HA model is dramatically simpler and faster than Kafka's.

1.  **Failure Detection:** The active Controller node detects a broker failure via missed heartbeats.
2.  **Leader Re-election:** The Controller immediately selects a healthy, available broker to become the new leader for all affected partitions.
3.  **Metadata Update:** The Controller executes a transactional update in etcd, changing the `leader_broker_id` and incrementing the `leader_epoch`.
4.  **Client Redirection:** Clients receive an error, refresh their metadata to discover the new leader, and seamlessly reconnect. The entire failover process is a metadata-only operation that completes in seconds.

### 2.6. Network Protocols and APIs

#### Client-Broker Protocol: QUIC / HTTP/3

*   **Rationale:** QUIC is used for client-broker communication to reduce connection latency (0-RTT/1-RTT handshakes), eliminate Head-of-Line (HOL) blocking, and support seamless connection migration.
*   **Rust Implementation:** The `quinn` and `h3` crates will be used to build the client-facing endpoint.

#### Inter-Node RPC Protocol: gRPC

*   **Rationale:** gRPC is used for all internal communication (Controller-to-Broker, Broker-to-Broker). It offers high performance with Protocol Buffers and provides strongly-typed API contracts, ideal for a distributed system.
*   **Rust Implementation:** The `tonic` crate will be used to build the internal RPC services.

### 2.7. Elasticity and Load Management

#### Auto-Balancing Controller

The auto-balancing logic runs continuously within the active Controller node.

*   **Algorithm Design:** The balancer implements a dynamic, weighted, streaming partitioning algorithm.
    1.  **Metric Ingestion:** The controller aggregates real-time metrics (CPU, network, WAL writes) from all brokers via heartbeats.
    2.  **Cost Function:** It maintains a dynamic "cost" for hosting each partition on its current leader.
    3.  **Imbalance Detection:** A rebalancing cycle is triggered if a broker's load exceeds a threshold or if the load variance across the cluster is too high.
    4.  **Greedy Migration:** The balancer identifies the most "expensive" partitions on the most loaded brokers and calculates the optimal move to a less-loaded broker.
    5.  **Execution:** It executes the move by issuing `resignLeader` and `becomeLeader` RPCs. The move is a fast, metadata-only operation.

#### Auto-Scaling Integration

*   **Scale-Out:** When a new broker instance joins, the auto-balancer automatically detects it and gradually migrates partitions to the new node until the load is balanced.
*   **Scale-In:** When an instance is scheduled for termination, a lifecycle hook notifies the controller, which puts the broker in a `DRAINING` state. The auto-balancer gracefully migrates all partitions off the draining node before signaling that it is safe to terminate the instance.

### 2.8. Extensibility: In-Stream Processing with WebAssembly (Wasm)

*   **Architecture and Workflow:**
    1.  **Module Management:** A developer compiles their transformation logic (e.g., in Rust) to a `.wasm` binary and uploads it via the Admin API. The controller stores the module in object storage and its metadata in etcd.
    2.  **Topic Configuration:** A topic is configured to associate a specific Wasm module with its data stream.
    3.  **Execution:** When a leader broker receives a message for a Wasm-enabled topic, it invokes a sandboxed Wasm runtime (`wasmtime`) before writing the message to the WAL.
    4.  **Transformation:** The broker calls an exported function from the Wasm module (e.g., `fn process_message(message: &[u8]) -> Vec<u8>`), passing the raw message and receiving the transformed message. The transformed message is then written to the log.

### 2.9. Administrative and Operational Tooling

A comprehensive, modern administrative API will be exposed via both gRPC and a RESTful gateway.

#### Admin API Endpoints

*   **Topic Management:** `CreateTopic`, `DeleteTopic`, `DescribeTopic`, `UpdateTopicConfig`.
*   **Cluster Management:** `DescribeCluster`, `ListBrokers`, `TriggerRebalance`.
*   **Consumer Group Management:** `DescribeConsumerGroup`, `ResetOffset`.
*   **Wasm Module Management:** `UploadWasmModule`, `AssignWasmToTopic`, `ListWasmModules`.

---

## Appendix

### A.1. Configuration Parameters

#### Broker Configuration (broker.toml)

| Parameter                  | Type    | Default             | Description                                                                                             |
| :------------------------- | :------ | :------------------ | :------------------------------------------------------------------------------------------------------ |
| `broker.id`                | String  | (generated)         | A unique identifier for the broker node.                                                                |
| `controller.endpoints`     | Array   | `["localhost:9093"]`  | A list of `host:port` addresses for the Controller quorum's RPC interface.                               |
| `listen.quic`              | String  | `0.0.0.0:9092`      | The network address and port for the client-facing QUIC listener.                                       |
| `listen.rpc`               | String  | `0.0.0.0:9093`      | The network address and port for the internal gRPC listener.                                            |
| `rack.id`                  | String  | (none)              | The availability zone or rack identifier for this broker. Essential for rack-aware balancing.           |
| `wal.path`                 | String  | `/var/lib/rustmq/wal` | The path to the raw block device or file to be used for the Write-Ahead Log.                              |
| `wal.capacity.bytes`       | Int64   | 10737418240 (10 GB) | The total capacity of the WAL.                                                                          |
| `cache.write.size.bytes`   | Int64   | (10% of RAM)        | The size of the in-memory cache for hot writes and tailing reads.                                       |
| `cache.read.size.bytes`    | Int64   | (25% of RAM)        | The size of the in-memory cache for cold data fetched from object storage.                              |
| `object_store.type`        | String  | `s3`                | The type of object store to use. Supported values: `s3`, `gcs`.                                         |
| `object_store.s3.bucket`   | String  | (required)          | The name of the S3 bucket to use for primary log storage.                                               |
| `object_store.s3.region`   | String  | `us-central1`       | The GCS region of the bucket.                                                                           |

#### Controller Configuration (controller.toml)

| Parameter                         | Type    | Default                | Description                                                                                             |
| :-------------------------------- | :------ | :--------------------- | :------------------------------------------------------------------------------------------------------ |
| `node.id`                         | String  | (required)             | A unique identifier for this controller node within the quorum.                                         |
| `listen.rpc`                      | String  | `0.0.0.0:9093`         | The network address and port for the internal gRPC listener for broker communication.                   |
| `listen.raft`                     | String  | `0.0.0.0:9094`         | The network address and port used for Raft consensus communication between controller nodes.            |
| `listen.http`                     | String  | `0.0.0.0:9642`         | The network address and port for the public-facing administrative REST API.                             |
| `raft.peers`                      | Array   | ``                     | A list of `node_id@host:port` for all other peers in the Raft quorum.                                     |
| `metastore.endpoints`             | Array   | `["localhost:2379"]`   | A list of `host:port` addresses for the external etcd cluster.                                          |
| `autobalancer.enable`             | Boolean | `true`                 | Toggles the automatic load balancing feature.                                                           |
| `autobalancer.trigger.cpu.threshold` | Float   | `0.80`                 | The CPU utilization threshold (0.0 to 1.0) that will trigger a rebalancing cycle.                       |
| `autobalancer.cooldown.seconds`   | Int     | `300`                  | The minimum time to wait between consecutive rebalancing decisions to prevent thrashing.                |
