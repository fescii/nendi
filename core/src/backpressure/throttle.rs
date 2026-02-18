use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tracing::debug;

/// Credit-based flow control for consumer streams.
///
/// Each consumer starts with a credit window. Each delivered event
/// consumes one credit. When credits reach zero, the server stops
/// sending until the consumer commits offsets (replenishing credits).
///
/// This prevents a slow consumer from unbounded memory growth on the
/// server side.
pub struct CreditThrottle {
  /// Current available credits.
  credits: Arc<AtomicU32>,
  /// Window size (initial / replenish amount).
  window: u32,
}

impl CreditThrottle {
  pub fn new(window: u32) -> Self {
    Self {
      credits: Arc::new(AtomicU32::new(window)),
      window,
    }
  }

  /// Try to consume one credit. Returns `true` if credit was available.
  pub fn try_consume(&self) -> bool {
    loop {
      let current = self.credits.load(Ordering::Acquire);
      if current == 0 {
        return false;
      }
      if self
        .credits
        .compare_exchange_weak(current, current - 1, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
      {
        return true;
      }
    }
  }

  /// Replenish credits (called when consumer commits offsets).
  pub fn replenish(&self, amount: u32) {
    let amount = amount.min(self.window);
    self.credits.fetch_add(amount, Ordering::Release);
    debug!(credits = self.available(), "credits replenished");
  }

  /// Reset credits to the full window.
  pub fn reset(&self) {
    self.credits.store(self.window, Ordering::Release);
  }

  /// Returns available credits.
  pub fn available(&self) -> u32 {
    self.credits.load(Ordering::Relaxed)
  }

  /// Returns the window size.
  pub fn window(&self) -> u32 {
    self.window
  }
}

impl Clone for CreditThrottle {
  fn clone(&self) -> Self {
    Self {
      credits: Arc::clone(&self.credits),
      window: self.window,
    }
  }
}
