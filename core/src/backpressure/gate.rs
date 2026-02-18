use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

/// Backpressure gate that can be opened or closed to pause the pipeline.
///
/// When the gate is closed, the ingestion side should stop feeding events
/// into the ring buffer. This is triggered by:
/// - Ring buffer exceeding high-water mark
/// - Memory budget exceeded
/// - WAL lag exceeding hard cap
/// - Manual pause command
pub struct BackpressureGate {
  /// Whether the gate is open (events flow) or closed (paused).
  open: Arc<AtomicBool>,
  /// Reason code for the current state.
  reason: Arc<std::sync::Mutex<Option<PauseReason>>>,
  /// Number of times the gate was closed.
  close_count: Arc<AtomicU64>,
}

/// Why the pipeline is paused.
#[derive(Debug, Clone)]
pub enum PauseReason {
  RingBufferFull,
  MemoryBudgetExceeded,
  WalLagHardCap,
  Manual,
}

impl BackpressureGate {
  pub fn new() -> Self {
    Self {
      open: Arc::new(AtomicBool::new(true)),
      reason: Arc::new(std::sync::Mutex::new(None)),
      close_count: Arc::new(AtomicU64::new(0)),
    }
  }

  /// Returns `true` if events should flow.
  pub fn is_open(&self) -> bool {
    self.open.load(Ordering::Acquire)
  }

  /// Close the gate (pause the pipeline).
  pub fn close(&self, reason: PauseReason) {
    self.open.store(false, Ordering::Release);
    self.close_count.fetch_add(1, Ordering::Relaxed);
    *self.reason.lock().unwrap() = Some(reason.clone());
    warn!(?reason, "backpressure gate closed");
  }

  /// Open the gate (resume the pipeline).
  pub fn open_gate(&self) {
    self.open.store(true, Ordering::Release);
    *self.reason.lock().unwrap() = None;
    info!("backpressure gate opened");
  }

  /// Returns the current pause reason, if any.
  pub fn pause_reason(&self) -> Option<PauseReason> {
    self.reason.lock().unwrap().clone()
  }

  /// Total number of times the gate was closed.
  pub fn close_count(&self) -> u64 {
    self.close_count.load(Ordering::Relaxed)
  }
}

impl Clone for BackpressureGate {
  fn clone(&self) -> Self {
    Self {
      open: Arc::clone(&self.open),
      reason: Arc::clone(&self.reason),
      close_count: Arc::clone(&self.close_count),
    }
  }
}
