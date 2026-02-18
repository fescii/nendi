## Solutions & Recommendations for Every Problem Identified

---

### Problem 1: Write Coalescing Latency — No Flush Policy Defined

**Solution: Dual-Threshold Adaptive Flusher**

Implement a flush coordinator that triggers on whichever condition fires first — size or time. This prevents both latency spikes (waiting too long) and throughput degradation (flushing too eagerly).

```rust
pub struct FlushPolicy {
    max_bytes: usize,       // e.g., 4MB
    max_age_us: u64,        // e.g., 500 microseconds
    current_bytes: usize,
    last_flush: Instant,
}

impl FlushPolicy {
    pub fn should_flush(&self) -> bool {
        self.current_bytes >= self.max_bytes
            || self.last_flush.elapsed().as_micros() as u64 >= self.max_age_us
    }
}
```

Run this on a dedicated thread with a spin-wait loop (not `tokio::sleep`) for sub-millisecond precision. The recommended starting values are 4MB or 500µs. Under light load, the time threshold fires first, keeping latency predictable. Under heavy load, the byte threshold fires first, keeping throughput high. Expose both values as runtime-configurable parameters via the admin API so you can tune without restarts.

---

### Problem 2: io_uring Integration Complexity and Privilege Requirements

**Solution: Tiered I/O Backend with Runtime Detection**

Don't make io_uring mandatory. Build an abstraction layer with two implementations behind a trait, selected at startup based on kernel capability detection.

```rust
#[async_trait]
pub trait IoBackend: Send + Sync {
    async fn read_socket(&self, fd: RawFd, buf: &mut [u8]) -> io::Result<usize>;
    async fn write_file(&self, fd: RawFd, buf: &[u8], offset: u64) -> io::Result<usize>;
}

pub struct UringBackend { ring: IoUring }
pub struct EpollBackend { runtime: tokio::runtime::Handle }
```

At startup, probe for io_uring availability:

```rust
fn probe_uring() -> bool {
    // Check kernel version >= 5.15 and CAP_SYS_ADMIN or unprivileged io_uring
    let version = rustix::process::uname();
    let cap_ok = caps::has_cap(None, caps::CapSet::Effective, caps::Capability::CAP_SYS_ADMIN)
        .unwrap_or(false);
    version_ge_5_15(&version) && (cap_ok || unprivileged_uring_allowed())
}
```

For Docker environments specifically, add to the `seccomp` profile or use `--privileged` only for io_uring calls. In the systemd unit file, add `AmbientCapabilities=CAP_SYS_ADMIN` to grant the specific capability without running the entire daemon as root.

For fixed buffer registration, pre-register exactly two buffer rings: one for database socket reads (e.g., 64 × 64KB buffers) and one for WAL write output (e.g., 32 × 128KB buffers). Keep them separate to avoid head-of-line blocking between ingestion and storage I/O paths.

**Prioritization recommendation:** Build and benchmark the epoll/tokio path first. Profile at 100k/sec. If you're within 15% of target, ship epoll and defer io_uring to v2. The complexity cost is real and io_uring bugs have historically caused data corruption under specific kernel versions.

---

### Problem 3: Backpressure Not Propagated Back to PostgreSQL WAL Sender

**Solution: Credit-Based Ingestion Throttle with Replication Acknowledgment Gating**

The key insight is that PostgreSQL's streaming replication protocol already has a mechanism for this: you simply stop sending `StandbyStatusUpdate` messages (the `r` messages that acknowledge received LSNs), which causes PostgreSQL to stop sending new WAL data after its send buffer fills. You need to make this explicit and intentional.

```rust
pub struct ReplicationThrottle {
    // Maximum number of unacknowledged WAL bytes allowed in-flight
    window_bytes: u64,
    // Bytes received but not yet persisted to RocksDB
    unacked_bytes: AtomicU64,
    // Notified when RocksDB confirms a WriteBatch commit
    drain_notify: Arc<Notify>,
}

impl ReplicationThrottle {
    pub async fn gate_ingestion(&self) {
        while self.unacked_bytes.load(Ordering::Acquire) > self.window_bytes {
            // Block the replication socket reader here
            // PostgreSQL will buffer on its side and stop sending
            self.drain_notify.notified().await;
        }
    }
    
    pub fn on_rocksdb_commit(&self, bytes_committed: u64) {
        self.unacked_bytes.fetch_sub(bytes_committed, Ordering::Release);
        self.drain_notify.notify_one();
    }
}
```

The replication reader task calls `gate_ingestion()` before processing each batch. When RocksDB confirms the WriteBatch, `on_rocksdb_commit()` releases the gate. This creates a clean flow-control loop: slow disk → gate fills → PostgreSQL pauses → no data loss, no queue overflow.

