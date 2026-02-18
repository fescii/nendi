//! DDL (schema change) event type.

use chrono::{DateTime, Utc};

use crate::offset::Offset;

/// A schema change event emitted when a table's DDL is modified.
#[derive(Debug, Clone)]
pub struct DdlEvent {
  /// Stream position of this DDL event.
  offset: Offset,

  /// The schema name (e.g. `"public"`).
  schema: String,

  /// The table name that was altered.
  table: String,

  /// The raw DDL statement text.
  ddl: String,

  /// New schema fingerprint after the DDL change.
  fingerprint: [u8; 16],

  /// Timestamp of the DDL commit in PostgreSQL.
  committed: DateTime<Utc>,
}

impl DdlEvent {
  /// Returns the schema name.
  pub fn schema(&self) -> &str {
    &self.schema
  }

  /// Returns the table name.
  pub fn table(&self) -> &str {
    &self.table
  }

  /// Returns the DDL statement text.
  pub fn ddl(&self) -> &str {
    &self.ddl
  }

  /// Returns the offset of this event.
  pub fn offset(&self) -> Offset {
    self.offset.clone()
  }

  /// Returns the new schema fingerprint.
  pub fn fingerprint(&self) -> &[u8; 16] {
    &self.fingerprint
  }

  /// Returns the commit timestamp.
  pub fn committed(&self) -> DateTime<Utc> {
    self.committed
  }
}
