use crate::decoding::PgOutputDecoder;
use bytes::Bytes;
use shared::event::ChangeEvent;
use shared::lsn::Lsn;
use tracing::{debug, info};

/// Reads the logical replication stream from PostgreSQL.
///
/// This module manages the replication connection (separate from the
/// normal query connection) and feeds decoded events to the pipeline.
pub struct ReplicationStream {
  decoder: PgOutputDecoder,
  /// The LSN of the last successfully processed message.
  last_processed_lsn: Lsn,
  /// The LSN to start replication from (0 = let PG decide).
  start_lsn: Lsn,
  /// Whether the stream is currently active.
  active: bool,
}

impl ReplicationStream {
  pub fn new(start_lsn: Lsn) -> Self {
    Self {
      decoder: PgOutputDecoder::new(),
      last_processed_lsn: start_lsn,
      start_lsn,
      active: false,
    }
  }

  /// Start the replication stream.
  ///
  /// In a real implementation this would issue
  /// `START_REPLICATION SLOT slot_name LOGICAL start_lsn`
  /// on a replication-mode connection. Here we define the interface.
  pub fn start(&mut self) {
    self.active = true;
    info!(
        start_lsn = %self.start_lsn,
        "replication stream started"
    );
  }

  /// Stop the replication stream gracefully.
  pub fn stop(&mut self) {
    self.active = false;
    info!("replication stream stopped");
  }

  /// Process a raw WAL message received from the replication connection.
  ///
  /// Returns a decoded `ChangeEvent` for data messages, or `None` for
  /// metadata-only messages (Begin, Commit, Relation).
  pub fn process_message(&mut self, lsn: u64, data: Bytes) -> anyhow::Result<Option<ChangeEvent>> {
    let event = self.decoder.decode(lsn, &data)?;
    self.last_processed_lsn = Lsn::new(lsn);

    if let Some(ref evt) = event {
      debug!(
          lsn = lsn,
          op = %evt.op,
          table = %evt.table,
          "decoded change event"
      );
    }

    Ok(event)
  }

  /// Returns the LSN of the last successfully processed message.
  pub fn last_processed_lsn(&self) -> Lsn {
    self.last_processed_lsn
  }

  /// Returns whether the stream is active.
  pub fn is_active(&self) -> bool {
    self.active
  }
}
