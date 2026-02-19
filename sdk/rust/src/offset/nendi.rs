//! Daemon-side offset store (commits via gRPC).

use async_trait::async_trait;
use tonic::transport::Channel;

use crate::error::NendiError;
use crate::offset::{Offset, OffsetStore};
use crate::proto::{self, NendiStreamClient};

/// Offset store that commits offsets to the Nendi daemon via gRPC.
///
/// This is the default store — offsets are tracked server-side.
/// The daemon uses committed offsets for garbage collection and
/// consumer lag monitoring.
pub struct NendiOffsetStore {
  consumer: String,
  client: NendiStreamClient<Channel>,
}

impl NendiOffsetStore {
  /// Create a new daemon-side offset store connected to the given channel.
  pub fn new(consumer: impl Into<String>, channel: Channel) -> Self {
    Self {
      consumer: consumer.into(),
      client: NendiStreamClient::new(channel),
    }
  }
}

#[async_trait]
impl OffsetStore for NendiOffsetStore {
  /// Query the daemon for the last committed LSN via `GetStatus` RPC.
  async fn load(&self, _stream_id: &str) -> Result<Option<Offset>, NendiError> {
    let req = proto::StatusRequest {
      consumer: self.consumer.clone(),
    };
    let resp = self
      .client
      .clone()
      .get_status(req)
      .await
      .map_err(NendiError::Disconnected)?;
    let body = resp.into_inner();
    let lsn = body.committed;
    if lsn == 0 {
      Ok(None)
    } else {
      Ok(Some(Offset::from_lsn(lsn)))
    }
  }

  /// Commit an offset to the daemon via `CommitOffset` RPC.
  async fn save(&self, _stream_id: &str, offset: &Offset) -> Result<(), NendiError> {
    let req = proto::CommitRequest {
      consumer: self.consumer.clone(),
      lsn: offset.as_lsn(),
    };
    let resp = self
      .client
      .clone()
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
}
