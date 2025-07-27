

# **AstraStream: A Cloud-Native Distributed Messaging System**

## **Part 1: Product Design**

### **1.1. Vision and Value Proposition**

#### **Core Vision**

The vision for AstraStream is to engineer a distributed messaging and event streaming platform built from first principles for the cloud-native era. AstraStream will provide the powerful, durable, and time-tested log-based semantics of Apache Kafka, while fundamentally re-architecting its storage and compute layers to achieve the elasticity, performance, and cost-efficiency that modern cloud environments demand. It is designed to be the definitive high-performance messaging backbone for applications deployed on cloud infrastructure like AWS and GCP, eliminating the architectural compromises and operational burdens inherent in legacy systems.

#### **Value Proposition**

AstraStream's primary value proposition is to deliver extreme throughput and predictable, single-digit millisecond P99 latency at a fraction of the total cost of ownership (TCO) of traditional Apache Kafka deployments. This is not an incremental improvement but a step-change in efficiency, achieved by strategically decoupling compute and storage.

The platform's design directly addresses the core limitations of Kafka in the cloud, such as the tight coupling of processing power and storage capacity, which forces users into expensive, inflexible scaling decisions.1 By adopting a shared storage architecture inspired by innovators like AutoMQ, AstraStream leverages low-cost object storage (e.g., Amazon S3, Google Cloud Storage) as its primary data repository. This enables near-infinite, affordable data retention and independent scaling of stateless compute brokers.2

Furthermore, AstraStream eliminates one of the most significant and often hidden costs of running Kafka in a multi-availability zone (multi-AZ) configuration: cross-AZ data replication traffic.4 By delegating data durability to the underlying cloud storage services, which provide their own multi-AZ redundancy, AstraStream removes the need for synchronous, application-level replication between brokers. This architectural shift not only slashes network costs but also simplifies the system, boosts write throughput, and enables unprecedented operational agility. The result is a system that promises the 10x cost savings and 100x elasticity demonstrated to be achievable with this architectural model, making it the superior choice for cost-conscious, high-performance workloads.6

### **1.2. Core Features and Capabilities**

AstraStream is designed to deliver a superior experience across four key pillars: performance, elasticity, cost-effectiveness, and developer-centric operations.

#### **Performance**

* **Single-Digit Millisecond Latency:** The write path is optimized for ultra-low latency. By utilizing a Write-Ahead Log (WAL) on high-performance block storage (e.g., AWS EBS) and employing Direct I/O, AstraStream can acknowledge producer requests in single-digit milliseconds at the 99th percentile, a performance characteristic demonstrated by similar architectures.4  
* **Extreme Throughput:** Throughput scales linearly with the addition of broker nodes. The elimination of in-band data replication means that a broker's network bandwidth is dedicated entirely to client traffic, allowing smaller instances to handle significantly more write throughput compared to a Kafka broker of the same size.4  
* **Zero-Copy Data Path:** For consumers, AstraStream will leverage zero-copy principles extensively. By using memory-mapped files and system calls like sendfile (or their modern asynchronous equivalents), data can be transferred from the broker's cache directly to the network socket, bypassing unnecessary copies through user-space application buffers and minimizing CPU overhead.9  
* **Workload Isolation:** The system architecture guarantees that read and write workloads are completely isolated. Historical "catch-up" reads, which fetch data from the object storage tier, will not impact the performance of real-time "tailing" reads or the latency of new message ingestion. This is achieved by maintaining separate, dedicated memory caches for hot and cold data, a critical feature that prevents the page cache pollution that plagues traditional Kafka deployments during catch-up reads.4

#### **Elasticity & Scalability**

* **Stateless Brokers and Instant Scaling:** Broker nodes in AstraStream are stateless; they do not permanently own any partition data. This is the cornerstone of the system's elasticity. It allows new brokers to be added to a cluster and become productive in seconds, as there is no data to copy. Similarly, scaling down is just as fast. This contrasts sharply with Kafka, where rebalancing a single partition can take minutes or hours depending on its size.4  
* **Continuous Auto-Balancing:** AstraStream will feature a built-in, continuously operating self-balancing component. This controller-driven process monitors broker load in real-time and automatically reassigns partition leadership to eliminate hotspots and evenly distribute traffic. This automated capability removes a significant operational burden from human operators and ensures optimal resource utilization across the cluster.4  
* **Independent Scaling of Compute and Storage:** The architecture fundamentally separates compute resources (brokers) from storage resources (object store). This allows teams to scale their processing power to handle traffic spikes without being forced to scale their storage, and conversely, to grow their stored data volume to petabytes without needing to add more compute nodes.1

#### **Cost-Effectiveness**

* **Object Storage as Primary Tier:** The vast majority of data will reside in low-cost, pay-as-you-go object storage like Amazon S3 or Google Cloud Storage. This dramatically reduces storage costs compared to the replicated, high-performance block storage required by Kafka, making long-term data retention economically feasible.2  
* **Elimination of Replication Traffic:** In a typical multi-AZ Kafka deployment, writing 1 GB of data generates an additional 2 GB of cross-AZ replication traffic, which is extremely expensive. AstraStream's architecture completely eliminates this traffic, leading to massive cost savings on networking, often one of the largest components of a Kafka bill.4  
* **Spot Instance Compatibility:** Because brokers are stateless, they are ideal candidates for running on significantly cheaper cloud Spot (AWS) or Preemptible (GCP) instances. The system is designed to handle the transient nature of these instances gracefully, with the controller automatically reassigning partitions away from a node that receives a termination notice.4

#### **Developer Experience & Operations**

* **Kafka API Compatibility:** AstraStream will be 100% compatible with the Apache Kafka producer and consumer protocol. This is a critical feature that ensures existing applications, tools, and ecosystem components (like Kafka Connect, client libraries, etc.) can migrate to AstraStream with zero code changes, providing a seamless adoption path.2  
* **Embeddable Stream Processing (Wasm):** The platform will allow developers to embed custom data transformation logic directly into the message broker. By leveraging WebAssembly (Wasm), developers can write safe, sandboxed, high-performance ETL functions in languages like Rust, which are then applied to messages as they flow through a topic.13  
* **Comprehensive Admin APIs:** A modern, RESTful Admin API will provide programmatic control over every aspect of the cluster, from topic management to observing consumer group lag and managing Wasm modules.  
* **Simplified Day-2 Operations:** The combination of automated balancing, rapid scaling, and self-healing capabilities drastically reduces the operational complexity and manual tuning required to run the system at scale, freeing up engineering teams to focus on building applications rather than managing infrastructure.4

