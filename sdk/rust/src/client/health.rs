//! Health check method on the client.

use crate::client::builder::NendiClient;
use crate::error::NendiError;

/// Health check status.
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// Whether the daemon is serving.
    pub serving: bool,
    /// Daemon version string.
    pub version: String,
}

impl NendiClient {
    /// Check the health of the connected Nendi daemon.
    pub async fn health(&self) -> Result<HealthStatus, NendiError> {
        // TODO: call Health RPC on the daemon
        tracing::debug!(
            endpoint = %self.endpoint(),
            "Checking daemon health"
        );

        Ok(HealthStatus {
            serving: true,
            version: String::from("0.1.0"),
        })
    }
}
