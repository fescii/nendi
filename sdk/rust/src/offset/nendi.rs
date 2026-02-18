//! Daemon-side offset store (commits via gRPC).

use async_trait::async_trait;

use crate::error::NendiError;
use crate::offset::{Offset, OffsetStore};

/// Offset store that commits offsets to the Nendi daemon via gRPC.
///
/// This is the default store — offsets are tracked server-side.
/// The daemon uses committed offsets for garbage collection and
/// consumer lag monitoring.
pub struct NendiOffsetStore {
    /// Placeholder for the gRPC client connection.
    _endpoint: String,
}

impl NendiOffsetStore {
    /// Create a new daemon-side offset store.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            _endpoint: endpoint.into(),
        }
    }
}

#[async_trait]
impl OffsetStore for NendiOffsetStore {
    async fn load(&self, _stream_id: &str) -> Result<Option<Offset>, NendiError> {
        // TODO: query daemon via GetStatus RPC for committed offset
        Ok(None)
    }

    async fn save(&self, _stream_id: &str, _offset: &Offset) -> Result<(), NendiError> {
        // TODO: call CommitOffset RPC on the daemon
        Ok(())
    }
}
