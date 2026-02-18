use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Tracks memory usage against a configured budget.
///
/// Components register allocations and the budget enforcer can trigger
/// backpressure when usage exceeds the threshold.
pub struct MemoryBudget {
  /// Total allowed bytes.
  budget_bytes: usize,
  /// Warning threshold (fraction of budget).
  warn_threshold: f64,
  /// Current usage in bytes.
  used: Arc<AtomicUsize>,
}

impl MemoryBudget {
  pub fn new(budget_bytes: usize, warn_threshold: f64) -> Self {
    Self {
      budget_bytes,
      warn_threshold: warn_threshold.clamp(0.0, 1.0),
      used: Arc::new(AtomicUsize::new(0)),
    }
  }

  pub fn from_config(cfg: &shared::config::MemoryConfig) -> Self {
    Self::new(cfg.budget_bytes, cfg.warn_threshold)
  }

  /// Record an allocation.
  pub fn allocate(&self, bytes: usize) {
    self.used.fetch_add(bytes, Ordering::Relaxed);
  }

  /// Record a deallocation.
  pub fn deallocate(&self, bytes: usize) {
    self.used.fetch_sub(bytes, Ordering::Relaxed);
  }

  /// Current usage in bytes.
  pub fn used(&self) -> usize {
    self.used.load(Ordering::Relaxed)
  }

  /// Usage as a fraction of the budget (0.0–1.0+).
  pub fn usage_ratio(&self) -> f64 {
    self.used() as f64 / self.budget_bytes as f64
  }

  /// Check if budget is exceeded.
  pub fn is_exceeded(&self) -> bool {
    self.used() > self.budget_bytes
  }

  /// Check if usage is above the warning threshold.
  pub fn is_warning(&self) -> bool {
    self.usage_ratio() > self.warn_threshold
  }

  /// Total budget in bytes.
  pub fn budget(&self) -> usize {
    self.budget_bytes
  }

  /// Remaining bytes before exceeding budget.
  pub fn remaining(&self) -> usize {
    self.budget_bytes.saturating_sub(self.used())
  }
}

impl Clone for MemoryBudget {
  fn clone(&self) -> Self {
    Self {
      budget_bytes: self.budget_bytes,
      warn_threshold: self.warn_threshold,
      used: Arc::clone(&self.used),
    }
  }
}
