/// gRPC-level backpressure using server-sent flow control.
///
/// When a consumer stream is slow, the server stops sending events
/// and applies backpressure upstream. This is layered ON TOP of the
/// credit-based throttle in the core crate.
pub struct GrpcBackpressure {
  /// Maximum number of pending (unacked) events per stream.
  max_pending: usize,
  /// Current pending count.
  pending: usize,
}

impl GrpcBackpressure {
  pub fn new(max_pending: usize) -> Self {
    Self {
      max_pending,
      pending: 0,
    }
  }

  /// Record an event being sent.
  pub fn on_send(&mut self) {
    self.pending += 1;
  }

  /// Record an acknowledgement from the consumer.
  pub fn on_ack(&mut self, count: usize) {
    self.pending = self.pending.saturating_sub(count);
  }

  /// Whether we should stop sending events.
  pub fn should_pause(&self) -> bool {
    self.pending >= self.max_pending
  }

  pub fn pending(&self) -> usize {
    self.pending
  }
}
