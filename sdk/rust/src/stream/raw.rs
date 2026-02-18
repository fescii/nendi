//! Raw (untyped) event stream.

use crate::error::NendiError;
use crate::event::ChangeEvent;
use crate::offset::Offset;

/// An async stream of raw `ChangeEvent` values from the Nendi daemon.
///
/// Created via `NendiClient::subscribe()` or `SubscriptionBuilder::subscribe()`.
///
/// Call `next()` to receive events one at a time. The stream applies
/// HTTP/2 backpressure automatically when your processing falls behind.
pub struct NendiStream {
    stream_id: String,
}

impl NendiStream {
    /// Create a new stream (internal).
    pub(crate) fn new(stream_id: String) -> Self {
        Self { stream_id }
    }

    /// Receive the next event from the stream.
    ///
    /// Returns `None` when the stream is cleanly closed by the daemon.
    /// Returns an error if the connection drops unexpectedly.
    pub async fn next(&mut self) -> Result<Option<ChangeEvent>, NendiError> {
        // TODO: read from tonic streaming response
        Ok(None)
    }

    /// Commit an offset — tells the daemon it's safe to GC
    /// events before this position.
    pub async fn commit(&mut self, offset: Offset) -> Result<(), NendiError> {
        tracing::debug!(
            stream_id = %self.stream_id,
            offset = %offset,
            "Committing offset"
        );
        // TODO: call CommitOffset RPC
        Ok(())
    }

    /// Returns the stream ID.
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }
}
