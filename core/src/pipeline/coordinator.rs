use super::batch::WriteBatch;
use super::flush::FlushPolicy;
use shared::event::ChangeEvent;
use shared::queue::RingBuffer;
use tracing::debug;

/// Coordinates the ingestion pipeline: ring buffer → batch → flush.
///
/// The coordinator runs in a dedicated task and drains the ring buffer
/// into write batches. When the flush policy triggers, the batch is
/// written to RocksDB.
pub struct PipelineCoordinator {
  ring: RingBuffer<ChangeEvent>,
  flush_policy: FlushPolicy,
  current_batch: WriteBatch,
  /// Total events processed.
  events_processed: u64,
  /// Total batches flushed.
  batches_flushed: u64,
}

impl PipelineCoordinator {
  pub fn new(ring: RingBuffer<ChangeEvent>, flush_policy: FlushPolicy) -> Self {
    Self {
      ring,
      flush_policy,
      current_batch: WriteBatch::new(),
      events_processed: 0,
      batches_flushed: 0,
    }
  }

  /// Drain available events from the ring buffer into the current batch.
  ///
  /// Returns the number of events drained.
  pub fn drain_ring(&mut self) -> usize {
    let mut count = 0;
    while let Some(event) = self.ring.try_pop() {
      let size = event.estimated_size();
      self.current_batch.push(event);
      self.flush_policy.accumulate(size);
      count += 1;
    }
    self.events_processed += count as u64;
    count
  }

  /// Check if the current batch should be flushed and return it if so.
  ///
  /// The caller is responsible for writing the batch to storage.
  pub fn maybe_flush(&mut self) -> Option<WriteBatch> {
    if self.flush_policy.should_flush() && !self.current_batch.is_empty() {
      let batch = std::mem::replace(&mut self.current_batch, WriteBatch::new());
      self.flush_policy.mark_flushed();
      self.batches_flushed += 1;

      debug!(
          events = batch.len(),
          bytes = batch.total_bytes(),
          min_lsn = %batch.min_lsn(),
          max_lsn = %batch.max_lsn(),
          "flushing batch"
      );

      Some(batch)
    } else {
      None
    }
  }

  /// Force flush the current batch regardless of the policy.
  pub fn force_flush(&mut self) -> Option<WriteBatch> {
    if !self.current_batch.is_empty() {
      let batch = std::mem::replace(&mut self.current_batch, WriteBatch::new());
      self.flush_policy.mark_flushed();
      self.batches_flushed += 1;
      Some(batch)
    } else {
      None
    }
  }

  pub fn events_processed(&self) -> u64 {
    self.events_processed
  }

  pub fn batches_flushed(&self) -> u64 {
    self.batches_flushed
  }

  pub fn ring_fill_ratio(&self) -> f64 {
    self.ring.fill_ratio()
  }
}
