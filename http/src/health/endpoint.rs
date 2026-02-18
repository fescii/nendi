use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;

/// Health check endpoint.
///
/// Reports readiness and liveness for Kubernetes-style health probes.
/// The daemon is "ready" when it has connected to PostgreSQL, opened
/// RocksDB, and started the replication stream.
pub struct HealthEndpoint {
  ready: Arc<AtomicBool>,
  live: Arc<AtomicBool>,
}

impl HealthEndpoint {
  pub fn new() -> Self {
    Self {
      ready: Arc::new(AtomicBool::new(false)),
      live: Arc::new(AtomicBool::new(true)),
    }
  }

  /// Mark the daemon as ready.
  pub fn set_ready(&self) {
    self.ready.store(true, Ordering::Release);
    info!("health: daemon is ready");
  }

  /// Mark the daemon as not ready (e.g. during drain).
  pub fn set_not_ready(&self) {
    self.ready.store(false, Ordering::Release);
  }

  /// Mark the daemon as not live (fatal error).
  pub fn set_not_live(&self) {
    self.live.store(false, Ordering::Release);
  }

  /// Returns `true` if the daemon is ready to serve traffic.
  pub fn is_ready(&self) -> bool {
    self.ready.load(Ordering::Acquire)
  }

  /// Returns `true` if the daemon is alive and should not be restarted.
  pub fn is_live(&self) -> bool {
    self.live.load(Ordering::Acquire)
  }

  /// Health check response as JSON.
  pub fn status_json(&self) -> String {
    format!(
      r#"{{"ready":{},"live":{}}}"#,
      self.is_ready(),
      self.is_live()
    )
  }
}

impl Clone for HealthEndpoint {
  fn clone(&self) -> Self {
    Self {
      ready: Arc::clone(&self.ready),
      live: Arc::clone(&self.live),
    }
  }
}
