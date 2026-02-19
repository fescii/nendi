//! Raw (untyped) event stream.

use tonic::Streaming;

use crate::error::NendiError;
use crate::event::ChangeEvent;
use crate::offset::Offset;
use crate::proto::{self, NendiStreamClient};

use tonic::transport::Channel;

/// An async stream of raw `ChangeEvent` values from the Nendi daemon.
///
/// Created via `NendiClient::subscribe()` or `SubscriptionBuilder::subscribe()`.
///
/// Call `next()` to receive events one at a time. The stream applies
/// HTTP/2 backpressure automatically when your processing falls behind.
pub struct NendiStream {
  stream_id: String,
  consumer_id: String,
  inner: Streaming<proto::ChangeEvent>,
  client: NendiStreamClient<Channel>,
}

impl NendiStream {
  /// Create a new stream from a live tonic streaming response (internal).
  pub(crate) fn new(
    stream_id: String,
    consumer_id: String,
    inner: Streaming<proto::ChangeEvent>,
    client: NendiStreamClient<Channel>,
  ) -> Self {
    Self {
      stream_id,
      consumer_id,
      inner,
      client,
    }
  }

  /// Receive the next event from the stream.
  ///
  /// Returns `None` when the stream is cleanly closed by the daemon.
  /// Returns an error if the connection drops unexpectedly.
  pub async fn next(&mut self) -> Result<Option<ChangeEvent>, NendiError> {
    match self.inner.message().await {
      Ok(Some(proto_event)) => {
        let event = ChangeEvent::from_proto(proto_event)?;
        Ok(Some(event))
      }
      Ok(None) => Ok(None),
      Err(status) => Err(NendiError::Disconnected(status)),
    }
  }

  /// Commit an offset — tells the daemon it's safe to GC
  /// events before this position.
  pub async fn commit(&mut self, offset: Offset) -> Result<(), NendiError> {
    tracing::debug!(
        stream_id = %self.stream_id,
        offset = %offset,
        "Committing offset"
    );

    let lsn = offset.as_lsn();
    let req = proto::CommitRequest {
      consumer: self.consumer_id.clone(),
      lsn,
    };

    let resp = self
      .client
      .commit_offset(req)
      .await
      .map_err(NendiError::Disconnected)?;

    let body = resp.into_inner();
    if !body.ok {
      return Err(NendiError::Daemon {
        code: 1,
        message: body.error,
      });
    }

    Ok(())
  }

  /// Returns the stream ID.
  pub fn stream_id(&self) -> &str {
    &self.stream_id
  }
}
