use shared::lsn::Lsn;
use std::time::Instant;
use tracing::debug;

/// Manages LSN acknowledgement (standby status updates) to PostgreSQL.
///
/// PostgreSQL requires periodic status updates from replication clients.
/// If no update is sent within `wal_sender_timeout` (default 60s), the
/// server disconnects the replication connection.
///
/// The AckManager batches LSN updates to avoid excessive round-trips:
/// - At most one ACK per `min_interval`
/// - At least one ACK per `max_interval` (heartbeat)
pub struct AckManager {
  /// The highest LSN we have confirmed durable (written to RocksDB).
  confirmed_flush_lsn: Lsn,
  /// The highest LSN we have received (may not yet be durable).
  received_lsn: Lsn,
  /// Minimum interval between ACKs.
  min_interval: std::time::Duration,
  /// Maximum interval between ACKs (heartbeat).
  max_interval: std::time::Duration,
  /// Timestamp of the last ACK sent.
  last_ack_time: Instant,
  /// Whether there are pending LSN updates to send.
  dirty: bool,
}

impl AckManager {
  pub fn new(min_interval: std::time::Duration, max_interval: std::time::Duration) -> Self {
    Self {
      confirmed_flush_lsn: Lsn::ZERO,
      received_lsn: Lsn::ZERO,
      min_interval,
      max_interval,
      last_ack_time: Instant::now(),
      dirty: false,
    }
  }

  /// Update the received LSN (highest WAL position seen).
  pub fn advance_received(&mut self, lsn: Lsn) {
    if lsn.is_ahead_of(self.received_lsn) {
      self.received_lsn = lsn;
      self.dirty = true;
    }
  }

  /// Update the confirmed flush LSN (durable in RocksDB).
  pub fn advance_confirmed(&mut self, lsn: Lsn) {
    if lsn.is_ahead_of(self.confirmed_flush_lsn) {
      self.confirmed_flush_lsn = lsn;
      self.dirty = true;
    }
  }

  /// Check if an ACK should be sent now.
  pub fn should_ack(&self) -> bool {
    let elapsed = self.last_ack_time.elapsed();

    // Always send if max_interval exceeded (heartbeat).
    if elapsed >= self.max_interval {
      return true;
    }

    // Send if dirty and min_interval has passed.
    if self.dirty && elapsed >= self.min_interval {
      return true;
    }

    false
  }

  /// Build the standby status update message and mark as sent.
  ///
  /// Returns `(write_lsn, flush_lsn, apply_lsn)` for the
  /// `StandbyStatusUpdate` message.
  pub fn build_status_update(&mut self) -> (Lsn, Lsn, Lsn) {
    let status = (
      self.received_lsn,
      self.confirmed_flush_lsn,
      self.confirmed_flush_lsn,
    );

    self.last_ack_time = Instant::now();
    self.dirty = false;

    debug!(
        received = %self.received_lsn,
        confirmed = %self.confirmed_flush_lsn,
        "sending standby status update"
    );

    status
  }

  /// Returns the confirmed flush LSN.
  pub fn confirmed_flush_lsn(&self) -> Lsn {
    self.confirmed_flush_lsn
  }
}