### **1.3. Target Use Cases**

AstraStream's unique combination of performance, cost-effectiveness, and scalability makes it an ideal platform for a wide range of modern, data-intensive use cases.

* **Real-time Analytics & Data Lakehouse Architectures:** AstraStream is perfectly suited to act as the high-throughput, low-latency ingestion layer for data lakes and lakehouses. Its ability to retain data indefinitely on object storage at low cost means it can serve as the "streaming source of truth." Data can be ingested once and then be made available for both real-time streaming analytics (e.g., Flink, Spark Streaming) and batch-oriented analytical queries directly against the object store (e.g., Athena, BigQuery, Snowflake). This eliminates the need for complex and brittle ETL pipelines that copy data from a short-term message queue to a long-term data store.  
* **Event-Driven Architectures & Microservices:** For organizations building applications around event-driven principles, AstraStream provides a highly available and resilient message bus. Its extreme elasticity allows it to gracefully handle the "spiky" traffic patterns common in microservice environments, such as sudden bursts of activity during a product launch or promotional event. The ability to scale compute resources in seconds without massive over-provisioning ensures that the system can meet demand without incurring exorbitant costs for idle capacity.  
* **Internet of Things (IoT) & Telemetry:** IoT applications often involve millions of endpoints (sensors, devices, vehicles) generating a continuous firehose of telemetry data. This data is high-volume and high-cardinality. AstraStream is designed to handle this scale efficiently. Its low-latency ingestion path ensures timely processing of critical events, while its cost-effective, long-term storage on S3/GCS is essential for historical analysis, model training, and anomaly detection over large datasets.  
* **Financial Services:** In the financial sector, processing market data feeds, trade orders, and transaction logs demands both ultra-low latency and uncompromising data durability. AstraStream's architecture, which provides single-digit millisecond publish latency and delegates durability to enterprise-grade cloud storage, meets these stringent requirements. The platform can serve as the central nervous system for real-time risk assessment, fraud detection, and trade settlement systems.

### **1.4. Architectural Comparison and Positioning**

To fully appreciate the design of AstraStream, it is essential to position it relative to the industry incumbent, Apache Kafka, and its direct architectural inspiration, AutoMQ. The following table provides a concise comparison of their core architectural tenets and the resulting operational characteristics. This comparison highlights how AstraStream synthesizes the best aspects of both systems while introducing its own unique implementation choices.

| Feature / Aspect | Apache Kafka (Shared-Nothing) | AutoMQ (Shared-Storage) | AstraStream (Proposed Design) |
| :---- | :---- | :---- | :---- |
| **Storage Model** | Tightly coupled compute and local disk storage. Tiered storage (KIP-405) is an add-on, not a fundamental redesign.1 | Decoupled compute and storage. Leverages EBS/Block Storage for a Write-Ahead Log (WAL) and S3 for primary, long-term storage.2 | **Fundamentally Decoupled:** Local NVMe for a high-performance WAL using Direct I/O, with Object Storage (S3/GCS) as the primary, infinite log storage tier. |
| **Data Replication** | Synchronous, in-band replication between broker replicas using the ISR protocol. This generates high volumes of costly cross-AZ network traffic.5 | No application-level data replication. Durability is fully delegated to the underlying cloud storage services (e.g., EBS and S3 multi-replica capabilities).4 | **No Inter-Broker Replication:** Durability is delegated entirely to the cloud storage layers. The WAL relies on durable block storage, and the primary log relies on object storage's built-in redundancy. |
| **Elasticity & Scaling** | Slow, complex, and disruptive. Scaling out requires copying massive amounts of partition data to new brokers, a process that can take hours or even days and impacts performance.1 | Extremely fast and non-disruptive. Stateless brokers allow scaling compute capacity in seconds. Partition reassignment is a metadata-only operation, not a data copy.2 | **Instantaneous Elasticity:** Stateless brokers scale in seconds. An integrated, continuous auto-balancer shifts traffic to new nodes in minutes by executing metadata-only partition reassignments. |
| **Primary Cost Drivers** | Over-provisioned compute instances, expensive high-performance disks (multiplied by the replication factor, typically 3x), and high cross-AZ data replication network fees.1 | On-demand compute (compatible with Spot instances), low-cost S3 for primary storage, minimal high-performance storage for the WAL, and S3/GCS API call costs.2 | **Optimized Cloud Spend:** On-demand or Spot compute instances, minimal local NVMe for the WAL, cheap object storage for the main log, and object storage API call costs. Cross-AZ replication traffic costs are eliminated. |
| **Failure Recovery** | Controller-led leader re-election among the in-sync replicas (ISR). The failover itself is fast, but subsequent data re-replication and cluster rebalancing are slow and resource-intensive.14 | Controller reassigns partition leadership to any healthy broker in the cluster. Because brokers are stateless and storage is shared, recovery is a near-instantaneous metadata update, completed in seconds.4 | **Metadata-Driven Failover:** The controller quorum detects failure and reassigns partition leadership to any available broker in seconds. The cluster then self-heals as the auto-balancer redistributes load from the failed node's partitions. |

## **Part 2: Technical Architecture and Implementation Blueprint**

This section provides a detailed technical specification for AstraStream, serving as a direct guide for the engineering team.

### **2.1. System Overview & Core Principles**

AstraStream's architecture is organized into two distinct logical planes: a **Control Plane** responsible for cluster state and coordination, and a **Data Plane** responsible for high-performance message transport. This separation is fundamental to achieving the system's goals of elasticity and operational simplicity.

The **Control Plane** consists of a quorum of Controller nodes and an external metastore. The Controllers are the "brain" of the cluster, managing all metadata and making authoritative decisions about the system's state.

The **Data Plane** consists of a fleet of stateless Broker nodes, which interface with the underlying cloud storage infrastructure. This infrastructure is composed of two specialized tiers: a low-latency Write-Ahead Log (WAL) on local block storage and a cost-effective, durable primary log on an object storage service.

This design is guided by four core architectural principles:

