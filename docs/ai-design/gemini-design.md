# RustMQ: A Cloud-Native, High-Performance Messaging System

## 1. Product Section

### 1.1. Vision
To provide a next-generation, distributed messaging system that offers extreme performance, scalability, and operational efficiency. RustMQ is designed from the ground up for the cloud era, combining the low-latency benefits of local disk with the infinite scalability and cost-effectiveness of object storage, all while ensuring the memory safety and concurrency advantages of Rust.

### 1.2. Core Principles
*   **Performance First:** Leverage Rust's low-level control and async ecosystem to achieve maximum throughput and minimal latency. Employ zero-copy data paths wherever feasible.
*   **Cloud-Native Elasticity:** Natively integrate with cloud object storage (AWS S3, GCP GCS) for long-term data retention, allowing for a small, fixed-size local cache and enabling rapid, cost-effective horizontal scaling.
*   **Cost-Effectiveness:** Decouple compute from storage. By offloading the bulk of data to cheap object storage, the compute cluster (brokers) can be scaled independently based on real-time load, drastically reducing operational costs compared to traditional systems that require large, expensive persistent disks.
*   **Reliability and Safety:** Utilize Rust's strict compile-time guarantees to eliminate entire classes of bugs. Implement a robust, leader-based replication model to ensure high availability and data durability.
*   **Developer-Friendly Extensibility:** Provide a safe, sandboxed environment for embedding custom data transformation logic directly into the message pipeline, enabling real-time, single-message ETL without external stream processing systems.

### 1.3. Key Features
*   **Tiered Storage:** A small, fast local disk layer acts as a write-ahead log (WAL) and read cache, while the vast majority of data resides in a cost-effective object storage tier.
*   **Infinite Retention:** Store data for any duration without the prohibitive cost of attached block storage, limited only by the capacity of your cloud object store.
*   **Horizontal Scalability:** Scale the broker cluster up or down in minutes to match demand, without complex data rebalancing operations for the entire dataset.
*   **High Availability:** A minimum of three nodes ensures no single point of failure, with automatic leader election and configurable write-acknowledgment for tunable durability.
*   **Modern Networking:** Utilizes QUIC (HTTP/3) for efficient, multiplexed client communication, reducing head-of-line blocking and connection overhead.
*   **Embedded Message Transforms:** Safely run user-provided Rust code compiled to WebAssembly (WASM) on each message, enabling powerful, real-time data manipulation.
*   **Comprehensive Admin APIs:** Manage and monitor the entire cluster through a clean, powerful set of gRPC and RESTful APIs.

### 1.4. Target Use Cases
*   **High-Throughput Log Aggregation:** Collect logs from thousands of services and store them indefinitely for analysis and security monitoring.
*   **Real-Time Event Sourcing:** Use RustMQ as the central source of truth for event-driven microservices architectures.
*   **Data Streaming for Analytics:** Feed massive data streams into analytics engines, data lakes, and feature stores for machine learning.
*   **IoT Data Ingestion:** Handle high-volume, high-cardinality data streams from IoT devices at the edge.

---

## 2. Technical Section

### 2.1. Architectural Overview

#### 2.1.1. Core Components

1.  **Broker:** The workhorse of the system. A Broker node is stateless regarding long-term data but manages a local disk cache. Its responsibilities include:
    *   Handling client produce/consume requests.
    *   Writing new messages to a local Write-Ahead Log (WAL).
    *   Serving reads from its local cache.
    *   Transparently fetching older data from the object storage tier if a read request misses the local cache.
    *   Uploading sealed data segments from its local WAL to object storage.
    *   Participating in data replication as a leader or follower for assigned partitions.
    *   Executing WASM-based message transforms.

2.  **Controller:** A cluster-wide singleton (with standby replicas for HA) responsible for cluster coordination. Its responsibilities include:
    *   Managing broker registration and health checks.
    *   Performing leader election for partitions when a broker fails.
    *   Storing and managing all cluster metadata (topic configurations, partition assignments, ISR lists) in the Metastore.
    *   Orchestrating dynamic partition rebalancing to maintain cluster health.
    *   Providing Admin APIs for cluster management.

3.  **Metastore:** An external, highly-available, and strongly consistent key-value store. It is the source of truth for all system metadata.
    *   **Recommended Implementations:** `etcd` or `FoundationDB`. These provide the necessary features like transactional multi-key updates and watch capabilities that the Controller relies on.
    *   **Responsibilities:** Persistently store topic definitions, partition assignments, broker information, and the index mapping data segments to their location in object storage.

#### 2.1.2. System Architecture Diagram

