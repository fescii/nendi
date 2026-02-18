//! Core change event type.

use bytes::Bytes;
use chrono::{DateTime, Utc};

use crate::event::Op;
use crate::offset::Offset;

/// A single change event received from a Nendi stream.
///
/// Carries the operation kind, table identity, the row payload,
/// and the offset for acknowledgment.
#[derive(Debug, Clone)]
pub struct ChangeEvent {
  /// Unique position of this event in the stream.
  offset: Offset,

  /// The type of database operation.
  op: Op,

  /// Source schema name (e.g. `"public"`).
  schema: String,

  /// Source table name (e.g. `"orders"`).
  table: String,

  /// Raw payload bytes (rkyv-serialized by daemon).
  raw: Bytes,

  /// Previous row state — only populated for UPDATE events.
  raw_old: Option<Bytes>,

  /// Schema fingerprint at time of this event.
  schema_fingerprint: [u8; 16],

  /// Wall-clock time the event was committed in PostgreSQL.
  committed: DateTime<Utc>,

  /// PostgreSQL transaction ID.
  xid: u32,
}

impl ChangeEvent {
  /// Returns the schema name of the source table.
  pub fn schema(&self) -> &str {
    &self.schema
  }

  /// Returns the table name of the source table.
  pub fn table(&self) -> &str {
    &self.table
  }

  /// Returns the operation kind.
  pub fn op(&self) -> Op {
    self.op
  }

  /// Returns the offset of this event in the stream.
  pub fn offset(&self) -> Offset {
    self.offset.clone()
  }

  /// Returns the commit timestamp from PostgreSQL.
  pub fn committed(&self) -> DateTime<Utc> {
    self.committed
  }

  /// Returns the PostgreSQL transaction ID.
  pub fn xid(&self) -> u32 {
    self.xid
  }

  /// Returns the schema fingerprint.
  pub fn schema_fingerprint(&self) -> &[u8; 16] {
    &self.schema_fingerprint
  }

  /// Returns the raw payload bytes.
  pub fn raw_bytes(&self) -> &[u8] {
    &self.raw
  }

  /// Deserialize the event payload into a typed struct.
  pub fn payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, crate::error::NendiError> {
    serde_json::from_slice(&self.raw).map_err(|e| crate::error::NendiError::Deserialize {
      table: format!("{}.{}", self.schema, self.table),
      source: Box::new(e),
    })
  }

  /// Deserialize the previous row state (UPDATE events only).
  pub fn old_payload<T: serde::de::DeserializeOwned>(
    &self,
  ) -> Result<Option<T>, crate::error::NendiError> {
    match &self.raw_old {
      Some(raw) => {
        let val =
          serde_json::from_slice(raw).map_err(|e| crate::error::NendiError::Deserialize {
            table: format!("{}.{}", self.schema, self.table),
            source: Box::new(e),
          })?;
        Ok(Some(val))
      }
      None => Ok(None),
    }
  }
}
