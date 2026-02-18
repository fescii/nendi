use std::time::Instant;

/// Dual-threshold adaptive flush policy.
///
/// Triggers a flush when EITHER condition fires first:
/// 1. Accumulated bytes exceed `max_bytes`
/// 2. Time since last flush exceeds `max_age_us`
///
/// This prevents both latency spikes (waiting too long) and throughput
/// degradation (flushing too eagerly). Per the mitigations doc, this
/// is the primary mechanism for controlling write amplification.
pub struct FlushPolicy {
  /// Maximum accumulated bytes before triggering a flush.
  max_bytes: usize,
  /// Maximum age in microseconds before triggering a flush.
  max_age_us: u64,
  /// Current accumulated byte count.
  current_bytes: usize,
  /// Timestamp of the last flush.
  last_flush: Instant,
  /// Total number of flushes performed.
  flush_count: u64,
}

impl FlushPolicy {
  pub fn new(max_bytes: usize, max_age_us: u64) -> Self {
    Self {
      max_bytes,
      max_age_us,
      current_bytes: 0,
      last_flush: Instant::now(),
      flush_count: 0,
    }
  }

  /// Create from the pipeline config.
  pub fn from_config(cfg: &shared::config::PipelineConfig) -> Self {
    Self::new(cfg.flush_max_bytes, cfg.flush_max_age_us)
  }

  /// Accumulate bytes from a new event.
  pub fn accumulate(&mut self, bytes: usize) {
    self.current_bytes += bytes;
  }

  /// Check if a flush should be triggered.
  pub fn should_flush(&self) -> bool {
    // Size threshold
    if self.current_bytes >= self.max_bytes {
      return true;
    }
    // Time threshold
    let age_us = self.last_flush.elapsed().as_micros() as u64;
    if age_us >= self.max_age_us && self.current_bytes > 0 {
      return true;
    }
    false
  }

  /// Mark a flush as completed, resetting the counters.
  pub fn mark_flushed(&mut self) {
    self.current_bytes = 0;
    self.last_flush = Instant::now();
    self.flush_count += 1;
  }

  /// Returns the current accumulated byte count.
  pub fn current_bytes(&self) -> usize {
    self.current_bytes
  }

  /// Returns the total number of flushes.
  pub fn flush_count(&self) -> u64 {
    self.flush_count
  }

  /// Returns microseconds since the last flush.
  pub fn age_us(&self) -> u64 {
    self.last_flush.elapsed().as_micros() as u64
  }
}