1. **Storage-Compute Separation:** Brokers are treated as ephemeral, stateless processing units. The authoritative state of the system—the message log itself—resides entirely within a separate, shared storage layer. This principle is the key enabler of independent scaling.2  
2. **Stateless Data Plane:** Individual broker nodes do not "own" partitions or their data. A broker may temporarily be the leader for a partition, but this is a transient role assigned by the Control Plane. This statelessness is what allows for near-instantaneous scaling and failover, as leadership can be transferred between any two brokers without data movement.4  
3. **Durability Delegation:** AstraStream does not reinvent the wheel for data durability. Instead of implementing complex and costly application-level replication protocols like Kafka's ISR, it delegates the responsibility of data persistence and redundancy to the underlying cloud infrastructure. The WAL relies on the durability of services like AWS EBS or GCP Persistent Disk, which are themselves replicated, and the primary log relies on the extreme durability guarantees of object storage services like S3 (11 nines).3  
4. **Performance through Specialization:** Each component in the data path is highly specialized for a single task. The WAL is optimized for one thing: extremely fast, durable appends to provide low producer latency. The object storage tier is optimized for cheap, scalable, long-term storage. The brokers are optimized for efficient network I/O, caching, and orchestrating the movement of data between the tiers and clients.

### **2.2. Cluster Architecture and Node Roles**

The AstraStream cluster is composed of two primary types of nodes, each implemented as a distinct Rust application.

#### **Broker Nodes (Rust)**

* **Responsibilities:** The Broker is the workhorse of the data plane. Its primary responsibilities include:  
  * Handling all client network connections and API requests (Produce, Fetch, Metadata) over QUIC.  
  * Managing the high-performance write path, which involves validating producer requests and appending records to the shared WAL on its local block device.  
  * Orchestrating the asynchronous offloading of committed data from the WAL to the primary object storage tier.  
  * Serving read requests by fetching data from its in-memory caches (for hot data) or directly from object storage (for cold data).  
  * Communicating with the Control Plane via Cap'n Proto RPC to send heartbeats and receive commands (e.g., leadership assignments).  
* **Stateless Nature:** A broker holds no permanent state associated with any specific partition. While it maintains in-memory caches for performance, this data is considered transient and can be rebuilt. Any broker in the cluster is capable of assuming leadership for any partition at any time, as instructed by the controller. This is a direct departure from Kafka's model where a replica must hold a full copy of a partition's data on its local disk.4  
* **Implementation:** The broker will be implemented as a multi-threaded, asynchronous Rust application leveraging the tokio runtime. It will expose a QUIC endpoint for client communication and a separate Cap'n Proto RPC endpoint for control plane communication. All I/O operations—network, WAL, and object store access—will be fully asynchronous to maximize concurrency and resource utilization.

#### **Controller Quorum (Rust)**

* **Responsibilities:** The Controller Quorum acts as the centralized intelligence and authority for the entire cluster. Its responsibilities are critical and include:  
  * **Cluster Membership:** Tracking the liveness and health of all registered broker nodes via a heartbeat mechanism.  
  * **Metadata Management:** Acting as the gateway to the external metastore, handling all reads and writes of cluster metadata. This includes information about topics, partitions, broker locations, and log segment locations.  
  * **Partition Leadership:** The controllers are solely responsible for assigning leadership of each partition to a specific broker. They manage the leader epoch, a monotonically increasing number used to prevent split-brain scenarios during leader transitions.14  
  * **Auto-Balancing:** The controllers will run the continuous load-balancing algorithm, making decisions to reassign partitions to maintain optimal cluster health.  
* **Consensus:** To ensure the control plane is fault-tolerant and maintains a consistent view of the cluster state, the controllers will operate as a quorum of 3 or 5 nodes. They will use the Raft consensus algorithm to replicate their state machine and elect a single active leader among themselves. The raft-rs crate provides a solid, well-tested foundation for this implementation. This model is conceptually similar to the KRaft controller quorum in modern Kafka, but it manages a fundamentally different data plane of stateless brokers and shared storage.12

### **2.3. Metadata Management**

AstraStream will use an external, strongly consistent, distributed key-value store to persist all cluster metadata. This choice provides a clean separation of concerns between the cluster's state and its operational logic.

#### **External Metastore: etcd**

The chosen metastore is **etcd**, a widely adopted and battle-tested distributed key-value store that uses the Raft algorithm for consensus. Cloud providers offer managed etcd services or similar primitives, which can offload the operational complexity of managing the metastore itself.

Using an external metastore, as opposed to embedding it within the controller logic like KRaft does 12, offers significant architectural flexibility. It allows the metastore and the AstraStream controllers to be scaled, managed, and backed up independently using standard tooling. This clean boundary simplifies the design of the controller nodes, which can treat the metastore as a reliable, external dependency. While Kafka's tiered storage KIP-405 uses an internal Kafka topic to store metadata about remote segments 17, placing this critical state in a dedicated, transactional key-value store like etcd provides stronger consistency guarantees and simpler query patterns.

#### **Metadata Schema**

The following table defines the key-value schema that will be used to store all critical cluster state in etcd. The values will be serialized using a lightweight format like JSON or, for higher performance, Protobuf. This schema is the definitive source of truth for the cluster.

| Key Path (etcd) | Value (Serialized) | Description |
| :---- | :---- | :---- |
| /brokers/registered/{broker\_id} | { "host": "10.0.1.23", "port\_quic": 9092, "port\_rpc": 9093, "rack\_id": "us-central1-a" } | Contains the network endpoint information and availability zone for each live broker. Used for client routing and rack-aware balancing. |
| /topics/{topic\_name}/config | { "partitions": 64, "retention\_ms": \-1, "wasm\_module\_id": "pii\_scrubber\_v1" } | Defines the configuration for a topic, including partition count, data retention policy, and any associated stream processing module. |
| /topics/{topic\_name}/partitions/{partition\_id}/leader | { "broker\_id": "broker-007", "leader\_epoch": 142 } | The current leader broker and the monotonically increasing leader epoch for a specific partition. This is updated on every leader change. |
| /topics/{topic\_name}/partitions/{partition\_id}/segments/{segment\_start\_offset} | { "status": "IN\_WAL" | "UPLOADING" | "IN\_OBJECT\_STORE", "object\_key": "topics/my-topic/0/000...123.seg", "size\_bytes": 1073741824 } | Tracks the state and location of every log segment. This is the core metadata for the tiered storage engine, enabling brokers to find data whether it's in the WAL or object storage. |
| /consumer\_groups/{group\_id}/offsets/{topic\_name}/{partition\_id} | { "offset": 45678, "metadata": "client-specific-data" } | Stores the last committed offset for each partition consumed by a consumer group, enabling stateful consumption. |
| /wasm/modules/{module\_id} | { "name": "pii\_scrubber", "version": "v1", "object\_key": "wasm/pii\_scrubber\_v1.wasm" } | Metadata for registered WebAssembly modules, including their location in object storage for brokers to download. |

### **2.4. The AstraStream Storage Engine**

