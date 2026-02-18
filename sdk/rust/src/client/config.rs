//! Client configuration.

use std::time::Duration;

use crate::retry::RetryPolicy;

/// Configuration for a `NendiClient` connection.
#[derive(Debug, Clone)]
pub struct ClientConfig {
  /// The gRPC endpoint to connect to (e.g. `"http://nendi:50051"`).
  pub endpoint: String,

  /// Retry policy for reconnection.
  pub retry_policy: RetryPolicy,

  /// Connection timeout.
  pub connect_timeout: Duration,

  /// Request timeout for individual RPCs.
  pub request_timeout: Duration,
}

impl Default for ClientConfig {
  fn default() -> Self {
    Self {
      endpoint: "http://localhost:50051".into(),
      retry_policy: RetryPolicy::default(),
      connect_timeout: Duration::from_secs(5),
      request_timeout: Duration::from_secs(30),
    }
  }
}
