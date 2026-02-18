use std::time::Duration;

/// Exponential backoff retry policy for webhook delivery.
pub struct RetryPolicy {
  /// Maximum number of attempts (including the first).
  pub max_attempts: u32,
  /// Initial backoff duration.
  pub initial_backoff: Duration,
  /// Maximum backoff duration.
  pub max_backoff: Duration,
  /// Backoff multiplier.
  pub multiplier: f64,
}

impl RetryPolicy {
  pub fn new(max_attempts: u32, initial_backoff: Duration) -> Self {
    Self {
      max_attempts,
      initial_backoff,
      max_backoff: Duration::from_secs(60),
      multiplier: 2.0,
    }
  }

  pub fn from_target(target: &shared::config::WebhookTarget) -> Self {
    Self::new(target.retry_max_attempts, Duration::from_millis(500))
  }

  /// Compute the backoff duration for a given attempt (0-indexed).
  pub fn backoff_for(&self, attempt: u32) -> Duration {
    let backoff = self
      .initial_backoff
      .mul_f64(self.multiplier.powi(attempt as i32));
    backoff.min(self.max_backoff)
  }

  /// Should we retry after this attempt?
  pub fn should_retry(&self, attempt: u32) -> bool {
    attempt + 1 < self.max_attempts
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn exponential_backoff() {
    let policy = RetryPolicy::new(5, Duration::from_millis(100));
    assert_eq!(policy.backoff_for(0), Duration::from_millis(100));
    assert_eq!(policy.backoff_for(1), Duration::from_millis(200));
    assert_eq!(policy.backoff_for(2), Duration::from_millis(400));
    assert_eq!(policy.backoff_for(3), Duration::from_millis(800));
  }

  #[test]
  fn caps_at_max() {
    let mut policy = RetryPolicy::new(10, Duration::from_secs(1));
    policy.max_backoff = Duration::from_secs(30);
    assert_eq!(policy.backoff_for(10), Duration::from_secs(30));
  }
}
