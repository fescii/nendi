//! Error types for the Nendi SDK.

use crate::offset::Offset;

/// Top-level error type for all SDK operations.
#[derive(Debug, thiserror::Error)]
pub enum NendiError {
  /// Connection to daemon failed.
  #[error("Connection failed: {0}")]
  Connection(#[from] tonic::transport::Error),

  /// Stream ended unexpectedly (not a clean shutdown).
  #[error("Stream disconnected: {0}")]
  Disconnected(tonic::Status),

  /// Event payload could not be deserialized into the target type.
  #[error("Deserialization failed for {table}: {source}")]
  Deserialize {
    table: String,
    source: Box<dyn std::error::Error + Send + Sync>,
  },

  /// Consumer filter references a column not present in stream storage.
  #[error("Filter conflict: {0}")]
  FilterConflict(String),

  /// Offset was not found in the stream (event GC'd or stream reset).
  #[error("Offset not found — stream may have been reset. Resync required.")]
  OffsetNotFound {
    requested: Offset,
    earliest_available: Offset,
  },

  /// Daemon returned an error response.
  #[error("Daemon error [{code}]: {message}")]
  Daemon { code: u32, message: String },

  /// Offset store backend failed.
  #[error("Offset store error: {0}")]
  OffsetStore(Box<dyn std::error::Error + Send + Sync>),
}
