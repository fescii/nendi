//! StreamMux — multi-stream fan-in.

use crate::error::NendiError;
use crate::event::ChangeEvent;
use crate::stream::NendiStream;

/// Multiplexes multiple streams into a single async stream.
///
/// Events arrive in arrival order across all inner streams.
///
/// # Example
///
/// ```ignore
/// let mut mux = StreamMux::new()
///     .add("orders", orders_stream)
///     .add("payments", payments_stream);
///
/// while let Some((stream_id, event)) = mux.next().await? {
///     match stream_id {
///         "orders"   => handle_order(&event).await?,
///         "payments" => handle_payment(&event).await?,
///         _ => unreachable!(),
///     }
/// }
/// ```
pub struct StreamMux {
    streams: Vec<(String, NendiStream)>,
}

impl StreamMux {
    /// Create a new empty multiplexer.
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
        }
    }

    /// Add a stream to the multiplexer.
    pub fn add(mut self, id: &str, stream: NendiStream) -> Self {
        self.streams.push((id.to_string(), stream));
        self
    }

    /// Receive the next event from any of the inner streams.
    ///
    /// Returns a tuple of (stream_id, event).
    pub async fn next(&mut self) -> Result<Option<(&str, ChangeEvent)>, NendiError> {
        // TODO: use tokio::select! or futures::stream::SelectAll
        // to poll all inner streams concurrently
        for (id, stream) in &mut self.streams {
            if let Some(event) = stream.next().await? {
                return Ok(Some((id.as_str(), event)));
            }
        }
        Ok(None)
    }
}

impl Default for StreamMux {
    fn default() -> Self {
        Self::new()
    }
}