```mermaid
graph TD
    subgraph "Client Layer"
        Producer[Producer App]
        Consumer[Consumer App]
        Admin[Admin CLI/UI]
    end

    subgraph "RustMQ Cluster"
        subgraph "Broker Node 1"
            B1[Broker 1] -->|Writes/Reads| LD1[Local Disk Cache]
        end
        subgraph "Broker Node 2"
            B2[Broker 2] -->|Writes/Reads| LD2[Local Disk Cache]
        end
        subgraph "Broker Node 3"
            B3[Broker 3] -->|Writes/Reads| LD3[Local Disk Cache]
        end

        Controller[Controller]
    end

    subgraph "Persistence Layer"
        Metastore[(External Metastore <br> e.g., etcd)]
        ObjectStore[Cloud Object Storage <br> (S3 / GCS)]
    end

    Producer -- "Produce (QUIC)" --> B1
    Consumer -- "Fetch (QUIC)" --> B2
    Admin -- "Manage (gRPC/REST)" --> Controller

    B1 <-->|Replication (gRPC)| B2
    B2 <-->|Replication (gRPC)| B3
    B3 <-->|Replication (gRPC)| B1

    B1 -- "Upload/Fetch Segments" --> ObjectStore
    B2 -- "Upload/Fetch Segments" --> ObjectStore
    B3 -- "Upload/Fetch Segments" --> ObjectStore

    Controller -- "Read/Write Metadata" --> Metastore
    B1 -- "Heartbeat/Watch" --> Controller
    B2 -- "Heartbeat/Watch" --> Controller
    B3 -- "Heartbeat/Watch" --> Controller
```

### 2.2. Tiered Storage Subsystem

#### 2.2.1. Local Disk Layer (Fast Tier)
This layer is optimized for extremely fast writes and caching recent data for low-latency reads.

*   **Data Format:** A partition's data on a broker is stored as a sequence of segment files. A segment is a simple binary log file. Messages are appended with a format of `[8-byte length][N-byte payload]`. The payload contains the message headers and value.
*   **Indexing:** For each `*.log` segment file, a corresponding `*.index` file is created. The index maps a logical message offset to its physical byte position in the log file. This allows for efficient seeking without scanning the entire log file. The index is a memory-mapped file for performance.
*   **Write-Ahead Log (WAL):** All incoming writes are immediately appended to the active segment file and fsync'd to disk (configurable), ensuring durability before an acknowledgment is sent to the producer. This makes the local disk the system's WAL.

#### 2.2.2. Object Storage Layer (Infinite Tier)
This layer provides durable, scalable, and cost-effective long-term storage.

*   **Upload Strategy:**
    1.  **Seal Segment:** When a local segment file reaches a configured size (e.g., 256MB) or age, the broker "seals" it, making it immutable, and opens a new active segment for writes.
    2.  **Reliable Upload:** The sealed segment (`.log` and `.index` files) is uploaded to a designated path in the object store (e.g., `s3://bucket/topic/partition/start_offset.log`). We will use the cloud provider's SDKs (e.g., `rusoto` for AWS, `gcloud-sdk` for GCP) which handle multipart uploads, checksums, and retries to ensure data integrity.
    3.  **Update Metastore:** Upon successful upload confirmation from the cloud provider, the broker updates a metadata entry for the segment in the Metastore. This entry marks the segment as "available in object storage" and stores its URI. This is a critical atomic step managed by the Controller.
*   **Data Retrieval:** If a consumer requests an offset that is not in the local cache, the broker finds the corresponding segment URI in its metadata, downloads the segment from object storage on-demand, caches it locally, and serves the request.

#### 2.2.3. Local Cache Management Policy
The local disk acts as a cache. Its size is fixed and relatively small (e.g., 10-50 GB).
*   **Eviction Policy:** Once a segment has been successfully uploaded to object storage and its metadata updated, it becomes a candidate for eviction. The broker will use a simple Least Recently Used (LRU) policy. When local disk usage exceeds a high-water mark (e.g., 85% capacity), the broker will delete the oldest, sealed segments from local disk until usage drops below a low-water mark (e.g., 75%). The active segment is never evicted.

### 2.3. Data Replication & High Availability (HA)

*   **Partition & Replica Model:** Each topic partition has a configurable replication factor (minimum 3). The Controller assigns one broker as the **leader** and the others as **followers**. All writes and reads for a partition go to the leader.
*   **In-Sync Replica (ISR) Set:** A subset of followers that are fully caught up with the leader's log. The Controller manages this set.
*   **Write Path & Acknowledgment (`acks`):**
    1.  A producer sends a message to the partition leader.
    2.  The leader writes the message to its local WAL.
    3.  The leader forwards the message to all followers in the ISR set.
    4.  Followers write the message to their local WALs and send an acknowledgment back to the leader.
    5.  The leader sends an acknowledgment to the producer based on the `acks` setting:
        *   `acks=0`: Producer gets an immediate ack (fire-and-forget).
        *   `acks=1`: Producer gets an ack after the leader writes to its local WAL (default).
        *   `acks=all`: Producer gets an ack only after the leader receives acks from all replicas in the ISR set. This provides the strongest durability guarantee.
*   **Leader Election:** The Controller monitors broker health via heartbeats. If a leader fails, the Controller immediately promotes one of the followers from the ISR set to be the new leader and updates the metadata. Clients are notified of the new leader and redirect their requests.

### 2.4. Networking & Protocols

