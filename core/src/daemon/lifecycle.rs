use tokio::signal;
use tracing::info;

/// Manages the daemon lifecycle: startup → running → graceful shutdown.
pub struct Lifecycle {
  state: LifecycleState,
}

/// Current lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
  /// Initializing (loading config, connecting to PG, opening RocksDB).
  Initializing,
  /// Running (streaming events).
  Running,
  /// Shutting down (draining pipeline, closing connections).
  ShuttingDown,
  /// Stopped.
  Stopped,
}

impl Lifecycle {
  pub fn new() -> Self {
    Self {
      state: LifecycleState::Initializing,
    }
  }

  /// Transition to the next state.
  pub fn transition(&mut self, new_state: LifecycleState) {
    info!(
        from = ?self.state,
        to = ?new_state,
        "lifecycle state transition"
    );
    self.state = new_state;
  }

  /// Current state.
  pub fn state(&self) -> LifecycleState {
    self.state
  }

  /// Wait for a shutdown signal (SIGINT or SIGTERM).
  pub async fn wait_for_shutdown() {
    let ctrl_c = async {
      signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
      signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler")
        .recv()
        .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("received SIGINT"),
        _ = terminate => info!("received SIGTERM"),
    }
  }
}
