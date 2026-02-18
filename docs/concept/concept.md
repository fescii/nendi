# **High-Performance Engineering of a Standalone Rust Database Streaming Daemon: A Kafka-Free Architecture for Sub-Millisecond Change Data Capture**

The evolution of modern data architectures has reached a critical inflection point where the latency overhead of traditional message brokers, such as Apache Kafka, often conflicts with the requirements of real-time operational systems. While Kafka provides a robust distributed log, its reliance on the Java Virtual Machine (JVM), complex replication protocols, and network-hop-heavy architecture introduces significant tail latencies and operational complexity for single-region or localized streaming tasks. The engineering of dbstreamd, a standalone Rust-based daemon, addresses these constraints by consolidating ingestion, persistence, and delivery into a single high-performance binary. By utilizing an embedded RocksDB instance for durable logging and Rust’s asynchronous, zero-copy primitives, the system targets a sustained throughput of ![][image1] writes per second on a single-node envelope. This report details the technical feasibility, architectural design, and optimization strategies required to build such a system, emphasizing the convergence of database logical decoding and high-efficiency egress protocols.

## **Executive Summary**

The transition from a broker-centric streaming model to a daemon-centric one is driven by the need for sub-millisecond end-to-end latency and reduced infrastructure overhead. Traditional Change Data Capture (CDC) pipelines involve a database, an ingestion agent (e.g., Debezium), a Kafka cluster, and finally the consumer. Each stage introduces serialization cycles and network transitions. dbstreamd collapses this pipeline by embedding a log-structured merge-tree (LSM) engine directly into the ingestion daemon.

The proposed architecture leverages PostgreSQL’s logical decoding mechanism for data manipulation language (DML) changes and a sophisticated event trigger framework for data definition language (DDL) synchronization.1 Rust serves as the implementation language to provide memory safety without garbage collection, allowing for predictable performance at high throughput.4 The core storage layer utilizes RocksDB, tuned specifically for sequential NVMe writes, achieving ![][image2] IOPS by amortizing fsync costs through batched commits.5 Egress is handled via gRPC bidirectional streaming, employing HTTP/2 flow control to manage backpressure without blocking ingestion.8 The result is a system capable of serving as a primary synchronization engine for high-frequency trading, real-time analytics, and microservices state propagation with a fraction of the hardware footprint of a Kafka-based equivalent.

## **System Overview and Architecture**

The dbstreamd daemon is designed as a long-lived process, operationally similar to a high-performance web server like Nginx or a proxy like Envoy. It maintains persistent connections to source databases and exposes high-speed APIs to downstream consumers.

\+--------------------------------------------------------------------------+

| dbstreamd |

| |

| \+-------------------+ \+-------------------+ \+--------------+ |

| | Source Connectors | | Internal Pipeline | | Egress Layer | |

| | (Postgres/MySQL) | | (Ring Buffer) | | (gRPC/HTTP) | |

| \+---------+---------+ \+---------+---------+ \+------+-------+ |

| | | | |

| \+---------v---------+ \+---------v---------+ \+------v-------+ |

| | Logical Decoding | | Write Batching | | gRPC Stream | |

| | & DDL Triggers \+------\> & Serialization \+------\> Server | |

| \+-------------------+ \+---------+---------+ \+------+-------+ |

| | | |

| \+---------v---------+ \+------v-------+ |

| | RocksDB Log Store | | Webhook | |

| | (LSM-Tree Log) | | Dispatcher | |

| \+---------+---------+ \+--------------+ |

| | |

| \+---------v---------+ |

| | Offset Registry | |

| \+-------------------+ |

| |

\+--------------------------------------------------------------------------+

### **Component Breakdown**

The architecture is centered around a decoupled ingestion and egress model, connected by a high-speed persistent log.

1. **Database Connectors**: These modules implement the specific protocols required to stream changes from various engines. For PostgreSQL, this involves the streaming replication protocol and logical decoding plugins.3  
2. **Internal Event Queue**: A lock-free, bounded MPSC (Multi-Producer Single-Consumer) queue or a ring buffer that decouples the ingestion threads from the storage workers. This ensures that a slowdown in RocksDB compaction does not immediately block the database socket.  
3. **RocksDB Persistent Log**: The heart of the system, storing events as sequential key-value pairs where the key is a monotonic sequence number or a Log Sequence Number (LSN).6  
4. **Offset Registry**: A specialized RocksDB column family that tracks the last acknowledged LSN for each consumer and the last persisted LSN from each source database.12  
5. **gRPC Streaming Server**: A tonic-based implementation that provides low-latency, bidirectional communication with clients, supporting resumable streams from specific offsets.8  
6. **HTTP Webhook Dispatcher**: An asynchronous delivery engine with built-in retry logic, circuit breaking, and dead-letter queues for consumers that cannot use gRPC.

## **Multi-Database Connector Strategy**

To ensure future extensibility, dbstreamd utilizes a trait-based abstraction model. This allows the core pipeline to remain agnostic of the underlying database engine while providing a common interface for ingestion.

### **The SourceConnector Trait**

In Rust, this abstraction is defined through a set of asynchronous traits. The SourceConnector handles connection lifecycle, snapshotting, and the continuous streaming of changes.

Rust

\#\[async\_trait\]  
pub trait SourceConnector: Send \+ Sync {  
    /// Initializes the connection and creates replication slots if necessary.  
    async fn connect(&self) \-\> Result\<(), ConnectorError\>;  
      
    /// Starts the streaming process from a specific offset.  
    async fn stream\_changes(&self, from\_offset: Lsn) \-\> BoxStream\<'static, ChangeEvent\>;  
      
    /// Retrieves the current head LSN from the source database.  
    async fn get\_latest\_offset(&self) \-\> Result\<Lsn, ConnectorError\>;  
}

The ChangeEvent is a common envelope that standardizes data across different databases. It must contain the operation type (Insert, Update, Delete, DDL), the table identifier, the new/old row data, and the source-specific metadata (e.g., XID, LSN, timestamp).3

### **PostgreSQL Implementation Details**

PostgreSQL represents the primary target for dbstreamd. The ingestion engine utilizes logical decoding, which transforms the internal write-ahead log (WAL) into a stream of logical changes.2 The configuration must support wal\_level \= logical and a dedicated output plugin such as pgoutput for binary efficiency or wal2json for simpler debugging.13

| Plugin | Format | Performance Impact |
| :---- | :---- | :---- |
| pgoutput | Binary | Minimal; native PG format 14 |
| wal2json | JSON | High; significant CPU cost for serialization 14 |
| decoderbufs | Protobuf | Moderate; efficient but requires external dependencies 14 |

The logical decoding process is inherently limited to DML. To achieve full schema synchronization, dbstreamd implements a hybrid model using event triggers. These triggers fire on ddl\_command\_end and sql\_drop events, capturing the SQL command and object metadata.1 By emitting these DDL events as logical messages via pg\_logical\_emit\_message(), the daemon ensures that schema changes are interleaved with data changes in the correct transactional order.3

## **Standalone Daemon vs. Library Performance**

The decision to implement dbstreamd as a standalone daemon rather than a library or a set of microservices is rooted in OS-level optimization requirements.

### **Thread Models and Context Switching**

In a library-based model, the streaming logic shares the runtime and thread pool of the parent application. This often leads to "noisy neighbor" effects where heavy application logic starves the ingestion threads, causing replication lag. A standalone daemon allows for the use of a dedicated asynchronous runtime (e.g., a custom Tokio configuration) that can be tuned for I/O-intensive workloads.8

A critical advantage of the daemon model is the ability to implement a thread-per-core architecture. By pinning threads to physical CPU cores, the daemon minimizes context switching and maximizes cache locality. In high-throughput scenarios, context switches can consume a significant percentage of CPU cycles. A Rust daemon using io\_uring can submit batches of I/O requests to the kernel without a context switch for every single operation, significantly reducing the system-call overhead.18

### **Memory Locality and NUMA**

For high-performance servers, especially those with multi-socket configurations, NUMA (Non-Uniform Memory Access) is a major factor in performance. A standalone daemon can be launched with NUMA-aware memory allocators (like jemalloc or mimalloc) and pinned to the cores closest to the NVMe controller and network interface card (NIC).6 This reduces memory latency and prevents bus contention, which is essential for hitting the ![][image3] writes/sec target.

## **RocksDB Engineering for 100k+ Writes/sec**

RocksDB is chosen as the persistent log store due to its LSM-tree architecture, which is optimized for sequential write throughput.6 To sustain ![][image3] writes per second on an NVMe drive, the storage layer must be meticulously tuned to balance write amplification (![][image4]), read amplification (![][image5]), and space amplification (![][image6]).22

### **LSM-Tree Write Path Optimization**

The write path in RocksDB consists of the Write-Ahead Log (WAL), the MemTable, and the Sorted String Tables (SSTables).6

1. **WAL and Fsync**: Every write is logged to the WAL for durability. To achieve high throughput, dbstreamd utilizes batched writes (WriteBatch). Instead of calling fsync for every message, the daemon accumulates messages in a buffer and flushes them in a single operation. This amortizes the cost of disk synchronization across thousands of messages.5  
2. **MemTable Design**: The write\_buffer\_size and max\_write\_buffer\_number are critical. A larger MemTable (e.g., ![][image7] or ![][image8]) allows RocksDB to absorb more writes before triggering a flush, reducing the number of Level 0 (L0) files and subsequent compactions.7  
3. **Compaction Strategy**: For a streaming log where data is mostly sequential, Leveled Compaction is standard, but Universal Compaction can be more efficient for write-heavy workloads as it reduces write amplification at the cost of higher read amplification.7

### **Performance Tuning Profile**

To hit the performance target, the following RocksDB parameters are recommended for NVMe-based systems:

| Parameter | Recommended Value | Reason |
| :---- | :---- | :---- |
| write\_buffer\_size | ![][image8] | Absorbs massive write bursts; reduces L0 file churn 7 |
| max\_write\_buffer\_number | ![][image9] | Provides headroom during flush spikes 7 |
| max\_background\_jobs | ![][image9] | Parallelizes flushes and compactions across CPU cores 22 |
| max\_subcompactions | ![][image10] | Enables multi-threaded compaction of a single L0 file 7 |
| target\_file\_size\_base | ![][image11] | Smaller SSTables facilitate faster, more granular compactions 26 |
| use\_direct\_io\_for\_flush\_and\_compaction | true | Bypasses the OS page cache to prevent double-buffering 7 |

The formula for write amplification in a leveled LSM-tree is roughly ![][image12], where ![][image13] is the number of levels and ![][image14] is the level multiplier (typically ![][image15]).7 By increasing the size of the first level (max\_bytes\_for\_level\_base), the daemon can reduce the number of levels, thereby lowering ![][image4] and increasing sustained throughput.7

### **Optimal Key Structure**

For sequential logs, the RocksDB key should be a monotonic sequence number encoded as a big-endian 64-bit integer. This ensures that the keys are physically ordered on disk, which optimizes the write path and allows for extremely fast sequential range scans during replay.6

## **Write Optimization and Serialization Strategies**

The hot path of the ingestion pipeline—from the database socket to the RocksDB MemTable—must be devoid of allocations and expensive transformations.

### **Zero-Copy and Binary Serialization**

Standard serialization formats like JSON are unacceptable at ![][image3] messages/sec due to the overhead of string parsing and object allocation.9 Instead, dbstreamd should utilize zero-copy binary formats.

* **rkyv**: A zero-copy deserialization framework for Rust that allows accessing data in its archived form without any transformation.29 It can achieve performance nearly identical to native Rust structs by mapping bytes directly to types.30  
* **Protobuf**: While not purely zero-copy in the standard implementation, the prost crate in Rust provides high-performance serialization that is well-integrated with gRPC.8  
* **FlatBuffers**: An alternative that allows access to data without a parsing step, similar to rkyv, and is widely used in high-performance gaming and telemetry.

The recommendation for dbstreamd is rkyv for internal RocksDB storage and Protobuf for the gRPC egress layer to maintain compatibility with standard clients.29

### **Ingestion Pipeline Performance**

The ingestion pipeline uses a "Dragonsmouth" pattern:

1. **Buffer Pooling**: Pre-allocated byte buffers are used to read data from the database socket. This prevents the fragmentation associated with frequent small allocations.  
2. **Lock-Free Queues**: Changes are moved from the ingestion thread to the storage thread via a lock-free SPSC (Single-Producer Single-Consumer) or MPSC queue. This avoids mutex contention in the critical path.  
3. **Thread Pinning and Cache Alignment**: Threads are pinned to specific cores, and internal structures are padded to prevent "false sharing" where multiple cores fight for the same cache line.

## **Read and Replay Optimization**

A streaming daemon is only as good as its ability to catch up after a consumer disconnect. RocksDB’s ability to perform fast range scans and prefix seeks is critical for this.22