The storage engine is the most significant innovation in AstraStream, directly inspired by the high-performance, cloud-native design of AutoMQ's S3Stream.18 It is a two-tier system designed to provide both low-latency writes and cost-effective, infinite storage.

#### **Tier 1: The Low-Latency Write-Ahead Log (WAL)**

* **Storage Medium:** Each broker node will be provisioned with a dedicated, high-performance, network-attached block storage device, such as an AWS EBS GP3 volume, a GCP Persistent Disk, or a local NVMe SSD in a bare-metal environment. This device serves as the physical medium for the WAL.  
* **Implementation Strategy:** To achieve the lowest possible latency and avoid OS-level interference, the WAL will be implemented using **Direct I/O**. This technique bypasses the operating system's file system and page cache entirely, allowing the Rust application to write data blocks directly to the raw device. This prevents "cold reads" from object storage from polluting the cache and impacting write performance, a critical flaw in standard Kafka.4  
* **Rust Implementation:** The implementation will leverage the tokio-uring crate, which provides a high-level, safe abstraction over the Linux kernel's io\_uring interface. io\_uring is a state-of-the-art asynchronous I/O API that can submit and complete multiple I/O operations in a single system call, dramatically reducing overhead and maximizing the IOPS capabilities of the underlying device.  
* **Data Structure:** The WAL on the block device will be structured as a single, large circular buffer. All partitions for which a broker is the current leader will append their records into this one shared log file. This multiplexing of writes into a single sequential stream is highly efficient for block devices. Each record written to the WAL will be prepended with a header containing metadata such as the topic ID, partition ID, record length, and a CRC32 checksum for integrity. This design is heavily influenced by the "Delta WAL" concept, which has proven effective in similar systems.20  
* **Durability Guarantee:** A producer request with acks=all (or the equivalent setting) will only receive a success acknowledgement after the broker has received confirmation from the WAL's underlying block device that the write operation has been durably persisted. Since cloud block storage services like EBS are themselves replicated across multiple physical devices within an AZ, this provides a strong durability guarantee without needing application-level replication.12

#### **Tier 2: The Infinite Log on Object Storage**

* **Offloading Process:** A set of background tasks running on each broker is responsible for the offloading process. These tasks continuously monitor the WAL and identify data segments that have been committed and are eligible for archival. They read these segments from the WAL, batch them intelligently, and upload them to the configured object storage bucket (e.g., on S3 or GCS).  
* **Object Management Strategy:** A naive approach of uploading many small files to object storage is inefficient and costly due to API call overhead. To mitigate this, AstraStream will adopt a sophisticated object compaction strategy similar to that used by AutoMQ 21:  
  * **Stream Objects:** For topics with high, consistent throughput, data from a single partition will be aggregated and uploaded as large, monolithic "Stream Objects" (e.g., 1 GB in size). This is optimal for sequential scan performance.  
  * **Stream Set Objects:** For topics with low or bursty throughput, writing large objects per partition would result in high latency for data to appear in the object store. For these, data from multiple low-volume partitions will be merged and compacted into a single "Stream Set Object." This reduces the number of PUT requests and thus lowers API costs, at the expense of slightly more complex read logic.  
* **Metadata Update:** The offloading process is transactional with respect to the cluster's metadata. After a segment is successfully uploaded and its integrity is verified in the object store, the broker responsible for the upload will issue an RPC to the controller. The controller then transactionally updates the segment's entry in the etcd metastore, changing its status from IN\_WAL to IN\_OBJECT\_STORE and recording the unique object key.17  
* **Local WAL Cleanup:** Only after the metastore has been successfully updated does the broker consider the offload complete. It then advances its "trim offset" for the WAL, marking the space occupied by the now-archived segment as available for reuse by the circular buffer. This ensures that no data is ever lost, even if a broker crashes mid-upload.20

### **2.5. Data Flow, Caching, and High Availability**

The separation of storage and compute enables highly optimized data paths and a simple yet robust high-availability model.

#### **Write Path (Producer acks=all)**

The end-to-end process for a durable write is as follows:

1. A client producer establishes a QUIC connection to the broker identified as the leader for the target partition. It sends a ProduceRequest containing a batch of records.  
2. The leader broker receives the request, performs validation (e.g., authentication, schema checks), and prepares the record batch for writing.  
3. The broker appends the batch to its shared WAL on the local block device using an asynchronous io\_uring operation. The record batch is also concurrently written to an in-memory WriteCache.  
4. The underlying block storage device confirms that the write has been durably persisted.  
5. The broker immediately sends a ProduceResponse with a success code back to the producer. The latency perceived by the producer is determined by this critical path: network RTT \+ broker processing \+ WAL write latency.  
6. **Asynchronously**, in the background, the broker's WAL Uploader task will eventually read this data from the WAL, upload it to object storage, and trigger the metadata update in etcd, as described in the previous section. This offloading process happens completely outside the producer's critical path.

#### **Read Path and Workload Isolation**

AstraStream's architecture explicitly solves the critical performance problem of page cache pollution that affects traditional Kafka deployments.4 This is achieved by implementing two distinct, non-interfering cache systems within each broker, a model proven effective by AutoMQ.22

* **Hot Read (Tailing Consumer):** A consumer fetching data from the head of the log (i.e., a "tailing" or real-time consumer) will request a recent offset. The broker will serve this request directly from its in-memory **WriteCache**. This cache holds the most recently produced data that is also present in the WAL. This operation is extremely fast as it involves no disk or network I/O to the storage tiers.  
* **Cold Read (Catch-up Consumer):** A consumer requesting an old offset (e.g., for historical analysis or reprocessing) will trigger a "cold read." The broker first consults the metastore to find the segment corresponding to the requested offset. It will see the segment's status is IN\_OBJECT\_STORE and retrieve its object key. The broker then initiates a fetch from the object store (S3/GCS). The data is streamed back from the object store, passed to the consumer, and simultaneously written into a separate, dedicated **ReadCache**. This ReadCache is designed for cold data and uses an LRU (Least Recently Used) eviction policy. Crucially, this operation **does not** touch the WriteCache or the underlying OS page cache, thereby completely isolating the cold read workload from the hot write/read path and guaranteeing stable, low latency for producers and real-time consumers. The broker will also employ an intelligent prefetching strategy, speculatively fetching subsequent log objects from S3/GCS into the ReadCache in anticipation of the consumer's next request.

#### **High Availability & Failover Model**

AstraStream's HA model is dramatically simpler and faster than Kafka's In-Sync Replica (ISR) protocol.14 The failover process is as follows:

