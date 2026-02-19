//! Health check method on the client.

use crate::client::builder::NendiClient;
use crate::error::NendiError;

/// Health check status returned by `NendiClient::health()`.
#[derive(Debug, Clone)]
pub struct HealthStatus {
  /// Whether the daemon is serving.
  pub serving: bool,
  /// Daemon version string.
  pub version: String,
}

impl NendiClient {
  /// Check the health of the connected Nendi daemon.
  ///
  /// Calls the `GetStatus` RPC with the empty consumer name as a
  /// connectivity probe. Returns `serving: true` if the daemon responds.
  pub async fn health(&self) -> Result<HealthStatus, NendiError> {
    use crate::proto::{self, NendiStreamClient};

    tracing::debug!(
        endpoint = %self.endpoint(),
        "Checking daemon health"
    );

    let req = proto::StatusRequest {
      consumer: String::new(),
    };

    let mut client = NendiStreamClient::new(self.channel());
    let _resp = client
      .get_status(req)
      .await
      .map_err(NendiError::Disconnected)?;

    Ok(HealthStatus {
      serving: true,
      version: String::from("0.1.0"),
    })
  }
}