#### 2.4.1. Client-to-Broker Protocol: QUIC/HTTP3
*   **Justification:** We will use QUIC as the primary client-facing protocol.
    *   **Reduced Latency:** QUIC's 0-RTT or 1-RTT handshakes are faster than TCP+TLS.
    *   **No Head-of-Line Blocking:** A lost packet for one stream (e.g., a request for partition A) does not block other streams (a request for partition B) on the same connection. This is critical for high-performance messaging.
    *   **Connection Migration:** Seamlessly handles clients switching networks (e.g., Wi-Fi to cellular).
*   **Implementation:** The `quinn` crate in Rust provides a robust, async-native implementation of the QUIC protocol.

#### 2.4.2. Internal RPC Protocol: gRPC
*   **Justification:** For internal communication (Broker-to-Broker replication, Controller-to-Broker commands), we will use gRPC.
    *   **Performance:** gRPC uses Protocol Buffers, a highly efficient binary serialization format.
    *   **Strong Typing:** The service definitions (`.proto` files) create a clear, strongly-typed contract for all internal APIs, reducing integration errors.
    *   **Streaming Support:** Native support for bidirectional streaming is ideal for data replication and health checks.
*   **Implementation:** The `tonic` crate is the standard for building high-performance gRPC services and clients in Rust.

### 2.5. Performance & Extensibility

#### 2.5.1. Zero-Copy Implementation in Rust
Zero-copy techniques are critical for maximizing throughput by avoiding redundant data copies between user space and kernel space.
*   **Key Data Path:** The most common path is from a producer's socket, to the broker's disk, and from the disk back to a consumer's socket.
*   **Rust Implementation:**
    *   **Producer to Broker Disk:** When reading from the network socket, data can be read directly into a page-aligned buffer that can be written to disk with minimal copying.
    *   **Broker Disk to Consumer:** This is the primary opportunity. We can use the `sendfile(2)` system call (available in Rust via the `nix` crate). `sendfile` instructs the kernel to send bytes directly from a file descriptor (our log segment file) to a socket descriptor (the consumer's connection) without ever copying the data into the application's user-space buffers.

#### 2.5.2. Single Message Transform (SMT) via WASM
This feature allows users to embed custom logic safely.
*   **Mechanism:**
    1.  **Develop & Compile:** A user writes a simple Rust function that takes a `&[u8]` (the message payload) and returns a `Vec<u8>` (the transformed payload). They compile this function to a WebAssembly (`.wasm`) module.
    2.  **Deploy:** The user uploads this `.wasm` module and associates it with a specific topic via the Admin API.
    3.  **Execution:** The Broker uses a WASM runtime like `wasmtime`. When a message for the associated topic is produced, the broker invokes the WASM function within a secure sandbox.
    4.  **Pipeline:** The transformed message is then written to the WAL and proceeds through the replication pipeline. This ensures that all consumers see the transformed data.
*   **Benefits:** This approach is extremely secure (WASM is sandboxed), performant (near-native speed), and prevents user code from crashing the broker.

### 2.6. Scalability & Management

#### 2.6.1. Dynamic Partition Rebalancing
*   **Algorithm:** The Controller continuously monitors load metrics (e.g., bytes-in/sec, message rate, partition size) for all partitions across all brokers.
    1.  **Trigger:** If a broker exceeds a load threshold for a sustained period, or if a new broker joins the cluster, the Controller initiates a rebalance.
    2.  **Plan:** The Controller calculates a new partition assignment plan to distribute the load more evenly.
    3.  **Execution:** For a partition `P` moving from `Broker A` to `Broker B`, the Controller executes the move gracefully:
        a. It instructs `Broker B` to become a follower for partition `P`.
        b. `Broker B` catches up fully with the leader's log.
        c. The Controller adds `Broker B` to the ISR set.
        d. The Controller promotes `Broker B` to be the new leader.
        e. It instructs `Broker A` to stop being a replica for `P`.

#### 2.6.2. Admin APIs
A comprehensive set of APIs will be exposed via gRPC (for programmatic access) and a RESTful gateway (for easy tooling and UI integration).
*   **Topic Management:** `CreateTopic`, `DeleteTopic`, `DescribeTopic`, `UpdateTopicConfig`.
*   **Cluster Management:** `DescribeCluster`, `ListBrokers`, `TriggerRebalance`.
*   **Subscription/Consumer Group Management:** `DescribeConsumerGroup`, `ResetOffset`.
*   **WASM Management:** `UploadWasmModule`, `AssignWasmToTopic`.

### 2.7. Metadata Management

*   **Schema:** The metadata stored in `etcd` will be structured logically.
    *   `/brokers/{broker_id}` -> Broker connection info and status.
    *   `/topics/{topic_name}/config` -> Topic-level configuration (replication factor, etc.).
    *   `/topics/{topic_name}/partitions/{partition_id}/state` -> Leader ID, ISR set.
    *   `/topics/{topic_name}/partitions/{partition_id}/segments/{start_offset}` -> Segment metadata, including its URI in object storage.
*   **Consistency:** The Controller will use transactions provided by the metastore to ensure atomic updates. For example, changing a partition leader requires atomically updating the leader ID and the ISR set in a single transaction. The `watch` mechanism will be used to react to changes, such as a broker's heartbeat expiring.