1. **Failure Detection:** The active Controller node detects that a broker has failed when it misses several consecutive heartbeats.  
2. **Leader Re-election:** The Controller immediately identifies all partitions for which the failed broker was the leader. For each of these partitions, it consults its view of cluster load and selects a healthy, available broker to become the new leader.  
3. **Metadata Update:** The Controller executes a transactional update in the etcd metastore, changing the leader\_broker\_id and incrementing the leader\_epoch for each affected partition.  
4. **Client Redirection:** When clients attempt to produce to or fetch from the old (failed) leader, they will receive a connection error. Upon retrying or on their next metadata refresh, they will query the cluster for the new leader information, receive the updated location from the metastore, and seamlessly connect to the newly appointed leader. The entire process is a metadata-only operation. Because the log data resides in a shared storage layer accessible to all brokers, no data needs to be copied or replicated. This allows for failover and recovery to complete in a matter of seconds, a core advantage of the shared-storage architecture.2

### **2.6. Network Protocols and APIs**

The choice of network protocols is critical for performance, reliability, and interoperability. AstraStream will use distinct, specialized protocols for client-facing and internal communication.

#### **Client-Broker Protocol: QUIC / HTTP/3**

* **Rationale:** For communication between clients (producers and consumers) and brokers, AstraStream will use QUIC as its transport protocol, with HTTP/3 as the application layer framing. This is a deliberate choice over traditional TCP for several key reasons:  
  * **Reduced Connection Latency:** QUIC integrates the transport and cryptographic handshakes, often achieving a connection in a single round-trip (1-RTT) or even zero round-trips (0-RTT) for returning clients. This is significantly faster than TCP's three-way handshake followed by a separate TLS handshake.26  
  * **Elimination of Head-of-Line (HOL) Blocking:** QUIC supports multiple independent streams over a single connection. If a packet for one stream is lost, it only blocks that specific stream, while others can continue to make progress. This is a major advantage over TCP, where a single lost packet blocks all multiplexed streams on that connection.28  
  * **Connection Migration:** QUIC connections are identified by a unique connection ID, not by the source IP/port tuple. This allows clients to seamlessly migrate between networks (e.g., from Wi-Fi to cellular) without dropping and re-establishing the connection, which is invaluable for mobile and IoT use cases.28  
* **Rust Implementation:** The broker's client-facing endpoint will be built using the quinn crate, a mature and high-performance pure-Rust implementation of the QUIC protocol. The h3 crate will be used on top of quinn to handle HTTP/3 framing.  
* **API Specification:** The following table defines the primary methods of the client-broker API, which will be implemented as requests over separate bidirectional QUIC streams.

| Method | Request Payload (e.g., JSON over QUIC stream) | Response Payload | Description |
| :---- | :---- | :---- | :---- |
| Produce | { "topic": "...", "partition\_id":..., "records": \[...\] } | { "topic": "...", "partition\_id":..., "base\_offset":..., "error\_code":... } | Writes a batch of records to a specific partition. |
| Fetch | { "topic": "...", "partition\_id":..., "fetch\_offset":..., "max\_bytes":... } | { "topic": "...", "partition\_id":..., "records": \[...\], "high\_watermark":... } | Reads a batch of records starting from a given offset. |
| GetMetadata | { "topics": \["..."\] } | { "brokers": \[...\], "topics\_metadata": \[...\] } | Allows clients to discover cluster topology, topic configurations, and current partition leaders. |
| CommitOffset | { "group\_id": "...", "topic": "...", "partition\_id":..., "offset":... } | { "error\_code":... } | Commits a consumer group's offset for a partition. |

#### **Inter-Node RPC Protocol: Cap'n Proto**

* **Rationale:** For internal communication between trusted components, specifically Controller-to-Broker and Broker-to-Controller RPCs, performance is the single most important factor. While gRPC is a popular and robust choice, it relies on Protobuf, which involves a distinct serialization/deserialization step—parsing data from the wire and copying it into language-native data structures.31 Cap'n Proto, by contrast, is a "zero-copy" serialization protocol. Its wire format is designed to be identical to its in-memory data layout, meaning that messages can be read and accessed directly from the network buffer without any parsing or memory allocation. For the frequent, low-latency control messages that govern the cluster (like heartbeats and leadership commands), this elimination of overhead provides a significant performance advantage.31 While gRPC offers a broader ecosystem, this is not a concern for internal RPCs within a self-contained, Rust-based system.33  
* **Rust Implementation:** The RPC framework will be implemented using the capnp-rpc crate, which provides an asynchronous RPC system built on top of the core Cap'n Proto serialization library.  
* **API Specification:** The following table defines the core RPC interfaces for internal cluster management.

| Interface | Method | Parameters | Returns | Description |
| :---- | :---- | :---- | :---- | :---- |
| ControllerToBroker | becomeLeader | topic: Text, partitionId: UInt32, epoch: UInt64 | ack: Void | Instructs a broker to assume leadership for a given partition and epoch. |
| ControllerToBroker | resignLeader | topic: Text, partitionId: UInt32, epoch: UInt64 | ack: Void | Instructs a broker to gracefully relinquish leadership of a partition. |
| BrokerToController | heartbeat | brokerId: Text, metrics: BrokerMetrics | commands: List(ControllerCommand) | A broker sends its health status and load metrics to the controller and receives a list of commands to execute in response. |

### **2.7. Elasticity and Load Management**

AstraStream's elasticity is not just a feature but a core design principle, enabled by its stateless brokers and managed by an intelligent, automated control plane.

#### **Auto-Balancing Controller**

The auto-balancing logic is a critical component that runs continuously within the active Controller node. It is responsible for ensuring that load is evenly distributed across all available brokers, preventing performance bottlenecks and maximizing resource utilization.

