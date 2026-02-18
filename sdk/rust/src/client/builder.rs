//! NendiClient builder pattern.

use std::time::Duration;

use crate::client::config::ClientConfig;
use crate::error::NendiError;
use crate::retry::RetryPolicy;
use crate::subscription::SubscriptionBuilder;

/// The main entry point for interacting with a Nendi daemon.
///
/// Created via `NendiClient::new()` for quick setup, or
/// `NendiClient::builder()` for full configuration.
pub struct NendiClient {
  config: ClientConfig,
}

/// Builder for configuring a `NendiClient`.
pub struct ClientBuilder {
  config: ClientConfig,
}

impl NendiClient {
  /// Connect to a Nendi daemon at the given endpoint with default settings.
  pub async fn new(endpoint: &str) -> Result<Self, NendiError> {
    Self::builder().endpoint(endpoint).build().await
  }

  /// Create a builder for a fully configured client.
  pub fn builder() -> ClientBuilder {
    ClientBuilder {
      config: ClientConfig::default(),
    }
  }

  /// Create a subscription builder for the given stream.
  pub fn subscription(&self, stream_id: &str) -> SubscriptionBuilder<'_> {
    SubscriptionBuilder::new(self, stream_id)
  }

  /// Quick subscribe to a stream with default settings.
  pub async fn subscribe(&self, stream_id: &str) -> Result<crate::stream::NendiStream, NendiError> {
    self.subscription(stream_id).subscribe().await
  }

  /// Returns the endpoint this client is connected to.
  pub fn endpoint(&self) -> &str {
    &self.config.endpoint
  }

  /// Returns the client configuration.
  pub fn config(&self) -> &ClientConfig {
    &self.config
  }
}

impl ClientBuilder {
  /// Set the daemon endpoint.
  pub fn endpoint(mut self, endpoint: &str) -> Self {
    self.config.endpoint = endpoint.to_string();
    self
  }

  /// Set the retry policy for reconnection.
  pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
    self.config.retry_policy = policy;
    self
  }

  /// Set the connection timeout.
  pub fn connect_timeout(mut self, timeout: Duration) -> Self {
    self.config.connect_timeout = timeout;
    self
  }

  /// Set the request timeout.
  pub fn request_timeout(mut self, timeout: Duration) -> Self {
    self.config.request_timeout = timeout;
    self
  }

  /// Build and connect the client.
  pub async fn build(self) -> Result<NendiClient, NendiError> {
    // TODO: establish tonic channel connection
    tracing::info!(
        endpoint = %self.config.endpoint,
        "Connecting to Nendi daemon"
    );

    Ok(NendiClient {
      config: self.config,
    })
  }
}