Set `window_bytes` to approximately 2× the `write_buffer_size` (so ~512MB with the recommended config) to give enough headroom for burst absorption without unbounded memory growth.

---

### Problem 4: WAL Slot Bloat — Mitigation is Too Soft

**Solution: Autonomous Slot Advancement with Hard Lag Cap and Forced Resync Protocol**

Implement three distinct behaviors based on consumer lag:

```rust
pub enum SlotPolicy {
    // Consumer is healthy — advance slot normally with consumer ACKs
    TrackConsumer { max_lag_bytes: u64 },
    // Consumer is lagging — advance slot independently, consumer catches up from RocksDB
    IndependentAdvance { lag_cap_bytes: u64 },
    // Consumer is too far behind or disconnected too long — invalidate and force resync
    ForceResync { reason: String },
}

pub struct SlotManager {
    policy: SlotPolicy,
    // The LSN we report to PostgreSQL as "safely consumed"
    // This is DECOUPLED from consumer ACK LSN
    reported_flush_lsn: Lsn,
    // Minimum LSN across all active consumers
    consumer_watermark: Lsn,
}
```

The critical design decision: **the LSN reported to PostgreSQL for WAL cleanup should be `min(consumer_watermark, rocksdb_persisted_lsn - lag_cap)`**. This means:

- PostgreSQL can clean up WAL up to whatever RocksDB has durably stored, minus a safety buffer.
- Consumers catch up from RocksDB, not from PostgreSQL WAL.
- PostgreSQL's disk is never at risk from a slow consumer.

Add a configuration block:

```toml
[slot_management]
max_lag_bytes = 10_737_418_240      # 10GB — warn threshold
force_resync_lag_bytes = 53_687_091_200  # 50GB — hard cap, force resync
disconnected_resync_after_secs = 3600   # 1 hour disconnected = force resync
```

When `force_resync` triggers, the daemon invalidates the consumer's offset token and marks it as `NEEDS_SNAPSHOT`. On reconnect, the client receives an error response on the `Subscribe` RPC that includes a snapshot endpoint URL rather than a stream offset. The consumer must perform a full initial sync before resuming streaming.

---

### Problem 5: Incomplete Memory Accounting

**Solution: Unified Memory Budget with Explicit Allocation Limits**

Model all memory consumers explicitly and enforce a global budget at startup. If the sum exceeds available RAM minus a 20% OS headroom, the daemon refuses to start with a clear error message rather than getting OOM-killed at 3am.

```rust
pub struct MemoryBudget {
    total_available: usize,
    rocksdb_block_cache: usize,
    rocksdb_memtables: usize,       // write_buffer_size × max_write_buffer_number
    rocksdb_index_filters: usize,   // ~5% of block_cache as rule of thumb
    rocksdb_iterators: usize,       // active_consumers × iterator_pinned_block_size
    rocksdb_compaction_buffers: usize, // max_background_jobs × compaction_io_buffer
    tokio_task_stack: usize,        // estimated_tasks × stack_size (default 2MB each)
    ring_buffer: usize,
    webhook_worker_pool: usize,
    os_headroom: usize,             // 20% of total
}

impl MemoryBudget {
    pub fn validate(&self) -> Result<(), StartupError> {
        let committed = self.rocksdb_block_cache
            + self.rocksdb_memtables
            + self.rocksdb_index_filters
            + self.rocksdb_iterators
            + self.rocksdb_compaction_buffers
            + self.tokio_task_stack
            + self.ring_buffer
            + self.webhook_worker_pool
            + self.os_headroom;
        
        if committed > self.total_available {
            return Err(StartupError::InsufficientMemory {
                required: committed,
                available: self.total_available,
            });
        }
        Ok(())
    }
}
```

Specific values for the missed components:

- **Iterator pinned blocks:** Each active gRPC streaming consumer holds one RocksDB iterator. Each iterator can pin up to one block cache block. Budget `num_max_consumers × block_size` (default 4KB–32KB depending on your block_size setting).
- **Compaction I/O buffers:** Each background compaction job holds read and write buffers. Budget `max_background_jobs × 2 × compaction_io_buffer_size` (typically 1MB each = 16MB for 8 jobs).
- **Tokio task stacks:** Each gRPC connection spawns 2–3 tasks. At 10,000 consumers, this is 20,000–30,000 tasks. Tokio's default stack is 2MB. Use `tokio::Builder::thread_stack_size(512 * 1024)` to reduce this to 512KB if your task code is shallow.
- **In-flight WAL write buffers:** During a MemTable flush, RocksDB holds the write buffer in memory while writing the SSTable. This means peak memory includes one extra `write_buffer_size` (256MB) during flush spikes.

