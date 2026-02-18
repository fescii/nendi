//! Retry policy for reconnection.

use std::time::Duration;

/// Policy that controls how the SDK reconnects after a disconnect.
#[derive(Debug, Clone)]
pub enum RetryPolicy {
  /// No retry — error immediately on any disconnect.
  None,

  /// Retry at a fixed interval.
  Fixed {
    /// How long to wait between retries.
    interval: Duration,
    /// Maximum number of attempts (0 = infinite).
    max_attempts: u32,
  },

  /// Retry with exponential backoff.
  Exponential {
    /// Initial backoff duration.
    initial: Duration,
    /// Maximum backoff duration (cap).
    max: Duration,
    /// Maximum number of attempts (0 = infinite).
    max_attempts: u32,
  },
}

impl RetryPolicy {
  /// Create a policy that never retries.
  pub fn none() -> Self {
    Self::None
  }

  /// Create a fixed-interval retry policy.
  pub fn fixed(interval: Duration, max_attempts: u32) -> Self {
    Self::Fixed {
      interval,
      max_attempts,
    }
  }

  /// Create an exponential backoff retry policy.
  pub fn exponential(initial: Duration, max: Duration, max_attempts: u32) -> Self {
    Self::Exponential {
      initial,
      max,
      max_attempts,
    }
  }

  /// Compute the delay for a given attempt number.
  pub fn delay_for(&self, attempt: u32) -> Option<Duration> {
    match self {
      Self::None => None,
      Self::Fixed {
        interval,
        max_attempts,
      } => {
        if *max_attempts > 0 && attempt >= *max_attempts {
          None
        } else {
          Some(*interval)
        }
      }
      Self::Exponential {
        initial,
        max,
        max_attempts,
      } => {
        if *max_attempts > 0 && attempt >= *max_attempts {
          None
        } else {
          let delay = initial.saturating_mul(2u32.saturating_pow(attempt));
          Some(delay.min(*max))
        }
      }
    }
  }
}

impl Default for RetryPolicy {
  fn default() -> Self {
    // Default: exponential backoff, 50ms start, 60s cap, infinite retries
    Self::exponential(Duration::from_millis(50), Duration::from_secs(60), 0)
  }
}
