//! StreamMux — multi-stream fan-in using concurrent polling.

use futures::stream::{self, SelectAll, StreamExt};
use std::pin::Pin;

use crate::error::NendiError;
use crate::event::ChangeEvent;
use crate::stream::NendiStream;

/// A single labeled async stream item emitted by the mux.
type LabeledItem = Result<(String, ChangeEvent), NendiError>;

/// A pinned stream of labeled items.
type BoxedLabeled = Pin<Box<dyn futures::Stream<Item = LabeledItem> + Send>>;

/// Multiplexes multiple streams into a single async stream.
///
/// Events arrive in **arrival order** across all inner streams using
/// `futures::stream::select_all`, which fairly polls all inner streams
/// concurrently — no sequential scan, no per-stream task spawn.
///
/// # Example
///
/// ```ignore
/// let mut mux = StreamMux::new()
///     .add("orders", orders_stream)
///     .add("payments", payments_stream);
///
/// while let Some((stream_id, event)) = mux.next().await? {
///     match stream_id.as_str() {
///         "orders"   => handle_order(&event).await?,
///         "payments" => handle_payment(&event).await?,
///         _ => unreachable!(),
///     }
/// }
/// ```
pub struct StreamMux {
  inner: SelectAll<BoxedLabeled>,
}

impl StreamMux {
  /// Create a new empty multiplexer.
  pub fn new() -> Self {
    Self {
      inner: stream::select_all(Vec::<BoxedLabeled>::new()),
    }
  }

  /// Add a stream to the multiplexer.
  ///
  /// The stream is labeled with `id` so you can identify which logical
  /// stream each event came from in `next()`.
  pub fn add(mut self, id: &str, stream: NendiStream) -> Self {
    let label = id.to_string();
    let labeled: BoxedLabeled = Box::pin(unfold_stream(label, stream));
    self.inner.push(labeled);
    self
  }

  /// Receive the next event from any of the inner streams.
  ///
  /// Returns `(stream_id, event)`. Returns `None` when all inner
  /// streams have closed.
  pub async fn next(&mut self) -> Result<Option<(String, ChangeEvent)>, NendiError> {
    match self.inner.next().await {
      Some(Ok(item)) => Ok(Some(item)),
      Some(Err(e)) => Err(e),
      None => Ok(None),
    }
  }
}

/// Convert a `NendiStream` into a `Stream<Item = LabeledItem>` where each
/// item is labeled with the given stream ID.
fn unfold_stream(
  label: String,
  stream: NendiStream,
) -> impl futures::Stream<Item = LabeledItem> + Send {
  futures::stream::unfold((label, stream), |(label, mut s)| async move {
    match s.next().await {
      Ok(Some(event)) => Some((Ok((label.clone(), event)), (label, s))),
      Ok(None) => None,
      Err(e) => Some((Err(e), (label, s))),
    }
  })
}

impl Default for StreamMux {
  fn default() -> Self {
    Self::new()
  }
}
