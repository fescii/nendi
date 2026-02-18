use crate::schema::buffer::{DdlBuffer, DdlChange};
use tracing::info;

/// Handles DDL events received via PostgreSQL event triggers.
///
/// When a DDL statement is executed on a monitored table, the event
/// trigger fires, calls the capture function, which emits a logical
/// decoding message. The replication stream picks up this message and
/// routes it here.
pub struct DdlHandler {
  buffer: DdlBuffer,
  /// Tables we care about (publication tables).
  monitored_tables: Vec<String>,
}

impl DdlHandler {
  pub fn new(monitored_tables: Vec<String>) -> Self {
    Self {
      buffer: DdlBuffer::new(),
      monitored_tables,
    }
  }

  /// Process a DDL notification from the logical decoding message.
  ///
  /// The message content is expected to be a JSON payload from the
  /// `nendi_capture_ddl` function.
  pub fn handle_ddl_message(&mut self, lsn: u64, content: &str) -> anyhow::Result<()> {
    let parsed = super::parser::parse_ddl_message(content)?;

    // Only buffer DDL for monitored tables.
    if !self.is_monitored(&parsed.table) {
      return Ok(());
    }

    info!(
        lsn = lsn,
        table = %parsed.table,
        kind = ?parsed.kind,
        "DDL change detected"
    );

    self.buffer.push(DdlChange {
      lsn,
      table: parsed.table,
      kind: parsed.kind,
      statement: parsed.statement,
    });

    Ok(())
  }

  /// Drain buffered DDL changes up to the given LSN.
  pub fn drain_up_to(&mut self, lsn: u64) -> Vec<DdlChange> {
    self.buffer.drain_up_to(lsn)
  }

  fn is_monitored(&self, table: &str) -> bool {
    self.monitored_tables.iter().any(|t| t == table)
  }

  /// Update the list of monitored tables (e.g. after publication reconcile).
  pub fn set_monitored_tables(&mut self, tables: Vec<String>) {
    self.monitored_tables = tables;
  }
}
