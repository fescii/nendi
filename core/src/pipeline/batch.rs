use shared::event::ChangeEvent;
use shared::lsn::Lsn;

/// A write batch of events to be flushed to RocksDB.
///
/// Events are accumulated from the ring buffer until the flush policy
/// triggers. The entire batch is written atomically using RocksDB's
/// WriteBatch for crash consistency.
pub struct WriteBatch {
  events: Vec<ChangeEvent>,
  min_lsn: Lsn,
  max_lsn: Lsn,
  total_bytes: usize,
}

impl WriteBatch {
  pub fn new() -> Self {
    Self {
      events: Vec::new(),
      min_lsn: Lsn::MAX,
      max_lsn: Lsn::ZERO,
      total_bytes: 0,
    }
  }

  pub fn with_capacity(cap: usize) -> Self {
    Self {
      events: Vec::with_capacity(cap),
      min_lsn: Lsn::MAX,
      max_lsn: Lsn::ZERO,
      total_bytes: 0,
    }
  }

  /// Add an event to the batch.
  pub fn push(&mut self, event: ChangeEvent) {
    let lsn = event.lsn();
    let size = event.estimated_size();

    if lsn < self.min_lsn {
      self.min_lsn = lsn;
    }
    if lsn > self.max_lsn {
      self.max_lsn = lsn;
    }

    self.total_bytes += size;
    self.events.push(event);
  }

  /// Returns the events in this batch.
  pub fn events(&self) -> &[ChangeEvent] {
    &self.events
  }

  /// Consume the batch, returning the events.
  pub fn into_events(self) -> Vec<ChangeEvent> {
    self.events
  }

  /// Returns the number of events in this batch.
  pub fn len(&self) -> usize {
    self.events.len()
  }

  /// Returns whether the batch is empty.
  pub fn is_empty(&self) -> bool {
    self.events.is_empty()
  }

  /// Returns the total estimated byte size.
  pub fn total_bytes(&self) -> usize {
    self.total_bytes
  }

  /// Returns the minimum LSN in this batch.
  pub fn min_lsn(&self) -> Lsn {
    self.min_lsn
  }

  /// Returns the maximum LSN in this batch.
  pub fn max_lsn(&self) -> Lsn {
    self.max_lsn
  }

  /// Clear the batch for reuse.
  pub fn clear(&mut self) {
    self.events.clear();
    self.min_lsn = Lsn::MAX;
    self.max_lsn = Lsn::ZERO;
    self.total_bytes = 0;
  }
}
