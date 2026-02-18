use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Circuit breaker for webhook endpoints.
///
/// Implements the standard 3-state pattern:
/// - **Closed**: Requests flow normally. On N consecutive failures,
///   transitions to Open.
/// - **Open**: All requests are rejected immediately. After a timeout,
///   transitions to HalfOpen.
/// - **HalfOpen**: One probe request is allowed. On success → Closed,
///   on failure → Open.
pub struct CircuitBreaker {
  state: CircuitState,
  /// Consecutive failure count.
  failure_count: u32,
  /// Failure threshold before opening the circuit.
  failure_threshold: u32,
  /// Time to wait in Open state before probing.
  recovery_timeout: Duration,
  /// When the circuit was opened.
  opened_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
  Closed,
  Open,
  HalfOpen,
}

impl CircuitBreaker {
  pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
    Self {
      state: CircuitState::Closed,
      failure_count: 0,
      failure_threshold,
      recovery_timeout,
      opened_at: None,
    }
  }

  /// Check if a request should be allowed.
  pub fn allow_request(&mut self) -> bool {
    match self.state {
      CircuitState::Closed => true,
      CircuitState::Open => {
        if let Some(opened_at) = self.opened_at {
          if opened_at.elapsed() >= self.recovery_timeout {
            self.state = CircuitState::HalfOpen;
            info!("circuit breaker: Half-Open (probing)");
            true
          } else {
            false
          }
        } else {
          false
        }
      }
      CircuitState::HalfOpen => {
        // Only one probe request is allowed.
        false // Already probing.
      }
    }
  }

  /// Record a successful request.
  pub fn on_success(&mut self) {
    match self.state {
      CircuitState::HalfOpen => {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.opened_at = None;
        info!("circuit breaker: Closed (recovered)");
      }
      _ => {
        self.failure_count = 0;
      }
    }
  }

  /// Record a failed request.
  pub fn on_failure(&mut self) {
    self.failure_count += 1;
    match self.state {
      CircuitState::Closed => {
        if self.failure_count >= self.failure_threshold {
          self.state = CircuitState::Open;
          self.opened_at = Some(Instant::now());
          warn!(
            failures = self.failure_count,
            "circuit breaker: Open (tripped)"
          );
        }
      }
      CircuitState::HalfOpen => {
        self.state = CircuitState::Open;
        self.opened_at = Some(Instant::now());
        warn!("circuit breaker: Open (probe failed)");
      }
      CircuitState::Open => {}
    }
  }

  pub fn state(&self) -> CircuitState {
    self.state
  }

  pub fn failure_count(&self) -> u32 {
    self.failure_count
  }
}