* **Algorithm Design:** The balancer will implement a **dynamic, weighted, streaming partitioning algorithm**. This approach is inspired by research in streaming graph partitioning, which focuses on making locally optimal decisions based on partial, real-time data, a perfect fit for this use case.34 The algorithm operates in a continuous loop:  
  1. **Metric Ingestion:** The controller aggregates real-time performance metrics from all brokers, received via their regular heartbeats. Key metrics include CPU utilization, network bytes in/out per second, WAL write rate, and the number of active client connections.  
  2. **Cost Function:** The controller maintains a dynamic "cost" for hosting each partition on its current leader broker. This cost is a weighted function of the resources that partition is consuming (e.g., cost \= w1 \* cpu\_share \+ w2 \* network\_share).  
  3. **Imbalance Detection:** The algorithm continuously monitors the overall cluster balance. A rebalancing cycle is triggered if a broker's total load exceeds a configurable threshold (e.g., 80% CPU for more than 60 seconds) or if the standard deviation of load across brokers surpasses a certain value.  
  4. **Greedy Migration Strategy:** When triggered, the balancer identifies the most "expensive" partitions on the most heavily loaded brokers. For each candidate partition, it calculates the projected cost of moving it to every other less-loaded broker in the cluster.  
  5. **Execution:** The balancer greedily selects the partition move that yields the greatest reduction in the overall cluster imbalance score. It then executes this move by issuing a resignLeader RPC to the current leader and a becomeLeader RPC to the target broker. Because the move is a fast, metadata-only operation, the balancer can afford to make these adjustments frequently and with minimal disruption.

#### **Auto-Scaling Integration**

The auto-balancing controller is designed to work seamlessly with standard cloud auto-scaling mechanisms (e.g., AWS Auto Scaling Groups).

* **Scale-Out:** When a cloud provider's auto-scaling policy adds a new broker instance to the cluster, the new node starts up, connects to the network, and registers itself with the Controller quorum. The auto-balancer immediately detects this new, completely unloaded broker. It will automatically and gradually begin migrating partitions to the new node until the cluster load is once again evenly distributed. This entire process, from instance launch to it serving a balanced share of traffic, is designed to complete within minutes.  
* **Scale-In:** When an instance is scheduled for termination (e.g., by a scale-in policy or because a Spot instance is being reclaimed), the cloud provider's lifecycle hooks will trigger a notification. The controller receives this signal and places the target broker into a DRAINING state. In this state, the broker will no longer be considered a candidate for new leadership assignments. The auto-balancer then prioritizes migrating all existing partition leaderships off the draining node to other healthy nodes in the cluster. Once the broker is leading zero partitions, the controller signals back to the lifecycle hook that it is safe to terminate the instance, ensuring a graceful shutdown with no data loss or service interruption.

### **2.8. Extensibility: In-Stream Processing with WebAssembly (Wasm)**

To provide powerful and flexible single-message ETL capabilities without the overhead of a separate stream processing framework, AstraStream will integrate a WebAssembly (Wasm) runtime directly into the broker's data path.

* **Architecture and Workflow:**  
  1. **Module Management:** A developer compiles their transformation logic (written in a Wasm-compatible language like Rust, C++, or TinyGo) into a standard .wasm binary. They upload this module to the AstraStream cluster via the Admin API. The controller stores the Wasm binary in a dedicated object storage bucket and records its metadata (ID, version, function signatures) in the etcd metastore.  
  2. **Topic Configuration:** A topic can be configured to associate a specific, registered Wasm module with its data stream. This configuration is stored as part of the topic's metadata.  
  3. **Execution:** When a broker that is the leader for a partition in a Wasm-enabled topic receives a ProduceRequest, its processing pipeline is augmented. Before writing the message to the WAL, the broker invokes a sandboxed Wasm runtime.  
  4. **Sandboxing:** The broker will use a mature, secure, and high-performance Wasm runtime like wasmtime. The Wasm module is instantiated within a secure sandbox with strictly enforced resource limits (CPU time, memory allocation). This is critical to ensure that a buggy or malicious user-provided module cannot crash or compromise the stability of the broker process.  
  5. **Transformation:** The broker calls a predefined, exported function from the Wasm module, for example, fn process\_message(message: &\[u8\]) \-\> Vec\<u8\>. The broker passes the raw message bytes into the sandbox, and the Wasm module returns a new byte vector containing the transformed message.  
  6. **Routing:** The transformed message is then written to a designated output topic (or potentially back to the same topic, depending on the use case). This allows for the creation of complex data processing topologies directly within the messaging system. This approach provides a safe, performant, and language-agnostic method for common ETL tasks like PII filtering, data enrichment from a sidecar cache, or format conversion.13

### **2.9. Administrative and Operational Tooling**

To ensure that AstraStream is as easy to manage as it is powerful, a comprehensive, modern administrative API is essential.

#### **Admin API (RESTful)**

A standard, well-documented RESTful HTTP API will be exposed by the Controller quorum (potentially via a dedicated, load-balanced API gateway service) to provide a single point of entry for all cluster management and observability tasks. This API will be the foundation for a future web UI, a command-line interface (CLI), and integration with third-party monitoring and automation tools.

* **Core API Endpoints:**  
  * **Topic Management:**  
    * POST /v1/topics: Create a new topic with specified partition count, retention policies, and other configurations.  
    * GET /v1/topics/{topic\_name}: Retrieve detailed configuration and status for a specific topic.  
    * PUT /v1/topics/{topic\_name}/config: Update the configuration of an existing topic.  
    * DELETE /v1/topics/{topic\_name}: Delete a topic and its associated data.  
  * **Cluster Management & Observability:**  
    * GET /v1/cluster/status: Provides a high-level overview of the cluster's health, including the status of all controller and broker nodes.  
    * GET /v1/brokers: Lists all registered brokers and their current load metrics.  
    * GET /v1/consumer\_groups: Lists all active consumer groups.  
    * GET /v1/consumer\_groups/{group\_id}: Inspects the status of a specific consumer group, including the committed offset and calculated lag for each partition it is consuming.  
  * **Wasm Module Management:**  
    * POST /v1/wasm/modules: Upload a new Wasm binary and register it with the system.  
    * GET /v1/wasm/modules: List all registered Wasm modules.  
    * GET /v1/wasm/modules/{module\_id}: Retrieve metadata for a specific module.  
    * DELETE /v1/wasm/modules/{module\_id}: De-register a Wasm module.

## **Appendix**

### **A.1. Configuration Parameters**

The following is a representative subset of the critical configuration parameters that will be available for tuning AstraStream components. A complete list will be maintained in the official documentation. These parameters are inspired by the detailed configuration options provided by mature systems like Kafka and AutoMQ, providing operators with the necessary levers to optimize for their specific workloads.37

#### **Broker Configuration (broker.toml)**

