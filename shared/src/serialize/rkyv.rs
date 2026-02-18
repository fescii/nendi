// Rkyv (zero-copy) serialization for high-performance internal storage.
//
// Events stored in RocksDB use rkyv for zero-copy reads. This avoids
// deserialization overhead on the read path — the archived bytes can be
// accessed directly as typed structs.

use crate::event::OpType;

/// Rkyv-serializable representation of a change event stored in RocksDB.
///
/// This is the "at rest" format. It mirrors `ChangeEvent` but uses
/// rkyv-compatible types for zero-copy access.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[rkyv(crate = rkyv)]
pub struct StoredEvent {
    pub lsn: u64,
    pub xid: u32,
    pub op: u8,
    pub schema: String,
    pub table: String,
    pub schema_fingerprint: u64,
    pub commit_timestamp_us: i64,
    /// Serialized column data as raw bytes (avoids nested rkyv complexity).
    pub payload: Vec<u8>,
}

impl StoredEvent {
    /// Convert from the canonical `ChangeEvent` for storage.
    pub fn from_event(event: &crate::event::ChangeEvent) -> Self {
        let payload = crate::serialize::proto::encode_event(event);
        Self {
            lsn: event.lsn,
            xid: event.xid,
            op: op_to_u8(event.op),
            schema: event.table.schema.clone(),
            table: event.table.table.clone(),
            schema_fingerprint: event.schema_fingerprint,
            commit_timestamp_us: event.commit_timestamp_us,
            payload,
        }
    }
}

fn op_to_u8(op: OpType) -> u8 {
    match op {
        OpType::Insert => 1,
        OpType::Update => 2,
        OpType::Delete => 3,
        OpType::Truncate => 4,
    }
}
