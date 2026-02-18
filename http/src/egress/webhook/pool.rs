use reqwest::Client;
use std::time::Duration;

/// HTTP connection pool for webhook dispatching.
///
/// Wraps `reqwest::Client` with settings tuned for CDC delivery:
/// - Connection keep-alive for reducing TCP overhead
/// - Bounded pool size to prevent resource exhaustion
/// - Configurable timeouts
pub struct ConnectionPool {
  client: Client,
  pool_size: usize,
}

impl ConnectionPool {
  pub fn new(pool_size: usize, timeout: Duration) -> Self {
    let client = Client::builder()
      .pool_max_idle_per_host(pool_size)
      .pool_idle_timeout(Duration::from_secs(90))
      .timeout(timeout)
      .tcp_keepalive(Duration::from_secs(30))
      .build()
      .expect("failed to build connection pool");

    Self { client, pool_size }
  }

  pub fn client(&self) -> &Client {
    &self.client
  }

  pub fn pool_size(&self) -> usize {
    self.pool_size
  }
}