| Parameter | Type | Default | Description |
| :---- | :---- | :---- | :---- |
| broker.id | String | (generated) | A unique identifier for the broker node. If not set, a UUID will be generated on startup. |
| controller.endpoints | Array | \["localhost:9094"\] | A list of host:port addresses for the Controller quorum's RPC interface. |
| listen.quic | String | 0.0.0.0:9092 | The network address and port for the client-facing QUIC listener. |
| listen.rpc | String | 0.0.0.0:9093 | The network address and port for the internal Cap'n Proto RPC listener. |
| rack.id | String | (none) | The availability zone or rack identifier for this broker. Essential for rack-aware balancing. |
| wal.path | String | /var/lib/astrastream/wal | The path to the raw block device or file to be used for the Write-Ahead Log. |
| wal.capacity.bytes | Int64 | 10737418240 (10 GB) | The total capacity of the WAL. Determines how much un-offloaded data can be buffered locally. |
| cache.write.size.bytes | Int64 | (10% of RAM) | The size of the in-memory cache for hot writes and tailing reads. |
| cache.read.size.bytes | Int64 | (25% of RAM) | The size of the in-memory cache for cold data fetched from object storage. |
| object\_store.type | String | s3 | The type of object store to use. Supported values: s3, gcs. |
| object\_store.s3.endpoint | String | https://storage.googleapis.com | The endpoint URL for the GCS object storage service. |
| object\_store.s3.bucket | String | (required) | The name of the S3 bucket to use for primary log storage. |
| object\_store.s3.region | String | us-central1 | The GCS region of the bucket. |

#### **Controller Configuration (controller.toml)**

