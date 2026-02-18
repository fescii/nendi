use shared::event::ChangeEvent;
use std::time::Duration;

/// Batches events into micro-batches before dispatching to webhooks.
///
/// This amortizes HTTP overhead: instead of one POST per event,
/// we send a batch of N events in a single payload.
pub struct WebhookBatcher {
  /// Maximum events per batch.
  max_batch_size: usize,
  /// Maximum time to wait before dispatching a partial batch.
  max_linger: Duration,
  /// Current batch buffer.
  buffer: Vec<ChangeEvent>,
}

impl WebhookBatcher {
  pub fn new(max_batch_size: usize, max_linger: Duration) -> Self {
    Self {
      max_batch_size,
      max_linger,
      buffer: Vec::with_capacity(max_batch_size),
    }
  }

  pub fn from_target(target: &shared::config::WebhookTarget) -> Self {
    Self::new(target.batch_size, Duration::from_millis(target.linger_ms))
  }

  /// Add an event to the batch.
  pub fn push(&mut self, event: ChangeEvent) {
    self.buffer.push(event);
  }

  /// Check if the batch is full.
  pub fn is_full(&self) -> bool {
    self.buffer.len() >= self.max_batch_size
  }

  /// Drain the current batch.
  pub fn drain(&mut self) -> Vec<ChangeEvent> {
    std::mem::replace(&mut self.buffer, Vec::with_capacity(self.max_batch_size))
  }

  pub fn len(&self) -> usize {
    self.buffer.len()
  }

  pub fn is_empty(&self) -> bool {
    self.buffer.is_empty()
  }

  pub fn max_linger(&self) -> Duration {
    self.max_linger
  }
}
