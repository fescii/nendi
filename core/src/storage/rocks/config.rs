use std::path::PathBuf;

/// RocksDB configuration tuned for the CDC write pattern.
///
/// Key insights from the architecture docs:
/// - Write-heavy workload (LSM tree is ideal)
/// - Sequential keys (big-endian LSN prefix)
/// - Direct I/O for flush/compaction to bypass page cache
/// - Large write buffers to batch writes
pub struct RocksConfig {
  /// Data directory path.
  pub data_dir: PathBuf,
  /// Write buffer size (default: 256 MiB).
  pub write_buffer_size: usize,
  /// Maximum number of write buffers before stalling.
  pub max_write_buffers: usize,
  /// Block cache size for reads.
  pub block_cache_size: usize,
  /// Enable direct I/O for flush and compaction.
  pub direct_io: bool,
  /// Compression type.
  pub compression: CompressionType,
  /// TTL for events (0 = no expiry).
  pub ttl_secs: u64,
}

/// Compression types supported.
#[derive(Debug, Clone, Copy)]
pub enum CompressionType {
  None,
  Snappy,
  Lz4,
  Zstd,
}

impl RocksConfig {
  pub fn from_storage_config(cfg: &shared::config::StorageConfig) -> Self {
    let compression = match cfg.compression.as_str() {
      "none" => CompressionType::None,
      "snappy" => CompressionType::Snappy,
      "zstd" => CompressionType::Zstd,
      _ => CompressionType::Lz4,
    };

    Self {
      data_dir: PathBuf::from(&cfg.data_dir),
      write_buffer_size: cfg.write_buffer_size,
      max_write_buffers: cfg.max_write_buffers,
      block_cache_size: cfg.block_cache_size,
      direct_io: cfg.direct_io,
      compression,
      ttl_secs: cfg.ttl_secs,
    }
  }

  /// Build RocksDB options from this config.
  pub fn to_rocksdb_options(&self) -> rocksdb::Options {
    let mut opts = rocksdb::Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    opts.set_write_buffer_size(self.write_buffer_size);
    opts.set_max_write_buffer_number(self.max_write_buffers as i32);

    // Compression
    match self.compression {
      CompressionType::None => {
        opts.set_compression_type(rocksdb::DBCompressionType::None);
      }
      CompressionType::Snappy => {
        opts.set_compression_type(rocksdb::DBCompressionType::Snappy);
      }
      CompressionType::Lz4 => {
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
      }
      CompressionType::Zstd => {
        opts.set_compression_type(rocksdb::DBCompressionType::Zstd);
      }
    }

    // Level compaction style (good for sequential writes)
    opts.set_compaction_style(rocksdb::DBCompactionStyle::Level);
    opts.set_level_compaction_dynamic_level_bytes(true);

    // Enable statistics (feeds Prometheus metrics)
    opts.enable_statistics();

    opts
  }
}
