use thiserror::Error;

/// Top-level error type for the Nendi daemon.
///
/// Each variant corresponds to a subsystem boundary. Internal subsystem
/// errors are attached as `#[source]` where possible so that `anyhow`
/// chains preserve the full cause.
#[derive(Debug, Error)]
pub enum NendiError {
    // ── Connector ──────────────────────────────────────────────
    #[error("connector: failed to connect to source database")]
    ConnectFailed(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("connector: replication stream terminated unexpectedly")]
    ReplicationStreamClosed,

    #[error("connector: replication slot '{0}' does not exist")]
    SlotNotFound(String),

    #[error("connector: replication slot '{0}' is already active")]
    SlotInUse(String),

    #[error("connector: WAL lag {lag_bytes} bytes exceeds hard cap {cap_bytes} bytes")]
    WalLagExceeded { lag_bytes: u64, cap_bytes: u64 },

    // ── Decoding ───────────────────────────────────────────────
    #[error("decoding: unsupported pgoutput message type: {0}")]
    UnsupportedMessage(String),

    #[error("decoding: malformed pgoutput payload")]
    MalformedPayload(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("decoding: schema fingerprint mismatch for table {table} (expected {expected}, got {actual})")]
    SchemaFingerprintMismatch {
        table: String,
        expected: u64,
        actual: u64,
    },

    // ── Storage ────────────────────────────────────────────────
    #[error("storage: RocksDB operation failed")]
    StorageFailed(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("storage: WAL write failed")]
    WalWriteFailed(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("storage: data directory '{0}' is not writable")]
    DataDirNotWritable(String),

    // ── Egress ─────────────────────────────────────────────────
    #[error("egress: gRPC stream error for consumer '{consumer}'")]
    GrpcStreamError {
        consumer: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("egress: webhook delivery failed for target '{target}'")]
    WebhookDeliveryFailed {
        target: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("egress: circuit breaker open for target '{0}'")]
    CircuitBreakerOpen(String),

    // ── Config ─────────────────────────────────────────────────
    #[error("config: failed to load configuration")]
    ConfigLoadFailed(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("config: invalid value for '{key}': {reason}")]
    ConfigInvalid { key: String, reason: String },

    #[error("config: publication '{0}' references unknown table")]
    PublicationTableNotFound(String),

    // ── Publication ────────────────────────────────────────────
    #[error("publication: reconcile failed for '{0}'")]
    PublicationReconcileFailed(String),

    // ── Offset ─────────────────────────────────────────────────
    #[error("offset: consumer '{consumer}' attempted to commit LSN {attempted} which is behind current {current}")]
    OffsetRegression {
        consumer: String,
        attempted: u64,
        current: u64,
    },

    // ── Pipeline ───────────────────────────────────────────────
    #[error("pipeline: ingestion filter rejected event (table={table}, op={op})")]
    FilterRejected { table: String, op: String },

    #[error("pipeline: backpressure — ring buffer full, {pending} events pending")]
    BackpressureFull { pending: usize },

    // ── Memory ─────────────────────────────────────────────────
    #[error("memory: budget exceeded ({used_bytes} / {budget_bytes} bytes)")]
    MemoryBudgetExceeded {
        used_bytes: usize,
        budget_bytes: usize,
    },

    // ── Shutdown ───────────────────────────────────────────────
    #[error("daemon: graceful shutdown initiated")]
    ShutdownRequested,

    // ── Generic ────────────────────────────────────────────────
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