### **Efficient Replay Architecture**

When a consumer connects and requests data from a specific offset, the daemon performs a Seek operation to find the starting point.

1. **Prefix Seeks**: If the log is sharded by table or partition, prefix bloom filters allow RocksDB to skip entire SSTables that do not contain the requested shard, significantly reducing read amplification.22  
2. **Read-Ahead**: For sequential replays, the daemon configures compaction\_readahead\_size and utilizes the OS posix\_fadvise (via RocksDB’s ReadOptions) to signal the kernel to pre-fetch blocks from disk.26  
3. **Block Cache Configuration**: A large block cache (![][image16]) caches uncompressed data blocks, allowing frequent "catch-up" reads to bypass the I/O layer entirely.6

### **Consumer Offset Design**

Offsets are opaque byte arrays that the consumer passes back to the daemon during reconnection. Internally, these map to the 64-bit sequence numbers used as RocksDB keys. The Offset Registry ensures that the daemon knows exactly which data can be safely deleted from RocksDB through a background compaction filter or DeleteRange operation once all consumers have moved past a certain point.

## **gRPC Streaming API Design**

gRPC is the ideal egress protocol for high-throughput streaming due to its native support for HTTP/2 multiplexing and flow control.8

### **High-Throughput gRPC in Rust**

The tonic library provides an asynchronous implementation of gRPC that is highly efficient in Rust.

* **Bidirectional Streaming**: Allows the client to send acknowledgments back to the server on the same connection, which the server uses to update the offset registry in real-time.8  
* **Backpressure Handling**: gRPC’s window-based flow control ensures that if the consumer’s network buffer is full, the server stops sending data, naturally propagating backpressure back to the RocksDB reader.9  
* **Zero-Copy Response Streaming**: By using bytes::Bytes or rkyv-aligned buffers, the gRPC server can send data from the RocksDB block cache to the network card with minimal CPU intervention.9

### **API Blueprint**

Protocol Buffers

service StreamService {  
    // Starts a stream of changes from a given offset.  
    rpc Subscribe(SubscribeRequest) returns (stream ChangeEvent);  
      
    // Allows the client to commit an offset for durability.  
    rpc CommitOffset(CommitRequest) returns (CommitResponse);  
}

message SubscribeRequest {  
    string stream\_id \= 1;  
    bytes start\_offset \= 2; // Opaque LSN/Sequence number  
    repeated string table\_filters \= 3;  
}

message ChangeEvent {  
    bytes offset \= 1;  
    OpType op \= 2;  
    string schema \= 3;  
    string table \= 4;  
    bytes payload \= 5; // Serialized rkyv/Protobuf data  
}

## **HTTP Webhook Delivery**

For consumers that cannot maintain a persistent gRPC connection, dbstreamd provides an HTTP webhook dispatcher. This component must be designed for reliability and massive concurrency.

### **Webhook Dispatcher Architecture**

The dispatcher uses a pool of asynchronous workers (e.g., using reqwest with a connection pool).

1. **Batching**: To improve efficiency, the dispatcher can batch multiple events into a single POST request.  
2. **Retry with Exponential Backoff**: If a webhook fails, it is moved to a "Retry Queue" with an increasing delay between attempts.32  
3. **Circuit Breaker**: If a specific endpoint fails consistently, the circuit breaker trips, and the daemon stops attempting deliveries to that endpoint for a cool-down period.  
4. **Dead-Letter Queue (DLQ)**: Events that fail all retry attempts are moved to a DLQ column family in RocksDB for manual inspection or later reprocessing.

| Strategy | Implementation | Benefit |
| :---- | :---- | :---- |
| **Idempotency** | X-Event-ID Header | Prevents double-processing on retries 35 |
| **Concurrency** | Bounded Worker Pool | Prevents the daemon from exhausting file handles |
| **Isolation** | Per-Target Queues | A slow webhook target doesn't block others |

## **Reliability and Recovery Model**

Reliability in a streaming daemon is defined by its ability to provide "exactly-once" semantics and recover from crashes without data loss.35

### **LSN Persistence and Crash Recovery**

The daemon maintains an atomic mapping of (Source, LSN) in its offset registry. Every time a WriteBatch is committed to RocksDB, the registry is updated.

Upon restart:

1. The daemon queries the registry to find the last successfully persisted LSN for each source database.  
2. It re-establishes the replication connection to PostgreSQL, requesting data starting from last\_persisted\_lsn \+ 1\.2  
3. PostgreSQL replays the WAL from that point, ensuring no gaps in the stream.

### **Exactly-Once Processing Feasibility**

While "exactly-once delivery" over a network is formally impossible (the Two Generals Problem), "exactly-once processing" is achieved through idempotency.35 Each ChangeEvent carries a unique LSN. Both the internal RocksDB storage and the downstream consumers use this LSN to de-duplicate messages. If the daemon restarts and sends a duplicate LSN, the consumer simply overwrites its previous state with the same data, resulting in no net change.12

### **Durability Modes**

The daemon should support configurable durability modes:

* **Synchronous**: fsync on every batch. Safest but slowest.  
* **Asynchronous**: Periodic flushes to disk. Offers much higher throughput with a risk of losing a few milliseconds of data in a power-loss event.5

## **Health and Observability**

A high-performance daemon must be a "white-box" system. Observability is integrated into the core using the Prometheus metrics format.38

### **Required Metrics List**

| Category | Metric | Description |
| :---- | :---- | :---- |
| **Ingestion** | ingestion\_lsn\_lag | The delta between DB head LSN and daemon LSN 40 |
| **Storage** | rocksdb\_write\_stall\_duration | Time spent throttling due to compaction 6 |
| **Storage** | rocksdb\_memtable\_usage\_bytes | Real-time memory pressure from the LSM engine |
| **Egress** | grpc\_active\_streams | Number of connected gRPC clients |
| **Egress** | egress\_messages\_total | Total messages dispatched (Counter) |
| **Health** | daemon\_uptime\_seconds | Total time since last restart |

### **Health Endpoint Specification**

The /health endpoint provides a JSON summary of the system state:

* status: "UP" | "DEGRADED" | "DOWN"  
* connectors: Status of each source database connection.  
* storage: Disk space availability and RocksDB error count.  
* egress: Status of gRPC server and webhook dispatcher.

## **Performance Target Validation and Hardware Sizing**

To validate the ![][image3] writes/sec target, we analyze the I/O and CPU requirements.

### **Performance Envelope Estimate**

Assuming a ![][image17] average message size:

* **Throughput**: ![][image18] raw ingestion.  
* **Write Amplification**: With a ![][image4] of ![][image19], the disk must sustain ![][image20] of sequential writes during heavy compaction.22  
* **CPU**: Serialization and gRPC framing for ![][image21] msgs/sec will require approximately ![][image22] physical cores on modern x86\_64 or ARM64 (Graviton/Ampere) hardware.18

### **Hardware Sizing Recommendations**

| Component | Minimum Specification | Recommended Specification |
| :---- | :---- | :---- |
| **CPU** | **![][image9]** Cores (![][image23]) | ![][image10] Cores (High clock speed) |
| **Memory** | **![][image24]** RAM | ![][image25] RAM (ECC) |
| **Storage** | **![][image26]** NVMe (Gen3) | ![][image27] NVMe (Gen4, High DWPD) |
| **Network** | **![][image28]** Ethernet | ![][image29] Ethernet |
| **OS** | Linux (Kernel 5.15+) | Linux (Kernel 6.x for io\_uring multishot) |

## **Advanced Speed Improvements**

For deployments requiring the absolute lowest latency and highest density, dbstreamd can utilize advanced Linux kernel features.

### **Kernel Bypass and io\_uring**

Using io\_uring allows the daemon to manage I/O as a series of asynchronous completions rather than blocking readiness notifications (epoll).

* **Fixed Buffers**: Pre-registering memory buffers with the kernel prevents the overhead of mapping/unmapping pages for every I/O operation.18  
* **SQPOLL**: Enabling the kernel-side poller thread allows the daemon to submit I/O without any system calls, as the kernel automatically scans the submission queue for new entries.18  
* **Multishot Receive**: For gRPC and database sockets, io\_uring's multishot mode allows the daemon to receive multiple data packets in a single completion event, further reducing CPU churn.19

### **OS-Level Tuning**

To support high-throughput streaming, the following kernel parameters should be optimized:

* vm.dirty\_ratio=10 and vm.dirty\_background\_ratio=5: Ensures that disk writes are flushed frequently, preventing a massive I/O stall when the page cache fills up.  
* net.core.rmem\_max and net.core.wmem\_max: Increases the maximum socket buffer sizes to handle HTTP/2 window bursts.  
* hugepages: Allocating the RocksDB block cache and internal buffers on huge pages reduces TLB (Translation Lookaside Buffer) misses, which can improve performance by ![][image30] in memory-intensive workloads.

## **Docker and Deployment Considerations**

dbstreamd is distributed as a static single binary to simplify deployment across Linux environments.

### **Docker Guidance**

When running in Docker, the following configurations are essential:

1. **Volume Performance**: Always use a host-mount (bind mount) for the RocksDB data directory to bypass the performance overhead of OverlayFS.22  
2. **Resource Limits**: Set explicit CPU and memory limits. RocksDB consumes off-heap memory, so the container limit must account for both the process memory and the RocksDB block cache to prevent OOM termination by the OS.6  
3. **Networking**: Use network: host if possible to reduce the latency of the Docker bridge network.

### **Systemd Service Integration**

For bare-metal or VM deployments, a standard systemd service file ensures high availability:

* Restart=always: Automatically restarts the daemon on crash.  
* LimitNOFILE=65535: RocksDB requires a high number of open file handles for its SSTables.6  
* CPUAffinity: (Optional) Can be used to pin the daemon to specific cores at the OS level.

## **Risk Analysis**

1. **Compaction Debt**: If the ingestion rate exceeds the disk's ability to compact files, RocksDB will stall writes. This is the single biggest risk to the ![][image31] target. Mitigation involves using high-end NVMe drives and monitoring the level0\_file\_num\_compaction\_trigger.6  
2. **PostgreSQL WAL Bloat**: If the daemon is slow or disconnected, PostgreSQL will accumulate WAL files to protect the replication slot. This can fill the database disk. Mitigation involves heartbeat events and aggressive alerting on replication lag.2  
3. **Memory Pressure**: RocksDB’s memory usage is complex (MemTables \+ Block Cache \+ Index/Filter blocks). Improper tuning can lead to the OOM killer terminating the process. Mitigation involves using a shared\_block\_cache across all RocksDB instances within the daemon.6

## **Phased Implementation Roadmap**

### **Phase 1: Core Foundation (Months 1-2)**

* Implement the SourceConnector trait and PostgreSQL binary logical decoding.  
* Set up the RocksDB storage engine with basic leveling compaction.  
* Build the internal ring-buffer and basic rkyv serialization.

### **Phase 2: High-Performance Egress (Months 3-4)**

* Develop the gRPC bidirectional streaming server using tonic.  
* Implement the offset registry and LSN persistence logic.  
* Add the HTTP webhook dispatcher with basic retry logic.

### **Phase 3: Reliability and Advanced Tuning (Months 5-6)**

* Implement DDL event triggers and ordering guarantees.  
* Integrate io\_uring for asynchronous I/O and zero-copy buffers.  
* Conduct comprehensive performance benchmarks and finalize the RocksDB tuning profile.  
* Add Prometheus metrics and health monitoring endpoints.

## **Conclusion**

The construction of a standalone, Kafka-free database streaming daemon in Rust is not only technically feasible but represents a superior architectural choice for latency-sensitive applications. By leveraging RocksDB as an embedded log, the system bypasses the complexities of distributed broker management while providing the durability and consistency required for enterprise workloads. The target of ![][image3] writes per second is achievable through a combination of Rust’s zero-cost abstractions, meticulous RocksDB tuning, and the adoption of modern Linux I/O primitives like io\_uring. This architecture enables a highly dense, efficient, and reliable data synchronization layer that meets the demands of modern real-time infrastructure.

#### **Works cited**