Expose all of these via the `/health` endpoint and as Prometheus gauges.

---

### Problem 6: DDL Ordering Race Condition

**Solution: Schema Registry with Per-Event Schema Fingerprinting**

The race is: DDL logical message arrives slightly after the first DML event that uses the new schema. You need to make schema version a first-class citizen of every event.

**Step 1: Tag every DML event with a schema fingerprint at the PostgreSQL trigger level.**

```sql
-- Add to the event trigger that emits DDL logical messages
CREATE OR REPLACE FUNCTION capture_ddl_changes()
RETURNS event_trigger AS $$
DECLARE
    schema_hash TEXT;
BEGIN
    -- Compute a hash of the current table structure
    SELECT md5(string_agg(column_name || ':' || data_type, ',' ORDER BY ordinal_position))
    INTO schema_hash
    FROM information_schema.columns
    WHERE table_name = TG_TABLE_NAME;
    
    -- Emit with the hash so consumers can detect schema mismatches
    PERFORM pg_logical_emit_message(
        true,  -- transactional — this is key, ensures ordering in WAL
        'schema_change',
        json_build_object(
            'table', TG_TABLE_NAME,
            'schema_hash', schema_hash,
            'ddl', current_query()
        )::text
    );
END;
$$ LANGUAGE plpgsql;
```

The `transactional = true` flag on `pg_logical_emit_message` is critical — it ensures the DDL message appears in the WAL stream in the exact transactional position relative to the DDL statement, not asynchronously.

**Step 2: Embed the schema fingerprint in every DML ChangeEvent.**

```rust
pub struct ChangeEvent {
    pub lsn: Lsn,
    pub op: OpType,
    pub table: TableId,
    pub schema_fingerprint: [u8; 16],  // MD5 of schema at time of change
    pub payload: Bytes,
}
```

**Step 3: In the daemon's stream processor, maintain a schema registry and enforce ordering.**

```rust
pub struct SchemaRegistry {
    // table_id -> (fingerprint, schema_definition)
    schemas: DashMap<TableId, (SchemaFingerprint, SchemaDefinition)>,
    // If we receive a DML with an unknown fingerprint, buffer it here
    // until the DDL message arrives and schema is registered
    pending: DashMap<SchemaFingerprint, VecDeque<ChangeEvent>>,
}

impl SchemaRegistry {
    pub fn process_event(&self, event: ChangeEvent) -> ProcessResult {
        if self.schemas.contains_key_with_fingerprint(&event.table, &event.schema_fingerprint) {
            ProcessResult::Ready(event)
        } else {
            // Schema not yet registered — buffer and wait
            self.pending
                .entry(event.schema_fingerprint)
                .or_default()
                .push_back(event);
            ProcessResult::Buffered
        }
    }
    
    pub fn register_schema(&self, fingerprint: SchemaFingerprint, def: SchemaDefinition) {
        self.schemas.insert(def.table_id, (fingerprint, def));
        // Drain buffered events that were waiting for this schema
        if let Some(buffered) = self.pending.remove(&fingerprint) {
            // Re-process all buffered events now that schema is known
        }
    }
}
```

Add a hard timeout: if a DML event references an unknown schema fingerprint and no DDL message arrives within 5 seconds, emit an alert and force the consumer to do a schema refresh rather than silently dropping or misprocessing the event.

---

### Problem 7: Multi-Tenant I/O Starvation Between Column Families

**Solution: Per-Tenant RocksDB Rate Limiting with Isolated Compaction Pools**

RocksDB's `SstFileManager` and `RateLimiter` apply globally. For multi-tenant isolation, you need per-tenant rate limits enforced before they hit the shared compaction infrastructure.

```rust
pub struct TenantStorageConfig {
    pub tenant_id: TenantId,
    // Max bytes/sec this tenant can write to RocksDB
    pub write_rate_limit: u64,
    // Max compaction bytes/sec for this tenant's column families
    pub compaction_rate_limit: u64,
    // Dedicated column families for this tenant's data and offsets
    pub data_cf: ColumnFamilyHandle,
    pub offset_cf: ColumnFamilyHandle,
}
```

At the RocksDB level, use a single shared `Env` but create per-tenant `RateLimiter` instances:

```rust
let tenant_limiter = Arc::new(
    rocksdb::RateLimiter::new(
        tenant_config.write_rate_limit,  // bytes/sec
        100_000,                          // refill period microseconds
        10,                               // fairness
    )
);

let mut cf_options = Options::default();
cf_options.set_rate_limiter(&tenant_limiter);
```

