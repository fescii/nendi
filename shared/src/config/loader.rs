use serde::Deserialize;
use std::collections::HashMap;

/// Root configuration for the Nendi daemon.
///
/// Loaded from TOML files via the `config` crate with environment-variable
/// overrides (prefix: `NENDI_`).
#[derive(Debug, Clone, Deserialize)]
pub struct NendiConfig {
    /// Source database connection settings.
    pub source: SourceConfig,
    /// Replication slot settings.
    pub slot: SlotConfig,
    /// Publication management.
    pub publication: PublicationConfig,
    /// Internal storage (RocksDB) settings.
    pub storage: StorageConfig,
    /// Pipeline / ingestion filter settings.
    pub pipeline: PipelineConfig,
    /// gRPC egress settings.
    pub grpc: GrpcConfig,
    /// Webhook egress settings (optional).
    pub webhook: Option<WebhookConfig>,
    /// Memory budget.
    pub memory: MemoryConfig,
    /// Observability settings.
    pub observability: ObservabilityConfig,
}

/// Source database connection configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceConfig {
    /// PostgreSQL connection string.
    pub connection_string: String,
    /// Connection pool size for schema queries (not replication).
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    /// Application name shown in `pg_stat_activity`.
    #[serde(default = "default_app_name")]
    pub application_name: String,
}

/// Replication slot configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SlotConfig {
    /// Replication slot name.
    pub name: String,
    /// Output plugin — must be `pgoutput`.
    #[serde(default = "default_output_plugin")]
    pub output_plugin: String,
    /// Whether to create the slot if it does not exist.
    #[serde(default = "default_true")]
    pub auto_create: bool,
    /// Hard WAL lag cap in bytes. If lag exceeds this, Nendi pauses
    /// replication to avoid bloating `pg_wal/`.
    #[serde(default = "default_wal_lag_cap")]
    pub wal_lag_hard_cap_bytes: u64,
    /// How often to check WAL lag, in seconds.
    #[serde(default = "default_lag_check_interval")]
    pub lag_check_interval_secs: u64,
}

/// Publication management configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct PublicationConfig {
    /// Publication name.
    pub name: String,
    /// Tables to include in the publication.
    pub tables: Vec<PublicationTable>,
}

/// A single table within a publication, with optional operation and row
/// filter.
#[derive(Debug, Clone, Deserialize)]
pub struct PublicationTable {
    /// Fully-qualified table: `schema.table`
    pub table: String,
    /// Operations to include (defaults to all).
    #[serde(default = "default_ops")]
    pub operations: Vec<String>,
    /// Optional SQL WHERE clause for row filtering.
    #[serde(rename = "where")]
    pub predicate: Option<String>,
    /// Columns to include (defaults to all).
    pub columns: Option<Vec<String>>,
}

/// RocksDB storage configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// Data directory path.
    pub data_dir: String,
    /// Write buffer size in bytes.
    #[serde(default = "default_write_buffer")]
    pub write_buffer_size: usize,
    /// Maximum number of write buffers.
    #[serde(default = "default_max_write_buffers")]
    pub max_write_buffers: usize,
    /// Block cache size in bytes.
    #[serde(default = "default_block_cache")]
    pub block_cache_size: usize,
    /// Enable direct I/O for flush and compaction.
    #[serde(default = "default_true")]
    pub direct_io: bool,
    /// Compression type (none, snappy, lz4, zstd).
    #[serde(default = "default_compression")]
    pub compression: String,
    /// TTL for events in seconds (0 = no expiry).
    #[serde(default)]
    pub ttl_secs: u64,
}

/// Pipeline configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct PipelineConfig {
    /// Flush policy: max accumulated bytes before flushing.
    #[serde(default = "default_flush_bytes")]
    pub flush_max_bytes: usize,
    /// Flush policy: max age in microseconds before flushing.
    #[serde(default = "default_flush_age_us")]
    pub flush_max_age_us: u64,
    /// Ring buffer capacity (number of events).
    #[serde(default = "default_ring_capacity")]
    pub ring_buffer_capacity: usize,
    /// Ingestion filter expressions, keyed by table name.
    #[serde(default)]
    pub filters: HashMap<String, String>,
}

/// gRPC server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct GrpcConfig {
    /// Listen address (e.g. `0.0.0.0:4222`).
    pub listen_addr: String,
    /// Per-consumer credit window size.
    #[serde(default = "default_credit_window")]
    pub credit_window: u32,
    /// Maximum concurrent consumer streams.
    #[serde(default = "default_max_consumers")]
    pub max_consumers: usize,
}

/// Webhook egress configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookConfig {
    /// Webhook targets keyed by name.
    pub targets: HashMap<String, WebhookTarget>,
}

/// A single webhook delivery target.
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookTarget {
    /// Destination URL.
    pub url: String,
    /// Tables/ops to subscribe to.
    pub tables: Vec<String>,
    /// Batch size for micro-batching.
    #[serde(default = "default_webhook_batch")]
    pub batch_size: usize,
    /// Batch linger time in milliseconds.
    #[serde(default = "default_webhook_linger")]
    pub linger_ms: u64,
    /// Circuit breaker failure threshold.
    #[serde(default = "default_circuit_threshold")]
    pub circuit_breaker_threshold: u32,
    /// Retry policy: maximum attempts.
    #[serde(default = "default_retry_max")]
    pub retry_max_attempts: u32,
    /// HMAC signing secret (optional).
    pub signing_secret: Option<String>,
}

/// Memory budget configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryConfig {
    /// Total memory budget in bytes for the daemon.
    #[serde(default = "default_memory_budget")]
    pub budget_bytes: usize,
    /// Warning threshold as a fraction (0.0–1.0).
    #[serde(default = "default_memory_warn")]
    pub warn_threshold: f64,
}

/// Observability configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilityConfig {
    /// Log level filter (e.g. `info`, `debug`, `trace`).
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Prometheus metrics listen address.
    #[serde(default = "default_metrics_addr")]
    pub metrics_addr: String,
}

// ── Default value functions ─────────────────────────────────────────

fn default_pool_size() -> u32 {
    4
}
fn default_app_name() -> String {
    "nendi".to_string()
}
fn default_output_plugin() -> String {
    "pgoutput".to_string()
}
fn default_true() -> bool {
    true
}
fn default_wal_lag_cap() -> u64 {
    1_073_741_824
} // 1 GiB
fn default_lag_check_interval() -> u64 {
    10
}
fn default_ops() -> Vec<String> {
    vec![
        "insert".to_string(),
        "update".to_string(),
        "delete".to_string(),
        "truncate".to_string(),
    ]
}
fn default_write_buffer() -> usize {
    268_435_456
} // 256 MiB
fn default_max_write_buffers() -> usize {
    4
}
fn default_block_cache() -> usize {
    536_870_912
} // 512 MiB
fn default_compression() -> String {
    "lz4".to_string()
}
fn default_flush_bytes() -> usize {
    4_194_304
} // 4 MiB
fn default_flush_age_us() -> u64 {
    500
} // 500 µs
fn default_ring_capacity() -> usize {
    65_536
}
fn default_credit_window() -> u32 {
    1024
}
fn default_max_consumers() -> usize {
    256
}
fn default_webhook_batch() -> usize {
    100
}
fn default_webhook_linger() -> u64 {
    50
}
fn default_circuit_threshold() -> u32 {
    5
}
fn default_retry_max() -> u32 {
    3
}
fn default_memory_budget() -> usize {
    1_073_741_824
} // 1 GiB
fn default_memory_warn() -> f64 {
    0.85
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_metrics_addr() -> String {
    "0.0.0.0:9090".to_string()
}
