## Analysis of dbstreamd Architecture

### What's Strong

**The core thesis is sound.** Collapsing the Debezium → Kafka → Consumer pipeline into a single binary genuinely eliminates 2-3 network hops and their associated serialization costs. For single-region deployments this is a real win, not just theoretical.

**RocksDB tuning recommendations are solid.** The write_buffer_size at 256MB, direct I/O for flush/compaction, and big-endian monotonic keys are all well-reasoned. The LSM write path analysis is accurate.

**Serialization choices are correct.** rkyv internally, Protobuf on the egress boundary is exactly the right call. JSON at 100k/sec would be a disaster.

**The SourceConnector trait design is clean** — async_trait, BoxStream, LSN-typed offsets. This is idiomatic Rust.

---

### Real Problems (Speed-Prioritized)

**1. The biggest gap: no mention of write coalescing latency tuning.**
The document says "accumulate messages in a buffer and flush in a single operation" but never specifies *when* to flush. A naive timer-based flush at even 1ms intervals caps your effective latency at 1ms. You need a hybrid: flush when buffer hits N bytes *or* T microseconds, whichever comes first. This is the difference between 200µs and 1ms p99.

**2. io_uring integration is described but not designed.**
SQPOLL requires root or `CAP_SYS_ADMIN` in most kernel configs, and registering fixed buffers for *both* the database socket reads *and* RocksDB WAL writes simultaneously is genuinely complex. The document treats this as a checkbox item when it's actually the hardest implementation challenge and probably a Phase 1 risk, not Phase 3.

**3. The "Dragonsmouth" pipeline has a hidden bottleneck.**
The SPSC queue between ingestion and storage threads is fine, but if the RocksDB MemTable stalls (compaction debt), this queue fills up and *then what*? The document mentions backpressure to gRPC consumers but doesn't address backpressure propagation *back to the PostgreSQL WAL sender*. You cannot just drop WAL messages. You'd have to pause the replication slot's `XLogData` acknowledgments — which is PostgreSQL-specific behavior that needs explicit design.

**4. The WAL bloat risk is understated.**
This is listed at the bottom of the risk section but it's actually an existential threat. A single slow or disconnected consumer holding an unadvanced replication slot will cause PostgreSQL to retain WAL indefinitely. The mitigation ("heartbeat events and aggressive alerting") is too soft. The daemon needs a hard policy: either advance the slot independently of consumer acknowledgment with a configurable lag cap, or automatically drop/invalidate the slot after a threshold and force consumers to do a full snapshot resync.

**5. Memory accounting is incomplete.**
The document lists MemTables + Block Cache + Index/Filter blocks as RocksDB memory components, but misses: iterator pinned blocks, write buffers in-flight during flush, and the Tokio async runtime's task memory. For a 32GB recommended spec this matters when sizing the shared_block_cache.

**6. DDL ordering guarantee is hand-wavy.**
`pg_logical_emit_message()` is clever, but the document doesn't address the gap between when the DDL trigger fires and when the DDL is visible in the WAL stream. There's a real race where the daemon could receive a row-level change for a table whose schema it hasn't yet received via the logical message. This needs either a schema registry with versioned schema IDs per event, or a hard serialization fence.

---

### Architectural Omissions

**No mention of multi-tenant isolation.** If you're streaming from multiple databases, compaction on one tenant's column family can starve I/O for another. RocksDB's rate limiter (`SstFileManager`) needs explicit configuration per-instance or you get unpredictable tail latency.

**The offset registry design has a GC problem.** DeleteRange after all consumers advance past a point is mentioned but the implementation is non-trivial — you need a background thread doing periodic watermark calculation, and DeleteRange in RocksDB triggers compaction that can itself cause write stalls. This needs its own tuning.

**Webhook dispatcher is underspecified for speed.** "Bounded worker pool" using reqwest is fine for correctness but reqwest's default connection pooling is per-host. If you have 50 webhook targets, you need 50 connection pools or you serialize deliveries to the same host. This is a significant throughput issue.

---

### What to Prioritize for Speed

In rough order of impact:

1. Get the flush coalescing policy right first — it dominates your latency numbers more than anything else.
2. Nail the backpressure propagation chain end-to-end before optimizing anything else, or you'll build a system that performs great under ideal conditions and falls apart under load.
3. Treat WAL slot management as a first-class feature, not a monitoring concern.
4. io_uring is worth pursuing but profile *without* it first to establish a baseline — tokio + epoll may get you to 100k/sec with far less complexity.

The document is a strong engineering foundation, but it reads like it was written by someone who has read about these systems more than operated them under production load. The hard problems are in the gaps between the components, not within them.
