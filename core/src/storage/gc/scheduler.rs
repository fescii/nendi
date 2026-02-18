use super::watermark::WatermarkTracker;
use std::time::Duration;
use tracing::debug;

/// GC scheduler that periodically triggers compaction based on the
/// consumer watermark.
///
/// The scheduler runs in a background task and:
/// 1. Queries the watermark tracker for the current safe-to-GC LSN
/// 2. Triggers a RocksDB manual compaction with the EventGcFilter
/// 3. Cleans up old WAL segments
pub struct GcScheduler {
  /// Interval between GC checks.
  interval: Duration,
  /// Watermark tracker to determine safe-to-GC LSN.
  watermark: WatermarkTracker,
  /// Last watermark used for GC.
  last_gc_watermark: shared::lsn::Lsn,
  /// Total GC runs performed.
  gc_runs: u64,
}

impl GcScheduler {
  pub fn new(interval: Duration) -> Self {
    Self {
      interval,
      watermark: WatermarkTracker::new(),
      last_gc_watermark: shared::lsn::Lsn::ZERO,
      gc_runs: 0,
    }
  }

  /// Returns a reference to the watermark tracker.
  pub fn watermark_tracker(&self) -> &WatermarkTracker {
    &self.watermark
  }

  /// Check if it's time to run GC and return the watermark if so.
  pub fn maybe_gc(&mut self) -> Option<shared::lsn::Lsn> {
    let current_watermark = self.watermark.watermark();

    // Only GC if the watermark has advanced.
    if current_watermark.is_ahead_of(self.last_gc_watermark) {
      self.last_gc_watermark = current_watermark;
      self.gc_runs += 1;

      debug!(
          watermark = %current_watermark,
          gc_run = self.gc_runs,
          "GC triggered"
      );

      Some(current_watermark)
    } else {
      None
    }
  }

  pub fn interval(&self) -> Duration {
    self.interval
  }

  pub fn gc_runs(&self) -> u64 {
    self.gc_runs
  }
}
