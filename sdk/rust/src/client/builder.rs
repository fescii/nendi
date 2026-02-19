//! NendiClient builder pattern.

use std::sync::Arc;
use std::time::Duration;

use tonic::transport::Channel;

use crate::client::config::ClientConfig;
use crate::error::NendiError;
use crate::retry::RetryPolicy;
use crate::subscription::SubscriptionBuilder;

/// The main entry point for interacting with a Nendi daemon.
///
/// Created via `NendiClient::new()` for quick setup, or
/// `NendiClient::builder()` for full configuration.
#[derive(Clone)]
pub struct NendiClient {
  pub(crate) config: ClientConfig,
  /// Cheap-to-clone tonic channel (Arc internally).
  pub(crate) channel: Channel,
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
  pub fn subscription<'c>(&'c self, stream_id: &str) -> SubscriptionBuilder<'c> {
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

  /// Returns a reference to the underlying tonic channel.
  pub(crate) fn channel(&self) -> Channel {
    self.channel.clone()
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

  /// Build and connect the client using a lazy tonic channel.
  ///
  /// The channel does not actually open a TCP connection until the
  /// first RPC is made — so this method returns immediately even if
  /// the daemon is not running yet.
  pub async fn build(self) -> Result<NendiClient, NendiError> {
    tracing::info!(
        endpoint = %self.config.endpoint,
        "Connecting to Nendi daemon"
    );

    let endpoint = tonic::transport::Endpoint::from_shared(self.config.endpoint.clone())
      .map_err(|e| NendiError::Connection(e.into()))?
      .connect_timeout(self.config.connect_timeout)
      .timeout(self.config.request_timeout);

    // `connect_lazy` returns instantly — no TCP dial happens here.
    let channel = endpoint.connect_lazy();

    Ok(NendiClient {
      config: self.config,
      channel,
    })
  }
}
