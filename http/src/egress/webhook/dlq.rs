use shared::event::ChangeEvent;
use std::collections::VecDeque;
use tracing::warn;

/// Dead Letter Queue for events that fail webhook delivery after
/// all retries are exhausted.
///
/// Failed events are stored here for later manual inspection or
/// replay. The DLQ has a bounded size to prevent unbounded growth.
pub struct DeadLetterQueue {
  queue: VecDeque<DlqEntry>,
  max_size: usize,
  total_entries: u64,
}

#[derive(Debug)]
pub struct DlqEntry {
  pub event: ChangeEvent,
  pub error: String,
  pub attempts: u32,
  pub timestamp_us: u64,
}

impl DeadLetterQueue {
  pub fn new(max_size: usize) -> Self {
    Self {
      queue: VecDeque::with_capacity(max_size),
      max_size,
      total_entries: 0,
    }
  }

  /// Push a failed event into the DLQ.
  pub fn push(&mut self, event: ChangeEvent, error: String, attempts: u32) {
    if self.queue.len() >= self.max_size {
      // Evict the oldest entry.
      self.queue.pop_front();
      warn!("DLQ full, evicting oldest entry");
    }

    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_micros() as u64;

    self.queue.push_back(DlqEntry {
      event,
      error,
      attempts,
      timestamp_us: now,
    });
    self.total_entries += 1;
  }

  /// Drain all entries from the DLQ (for manual replay).
  pub fn drain_all(&mut self) -> Vec<DlqEntry> {
    self.queue.drain(..).collect()
  }

  /// Current number of entries in the DLQ.
  pub fn len(&self) -> usize {
    self.queue.len()
  }

  pub fn is_empty(&self) -> bool {
    self.queue.is_empty()
  }

  /// Total entries ever added to the DLQ.
  pub fn total_entries(&self) -> u64 {
    self.total_entries
  }
}