Additionally, configure the global compaction job pool to use a priority queue that round-robins across tenants rather than FIFO. This prevents a single tenant with a large compaction backlog from consuming all background threads. RocksDB doesn't expose this directly, so you need to implement it at the daemon level by managing compaction triggers manually using `CompactRangeOptions` and scheduling them via a tenant-aware scheduler thread.

---

### Problem 8: Offset Registry GC Causing Write Stalls

**Solution: Two-Phase GC with Compaction Filter and Watermark-Separated Key Spaces**

The naive DeleteRange approach triggers immediate compaction. Instead, use a lazy GC strategy with a compaction filter:

```rust
pub struct EventGcFilter {
    // The minimum LSN across all active consumers — events before this are safe to delete
    global_watermark: Arc<AtomicU64>,
}

impl CompactionFilter for EventGcFilter {
    fn filter(&mut self, _level: u32, key: &[u8], _value: &[u8]) -> Decision {
        let lsn = u64::from_be_bytes(key[..8].try_into().unwrap());
        let watermark = self.global_watermark.load(Ordering::Relaxed);
        
        if lsn < watermark {
            Decision::Remove  // Let compaction drop this key naturally
        } else {
            Decision::Keep
        }
    }
}
```

This approach lets RocksDB GC expired events as a side-effect of normal compaction rather than triggering additional compaction. No write stalls from DeleteRange.

For the watermark calculation, run a dedicated background task:

```rust
async fn watermark_updater(
    consumer_registry: Arc<ConsumerRegistry>,
    global_watermark: Arc<AtomicU64>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let min_lsn = consumer_registry.minimum_acknowledged_lsn();
        // Only advance — never move watermark backward
        let current = global_watermark.load(Ordering::Relaxed);
        if min_lsn > current {
            global_watermark.store(min_lsn, Ordering::Relaxed);
        }
    }
}
```

For the key space design, separate live data from GC-eligible data using a key prefix so prefix bloom filters can skip GC'd ranges entirely during reads. Use prefix `0x01` for active events and `0x00` for events past the watermark (written this way during the watermark update, allowing bloom filters to skip `0x00` blocks on live reads).

---

### Problem 9: Webhook Dispatcher Connection Pool Serialization

**Solution: Per-Target Connection Pools with Adaptive Worker Allocation**

```rust
pub struct WebhookDispatcher {
    // One pool per target — keyed by host:port, not full URL
    pools: DashMap<TargetKey, TargetDispatcher>,
    global_concurrency_limit: Arc<Semaphore>,
}

pub struct TargetDispatcher {
    client: reqwest::Client,  // Each has its own connection pool
    worker_tx: mpsc::Sender<EventBatch>,
    config: TargetConfig,
}

impl WebhookDispatcher {
    pub async fn register_target(&self, target: WebhookTarget) {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(target.max_connections)
            .tcp_keepalive(Duration::from_secs(30))
            .timeout(Duration::from_secs(target.timeout_secs))
            .build()
            .unwrap();
        
        let key = TargetKey::from(&target);
        self.pools.insert(key, TargetDispatcher::new(client, target));
    }
}
```

The `global_concurrency_limit` semaphore caps the total number of in-flight HTTP requests across all targets (e.g., 1000), preventing file descriptor exhaustion. Each target's pool is bounded separately (e.g., 20 connections per target). This gives you both per-target isolation and global resource safety.

For the batching behavior, implement a per-target micro-batcher using the same dual-threshold pattern from Problem 1:

```rust
pub struct MicroBatcher {
    buffer: Vec<ChangeEvent>,
    max_events: usize,    // e.g., 100 events
    max_age_ms: u64,      // e.g., 50ms
    last_flush: Instant,
}
```

This means a busy target gets batched requests (efficient) while a quiet target still flushes within 50ms (low latency). Add an `X-Batch-Size` header to webhook payloads so consumers can distinguish single-event vs. batch deliveries and optimize their processing accordingly.

---

### Summary Priority Order

| Priority | Problem | Effort | Impact |
|---|---|---|---|
| 1 | Flush coalescing policy | Low | Highest — defines your latency floor |
| 2 | Backpressure to PostgreSQL | Medium | Critical for correctness under load |
| 3 | WAL slot hard cap + resync | Medium | Prevents catastrophic disk fill |
| 4 | DDL schema fingerprinting | Medium | Correctness — prevents silent data corruption |
| 5 | Memory budget validation | Low | Prevents 3am OOM kills |
| 6 | Offset GC with compaction filter | Low | Prevents write stalls at scale |
| 7 | Per-target webhook pools | Low | Throughput for HTTP consumers |
| 8 | Multi-tenant I/O isolation | High | Only needed with multiple tenants |
| 9 | io_uring tiered backend | High | Defer until epoll baseline is proven |
