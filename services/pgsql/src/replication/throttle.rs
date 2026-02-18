use std::time::{Duration, Instant};
use tracing::debug;

/// Replication stream throttle to prevent overwhelming the pipeline.
///
/// Uses a token-bucket algorithm: the stream can consume up to `burst`
/// messages immediately, then is limited to `rate` messages per second.
/// When the bucket is empty, the stream should back off.
pub struct ReplicationThrottle {
  /// Maximum burst size.
  burst: u64,
  /// Messages per second sustained rate.
  rate: f64,
  /// Current token count.
  tokens: f64,
  /// Last time tokens were refilled.
  last_refill: Instant,
}

impl ReplicationThrottle {
  /// Create a new throttle with the given rate limit.
  ///
  /// - `msgs_per_sec`: sustained message rate
  /// - `burst`: maximum burst above the sustained rate
  pub fn new(msgs_per_sec: f64, burst: u64) -> Self {
    Self {
      burst,
      rate: msgs_per_sec,
      tokens: burst as f64,
      last_refill: Instant::now(),
    }
  }

  /// Create a disabled (unlimited) throttle.
  pub fn unlimited() -> Self {
    Self {
      burst: u64::MAX,
      rate: f64::MAX,
      tokens: f64::MAX,
      last_refill: Instant::now(),
    }
  }

  /// Try to acquire a token. Returns `true` if allowed, `false` if
  /// the caller should back off.
  pub fn try_acquire(&mut self) -> bool {
    self.refill();

    if self.tokens >= 1.0 {
      self.tokens -= 1.0;
      true
    } else {
      debug!(tokens = self.tokens, "replication throttle: backing off");
      false
    }
  }

  /// How long the caller should wait before retrying.
  pub fn backoff_duration(&self) -> Duration {
    if self.tokens >= 1.0 {
      Duration::ZERO
    } else {
      let deficit = 1.0 - self.tokens;
      Duration::from_secs_f64(deficit / self.rate)
    }
  }

  fn refill(&mut self) {
    let now = Instant::now();
    let elapsed = now.duration_since(self.last_refill);
    let new_tokens = elapsed.as_secs_f64() * self.rate;
    self.tokens = (self.tokens + new_tokens).min(self.burst as f64);
    self.last_refill = now;
  }
}