1. Understanding DDL awareness \- CDC Replication \- IBM, accessed February 18, 2026, [https://www.ibm.com/docs/en/idr/11.4.0?topic=ucm-understanding-ddl-awareness-1](https://www.ibm.com/docs/en/idr/11.4.0?topic=ucm-understanding-ddl-awareness-1)  
2. A Guide to PostgreSQL Change Data Capture \- Streamkap, accessed February 18, 2026, [https://streamkap.com/resources-and-guides/postgresql-change-data-capture](https://streamkap.com/resources-and-guides/postgresql-change-data-capture)  
3. Understanding logical replication in Postgres \- Springtail, accessed February 18, 2026, [https://www.springtail.io/blog/postgres-logical-replication](https://www.springtail.io/blog/postgres-logical-replication)  
4. Consuming CDC with Java, Go… and Rust\! \- ScyllaDB, accessed February 18, 2026, [https://www.scylladb.com/2025/12/16/consuming-cdc-java-go-and-rust/](https://www.scylladb.com/2025/12/16/consuming-cdc-java-go-and-rust/)  
5. BVLSM: Write-Efficient LSM-Tree Storage via WAL-Time Key-Value Separation \- arXiv, accessed February 18, 2026, [https://arxiv.org/html/2506.04678v1](https://arxiv.org/html/2506.04678v1)  
6. A Deep Dive into RocksDB for Apache Kafka Streams: Usage and Optimization \- AutoMQ, accessed February 18, 2026, [https://www.automq.com/blog/rocksdb-kafka-streams-usage-optimization](https://www.automq.com/blog/rocksdb-kafka-streams-usage-optimization)  
7. 6\. RocksDB Tuning Guide \- Quasar documentation, accessed February 18, 2026, [https://doc.quasar.ai/master/administration/rocksdb\_tuning\_guide.html](https://doc.quasar.ai/master/administration/rocksdb_tuning_guide.html)  
8. How to Build Bidirectional gRPC Streaming with tonic in Rust \- OneUptime, accessed February 18, 2026, [https://oneuptime.com/blog/post/2026-01-25-bidirectional-grpc-streaming-tonic-rust/view](https://oneuptime.com/blog/post/2026-01-25-bidirectional-grpc-streaming-tonic-rust/view)  
9. Boosting Node.js gRPC performance with NAPI and Rust \- Triton Blog, accessed February 18, 2026, [https://blog.triton.one/supercharging-the-javascript-sdk-with-napi/](https://blog.triton.one/supercharging-the-javascript-sdk-with-napi/)  
10. Documentation: 18: 47.2. Logical Decoding Concepts \- PostgreSQL, accessed February 18, 2026, [https://www.postgresql.org/docs/current/logicaldecoding-explanation.html](https://www.postgresql.org/docs/current/logicaldecoding-explanation.html)  
11. durable-streams/durable-streams: The open protocol for real-time sync to client applications \- GitHub, accessed February 18, 2026, [https://github.com/durable-streams/durable-streams](https://github.com/durable-streams/durable-streams)  
12. Feature Summary • Akka Edge \- Akka Documentation, accessed February 18, 2026, [https://doc.akka.io/libraries/akka-edge/current/feature-summary.html](https://doc.akka.io/libraries/akka-edge/current/feature-summary.html)  
13. Chapter 2\. Change Data Capture Connector for PostgreSQL \- Red Hat Documentation, accessed February 18, 2026, [https://docs.redhat.com/en/documentation/red\_hat\_integration/2019-12/html/change\_data\_capture\_user\_guide/debezium-connector-for-postgresql](https://docs.redhat.com/en/documentation/red_hat_integration/2019-12/html/change_data_capture_user_guide/debezium-connector-for-postgresql)  
14. The Wonders of Postgres Logical Decoding Messages \- InfoQ, accessed February 18, 2026, [https://www.infoq.com/articles/wonders-of-postgres-logical-decoding-messages/](https://www.infoq.com/articles/wonders-of-postgres-logical-decoding-messages/)  
15. Logical replication and logical decoding in Azure Database for PostgreSQL, accessed February 18, 2026, [https://learn.microsoft.com/en-us/azure/postgresql/configure-maintain/concepts-logical](https://learn.microsoft.com/en-us/azure/postgresql/configure-maintain/concepts-logical)  
16. How to Implement PostgreSQL Event Triggers \- OneUptime, accessed February 18, 2026, [https://oneuptime.com/blog/post/2026-01-30-postgresql-event-triggers/view](https://oneuptime.com/blog/post/2026-01-30-postgresql-event-triggers/view)  
17. Documentation: 18: 38.1. Overview of Event Trigger Behavior \- PostgreSQL, accessed February 18, 2026, [https://www.postgresql.org/docs/current/event-trigger-definition.html](https://www.postgresql.org/docs/current/event-trigger-definition.html)  
18. io\_uring How flashQ Achieves Kernel-Level Async IO Performance ..., accessed February 18, 2026, [https://dev.to/egeominotti/iouring-how-flashq-achieves-kernel-level-async-io-performance-15d2](https://dev.to/egeominotti/iouring-how-flashq-achieves-kernel-level-async-io-performance-15d2)  
19. From epoll to io\_uring's Multishot Receives — Why 2025 Is the Year We Finally Kill the Event Loop \- Codemia, accessed February 18, 2026, [https://codemia.io/blog/path/From-epoll-to-iourings-Multishot-Receives--Why-2025-Is-the-Year-We-Finally-Kill-the-Event-Loop](https://codemia.io/blog/path/From-epoll-to-iourings-Multishot-Receives--Why-2025-Is-the-Year-We-Finally-Kill-the-Event-Loop)  
20. RocksDB | A persistent key-value store | RocksDB, accessed February 18, 2026, [https://rocksdb.org/](https://rocksdb.org/)  
21. RocksDB: Your Key-Value Store Powerhouse (and Why You Should Care) \- Dev.to, accessed February 18, 2026, [https://dev.to/ashokan/rocksdb-your-key-value-store-powerhouse-and-why-you-should-care-3880](https://dev.to/ashokan/rocksdb-your-key-value-store-powerhouse-and-why-you-should-care-3880)  
22. RocksDB-Tuning-Guide · rocksdbbook, accessed February 18, 2026, [https://zhangyuchi.gitbooks.io/rocksdbbook/content/RocksDB-Tuning-Guide.html](https://zhangyuchi.gitbooks.io/rocksdbbook/content/RocksDB-Tuning-Guide.html)  
23. RocksDB Tuning Guide \- GitHub, accessed February 18, 2026, [https://github.com/facebook/rocksdb/wiki/RocksDB-Tuning-Guide](https://github.com/facebook/rocksdb/wiki/RocksDB-Tuning-Guide)  
24. RocksDB Wiki \- Augment Code, accessed February 18, 2026, [https://www.augmentcode.com/open-source/facebook/rocksdb](https://www.augmentcode.com/open-source/facebook/rocksdb)  
25. RocksDB 101: Optimizing stateful streaming in Apache Spark with Amazon EMR and AWS Glue | AWS Big Data Blog, accessed February 18, 2026, [https://aws.amazon.com/blogs/big-data/rocksdb-101-optimizing-stateful-streaming-in-apache-spark-with-amazon-emr-and-aws-glue/](https://aws.amazon.com/blogs/big-data/rocksdb-101-optimizing-stateful-streaming-in-apache-spark-with-amazon-emr-and-aws-glue/)  
26. Ceph RocksDB Tuning Deep-Dive, accessed February 18, 2026, [https://ceph.io/en/news/blog/2022/rocksdb-tuning-deep-dive/](https://ceph.io/en/news/blog/2022/rocksdb-tuning-deep-dive/)  
27. Optimizing RocksDB Write Amplification on FDP SSDs | Samsung Semiconductor Global, accessed February 18, 2026, [https://semiconductor.samsung.com/news-events/tech-blog/optimizing-rocksdb-write-amplification-on-fdp-ssds/](https://semiconductor.samsung.com/news-events/tech-blog/optimizing-rocksdb-write-amplification-on-fdp-ssds/)  
28. Rkyv: A zero-copy deserialization framework for rust | Hacker News, accessed February 18, 2026, [https://news.ycombinator.com/item?id=38976896](https://news.ycombinator.com/item?id=38976896)  
29. rkyv \- Rust \- Docs.rs, accessed February 18, 2026, [https://docs.rs/rkyv](https://docs.rs/rkyv)  
30. Zero-copy (de)serialization | Hyper-Efficient Message Streaming at ..., accessed February 18, 2026, [https://iggy.apache.org/blogs/2025/05/08/zero-copy-deserialization/](https://iggy.apache.org/blogs/2025/05/08/zero-copy-deserialization/)  
31. Whats the best way to start on zero copy serialization / networking with rust? \- Reddit, accessed February 18, 2026, [https://www.reddit.com/r/rust/comments/1mgyzfl/whats\_the\_best\_way\_to\_start\_on\_zero\_copy/](https://www.reddit.com/r/rust/comments/1mgyzfl/whats_the_best_way_to_start_on_zero_copy/)  
32. Building Scalable Embedding Services with Rust and gRPC \- DEV Community, accessed February 18, 2026, [https://dev.to/salim4n/building-scalable-embedding-services-with-rust-and-grpc-ll9](https://dev.to/salim4n/building-scalable-embedding-services-with-rust-and-grpc-ll9)  
33. Streaming gRPC with Rust \- Kevin Hoffman \- Medium, accessed February 18, 2026, [https://kevinhoffman.medium.com/streaming-grpc-with-rust-d978fece5ef6](https://kevinhoffman.medium.com/streaming-grpc-with-rust-d978fece5ef6)  
34. High-Performance Solana Streaming with LaserStream SDKs \- Helius, accessed February 18, 2026, [https://www.helius.dev/blog/laserstream-sdks](https://www.helius.dev/blog/laserstream-sdks)  
35. How to Fix "Exactly-Once" Semantics in Streaming \- OneUptime, accessed February 18, 2026, [https://oneuptime.com/blog/post/2026-01-24-exactly-once-semantics-streaming/view](https://oneuptime.com/blog/post/2026-01-24-exactly-once-semantics-streaming/view)  
36. Deep Dive into Message Delivery Guarantees in Kafka \[Part 2\] | by Jirapat Atiwattanachai, accessed February 18, 2026, [https://life.wongnai.com/deep-dive-into-message-delivery-guarantees-in-kafka-part-2-04c770e62abb](https://life.wongnai.com/deep-dive-into-message-delivery-guarantees-in-kafka-part-2-04c770e62abb)  
37. seglo/exactly-once-streams: An engineering report on using transactions in Kafka 0.11.0.0, accessed February 18, 2026, [https://github.com/seglo/exactly-once-streams](https://github.com/seglo/exactly-once-streams)  
38. Prometheus Monitoring: From Zero to Hero, The Right Way \- Dash0, accessed February 18, 2026, [https://www.dash0.com/guides/prometheus-monitoring](https://www.dash0.com/guides/prometheus-monitoring)  
39. Metrics \- Restate docs, accessed February 18, 2026, [https://docs.restate.dev/server/monitoring/metrics](https://docs.restate.dev/server/monitoring/metrics)  
40. Monitoring PostgreSQL Replication Lag Using Prometheus Postgres ..., accessed February 18, 2026, [https://knowledge.broadcom.com/external/article/427872/monitoring-postgresql-replication-lag-us.html](https://knowledge.broadcom.com/external/article/427872/monitoring-postgresql-replication-lag-us.html)  
41. Prometheus Metrics \- Litestream, accessed February 18, 2026, [https://litestream.io/reference/metrics/](https://litestream.io/reference/metrics/)

[image1]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAFcAAAAXCAYAAAB+kNMAAAADo0lEQVR4Xu2YWahNYRTHl3lKhoQUlwcZIpkuHrgyZHgxPfAgmQkPhgyZSaEMEd1MIVOEcuMqypA8yPBAEhIZQsn0QCmx/metvc/e391733XuPbcr7V/966z/95212+t8e3/rO0QpKSkp1UI7Vi3XVFqzDrGus+6wZoWHM9RmbWRdY91nbWM1CM2wYc0zgHWedZN1gzUoNCpYc1UJdVk9WetZn1h9QqNCC9ZD1jyN27Kek3wnyCnWcZIbqs8qIbmpXLHk6c96xyrUeBjrJ2uIP0Ow5MqAX2ozq8AdqARvWKWsq6w/FF3c7STFDTKX5Meoo/EYku+39GcQdVWvX8ArD2ueu6zdgRicICmehzWXTwfWLlYxycR8sYzii/uKddrxsEIwf6DGR1kfs8MZsFp+s1Y6fhKWPB1Jrr3AnyGsY30nmQ8suSJpxdrKOkllH4WKEFdcvGvh73P83uqv1hgr+0l22OcL64prJmDJM5Hk2pOzwxkWqV+ksSVXIk1ICnOB5KI1w8Nm4oqLGP4ex++u/mGNv1LZVwf4wHrhmglY8iwmuTbuN8h89adrbMlloh5rNusSaxplHw0rccXFKogqrvfuwmYB8DnqRrDpvHXNBCx58LREFRf7APw5Glty5QSKeoZko2rkjCURV1xsolHF7aI+3mvgB0XfCG7ipWsmYMmznKKLi6LCn6GxJZcJFHUK6zHrFmtEeLhcvOL2dfxO6u91/G7q79T4NetRdtjnPUl/acWSBz02rj0pO5wBrSL88RpbciWCPhWPw1OSVYSVVhHiioseFz4OEEEwD/4ajR+Q9L4u30jaPCuWPBNIrj3VHxWWqD9YY0uuSHCSwi94j7WKwr1cRfCKW+gOMM9Y5xxvFMn80RofJNmFgzQkmbPF8ZOw5Gmv8UJvgoIzAA4STTW25ArRmGRXvEyyK2IjywdecaOa600kT0aNgLeU5PHyDhHDSb5f4M+QXPB6BbxmJBtu3DHbmgdH3gOBGJSwzgZia64MRayLrHEUvtF8sJbkokPdAZJj423WWI1xNsdqdvtMnNt3BOJjVLY/RncRteqCWPLgMIV2qo3G6F4+k+wRQSy5qowjJCcw3DD0i2RVeP8jeDRnrSCZv5/kB3ZBj40dG50FHkk8DW7fjacAJyQcfuKw5AGdWRtI/j8oZvUID2ew5vpvwCbptXApeQZ/qMx0zeoG71y0EBaVkrwr/zWwV+BcX9nuJiUC/Mk00jVTUlJy5S/RUhDcanI/FgAAAABJRU5ErkJggg==>

[image2]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAFIAAAAXCAYAAACYuRhEAAADxUlEQVR4Xu2YaahNURiGP/OQH6ZMpXslGTKU2Q9DQvHDmPhhikTImCGZU5QxkUwhZMgckfEqRDLEDyGRIUTGUEq87/nWvnftZa97173nlivnqbfW96511j772/us9a0jkiFDhv+EUlAD17QYDp2CrkIHoKx4d4o60DboInQdGhvvDqYTdBi6BOVAXWK9SlloCXQBugmthCrFRighcxUL9aD+0FnoiNMXMQG6A9Uy8QLotRWTmtBd0bGkPvQIWhQNCKQj9BJqb+Ie0Heoe+4IZS+0WzShFaHjokm1CZ0rbSaL3vx66IckJ7K26MUHW15p6Dk00/JWic5lMx56B5Vz/Py4Aa1zvD2iiYroB/2S+INsZrwOlhcyl48drhHKV0lO5BjRL9jS8c9Bp634KbTfigmfPD/b2fF9NBIdP8nxF0KfRd8+shN6k9edgn0/obkmDp3Lh31vhcKXSL6t/EL8qdpw3fki+rPi2sgxm2IjRNoYf57j+xgiOn6Y408zflcT882/n9edywfojGmHzuUjmqfQ+BJJjxdmsmz2Gb8h1Na0mXSbFsbf7vg+pouOZxJsJhp/tIk/yp/LCOG6/di0Q+fywT2jSPgSySeTlEiuNfSbij7dpERG6xY3hRD45ibdPNda+uNMzHZSIrmxvDDt0Ll8FHsiT0hyIpkc+lyLWGIkJZJJps81LYTZknzzvGn6XK/JN0lOJJP4xLRD5/KRViKPuibYJXrhuo7P8oN+DaixaW+IjRBpbvw1ju+DdSfHD3V8llT0B5r4GXQvrzuXV6I1JQmdi7s641DlpD6VD75ErhWdwC3WOZY+SyHWkGyzGLdpZ/z5ju9jkOj4UY4/w/jdTHxLtEZ1+SR5b1LoXD7SeiOPuSYYKXrh1o5/GbpixQ+hQ1ZMeot+to/j+8gWHT/V8ZeJ1rJVTbxVdIe2qSz62eUmzjZxQXP5SCuRSYVqNdG6a4TllRcttKdY3lLogehRM4IFO39udkHeS3Q398Gj3BbH4/c6aMU9RZOUZXksxN0HHjKXjyIlsoLoUzov8URE8AjJM3YZE3P9uS1aQ0awzTeUYwnPvXxL7Tou2nxYpvhOO1xCWMLw6Eq4878XXYdteLZebcVcy906NnSuJAqVSJ48rokmkTdI8cKcpIo1jvBNWiH6hblAV493p6A3R/R4tRkaEOvVI91b0eMoNyIfTaDFohvaRqhVvDsF12buwKwU+FOfZTyXkLmSKFQi/xZ8ECybSjL/RCJPukYJhKe1Ek1f0b/hMqQJd/eC/nnJkCFDkfgNxdIMQTI7/yQAAAAASUVORK5CYII=>

[image3]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEMAAAAXCAYAAABQ1fKSAAADj0lEQVR4Xu2YaahOQRjHH/uSD7ZspXslWbKU3QdLQvHBmvhwLZEIWbMke4qydiPZQsiSPSK7QiRLfBASWUJkDaXE//8+c947Z+7MNflCeX/1r/P8Z87cd55zZuY5VyRHjhx/QCmogWtaDINOQFegfVBeujlDHWgLdB66Bo1JN0fTCToIXYQuQF1SrUpZaDF0DroBrYAqpXooMWNlqQf1h05Dh5y2hPHQbaiWiedDr6yY1ITuiPYl9aGH0MKkQyQdoRdQexP3gL5B3bM9lN3QTtGkVISOiibGJnasDJNEJ7AW+i7+ZNQWHWCw5ZWGnkEzLG+l6Fg246C3UDnHL4nrUKHj7RKdbEI/6KekH0Yz43WwvJixvHwRfzJGi/6Rlo5/BjppxU+gvVZM+AR4b2fHD9FItP9Ex18AfRJ9C8h26HVRcwa2/YDmmDh2LC+hZPCt4aB87W24Dj+LvqLcK9hnQ6qHSBvjz3X8EENE+xc4/lTjdzUx38B7Rc1Z3kOnzHXsWF5CyaDHmzlhmz3Gbwi1NddMnE0L4291/BDTRPtzIjYTjD/KxB+k+JIk3McemevYsbyEksFM+5LBtUe/qWiWfclI1jE3uhj4BvkmwL2H/lgT89qXDG6Wz8117FheQsk4Jv5kcIL0uTZ5fPmSwUTR5xqPYZb4J8AfTp/7F/kq/mQwEY/NdexYXpiMw64JdojeXNfxebTRrwE1NtfrUj1Emht/teOHYF3C/kMdn8c1/YEmfgrdLWrO8lK05iCxY3kJJWON6M1uQca+9HnMssbgNQsum3bGn+f4IQaJ9h/p+NON383EN0VrGJePovUSiR3LC5NxxDXBCNGbWzv+JeiyFT+ADlgx6S16bx/HD5Ev2n+K4y8VrXWqmniz6MlhU1n03mUmzjfx78bywmT4ipFqoufycMsrL1pMTba8JdB90bI+gUUZX1276OolesqEYNm8yfH4u/ZbcU/RieZZHost96HFjFWMCqLZOivpySSwXOc3SRkTcz3eEq0xEnjNN4V9Cb8T+LbY53yyofIIDFWlXI48HvmZQHgivRPdl2z4LbLKirm3uXVO7FgZWCFeFU0EfyTFm7nuqlj9CJ/octE/WghVTzdnoDcb2gZthAakWrV8fiNa+nNzDdEEWiS6Sa+HWqWbM3Cv4snAE4zLZqbxXGLG+qswmTySc4DjrvG/0lf0XwA5RE+dEr8Yc+T4N/gFdbT3EsmFfM8AAAAASUVORK5CYII=>

[image4]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADMAAAAYCAYAAABXysXfAAACXUlEQVR4Xu2WTYhPYRTGH59RyE5G2cxClAUrCcWsjJnJhmJBvimU8jFpamxkUizGKNkJG0mULMjGDFkoSfnIZsq3lJCExPM47x3Hce/9X3Zyf/XUfc9z7/vee99zzr1ATU3Nf81w6gL1mPpGfaWuU1OpcdRl6kXy3lJXqNE/rgTWUh+S9446lOKNGEndpLZGw7GZugebW+qHrX2beuPid7MLPHtg5vpokKMwb0k0yFLqJDU2GiXshM13IBoBzfmRehCNRC91MQbFGtgCWihyFuatCPERsF0dH+JlzIbtqOY7EbzIIth5B6ORaKEOx6DoQP7bWkgdSV5Mi23U8hArQ+m1Hfbwmk8pXMZ+2HkLXKyVWuyOVzlviPmwC4+52ChqNbUsefucN5k678ZV0MJZOqrGcvPdcYN6BXsJQi9BNTNz6IwCZsBu+IyLKa20eLbd2qGM09Q0N27EdNg8GQ9hDaUI3fgX2LpeT/1JRUyCnayOIZqo9nQ8K3l6AKH66krHVVDH3BtiA7A5i+pNa8vf7WJbqFNuXIi2UhffSuN1zlOblncJ9tBX8bM9V6ETv7/hTEW7q8KWP8fFNsCaRyVeU4PUXFhaZEyATaxvg1JtnvMaoXm6Y5Ach82pBpPHHVgaZvUipoRxKfdhfX1TiA+jPlPvYd+cqmhhfQfGRIP0wB5mZTRgKS5Pbf+vuUY9oyZGA/aH8Ai2S1VQnai1SnmoFnTDed81vUx5u6LxJ5yjNsZgQrXUFoMF9FEvYTf0Cb/WXzOsyTxJ/vM0Vi3uSMeDydO1SjfFa2pqamr+Pb4DDl2Nm3laY50AAAAASUVORK5CYII=>

[image5]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAC0AAAAYCAYAAABurXSEAAACJUlEQVR4Xu2Vv0tWURzGnyKhiAYHCZJaIiKkoiYnoVoq6B9waDCjhAqEEmvQQKIIaoimtiiCJokEsUYjxCWirNyEFKvBsqFQB3sevuf4fvu+t/s6NtwPPPCe5zm/7nu+516goqLiv+MQ9ZJaoFapj6k9Rf1K7UGqKQ9owCZqgroQA8d56gNsPWkctuYb6rvz3+UB/+IJrOMu522hrif/nvPLuAzrfzMGAc39m/oUg4TWG4lm5DP1KppkN2wTszEo4DDVBev/MGSRo7B+t2OQOEbdjabnIGyCqzEgF2HZ0xgEVBaXqG2w/i/+juu4AevX4byT1An3+7TL6shHesB5WryT+gKrt2aXFaEFdOTiJxrX42vqG+xhhdZTTe9f69GAMeoHrH6HYLWkh9Bm85OXsQ923JlpatG1I9rgCmoXLmvOdypjK7VEPQq+ak3/RkvwIxupa8HT3dAmtLkiTsHyPuf1UI9duxTVjiY4F/zjyVfplNGP+n8sa6/r59EFU97uvLOwS7wu7sAmaAt+rvOB4HtUFoPRJA9gY4/EIPEWVj65nkVraJcySX2lNgRft7/oBDJaQLW/OQbkFmysLnJkByx7FoP1shM2wXAMYJdQWXdqq95yfauO9cqSilCtamxRaelPUHYlBo3Qe1mf2mXYBPpwaJP+OPdQo9R76jlqb4f7sJPROF3gM8kX+hBpHs2nfD61t1O96fdMyjRWZSK/oqKioqLGH/PZhiBtfA+zAAAAAElFTkSuQmCC>

[image6]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACsAAAAYCAYAAABjswTDAAACIUlEQVR4Xu2VPUhWYRTH/36QZhFJk0a0BKVRoA3mBwghFuVQkDi59DFFg4sGQZBLNPgBgiU6vJANumilNIiDEX1JHxC0RLVEREsR5KBI/U/nuXne432vtjjI/cEP7nPO89x73uc997lASkrKhtNIH9N5+pSO01J6l+4w85Loo2M+aKijz+kS/U3f0hnoc7+EWGRZWLOKLvqaHjGxQ/QDfWZiSdTQZeiD10IKlIL2+ARppb9ovk8Ix6ALK3yCjNAeH4xBdv4c/Ug/uZynhC5A/704CukrH4y4DS12i0+QftrkgzFcpkXQAuQvLshOZ3Ec+ryrJrabXgnX2+iwyWUhfSaLpehK5Nj+BKQPD4frSazRb+QmdE51GOfRIXrp34wEDtBvWGns7/QhPWEn5aCYnjdj2RG5h+19zxNkv0iRB+2kJKTnTtNu+hK6+CfdayfF0EG3mvEN6NoWE7PI6SJt8sDEpMjPZpyTuGKkDQagD5WXJhf10Lff75B40cyznIHmO01sP82YcSw76bQPBhqgN23ziYD8/aPQfrO0Q9ddc/GIaBNqTWw73WXGsZylL3wwMEjfQYuKo5dW+SA5CS3mlk8E5J4/oMfTfyE3lIUXTEyOn+v0K91n4pZTdM4HA0ehxd7zCeg5Lrn7PrEeZmk5tH/u0Cn6CLprckJ45Ee9x0pf+rNwAvr5lNwi9CvVDP3oyPWbkBPlPpm/q1JSUlI2H38ABRp2JDI2zwcAAAAASUVORK5CYII=>

[image7]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAAAYCAYAAABKtPtEAAADpElEQVR4Xu2XW8iOWRTHl8GQGZNDwkQyjRA3KDcuRA4xFLkwM2pkBjmUck4OISRjmsKFSeYjZ5oGo+RMDhmZGmYuyRdzMaTGKAmJ9bP2fp/9rPd5v8/ld/H86t/37rX2s59nr7332usTKSkpcTRT9fTGQCvVdtUj1QPVXlW3XA9jgOqQ6oTqsGqeqnmuRxPkU9UE1RnVr84XOaDaquqsmqj6X3VP9UnSp6vqkuqz0Cagx1VbKj2aIKzQbdU21UspDsAwsYkxocgc1RvV94ltrWpx0oZeYuN+7OwtVKfFAsk4qHeuR57PVfVi/R6KLVab4Jse2i+C/1poo7Ohf60dm+OZFAdgqdgk5ic2BuNl7ILIbtXBpA0dxfrxt4hvVVfF+oxwvpQpqieq696R8I/qL28Ue5bxL3iHp1YAFogNUJfY2gcbOSGyJtiOqFoH20LV0UqPavaLrSDPfed8kS+C6LPB+SLsEPw/eIfYMcX31Ds8tQLQUjVV8ud9qNig5xMb250xsN9SLRHbjl2SPikfqP4U8/PM+rz7HYw5SrLgDs+7K8QjOd47JFvAHd7hqRWAIrgRGHS0s/OB6bneLHbeixgk2UfxzL7EF4m74rLY98Wd5flF9Vz1UWIjsAT1lepnVdvEV8j7BqCP6rVqpXcom8RWi3wQg1CX65GxSPVV+P27WC5IYdeRa/hwchBJrQh2KPnhP9UesUnzbadU91WTsq4NQwAaOq9A5mV7F01quWpX0h6iuiO1t+ZJVafwm1yQ5pO+qhnh9zixMZZl7hzsOvzkG88KscWa6R1FNBYArkE+dKdUFzckRZLMQGensMLuzzeF1ZWkjZ9JtAttrtN47bKr8A0Obc86Mb9/N8QEyA3RKATgmDcmrBYLEMkr8mP421/sRekZjLAtNzrbSLGJRbgO4yTI+GlFSiJ9LNVBj+Dnrk/rlAg7iXHrnb0QAkDlVgTn6IbkJ/ihWIEEnFOS0JjMXYGbYqyzcV0x0QjFFh86V/V1YmcFSWKU1UXwXvy+/oj8JDZuUb7KwZZkAuekOpL9xLL0H2KJ6KZYcqHyYnUjs1X/qr4R2yVUfyS6tA90EMsNrE6kh9iH+omSJLFTKxTxpZh/lrPzjnhTkZDTXZuDBEJ1xeTpjO6KTTSWr5S70edFxk8h2XFMGPM31eTER7a+KFmtQG5YFXx8IEHpHtocCUr0+B76spNYKJim+jvxUwHG8hfxDxvB9Nd0SUlJSUlJichbm1PmpHfxzmgAAAAASUVORK5CYII=>

[image8]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADgAAAAVCAYAAAATtC32AAADfUlEQVR4Xu1WWaiNURRe5nlOIXKFUEKEFJlJKNQ1l4yJDBmKxKVQFC9SygOZEikPyqwrM4VQxlzhwZQxhMT3nbXXOevf/Sd5cVPnq6/z77XWv8/+117r21ukgAL+G/QFr4Kfwu/0pDuDWWBXN64NbgB7OpuhO3gQPAMeDeNyQzfwAdgPbA7uB3+BS1wMcTrYPXeBVX0QUAy+kNyHLwKfg5WyEf8YJ8GhblwNLAO/gI2dnbtxA3wHHgfnO5+hKfgRXOZsl0ST0cLZiMrgCfCD5BLWLhGRRBvwiWjcS9F11wy+GWH8LfgvhrFUFC3Lh2C9EExsEw30pXoKLHLjNLBk+V4zZxsGlrhxjGngBdH3BkU+j0nge/By7HBgpdz2BmbxtejkzJBhc7DNczZmpMiN03BfdL6/wT7RHYgT6jE8kDHrI5+B66d/U+zoDA6IbCwdBg90Nn7gVPAweBO8DvZy/vqi79wBJ4JnRctzOVjFxXmwgjhXE9F31yXdGbQFh4BrRGPitRrmiPpHxo4YrURr+TxYwdmPgGvdeItoTzQM49aif0CB2SpaHSz7a+COEBODArc9PLMX9zqfwXb1HPgZrO58HofAr2Ct2BGDyvhUVFE9OkXjDpLMescw/gk2siBgQbB3cTYDlXpCeL4i2oseU0TXUQf8LkE4UsAKYX9SAHeLrmllIiKAf/ZWdLF/AjPFhVtTU1g4fpyNUFAc8vUOz0hTavbiK+djAmeG5xGic7Dc08CypX9x7PBguZRJ+qE8RlSaxzoby5eT2qJ4HnIcq9y4YGfZevA4YhsYmHXGsZeJpZJrkY3B1yOMY7B16PcXkQR4Rj0C+zhbf3BUeLYGL8m5s6JCITFQeHhWekyW9B0cLLpwA48LWyQVk1pg4Ln2RvJfFujnBnjNyIJNy+uZ9YJhheQUaTS4R5Jq2Ft0QaucjSXEg97HLRSN84pMUM75IQYmlHFzRVXYUBf8AR5wNg/2J/28gaVip2hzsoFLwbui2eKftQ8xzMwxyV2/KO9UVcbWCDaCzzwmVou+w12milJxfXapvKwY9pmhpeh/xh/CxNPOszIN40X9syN7Rt2ZHTrz0d8zeQ2jpPNOykVTbRs4v4HnFuX+nqgaU0UN3NlSUbnn/NxtqwAmjR9tVzqW7K0QZ7G8LrJ3CZ7JTKb5KXbcJOOzEFdAAQWUE34Dan/biUVvjHUAAAAASUVORK5CYII=>

[image9]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAVCAYAAAB7R6/OAAAAy0lEQVR4Xr3QLQ9BYRjG8cdLsCGYQmTTFKqmCbKmK4qXIhFN8gUwxUsxZJtiqu4DqGxmU/g/59z3EAWu7Xd27nNfe3bOMeavyWCKNWaowqfLOLZIyuzBEn0tdNDQQZLCXYcRJq+dkygeOrRlmCMgz2pYaMEed5XSAU3sENOCTR5nKVk9+L8qdI37LvaFtTTQZQtDHUgORyNfEcEF2beCTUKem7Rxm8GPtZuxvYRxQ+Fz52SjNxWcUIYXIdSNnKApGvfP7bFC6X35wzwB4zgru/HWis0AAAAASUVORK5CYII=>

[image10]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABEAAAAUCAYAAABroNZJAAABJUlEQVR4Xt2TsUuCURTFTzQImUuDQyDY2N5f0Fwg6Fza4CI0RntbkFO4Ci4uQeUaNTg5RoMRIS5tBUEQQUud+9336fXao7kO/IZ3zvvOezzuB/x7LZA1bxplyAG5IVfk2IarpBSCcxsY5aAfd8giyZPHNNwnd+SUfCJeIqe+QG8j2iJf03iqd/xcskI+SMt4WejBc4qV7EJPlVv/qljJCbSkTi5Jn5yRDbspVaykDS15IOvB2yFvkx1GsZIutKRpPBmHZ7OeSEouvAl9UCnZc/7QrRPFSo6gJdvOl9GYk5TIw3mVoSUV59+7dSIp6XmTWiKv5NB48iYyfDOSSZSBuoZu8KqRMSmEdZU8peEmGUAL5MrCCPofLaebghohu4XmxZn07+sbf8pCg31pNlsAAAAASUVORK5CYII=>

[image11]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAC8AAAAVCAYAAADWxrdnAAACvUlEQVR4Xu2WWajNURTGP3PIzAOSJB5kypDiwYMyP0gkQ5IkD2R6QYYbJVMp4tUtUaaUB5KhyCw8oHiREEohRSHxfWftdc7e2xluKUrnq1/37PXt8/+vs/faa1+grrr+WKPJCXKZnAvjatpKGvPgv9Bs8paMCeM15BVpUZyRqh/5TA7nxt9WT/KJrItiN8lP0ieKxTpNfqBy8sPIDfId9pwXqf2bpsPmiSfkYIi3JefJ4+C9IRcCBe0MRi8PUFPIlmgcayI5gKat/CFyHfb81pnn6kI2wObszjzXUpg/Jzeeknd5sIJURtqVHmha8g/IetiLB2Seay7ZCJszOfNcjbCd7hoHO8O+9IjMI1dgyemFraJ5Lq3A2vC5VvJDYS/VaukdkxLXNJP0hjWJL6RdahfUDFYut3KjP+zBOqwqhZakE7kL2/JYHcltlH5UreRXk4VkJOwdy1IbHcis8PcbojrONAL2/YYsjsEwQ1vSLYqvDPHhUUxnY0Y0rpX8Gdg50mLoWTtSu7C7KsOpMH9TahelRiL/GmyBN5M9MvRwGc+KU03zQ3x7GPclZ0t2QdWS1+6o/FyvybFovAi2I9Iu2LvGFd1UF8l7WFUkUgfQF/N68jrVL5WOk0Elu6BqyY9HWJ2gq+RO+NyeLI88/ciPKJMcbO5Xcio3XPdhXSHWAqQrr0Q1rsTiMM+1Dda7XY3kQ/i8BKVzo3Oku6BSctNgz1+RGy7Vni6puLusgn1pQhSL1R3mV1p5HWwl5lKdar52VF3IVSs5v4OG5IZLt5haZQOsLal9qtvsD+Ny8rNyNDeoUeQ50n8t1HXKJambVPGBWdz1EHZeciX3gS6QI7CrWVe5uk0lnYSVgJfMPVjHUNJacXUuxfVSL52xsFbYPIz3kpdhns/dF7w25BLsHMhT//d/CYRKPD+jddX13+sXDDSy5EfCuHcAAAAASUVORK5CYII=>

[image12]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAH0AAAAYCAYAAADXufLMAAAEsElEQVR4Xu2Ye+ifUxzHP8w1IyO5xR+m3GJzSchmaGnulyhbkRlDiHLPmjBsmcImScltjaSJKcyyuczKJZRLSqvNXQnZGonPa59z9pzv53eey2/bt5/VedW7vs/5nO95znOe87mcR6RQKBQKhUKhsNmypeol1UrVv6p/VO+q9lUNV72u+iHYflO9odpm3T9FJqv+DLbfVbNDextbqd5XXe0NCVeoPhcbG70tdu+PVb8m7Z/FPwwRj6iWSzWfVarFqpPSTpuI+1XfSnWvy3rNPWytekus3x/h93GJfR03i3WY4g3KXDHbad6gnK16WrW9NzRwg9h493qDgzHXqL70hsBDqld84xBwlNjzvOgNfWCkmFNyvzudLYX3gsP+rdrZ2dZzidhAvBDPC2K2C137MLEosaNrb+IIsQjBeE86mwdvod8sbwicrHrANw4B14rNc5I39IErVVep1kr9+o1SHSkWhYmQtZwpee87UfVwsPlwfI3qAtfWBGGdBWKTMB47sYkZYv3GJm2nqiYkvy9KbDl2VS2UKswRiqdJlaJSSHW3+cYOvCw2/i7e0AeeU+2v+lS11NmAsH65mEMwp+m95l7GiHV6NGljgItV5wfbHYltT9WC5LoLvKCYBqgB2vLxe6qfxDYLsFnI6Yeu79EOOfdx1bFi45wglo4+Uh2e9AM2MV40GHgenuVDb+gDW0h1H1IJ+d2DEzKne8Te2bgeq+NgsU7PJ22EcwaIYRaPjzyrOiC5buMg6S1uvhIrDOvgBZOPuG+q3IM2Md83BC5V/axaJFYbUFguUW2bdurAKWLzmukNfeBosU0MFHXcl2I7MlqqKEjeJ7xvV5kHsrvYIFTIsJfqjPAbj8DGiwby/+3hdxdyYfMdsTHr6gHujf2mpI189kxy3QUWoo6dxCIZ4X68VBFlMMTF5+UPlvtUT4iloC7cqjov/GYtuO9h4RrnjBsvOsyr4boWHphBPgjXeEKE4xs2BmFzvCn5nFjHLTLQY6PqogUFGvZjkjaOKBSBg4WwzWaeJ7Zhm+aeO6E0QbilqNrBG5TdpHfTphCqiXTpi2uDGijWDTHCnBWuWRfuB6cHW64oH8AvqhVi5znCcQSPYBBCICH++MTWBuNM943KY2JjUijm+ERsUVLv29tdd+FcsfzNAlDcUBxS0eaegePQ3b6xgT3EnoFzeY67VOf4xgQ2PHPqAmGc6BihmOPe16n2kyqsAx6PjZNSK1+InYununZ25V9i1S9n9q7wgjhH5/IKoY2JTfQGsdSCjePgxoJ3ey9kwUgT2KgzWFBeODn9wKRfG8ydeebOy3jv12LF8KYAj07rBsYlhD8oAz06FsCk1VY4Anwn+cP8SrGHwOu7wA3xKpSDsFcXgth02G70hg2Aj051sJAUlKtVr0nmi1ULnAKY5zjXzscavmLmNsOGwFpSrXtnZO58tRyRtO0jthmeStoaYWDOeDnI9eSKLsxR/Si2IOS7tD7Ao8ivq4L9+3BNrXB9+L0i2PgvYZ72/xOkJubJUY15UikT4r8R+4xNGzok/mEjwJPjWvIy0y+QHJljDcLHmGViaxb74vFU/IVCoVAoFAqFQqFQ2Dz4D11kEVl8+u8jAAAAAElFTkSuQmCC>

[image13]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAA0AAAAYCAYAAAAh8HdUAAAAsklEQVR4XmNgGAVYgRUQ7wbiV0D8H4i/A/EZIJ6HrAgXWM8A0WSMLoELMDFAbLqBLoEPWDBAbJmMLoEP1DBANAWhS+ADh4D4FxDzo0vgAiCFfxkgGokGICeBnNaELgEFxUAsji44hQGiyRldAgiEgPgouiAIXAfin0DMjSbOCMQLgTgbTZxBnQFiCyhFIANOIJ4ExJ8ZkALHjgGi8AIDRNMzKP8cEH+AioHwMpiGUUB3AADC8SRQwDvm9gAAAABJRU5ErkJggg==>

[image14]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABUAAAAYCAYAAAAVibZIAAABOklEQVR4Xu2TPSwFQRSFT/wVXhAtNSUFFYWSoBKJaESheAWFkqDRCBKJQuu1Co1WopAgNFqlkGipFEQ415l5mZkdOoVkv+TL7sy5M29n9z6g5F/STy/pO/2kD3FcYBKqM+/oQRzHHNILqLglyTyddBWq2UmyLLd0BVrQk2SeWboG1YwlWYE+WqMz0ILRKBVTtJue0VfaGsdFlukcHYA2rcYx2ui0u77R0zjOc0K7aAe06VYcf7+WRjoO5etxXKSZXgXjJ3oUjOehExjb0KbD9fQHRuhuMD6nN+6+QheDzH78hTYFc1k2od7z1Oizu1+ATmK0Q7187Ma/cg0t8GxAR7ROsK7wTLj5pWAuyyC9hz6Cx7ogt9j+OTbfm8zXsc3sCT+gQvs4/hUMQS3T4MZ79NHV+dp9l5WU/BVfKrw/RABl4xEAAAAASUVORK5CYII=>

[image15]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABMAAAAXCAYAAADpwXTaAAABNklEQVR4Xu2TvUoDQRSFrwoxRRoN+BMQEUlhY+EjWKS2SakWYpPCVLa+gI2Ib6CipPAnCIrRTiWdYm9lY2GVIAEbPXfuuMwed17AeOAr5pudwy47V6RvMwBmWAZZBpfgATTAdHrbUgJLoAVOae8nNfAExvx6C7wFa5cN8Az2wKdkl42DHqgGbhC8gs3ApfIh2WVr4AvMk78BV+SSxMr0rbVsivwJ6II8eZdYmTotmyB/7P0seZdY2bVklx16P0feJVZ2IdllB96Xybto2RlLZF/s0CT5I++L5F1iZTtih/hC67Pq9Zr8ipads0RWxQ4tkL8D9+SSaFmTJTICOmAlcDnwDuqBSzIsdstvxWaUo+OmMznk1+vgUeiOLYK2WJF+ivIiNqeF4DlNBWyL/ZBdMJre/s/fyDcBakYFAI5ZRwAAAABJRU5ErkJggg==>

[image16]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADgAAAAYCAYAAACvKj4oAAACrUlEQVR4Xu2WS6hNURjH/97k/SxioLzyLI9SktfAuxgIdUshZWCEKPKmRAZKyegmxMCAMvAqxQApJORdiAEhQpT4//33Zt3vnHvvOQx0dH716577fXuttdfe315rAVWqVAwT6FX6Ifu7pG66shlF79OJtDc9Sr/TVck1Fc1ZOjX5vxV9Qj/R7kk8pQmdSw/TW/ADukxPwG0G0oO/rjZt6Gl6G36AL+Gxc+/Qa3RR3qABVtPhMViMpnBZPqAdk/h++CaKlepgeoV+oZtp3yQ3iJ6nr+j6JJ6yDO57fojrwd7MclNCLrKFjo3BYjSHb0ad9kvie7LYiiQmVM66/iEdEHI5y+G242Iio5Z+o11CXOyG266MicA2lDhBMYJODrEzKHySesN6089oryQeGU9f02YxAZe2SlPlHOlEH9PPtH/IRbajjAlGVHIqv0vwDeXsgCetEmuIIXRrDGaMhPvYFOLT4e9P3/6MkCvGX01Qi8NTeEXN0Rv7SN/QFkm8XNbCE9TD20c3wGWpfrVgtf99aYP88QQXwoMNDfEa+MaOhXi5nIP717ef0hN+ezdQd5Lz4HFL9Stt97NlEbSAaJAxMUE2wh2siQmyGIUDqcTbphdl/yt+PMRzdsFtG/sERNlvsA+8MmqByJlE52S/F8CD17f0i6XwNQdoy5ATM+F8XJlzDsF5PbDGKGuCreHjmcozZR2dnf3W5v0CXl3ThSflFHyDo2MiYyecHxYT8JbzHh6jW8gVo6wJ1tK38GniAr0LL/O6GW3cOdPgJfwkXM5ds3gPupc+gk809aETjyYQUb9a1J7DZ+JSKHmCHVD4/aTGUtP+dITeg78nbdgX6Sx4n9SikKLTiU427+D+dPxLj2fX4T1R20rnrE0plDzBSuW/n6AOEzq8V6lSpcq/4wdaMaboK/1nwwAAAABJRU5ErkJggg==>

[image17]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACsAAAAYCAYAAABjswTDAAACG0lEQVR4Xu2WT0gVURTGvxALM9pEUC0EUUJQCJJaRAsRdN3ORbkxDNxoayPBpUjRImjVPyXc9Q9yYbkQRDQXgosEUYyIlBYVggm1qO/jzLx3OzPznOUL5gc/5J1z59wzM/feESgoSOUQbfTBauMMvULf0hcuV1UM0lV6n/7Cwc0+oCv0T+QGfRTkT9JvUW6fLtDWIC8uwh7MF9i4D9Hv2E/R30vxBWns4eBmRTtskimfIEdgzd+hx13OMwmrc9rFm2A3qps+4XIl8jY7BJvkhovrCT6kp1w8iy363gcjlmBzXPCJmLzNvoYVag5iV2FL5FgQq0QbrMa4T6D85tZojcuVyNNsHd2FrVVRT+/RvtKIfNyENdQZxA7TfvqVztGzQS5Bnma7YJPoKaqYXqN+D4eDcvAGdt1zepfepo/pdzoajMtEzb70QYdemyaZpiO0AbYZPsKeTB6Owq5Jm6uD/qYTLp4gT7M6tlRMazRGm0o3EMYq0Q0br6WQxiIs3+ITIWr2lQ8G6OOhIlpPIToFFF928SzGYOPP+UTENpIbOIGa1U7PQkeVitzyCfIOyQ2ThT4UaiiNa7A6+ihkosNc62gW9j9CGs9ghS77BOlBeS1XIn47qhVSCzu/f8LOWI1LoCehNaJGVURuwu4sPjOf0J0gP0+fRjlxnf4I8loOvUFenIfNo0+6xnzGv5/YdTpDB1DhbC0oKCj4j/gLN9CJE1pjVTQAAAAASUVORK5CYII=>

[image18]: <data:image/png;base64,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>

[image19]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAoAAAAWCAYAAAD5Jg1dAAAAxUlEQVR4XuXRPQtBYRjG8ZvBIBbZFcloNpBYKaMPoExmi6+AzcKofASbpKSMPgJlYMBgkvg/zov7nCwmg6t+w7nO1dN5Efl50mgioroCeur6lRIePgfk9cikjB322GCIjGdhx5w48pefUpQvhhN0McURfQTU5pUctkja1ymc0XYXdqJI+LoxLog7hTl+jhWCTkk6Yn2mllOEcMcVYacU63nNsKI6WSOrC7Fe6oSYLqsYyPtE8/tuaLgLlTpmWGCJmvf2P+YJP44j2u0MJQkAAAAASUVORK5CYII=>

[image20]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAGQAAAAYCAYAAAAMAljuAAAFZElEQVR4Xu2YZ4hlRRCFjznnnFjMOWDEH+qYc0IE82IWRcSECirGNQfEnMWcFcWMO5gDZhEzgytrBBMqKqL1TXXPq1tz3yz+mRnhHTjwuqrevd3V1VXVV+qhhx56+K/Y07hVFo4mljceapwzyDY2XhzGFfsaHze+ZLzHOKGpHsSixhuMk42vGg9uqsc9HjUunIWjic2M/yR+a9woGhkON76tzmRPNX4dxmBB47tyW7CU8RPjadVgnGNx42NZONrY3DjFOFXuzOuMKzYspEWMvxt3D7Lp5f87Psgukj8j4jDj98aZknxG41PGn9QJhPzeiOWMA3K7b4xPG2cvuoPK+I+i5wQzhs8U+9uMSxb7bjjaeEAWjjY4IddnYcKB8oWukeQs9okwHjDeHcagnsB84ipwwItymy2SLmJv44/GV7Ii4Evje1ko/y/PJ42OhOfUTN1jgj5Ne0Muly+IFBTxgPEX46zy2oHNNQ0LaZ0iPznJK+6QRzg2bHwbti/EZlLSVXCC0HNKM+aW637OioBljPdl4Vigz3infCEU7O+MlxinCzYPyheE0yPuKvJljeuW32xexOpFflOSA9Iedalu5tlN9SBoOuh6TpfbcOLaQN1Cv2NWGI6R60jH3XCKmil5zLCh8Qt5hACcS2o4YcjCc33bhtxe5CsbNym/84asUuTk8AxOT3UStYTnZdRT87zxV/lpbMP98jo3R5AxXzb5L+ONxrmCLuN1dX829Y/nkKInywN45oaF++Aquf4NDffDDPJu9jV593lLGQ8Dk5yQZDgPB9E1AVrBtg3BDjlRzMa2bQgTRc4EMo6T9/2ASVJLIibKCzFz/FNepNuAwwiiH4y3yp1HxD8pD7bdOqat2EC+Yd1A0JxXftfOca+OWifK69daQcYGE4xgNuPDxjfl/gD4lCaELDFNnCt34h5lzCIZLzZk4SBSkC8g75D4fUXDQlqtyEmDGaTIhcpvagntdgUTr3eYHeTPOKmjbqA2Dsdmhbx2/W08JCsCuHONdBmkTl5YfhMgVxrnLePaMOxUxpwynsXa6CQB5YCAIvtUcJrOCeNBUCf6jS+ruVMsgpccWcaXlvHSQxaOh4qc/3Ka+M2lMGK9IidiI2YxvhDGRDV2daG007WOnV9065dxxlly/dpZoU5BJ4LbwDvekaeUbuiXP+M3efqmXlZwEtCxqWcYr5af+pr+lpCnzEfKeESQB4kecnPt6QE7ykuITDCxjPOCcWhMMx/Lc3nEtvL/bpfkW8odXUH7W99BRxU3n3sFd5luTkPPXSM2IhU1ZQ4kecWmxsuyMIHLL40BqYu0+EHQ4WwCsxt2lr//qKzoBorMmknGcePF85fxfPKWcb8hC99MnBRfRKR+pKZjiPSvNPxiyKbj+Aocw8SPUDM/E+Esmk81baC+oKfjawNteNsJrSDFdrsj9cm/RtQ6B7g8fhjGn6m9e1tV7j8CMQZ3BJ+ohoE2kZxYTwhGLJC7QcQu8kisUUp+f0vNzoTfnBhsAcWMU7PPkIWDiX6qToEDE+QTz47HGcjzfCqoc+j5IhDBO0gf6Ggo2oona+HLQtvJAgTbVONKZUxQcQXgm14Fp5yUV1MtwPlcYJkDF83Pi13FPMYz5XWzFSzqWflNNTo0g2J1gbzIc8zrCYpARtdxs/Fa465Bx4L65SkSR3Hq+CYGcBibVC+fpDCchV21ZY7UHrC/8f2g54ZOF1Y5Rb65Wxf7NmyjTvfUBk4fm3mv/AvEJA2PdLIH76MrpaujxnAaY6DyhYM63S+35UId/dJDAV8ouAv1MA7AaaV+9jBOwL2hpssexgGoDStkYQ9jB4p0Dz308L/BvyAuQUbncOgeAAAAAElFTkSuQmCC>

[image21]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACgAAAAYCAYAAACIhL/AAAACeElEQVR4Xu2WSchPURjGH/OQDT5jJMn0hYWpKJGFYoWShWkhpW9BkSJDFiyESCwoU8iQOUWGLAxhQ1aGlChDKSIpG57ne9/7997Tvf9PX1m5T/3qvM8599z33P857/kDlf5DtSGDUzNoEblK7pMzZFC+u1l9yUFymzwky/LdrVN/MpvcIBeSvkxN5Anp7fEm8iHEUgN5ChsrDSQvyeZsQGu0AjbpXvITxQn2IT/IvOC1JW/JmuDthM0VtZx8Ih2CN5k8J7/I+eC3qO8oTnApbLIxiX+TXAvxa3I6xNJ02LNTEn+E++sSv67KEtTX1WT6yaK0+m+kM2zvacz+3AhgnPsbEl9fVv7ExK+rsgTlaTIlEXXK/SFkvLe1mKjR7h9O/LPkHWyr/LXKEryO4gRPuD+STPV2mmCj+8eDp/34mRzxeAA5RO6QVe4VqizBKyhOUC+VP5RM8naaoJKXfzR409xbAFuY9qH26BvYr1IqJXgxNaljsAn7Jf5J93uS4d7elxsBjHJ/V/C2ujeX7HBP+1sHTONLVZbgbtiEaRHXWPnaR6qBaqtIR01wf2PwHrn3EXZ4eoW+ulKCl1KTWgKbcGzi3yX3QvyCnAuxNBP27CyPs9O+nnSFve8ZrBK0KCV4OTWp7uQrWRy8jrACvDJ4W2AFWFdmJhXy9/hTqOfDEtSpl3RRKFbi2stpOaqpE+y2uIX8CzLpKtQd3M5j3bGPkV+52vqiGit1gX3VhbURdoi+kPYeawG6wbRNtpFh7tekSv8AlpxWIl7B7uVuYZw0g2yHHZo9pEe+u1ny1sJKyAEyJ9drfyL0bCZ9ORV85bA6+JUqVar0r/UbKl6UDCE+nREAAAAASUVORK5CYII=>

[image22]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACsAAAAXCAYAAACS5bYWAAABc0lEQVR4Xu2VPyhHURTHvyyKMPmT2IQwoCwUk0nKJmWyKINRVoPFJPlTkpmdhYUksiisJItFNoOJ7/2de5/zu7/38hvc31vupz71e+ec1znvvvu7D4hEMmmm77TTT1SANrpNr+gpnStOl2KKv2mPnwhML32lC/Z6jH7S/qTCY5h+IZ9hr+mJul6HzJG5upd0E1LU7eVCMgnpOatiXZC33KhiCTN0jS6j8iu7D+k54CfSqIFs6lrkM+wtpOcEvaA3dIe26CLHCp23v/MY9gnS8xjy2qvoHr2ndaoOTZBVNQWGPIZ9g/ScVrFBG1tSscITjKvrPIZ9hPTUZ3u7jZk/fQEz/WGSFsoddpGelemWvSeLc0jPBhVrtbEHF3CDZfnsCgOzAelntqSjw8aSlU3D3fjXyv4no5CeIyo2ZGOrKlaCOTJMUZ+fCMwB5DQwx2c15Ow1q5r6UZiid/jdAh/0qKgiLPWQT6zZei90FzJ4JBKJKH4AU8JcVt/h6TIAAAAASUVORK5CYII=>

[image23]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEcAAAAVCAYAAAAU9vPjAAADlklEQVR4Xu2XWahOURTHlzlDppCxrgcZy5AxcW944AUZMiYzGcpUhgwhU4pMSYYMIWWOCLnX8GLI8CIlpSTxYAwh8f/ftfe5+1v3fPe7X5dE51f/+vZa6+yzz957rb0/kYSEhD/IAOg+dA06B82GyqdEpKcHdFL02QKod4r3H6cDdBOq5dp9oJ/QtigiPd2hF1BX1+4HfRHt479gu+hkjHTtctBH6DNUwwel4Q601dgOQ2eNLaQ6NA06Dz2B7kIPoM1QBWgItDiKVnpCl6H3omPl2K5D/UXHexF65Hw/oNvQhsIny8hq0Q45KMKXfRV9UUmT00I0Zpaxr4A+QBWNnQyEnkMvoYlQncDH1OZEcSy5gT2E6ct3NrcOMFrUt8g6ykqz4DfTjC+5EtjiGCEaN9bY5zq7/UBOBu2saTWNz3NEdHdUsg7RRXsN3bMOxyHR/rtYx++CdYcpwXSpb3yWeaKD4SSFzHR2ToanM/QNuiTxH+5ZAp2yRkc30X7XW4foLn0nuiNLe5BkBbfjQ9F8zkt1xbJU4idnurOzrnhY8JkubQNbHEyN4dboWCbab1/rEN2l9B21DlAPuiDqX+Bs46AxUUQW8CG78nEslPjJ4aTQPsm1B7t2uh1RWliA2U9JmhFFF3EDOgZtEt25rFt7UyKyhGnFI7mVdQRMER2QP+U8HCDtvsDvcm1buLOhtmgfV63DUSDqb23spEnwm/4dkpraPG3tJFOFNIUa+YZjn2jAWmMPGSoaM97Y5zt7nmuzsLPNy6JlvxQfVH4Y4Bgm6ltpHaJXA6Ys71slwXTfKFrYS0VD6LvoLmFueg6IDoZbMR05ojFzjH2daH9cbcKVYlxuFFGcg6IxTMW4wfs+4urNIFHfbutwsEDvhJZbRyZyRDt+CzUI7D6/+WJPL2hU0Cb8y2AHxdPueNDOE+2LBTWOatArp8rG5+EFj3evqtYhepNn/3EFlunDI96nOOFFk/WP780Ii9UeKbq0TRV92WkpOhb9rZn2Ts5GeBl7CjV27TbQG6hlFKFw1fjsKtETyw+so+jtlilhJ9nDusdn863Dwf+E9IcXSg+/jWnN2zWzgAvEuuVPrYxUgbaIrhyv3azunCDOcAjtjKlr7Bw8awGPUW7f9qnuCP7f4mnxTPRjPkEnoHZOvMeEsH1LNO0ZzwnkRzK1uGhnRHc8fRR9awqfVCZAk91v1rvHon9X7AGSkJCQ8Nf5BV1P3g1YU4RTAAAAAElFTkSuQmCC>

[image24]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACwAAAAVCAYAAAA98QxkAAACdklEQVR4Xu2VS6hOURTH/16Rx0QSSojULddABmZkRFHEwEBeCVEGlAyQPErChMSEYkAeeQxEQkkoA0IkXQoTUR6FuAP+f2uv79vfuvvce01uqe9Xv75z1trfPmufsx9AkyZd0ouOi8GM/nQTvUmv0720T0OLHmIUnQcr4kLIOUNghZ6AFTmcvqRr8kY9wXr6mB6iv1BdsN7mR9hbFnPob3q01qIjY+g2eou+ovfoE7oi5ffR6enaWQl7cT9h/d9N97ID31AueCj9QQ9nsUGwQU7KYjlb6Xf6kM5CfaD6Ojvog5QfkOKRd7DBdUpVwUtho9XX6A7HYO23096Nqb/0pW/plZhITID9f39MRKoK1h/VwSp6id6m5+jUvFFCg/JiO+Mq3RCDibWwPubGRKSq4OOwDl7QlhRbQr+icUpoIX6hbahPgSr20IkxmDgPm4Kads6I7LpGVcGnYAUfyGLaAj/Qs1lMebWrenPdoR/9TD/Rk3Q3bD1cyxs5KvhiDMIWmwrx1e08g+0sg9P9c1i7ybUW/85MWB8bY6JEVcE7UZ5T2g4VHwlbSLpWHyW0rSmfq90isguWmxITJfQwLarIAlgnC0Pc36jPNW1D7SjvDM4b2FfxtRDRvvseNuW6RAVfjkEyEDanNmcxdaiDRKefswU2gBlZLGcaLF9aJ0KnqQZ8OiZKaFVrZd5AeXTL6Ws6Ot0vg23urd4ANi20ODS4xXQ8bBHpsJhN78OeoR2mxCLYgOJxr4PriN9okntHPre0Leko9MXkrEu5R7D82Iasoemwmt6BfVr1p9+DdBidDysgRy/jKerP19Ty41jqkDlTa92kyX/KH4yvmcY3IqurAAAAAElFTkSuQmCC>

[image25]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACwAAAAVCAYAAAA98QxkAAACrUlEQVR4Xu2WS6iNURTH/97yjDzriiKRkozMXCYYSJgoIhJSFAYM0PFMeSUGBh6JvEokiYg8Qh5J1B2hMGGChJD4/+/a+9z1Ld8pJrfU+dW/831r7b3Pfqy19gfUqVOTKdRj6gZ1gVpGtS20ADpR+6l31GvqGNVQaNFKjKFuUz3T+0TqF7W32sI4kWz9qenUR+ol1cM3ag32wSY4K723oT5RX6huyTYBtvvyZZbC+m13tshgaj11nXpB3aWeUguSfwc1Pj1nFlJXqG+w8e+kd6mZTdRPakZ616Ry4zzh1dR3akV6FwoHtdEul7EOtmiF2mRYSIl21EbqQfJ3TvbIG9jiShnknhUimshVZ1uZbIedrVeyKaYjh2C+Cv7MBdEelgcXoyMxDNZ/Z3REFMfnYavv6+wdqHkoxquOUoNeczaxPNkrwR65BNuIMnK4TY0OzxrqCeyYGouuUlQxNOgkZ+sHS8bnaAmBWmyjhkdj4gz1lerqbAPcc4HZsInkxChjBCzuFaeeXbC+tXbub9BpfqDeU0epLbD/uewbRRQSWqEmFukCOwkfz5km2IRHR8c/kMvqqujIKNsHBltOmq3BrgpynDoIy3aPEkl9Pgd7RmVNfi9Vi8hmmG9sdAjFxQ/YbvZx9iOwTjpiT4U6h2Lm73bPKkMar6wyZF7BSuTI6Eio7r5FseZXGQKbmOJFCZO5mezTnG0mdR/FROgIu1Aya2H9Gp3NMw7mPxsdie6wBZ+MDs8p6gDsSMUi2KB+J0fBsv8R7MZ5CNspXTBKjIzGUHJoA+ZQQ2FJpPDR98o92GnOzR0Cum3130uCvTesKjWj8rMHdgzawVuwSfsY1fUb4y9rg2sntMjFsO8Tjak2+tV3iMJO3yGagGc+9QwtYyq08nUs6ZI5XW1dp85/ym8anqUO54QqSgAAAABJRU5ErkJggg==>

[image26]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACAAAAAVCAYAAAAnzezqAAABr0lEQVR4Xu2UPShFYRjH/0KIxGqzIIPNwCJGEoNdWZCy2JSkyEjCpGRSFh+lSPItm9goZWFRsqCUeP7neY/73sd7uQsp91e/7j3/89z3POe+H0CGP0iWWG7D36BMbBe3xGVz78fpF8/FafEFqRs4Fd/EB/FAPHPX9Bba/LW73tCfYEo8chnHZk3ssatJ4hHhBuqgTfIzpgE68IKX5Yjz4qSXVUHr+ILfkqqBUegDfYagA3eavEfs9a67oHVtXpaSUANcmJsmI7vQgbl+fMbERu96EVpX6mWE434i1ECuWGmyIuiccl1YKsQC9z1bvBO3E7cjasQlk0WEGgjRDH0rf65D1EPrLsU5cUQcFi/EE6/uAzawYsMA40hvXvkw1tWanIt13WQR6TbA7jkFhfaGYU+8h06FpdUGhA2s2tBQAn2rHXvDUCy+IsVcC902IGxgzYaGDmgD/Hu/ogVa12dvCPnQcyWJPPEZumKDW8TBxcSB/a0WYgZaV21y7qh96LMimqBzyoA/oFfQ45LbjQy460Ov5gl6nA66mpgJ8QaJOh7b8fHLNcEdMQs9ITP8c94BlStshpcb5gcAAAAASUVORK5CYII=>

[image27]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAC0AAAAVCAYAAADSM2daAAACbklEQVR4Xu2WS6iNURTH/95EuAMlMVBeyWNwKZQ8o8hjYGJCMfC4jEgiiUgmSMhAuRiQkYnySK5nMhETeeRKMSGUIkr81/3v3VnfOt857kTdW+dXv87da63z7X32WWfvCzRo0Cnm0Mf0W3pdX0x3PZrpSzqXjqSX6B+63dV0OW7SxW7cj7bT73SYiz+BPsxXeo8+TWPzA/Sct2l8TW/BcfogxX6lmuxD+oYehuasxwy6OQ96Qi3xig7JQXIamii3yUz6LL1mrKWs5pyL9aZn6TEXmwDVnXCxTJ5nf0wEFtIdeWCTfITeOCYHyZEU25rGB6BFevZANWtDfCPd5MbroLoVLpZZDuWuxERgEdyijal0vg+QG9DDFtAe9Hox3UEbVDMixA/SeW58EaprcjHDnmuLtdyakItY+xYWHRlNf9L70IP70PGFCmAQ1KPW55FxdED6uxf0Td6qpDuYQi9Drbkz5Mr456LP03fQSVKLJdAO+d4tYxZUZ6fTGbqP7qUv6CM6uVJal7qLXk0/00kxETiE2n3qsQVa3fQQt9/SVfqJTnPx4VB9PQvYed2O6gnKsF2y9hgYE4E70CZYm0SWQYu4EBMllO70KPqaznYx+zGtdOPMUGiy2zERGEx/Q71bxgboOa0hXkbVovtDV7e1hmc3tBuRVdBk9tXXYylU1xIT0Jx27lvenzS1qFp0K/0C3VJt9DnUa/ZAuxgi9oPqzGQnobqJIW4n0V36g24JuVoUFm1fYWx0b99Utw36UHYM5pxd83YV70o1maP0PSp1duXnq9t63E6SUyjfkFpU7XR3oFsu2v4vGhuDDf4nfwGowpwGBAA/bgAAAABJRU5ErkJggg==>

[image28]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAC4AAAAVCAYAAAA5BNxZAAACj0lEQVR4Xu2VS4iOYRTH/8htiHJrmpoFcolZKJQwGywohYWysJIUC1ZIqUnKwoJyK1JyjRVDFshKxILkUsyQBZkpkksSEv//d55n5nzn88036ZNPzb9+9T7nPO/7nvc9lwfoU5/+WP3I+GisZTWQ5eQaOR98NauN5AE5QL6hd4ErMyvIafKQtJHbpJWMJVPIibR3GnlEfpCbyVZ1fUblwBXIHfKV7EBxaU0l18kbst3ZR5KfZLezVVWVAp8JC+oZmRx8WethQc5ztmXJNt/ZqqqeAtdfaycvYT1RTs3kLRngbPthH6wS+yvqKfBdsL+2LjqCppOdwfaE3II1v3pBa2VPqieHYVlU36wl98ljcpEMS/uyZoV1QeUC1x+W7x0ZGHyVNAn2wffStaRaV6B61lFY3ygr78khUpf2KcN70rWkHupw6y4puAvRSK2GvfxcdPRCypDuneBsW5KtiVxKtiuwrHhprb+ftRkWY4nKBd4Ce9HW6KDWwHweTZyc4pPkabrOOg7bNyqth8BGsaZUVn/ykdxwtkWw+0qkwDWLo1bBbvAjLkq1qT1HyCBnf0UOurUa9AWszrPmwu5VY2fNSTZfKtKSsC5IgashonSovCZXUX4yXIa9KDZPJ9ng1gth+1R+WSod1bfvH/XBB9KY1mdhB2WJBpMvsAPkd8Ethvn1YZoIo5N9HNlHnsNO0CidrnvT9RjYKXqq212QsqzympHWs2FBa8pkacQW/f0FsBGloHKNKgiNruFun6SpcAZWs3qRjnHV4FLYnF/ZvbVLI2ANqGzpmduK3YV5r2l1DObXj9Mz42GldyiruZn/uZS9WN//hTaR72RodNS6NH7vRmMtayKsptUnn9K1pldF/QLdWZq31+makAAAAABJRU5ErkJggg==>

[image29]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADYAAAAVCAYAAAANfR1FAAADDklEQVR4Xu1WWaiNURhdZsk1Z4okc0rxgLy4CXHzQOLJUIaIQqYHUyJFJCFPKCHyYErm4SqzRCRPKMqUjF3qemAt397n7n/7/+M8nIdzb3fV6pxv7en/vv3tb2+gHvUoWTQge8ZigOnkOfIWeYzskWwuPXQlJ5KXyBNRm8cC8hHZ0dnryHeBXXJYRD4md5PVSHesE/mTnBJoDcnX5IpAi6EdVQCukS/I2+QTcpZr30aODP5/IX+TFU4rGqqQ7ths2IKDIv0yeT7SPNaSP8iH5DiymdMbkRvI+669udMFOfeLbBtoRUGWY9pNOdY90o+T35H8OGE/rP962M7GaAzb7bORfg92houOLMek6UM7R/pRp/cKNKW2dyoftNNLA1tnVeOWBFrRkOXYRaQ7dtjpA5ytj/tKPkdN6mVhM9k3sKfB5tpH3iVfkntglVo7vAmW1hvJYeQd108FrR/+gyzHziDdsUNO7+Ps7c4Od6JQHICNXQk7h21gxWwOOZOcT051fbTb3WwY9sLOa17IsZOxSByETdgl0o84vb2znzk7LjKFQGdOZ9NDO6Xzq+Ap5VuRq2HOtg76bYGtmffayXJsB2xwfHmrr3QVCKWL/muONKjcqz2kqqPQ39kznC0oC6RpbQ/ds1cDW/DZ1CLSE9BHnYpFWCpo8JBIv0HeDGzdUSrXaZXQ4xUs6v5cCvNg8+uh4KEUlKbXjqDAfSNX5XoATcjPKDAVT8ci7F7RpGFEm5IfycWBtgb2MeWBFmI4rD0+xwvJt5FWCQuU1hH82BG+AzHBaZOcXQ57DSWgKqbXxRVYfsfQk0tvRB1sYS6sSoV3mKJ6ARZFVTldA4qqxoyHVTKtEQZIGAjbRb1wBDmqoA3N9QCWw5zQ005oRz4lt+Z6WJDfeGMUahb0ua9yrXxu6Ts5jIVNpGKyEzZ5DKWhUktp+h42n353kR1g0U0bN5l8AMsYrd072fx3l6/D1q6EXeZK1xBlsOpaa6BgfULyfNUJDIbt/Ji4obZjGcwxf1/WCeih/QHmmM7Y6GTzv/gDDn+/jfMM7EIAAAAASUVORK5CYII=>

[image30]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEUAAAAXCAYAAABdy4LVAAADD0lEQVR4Xu2XWahNURjHP2MyhEuKzJKhhBeR6RgePJhCSB7kmjJlSDwoRPGCMo95MD5I4YGQLpFQlAcP4sVUKGPyIPH/W2vt8+3v7n3cl7Mv1/7Vr8761j7n7PXttda3tkhOzv9Oe7gf7oZ9TZ9mPextg6QnXAibq9gIuEO1s6abDSiGwHPwBqwSd68ajuMJHAt7wJdwjY+TBnA0PCvudxLhBT+Nb+FwfVEGtIAFeAR+iHdFDIav4CDf5sC/iRtDYB58qNob4DZ4CJ6Ed+EZccnqqK6LMQa+gK/hI3gY9opdUX4GwDfwFHwMP8a7I+7DXSbGgV5Q7YPwqmpPFDdTNEvgYhOLwSzz6fwtXJTkpHCZcxYvNXHOhM+woW/vgdeK3TIJrlJt/s4tWE/FqlGQfyMpM8QlZbaJr/Txkb69AD4odstGOM5/ZiKuw35RbwoFeBpuh5fgO7hT/pDJMpKWFD5tDp7J0XApMD7Xt1vBp3AYbAdvi9tcCfebzf5zSbibP4fdfZs7Nm9qbXRFtqQlheUzKSmLfJwVNDAQXhG3oXbxsTbiEtQkXFQK7vrhi4ET8BNsa+JZkJYUPqSkpDAZjFeauGWvuFURYDLPw2kqVhKWMP7RTNuRAUwKH4hlviTfE6sI41NMXMMSzpIc2AIPwAp4DI5Sfb/3jSp4B9ZX8TBVl6mYhTfD0lcTWRFqSlpSpoq7pzkmvtrHCyYeaAQvw9a+3RJ+EbdtkM7ijgIRjeEP+BU2VXFuuvyj8SqWFUwKS6ylq7h7WmHiW8Ud4LjBJrFO4kuOJ2D+Th8VY4mOcQ/2NzFWIZ4qOb2yhknhk0yCR3seLjU8uPHIngRfF+xRnqdiJkW/E7FMx5gA90lxpjCT38WVr9qAy40zt5ntEDfIZ7CDb3Ng7yX9BM6EdDIxjpOrY6hvs8iwSlWDmxezdVNc2Zoc7y47nJGcBZydfIqUg2WCeN7Q8K12k7izFd+E7SwPTIfLbdDDl93j4vbUoxJ/d6qzcJax3IZDm4WbLzd/zrpZpq9OwyqTk5OTU6v8AmhFpNOKAfhoAAAAAElFTkSuQmCC>

[image31]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEwAAAAYCAYAAABQiBvKAAAEHElEQVR4Xu2XaahWVRSGV3NpIVk5lYjYZFaQlQ00WD+CSppA/ZEDKaH0o6AQigaKJppooMGKBrHBzFIrSKxQsTks8VclhRTagFQWESVS7+Pau2+d7Tl6I/Eqngde7tnv2d/3nb3O3muta9bS0lLPg1LP0uwOdpEGl2ZgvPSm9L40WxpUvb2RftJT0iLpI+ny6u3/zd7S4tLc1gyQLpLekuYW9zJXSMulPml8k/R9GMOB0grzuTBQWindnCdsBcZI15XmtuRK80U+LP1l9QHrK/0hjQ7ertK30rTg3Wf+XZGp0lppj+CdKn0h/S29GvyuwPMdUprdxe9WH7DJ5os7tvDflhaE8SrppTCGs80/e3rhH5n8/7Jb9jc/6tsNTQFj97E4jliE3fGbeV4hdzHn8coMs+OTf0Phs/PwRxT+5uDFcSK2G5oChsfiCEpkVvKHSCeka4IbOSb5zxT+HGmN+dHuKuTYmDO7naaALbT6gD2f/KHSmem6DNhRyX8ueOSzn6Vn05ic9LS0VLo6eSXMoUI3QZ6daR7UJdKN1dsbOU96QXpH+kyaVL1tu0lTpI/NK/yMNG6kKWBvWH3ACAL+YdIp6boMGMHE58czI5N3qXmgyWPkuG/Md20dFJfLSjNBSqDYnJ/G50jrpIPDfXLru1Lv5PF3dbqGfaT50qfmzwys+0/bzCkgYPNK0/zNscD+hf9i8g+QjkjXj1RmmB2d/PuDd3vyLpHuTR75kUUxvw56v/1KM3GS+fddkMYUGnYYfSVMN19bfuEca3bOPWkMVHi6BNJL5jHpzjDehKaAPWD+QGVTy1x83gA9GNc0rZETkx+PCFse7wfzYnBQuFfHodLLpRng8z9Z5zvZ+TnXcY9d8qN0i3SXeXAIag4oO3G99HoadxkCxrYsmWj+MMMLny3+Xhh/Kb0SxnCu+WfJH5Cr6fVSD/Pf+9z82DRBkzy6NAvIlcwjpWww31VwmvnvXZPGdVxoPueq8saWIGCvlaZ5//OrNCF4e5o3pPFHbjNvSPObA3LPd9ZpXMeaPxxVFWgTGBNIcmHZfsAyaw4ogaDaHh482p0n0jVHjO+Pz57JvSEvkzmjwr3MGaWR2cu8m6eCxAVn+NeJPEIlAf5HpNLEhXDNjmMukEjZdeP+neFF4Rdp9zQmoOQOjjXHJS4cjrNqwSihPaG68ltAnqXC5SabtXxg3tbk38QjRZCDYV/pa+nuNIZe0q3mVbUCZ/lD82ARZfSVeXnmiyJUHxIlReAh61ScCN615i0Db/niyl3v1Plshp3FjuAZ6o7NHdapfnVQbGg3yKdPmgd9WGWGB4//aVeZt0j8d8JOjzCHwC42XzvPVD77DsEn1tnVLVvgZOnR0mxphh7trNJsaYYCUleAWmoYZJ7AW1p2Ev4BFsXkPRo703gAAAAASUVORK5CYII=>