| Parameter | Type | Default | Description |
| :---- | :---- | :---- | :---- |
| node.id | String | (required) | A unique identifier for this controller node within the quorum. |
| listen.rpc | String | 0.0.0.0:9094 | The network address and port for the internal Cap'n Proto RPC listener for broker communication. |
| listen.raft | String | 0.0.0.0:9095 | The network address and port used for Raft consensus communication between controller nodes. |
| listen.http | String | 0.0.0.0:9642 | The network address and port for the public-facing administrative REST API. |
| raft.peers | Array | \`\` | A list of node\_id@host:port for all other peers in the Raft quorum. |
| metastore.endpoints | Array | \["localhost:2379"\] | A list of host:port addresses for the external etcd cluster. |
| autobalancer.enable | Boolean | true | Toggles the automatic load balancing feature. |
| autobalancer.trigger.cpu.threshold | Float | 0.80 | The CPU utilization threshold (0.0 to 1.0) that will trigger a rebalancing cycle. |
| autobalancer.cooldown.seconds | Int | 300 | The minimum time to wait between consecutive rebalancing decisions to prevent thrashing. |

#### **Works cited**

1. 4 Cons of Kafka Tiered Storage You Must Know \- Medium, accessed July 26, 2025, [https://medium.com/@AutoMQ/4-cons-of-kafka-tiered-storage-you-must-know-c77b762f70eb](https://medium.com/@AutoMQ/4-cons-of-kafka-tiered-storage-you-must-know-c77b762f70eb)  
2. AutoMQ vs Apache Kafka, accessed July 26, 2025, [https://www.automq.com/docs/automq/what-is-automq/difference-with-apache-kafka](https://www.automq.com/docs/automq/what-is-automq/difference-with-apache-kafka)  
3. What is automq: Overview \- GitHub, accessed July 26, 2025, [https://github.com/AutoMQ/automq/wiki/What-is-automq:-Overview](https://github.com/AutoMQ/automq/wiki/What-is-automq:-Overview)  
4. AutoMQ vs Apache Kafka | Better Performance and Cost Effective, accessed July 26, 2025, [https://www.automq.com/automq-vs-kafka](https://www.automq.com/automq-vs-kafka)  
5. How AutoMQ Saves Nearly 100% of Kafka's Cross-AZ Traffic Costs \- Medium, accessed July 26, 2025, [https://medium.com/@AutoMQ/how-automq-saves-nearly-100-of-kafkas-cross-az-traffic-costs-f6da13c81554](https://medium.com/@AutoMQ/how-automq-saves-nearly-100-of-kafkas-cross-az-traffic-costs-f6da13c81554)  
6. AutoMQ Load \- Apache Doris, accessed July 26, 2025, [https://doris.apache.org/docs/ecosystem/automq-load/](https://doris.apache.org/docs/ecosystem/automq-load/)  
7. Introducing AutoMQ: a cloud-native replacement of Apache Kafka \- DEV Community, accessed July 26, 2025, [https://dev.to/automq/introducing-automq-a-cloud-native-replacement-of-apache-kafka-kkc](https://dev.to/automq/introducing-automq-a-cloud-native-replacement-of-apache-kafka-kkc)  
8. AutoMQ vs Kafka: An Independent In-Depth Evaluation and Comparison by Little Red Book, accessed July 26, 2025, [https://www.automq.com/blog/automq-vs-kafka-evaluation-and-comparison-by-little-red-book](https://www.automq.com/blog/automq-vs-kafka-evaluation-and-comparison-by-little-red-book)  
9. What is Zero Copy in Kafka? | NootCode Knowledge Hub, accessed July 26, 2025, [https://www.nootcode.com/knowledge/en/kafka-zero-copy](https://www.nootcode.com/knowledge/en/kafka-zero-copy)  
10. The Zero Copy Optimization in Apache Kafka \- 2 Minute Streaming, accessed July 26, 2025, [https://blog.2minutestreaming.com/p/apache-kafka-zero-copy-operating-system-optimization](https://blog.2minutestreaming.com/p/apache-kafka-zero-copy-operating-system-optimization)  
11. Apache Kafka Tiered Storage \- Instaclustr, accessed July 26, 2025, [https://www.instaclustr.com/support/documentation/kafka/useful-concepts/kafka-tiered-storage/](https://www.instaclustr.com/support/documentation/kafka/useful-concepts/kafka-tiered-storage/)  
12. Kafka Alternative Comparision: AutoMQ vs. Warpstream \- GitHub, accessed July 26, 2025, [https://github.com/AutoMQ/automq/wiki/Kafka-Alternative-Comparision:-AutoMQ-vs.-Warpstream](https://github.com/AutoMQ/automq/wiki/Kafka-Alternative-Comparision:-AutoMQ-vs.-Warpstream)  
13. How to Use WebAssembly for Real-Time Data Processing, accessed July 26, 2025, [https://blog.pixelfreestudio.com/how-to-use-webassembly-for-real-time-data-processing/](https://blog.pixelfreestudio.com/how-to-use-webassembly-for-real-time-data-processing/)  
14. Kafka Data Replication Protocol: A Complete Guide \- Confluent Developer, accessed July 26, 2025, [https://developer.confluent.io/courses/architecture/data-replication/](https://developer.confluent.io/courses/architecture/data-replication/)  
15. Kafka Deep Dive for System Design Interviews | Hello Interview System Design in a Hurry, accessed July 26, 2025, [https://www.hellointerview.com/learn/system-design/deep-dives/kafka](https://www.hellointerview.com/learn/system-design/deep-dives/kafka)  
16. Kafka architecture \- Redpanda, accessed July 26, 2025, [https://www.redpanda.com/guides/kafka-architecture](https://www.redpanda.com/guides/kafka-architecture)  
17. Apache Kafka® Tiered Storage in Depth: How Writes and Metadata ..., accessed July 26, 2025, [https://aiven.io/blog/apache-kafka-tiered-storage-in-depth-how-writes-and-metadata-flow](https://aiven.io/blog/apache-kafka-tiered-storage-in-depth-how-writes-and-metadata-flow)  
18. Optimizing Kafka with AutoMQ's S3Stream, accessed July 26, 2025, [https://www.automq.com/docs/automq/architecture/s3stream-shared-streaming-storage/overview](https://www.automq.com/docs/automq/architecture/s3stream-shared-streaming-storage/overview)  
19. automq/s3stream/README.md at main · AutoMQ/automq · GitHub, accessed July 26, 2025, [https://github.com/AutoMQ/automq/blob/main/s3stream/README.md](https://github.com/AutoMQ/automq/blob/main/s3stream/README.md)  
20. How to implement high-performance WAL based on raw devices ..., accessed July 26, 2025, [https://medium.com/@AutoMQ/how-to-implement-high-performance-wal-based-on-raw-devices-db4f36937297](https://medium.com/@AutoMQ/how-to-implement-high-performance-wal-based-on-raw-devices-db4f36937297)  
21. Cloud-Native Kafka with S3 Storage \- AutoMQ, accessed July 26, 2025, [https://www.automq.com/docs/automq/architecture/s3stream-shared-streaming-storage/s3-storage](https://www.automq.com/docs/automq/architecture/s3stream-shared-streaming-storage/s3-storage)  
22. Deep dive into the challenges of building Kafka on top of S3. \- AutoMQ, accessed July 26, 2025, [https://www.automq.com/blog/deep-dive-into-the-challenges-of-building-kafka-on-top-of-s3](https://www.automq.com/blog/deep-dive-into-the-challenges-of-building-kafka-on-top-of-s3)  
23. AutoMQ: Next-Gen Kafka with 1GB/s Cold Read, Elastic Cloud Streaming \- Medium, accessed July 26, 2025, [https://medium.com/@AutoMQ/automq-next-gen-kafka-with-1gb-s-cold-read-elastic-cloud-streaming-b4253aca17c4](https://medium.com/@AutoMQ/automq-next-gen-kafka-with-1gb-s-cold-read-elastic-cloud-streaming-b4253aca17c4)  
24. Kafka Replication: Concept & Best Practices \- GitHub, accessed July 26, 2025, [https://github.com/AutoMQ/automq/wiki/Kafka-Replication:-Concept-&-Best-Practices](https://github.com/AutoMQ/automq/wiki/Kafka-Replication:-Concept-&-Best-Practices)  
25. Kafka Replication and Committed Messages \- Confluent Documentation, accessed July 26, 2025, [https://docs.confluent.io/kafka/design/replication.html](https://docs.confluent.io/kafka/design/replication.html)  
26. MQTT Over QUIC: The Next-Generation IoT Standard Protocol, accessed July 26, 2025, [https://www.iotforall.com/mqtt-over-quic-next-generation-iot-standard-protocol](https://www.iotforall.com/mqtt-over-quic-next-generation-iot-standard-protocol)  
27. QUIC: The Secure Communication Protocol Shaping the Internet's Future \- Zscaler, accessed July 26, 2025, [https://www.zscaler.com/blogs/product-insights/quic-secure-communication-protocol-shaping-future-of-internet](https://www.zscaler.com/blogs/product-insights/quic-secure-communication-protocol-shaping-future-of-internet)  
28. QUIC \- Wikipedia, accessed July 26, 2025, [https://en.wikipedia.org/wiki/QUIC](https://en.wikipedia.org/wiki/QUIC)  
29. The Road to QUIC \- The Cloudflare Blog, accessed July 26, 2025, [https://blog.cloudflare.com/the-road-to-quic/](https://blog.cloudflare.com/the-road-to-quic/)  
30. What Is the QUIC Protocol? | EMQ, accessed July 26, 2025, [https://www.emqx.com/en/blog/quic-protocol-the-features-use-cases-and-impact-for-iot-iov](https://www.emqx.com/en/blog/quic-protocol-the-features-use-cases-and-impact-for-iot-iov)  
31. A Rustaceans view on gRPC and Cap'n Proto \- Swatinem, accessed July 26, 2025, [https://swatinem.de/blog/rust-grpc-capnp/](https://swatinem.de/blog/rust-grpc-capnp/)  
32. Cap'n Proto \- RPC at the speed of Rust \- Part 1 of 2 \- DEV Community, accessed July 26, 2025, [https://dev.to/kushalj/capn-proto-rpc-at-the-speed-of-rust-part-1-4joo](https://dev.to/kushalj/capn-proto-rpc-at-the-speed-of-rust-part-1-4joo)  
33. Load balancing and metrics \- Google Groups, accessed July 26, 2025, [https://groups.google.com/g/capnproto/c/p2LD505XMgs](https://groups.google.com/g/capnproto/c/p2LD505XMgs)  
34. GraSP: Distributed Streaming Graph Partitioning \- Robert Pienta, accessed July 26, 2025, [https://spicy.bike/data/15-kdd-grasp.pdf](https://spicy.bike/data/15-kdd-grasp.pdf)  
35. Streaming Balanced Graph Partitioning Algorithms for Random Graphs \- Google Research, accessed July 26, 2025, [https://research.google.com/pubs/archive/41525.pdf](https://research.google.com/pubs/archive/41525.pdf)  
36. Streaming Graph Partitioning: An Experimental Study \- VLDB Endowment, accessed July 26, 2025, [https://www.vldb.org/pvldb/vol11/p1590-abbas.pdf](https://www.vldb.org/pvldb/vol11/p1590-abbas.pdf)  
37. Configuration · AutoMQ/automq Wiki \- GitHub, accessed July 26, 2025, [https://github.com/AutoMQ/automq/wiki/Configuration](https://github.com/AutoMQ/automq/wiki/Configuration)  
38. Broker And Controller Configuration \- AutoMQ, accessed July 26, 2025, [https://www.automq.com/docs/automq/configuration/broker-and-controller-configuration](https://www.automq.com/docs/automq/configuration/broker-and-controller-configuration)