use shared::lsn::Lsn;

/// Handles replay of stored events for consumers that need to catch up.
///
/// When a consumer connects with a committed offset, the replay module
/// reads events from RocksDB starting at that offset and streams them
/// before switching to live events.
pub struct ReplayManager {
  /// Maximum events per replay batch.
  batch_size: usize,
}

impl ReplayManager {
  pub fn new(batch_size: usize) -> Self {
    Self { batch_size }
  }

  /// Determine the replay range for a consumer.
  pub fn replay_range(&self, committed_lsn: Lsn, head_lsn: Lsn) -> Option<ReplayRange> {
    if committed_lsn >= head_lsn {
      None // Consumer is caught up
    } else {
      Some(ReplayRange {
        start_lsn: committed_lsn.saturating_add(1),
        end_lsn: head_lsn,
      })
    }
  }

  pub fn batch_size(&self) -> usize {
    self.batch_size
  }
}

/// A range of events to replay.
#[derive(Debug)]
pub struct ReplayRange {
  pub start_lsn: Lsn,
  pub end_lsn: Lsn,
}

impl ReplayRange {
  /// Approximate number of events to replay.
  pub fn estimate_events(&self) -> u64 {
    self.end_lsn.get().saturating_sub(self.start_lsn.get())
  }
}
