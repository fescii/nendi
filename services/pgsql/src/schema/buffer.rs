use tracing::debug;

/// Buffers DDL changes received via logical decoding messages.
///
/// When a DDL change (ALTER TABLE, DROP COLUMN, etc.) is detected via
/// the event trigger, Nendi injects a logical message into the WAL.
/// This buffer collects those messages so the schema registry can be
/// updated in-order with the DML events.
pub struct DdlBuffer {
  pending: Vec<DdlChange>,
}

/// A buffered DDL change waiting to be applied to the schema registry.
#[derive(Debug, Clone)]
pub struct DdlChange {
  /// The LSN at which the DDL was committed.
  pub lsn: u64,
  /// The affected table.
  pub table: String,
  /// The type of DDL operation.
  pub kind: DdlKind,
  /// Raw DDL statement (for logging/audit).
  pub statement: String,
}

/// Classification of DDL changes.
#[derive(Debug, Clone)]
pub enum DdlKind {
  AlterTable,
  DropTable,
  RenameTable { new_name: String },
  AddColumn { column: String },
  DropColumn { column: String },
  AlterColumn { column: String },
  Other,
}

impl DdlBuffer {
  pub fn new() -> Self {
    Self {
      pending: Vec::new(),
    }
  }

  /// Push a DDL change into the buffer.
  pub fn push(&mut self, change: DdlChange) {
    debug!(
        lsn = change.lsn,
        table = %change.table,
        kind = ?change.kind,
        "buffered DDL change"
    );
    self.pending.push(change);
  }

  /// Drain all DDL changes at or before the given LSN.
  pub fn drain_up_to(&mut self, lsn: u64) -> Vec<DdlChange> {
    let (ready, remaining): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|c| c.lsn <= lsn);
    self.pending = remaining;
    ready
  }

  /// Returns the number of pending DDL changes.
  pub fn len(&self) -> usize {
    self.pending.len()
  }

  /// Returns true if there are no pending changes.
  pub fn is_empty(&self) -> bool {
    self.pending.is_empty()
  }
}
