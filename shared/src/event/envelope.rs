use serde::{Deserialize, Serialize};

use super::types::{OpType, RowData, TableId};
use crate::lsn::Lsn;

/// A fully-decoded change event from the PostgreSQL WAL.
///
/// This is the canonical event type that flows through the entire pipeline:
/// PG decoder → ingestion filter → RocksDB storage → consumer egress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// The LSN at which this change was committed.
    pub lsn: u64,

    /// The transaction ID that produced this change.
    pub xid: u32,

    /// The type of operation (insert, update, delete, truncate).
    pub op: OpType,

    /// The fully-qualified table that was modified.
    pub table: TableId,

    /// Schema fingerprint at the time of this event. Used to detect
    /// schema drift between producer and consumer.
    pub schema_fingerprint: u64,

    /// The new row state (populated for insert and update).
    pub after: Option<RowData>,

    /// The old row state (populated for update with REPLICA IDENTITY FULL,
    /// and for delete). Contains only key columns by default.
    pub before: Option<RowData>,

    /// Server-side timestamp (microseconds since Unix epoch) when the
    /// transaction was committed.
    pub commit_timestamp_us: i64,
}

impl ChangeEvent {
    /// Returns the LSN as a typed value.
    #[inline]
    pub fn lsn(&self) -> Lsn {
        Lsn::new(self.lsn)
    }

    /// Returns `true` if this is a data event (INSERT/UPDATE/DELETE).
    #[inline]
    pub fn is_data_event(&self) -> bool {
        matches!(self.op, OpType::Insert | OpType::Update | OpType::Delete)
    }

    /// Returns `true` if this is a truncate event.
    #[inline]
    pub fn is_truncate(&self) -> bool {
        matches!(self.op, OpType::Truncate)
    }

    /// Estimated memory footprint in bytes, used for memory budgeting.
    pub fn estimated_size(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();
        size += self.table.schema.len() + self.table.table.len();
        if let Some(ref row) = self.after {
            for col in &row.columns {
                size += col.name.len();
                size += col.value.as_ref().map_or(0, |v| v.len());
            }
        }
        if let Some(ref row) = self.before {
            for col in &row.columns {
                size += col.name.len();
                size += col.value.as_ref().map_or(0, |v| v.len());
            }
        }
        size
    }
}
