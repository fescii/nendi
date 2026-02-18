use bytes::Bytes;
use shared::event::{ChangeEvent, ColumnValue, OpType, RowData, TableId};
use tracing::{debug, trace, warn};

/// Decodes the PostgreSQL `pgoutput` logical replication binary protocol.
///
/// The pgoutput plugin emits messages in a binary format described in the
/// PostgreSQL documentation. Each message starts with a single-byte tag
/// identifying the message type.
///
/// # Message Types
///
/// | Tag | Type       | Description                          |
/// |-----|------------|--------------------------------------|
/// | `B` | Begin      | Transaction begin                    |
/// | `C` | Commit     | Transaction commit                   |
/// | `R` | Relation   | Relation (table) metadata            |
/// | `I` | Insert     | New row inserted                     |
/// | `U` | Update     | Row updated                          |
/// | `D` | Delete     | Row deleted                          |
/// | `T` | Truncate   | Table(s) truncated                   |
/// | `Y` | Type       | Custom type definition               |
/// | `O` | Origin     | Replication origin                   |
/// | `M` | Message    | Logical decoding message             |
pub struct PgOutputDecoder {
  /// Currently tracked relations (table OID → table metadata).
  relations: dashmap::DashMap<u32, RelationInfo>,
  /// Current transaction state.
  _current_xid: Option<u32>,
  /// Current transaction commit timestamp.
  _current_commit_ts: i64,
  /// Current transaction final LSN.
  _current_final_lsn: u64,
}

/// Metadata about a tracked relation (table).
#[derive(Debug, Clone)]
struct RelationInfo {
  schema: String,
  table: String,
  columns: Vec<ColumnMeta>,
  _replica_identity: ReplicaIdentity,
}

/// Column metadata from a Relation message.
#[derive(Debug, Clone)]
struct ColumnMeta {
  name: String,
  type_oid: u32,
  // Whether this column is part of the replica identity key.
  _is_key: bool,
}

/// PostgreSQL REPLICA IDENTITY setting.
#[derive(Debug, Clone, Copy)]
enum ReplicaIdentity {
  Default,
  Nothing,
  Full,
  Index,
}

impl PgOutputDecoder {
  pub fn new() -> Self {
    Self {
      relations: dashmap::DashMap::new(),
      _current_xid: None,
      _current_commit_ts: 0,
      _current_final_lsn: 0,
    }
  }

  /// Decode a single pgoutput message.
  ///
  /// Returns `Some(ChangeEvent)` for data messages (I/U/D/T), or `None`
  /// for metadata messages (B/C/R/Y/O).
  pub fn decode(&self, lsn: u64, data: &Bytes) -> anyhow::Result<Option<ChangeEvent>> {
    if data.is_empty() {
      return Ok(None);
    }

    let tag = data[0];
    let payload = &data[1..];

    match tag {
      b'B' => {
        self.decode_begin(payload)?;
        Ok(None)
      }
      b'C' => {
        self.decode_commit(payload)?;
        Ok(None)
      }
      b'R' => {
        self.decode_relation(payload)?;
        Ok(None)
      }
      b'I' => self.decode_insert(lsn, payload),
      b'U' => self.decode_update(lsn, payload),
      b'D' => self.decode_delete(lsn, payload),
      b'T' => self.decode_truncate(lsn, payload),
      b'Y' => {
        trace!("ignoring Type message");
        Ok(None)
      }
      b'O' => {
        trace!("ignoring Origin message");
        Ok(None)
      }
      b'M' => {
        debug!("received logical message");
        Ok(None)
      }
      other => {
        warn!(tag = other, "unknown pgoutput message tag");
        Ok(None)
      }
    }
  }

  fn decode_begin(&self, data: &[u8]) -> anyhow::Result<()> {
    if data.len() < 20 {
      anyhow::bail!("begin message too short");
    }
    let final_lsn = u64::from_be_bytes(data[0..8].try_into()?);
    let _commit_ts = i64::from_be_bytes(data[8..16].try_into()?);
    let xid = u32::from_be_bytes(data[16..20].try_into()?);

    // Store transaction context — in a real multi-threaded scenario
    // these would be thread-local or passed through the call chain.
    // For now we use interior mutability through the decoder's design.
    debug!(xid = xid, final_lsn = final_lsn, "begin transaction");

    Ok(())
  }

  fn decode_commit(&self, data: &[u8]) -> anyhow::Result<()> {
    if data.len() < 25 {
      anyhow::bail!("commit message too short");
    }
    // flags (1) + commit_lsn (8) + end_lsn (8) + commit_ts (8)
    let commit_lsn = u64::from_be_bytes(data[1..9].try_into()?);
    debug!(commit_lsn = commit_lsn, "commit transaction");
    Ok(())
  }

  fn decode_relation(&self, data: &[u8]) -> anyhow::Result<()> {
    let mut pos = 0;

    // Relation OID
    let rel_id = read_u32(data, &mut pos)?;
    // Namespace (schema)
    let schema = read_cstring(data, &mut pos)?;
    // Relation name
    let table = read_cstring(data, &mut pos)?;
    // Replica identity
    let replica = match read_u8(data, &mut pos)? {
      b'd' => ReplicaIdentity::Default,
      b'n' => ReplicaIdentity::Nothing,
      b'f' => ReplicaIdentity::Full,
      b'i' => ReplicaIdentity::Index,
      other => {
        warn!(byte = other, "unknown replica identity byte, defaulting");
        ReplicaIdentity::Default
      }
    };
    // Number of columns
    let n_cols = read_u16(data, &mut pos)?;
    let mut columns = Vec::with_capacity(n_cols as usize);

    for _ in 0..n_cols {
      let flags = read_u8(data, &mut pos)?;
      let name = read_cstring(data, &mut pos)?;
      let type_oid = read_u32(data, &mut pos)?;
      let _type_modifier = read_u32(data, &mut pos)?;
      columns.push(ColumnMeta {
        name,
        type_oid,
        _is_key: (flags & 1) != 0,
      });
    }

    debug!(
        rel_id = rel_id,
        schema = %schema,
        table = %table,
        cols = n_cols,
        "relation metadata"
    );

    self.relations.insert(
      rel_id,
      RelationInfo {
        schema,
        table,
        columns,
        _replica_identity: replica,
      },
    );

    Ok(())
  }

  fn decode_insert(&self, lsn: u64, data: &[u8]) -> anyhow::Result<Option<ChangeEvent>> {
    let mut pos = 0;
    let rel_id = read_u32(data, &mut pos)?;
    let tag = read_u8(data, &mut pos)?; // should be 'N'
    if tag != b'N' {
      anyhow::bail!("insert: expected 'N' tag, got {}", tag as char);
    }

    let rel = self
      .relations
      .get(&rel_id)
      .ok_or_else(|| anyhow::anyhow!("relation {} not found", rel_id))?;

    let after = decode_tuple_data(data, &mut pos, &rel.columns)?;

    Ok(Some(ChangeEvent {
      lsn,
      xid: 0, // filled by transaction context
      op: OpType::Insert,
      table: TableId::new(&rel.schema, &rel.table),
      schema_fingerprint: compute_fingerprint(&rel),
      after: Some(after),
      before: None,
      commit_timestamp_us: 0, // filled by commit
    }))
  }

  fn decode_update(&self, lsn: u64, data: &[u8]) -> anyhow::Result<Option<ChangeEvent>> {
    let mut pos = 0;
    let rel_id = read_u32(data, &mut pos)?;

    let rel = self
      .relations
      .get(&rel_id)
      .ok_or_else(|| anyhow::anyhow!("relation {} not found", rel_id))?;

    // Check for old tuple (K = key, O = old)
    let mut before = None;
    let tag = read_u8(data, &mut pos)?;
    if tag == b'K' || tag == b'O' {
      before = Some(decode_tuple_data(data, &mut pos, &rel.columns)?);
      let _new_tag = read_u8(data, &mut pos)?; // 'N'
    }

    let after = decode_tuple_data(data, &mut pos, &rel.columns)?;

    Ok(Some(ChangeEvent {
      lsn,
      xid: 0,
      op: OpType::Update,
      table: TableId::new(&rel.schema, &rel.table),
      schema_fingerprint: compute_fingerprint(&rel),
      after: Some(after),
      before,
      commit_timestamp_us: 0,
    }))
  }

  fn decode_delete(&self, lsn: u64, data: &[u8]) -> anyhow::Result<Option<ChangeEvent>> {
    let mut pos = 0;
    let rel_id = read_u32(data, &mut pos)?;

    let rel = self
      .relations
      .get(&rel_id)
      .ok_or_else(|| anyhow::anyhow!("relation {} not found", rel_id))?;

    let _tag = read_u8(data, &mut pos)?;
    // K = key columns, O = old full row
    let before = decode_tuple_data(data, &mut pos, &rel.columns)?;

    Ok(Some(ChangeEvent {
      lsn,
      xid: 0,
      op: OpType::Delete,
      table: TableId::new(&rel.schema, &rel.table),
      schema_fingerprint: compute_fingerprint(&rel),
      after: None,
      before: Some(before),
      commit_timestamp_us: 0,
    }))
  }

  fn decode_truncate(&self, lsn: u64, data: &[u8]) -> anyhow::Result<Option<ChangeEvent>> {
    let mut pos = 0;
    let n_rels = read_u32(data, &mut pos)?;
    let _options = read_u8(data, &mut pos)?;

    // Emit one event per truncated table.
    // For simplicity, return the first one — a real implementation
    // would return a Vec or use a callback.
    if n_rels > 0 {
      let rel_id = read_u32(data, &mut pos)?;
      if let Some(rel) = self.relations.get(&rel_id) {
        return Ok(Some(ChangeEvent {
          lsn,
          xid: 0,
          op: OpType::Truncate,
          table: TableId::new(&rel.schema, &rel.table),
          schema_fingerprint: compute_fingerprint(&rel),
          after: None,
          before: None,
          commit_timestamp_us: 0,
        }));
      }
    }

    Ok(None)
  }
}

// ── Wire-format helpers ────────────────────────────────────────────────

fn read_u8(data: &[u8], pos: &mut usize) -> anyhow::Result<u8> {
  if *pos >= data.len() {
    anyhow::bail!("unexpected end of message at offset {}", pos);
  }
  let val = data[*pos];
  *pos += 1;
  Ok(val)
}

fn read_u16(data: &[u8], pos: &mut usize) -> anyhow::Result<u16> {
  if *pos + 2 > data.len() {
    anyhow::bail!("unexpected end of message at offset {}", pos);
  }
  let val = u16::from_be_bytes(data[*pos..*pos + 2].try_into()?);
  *pos += 2;
  Ok(val)
}

fn read_u32(data: &[u8], pos: &mut usize) -> anyhow::Result<u32> {
  if *pos + 4 > data.len() {
    anyhow::bail!("unexpected end of message at offset {}", pos);
  }
  let val = u32::from_be_bytes(data[*pos..*pos + 4].try_into()?);
  *pos += 4;
  Ok(val)
}

fn read_cstring(data: &[u8], pos: &mut usize) -> anyhow::Result<String> {
  let start = *pos;
  while *pos < data.len() && data[*pos] != 0 {
    *pos += 1;
  }
  if *pos >= data.len() {
    anyhow::bail!("unterminated c-string at offset {}", start);
  }
  let s = std::str::from_utf8(&data[start..*pos])?.to_string();
  *pos += 1; // skip null terminator
  Ok(s)
}

/// Decode tuple data from a pgoutput message.
fn decode_tuple_data(
  data: &[u8],
  pos: &mut usize,
  columns: &[ColumnMeta],
) -> anyhow::Result<RowData> {
  let n_cols = read_u16(data, pos)? as usize;
  let mut result = Vec::with_capacity(n_cols);

  for i in 0..n_cols {
    let col_tag = read_u8(data, pos)?;
    let col_meta = columns.get(i);
    let name = col_meta
      .map(|c| c.name.clone())
      .unwrap_or_else(|| format!("col_{}", i));
    let type_oid = col_meta.map(|c| c.type_oid).unwrap_or(0);

    match col_tag {
      b'n' => {
        // NULL
        result.push(ColumnValue {
          name,
          type_oid,
          value: None,
        });
      }
      b'u' => {
        // Unchanged TOAST value — omitted
        result.push(ColumnValue {
          name,
          type_oid,
          value: None, // marked as unchanged
        });
      }
      b't' => {
        // Text format value
        let len = read_u32(data, pos)? as usize;
        if *pos + len > data.len() {
          anyhow::bail!("value extends beyond message boundary");
        }
        let value = std::str::from_utf8(&data[*pos..*pos + len])?.to_string();
        *pos += len;
        result.push(ColumnValue {
          name,
          type_oid,
          value: Some(value),
        });
      }
      b'b' => {
        // Binary format — store as hex for now
        let len = read_u32(data, pos)? as usize;
        if *pos + len > data.len() {
          anyhow::bail!("binary value extends beyond message boundary");
        }
        let hex = hex_encode(&data[*pos..*pos + len]);
        *pos += len;
        result.push(ColumnValue {
          name,
          type_oid,
          value: Some(format!("\\x{}", hex)),
        });
      }
      other => {
        warn!(tag = other, "unknown tuple column tag");
        result.push(ColumnValue {
          name,
          type_oid,
          value: None,
        });
      }
    }
  }

  Ok(RowData { columns: result })
}

/// Simple hex encoding without external dependency.
fn hex_encode(data: &[u8]) -> String {
  data.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Compute a schema fingerprint from relation metadata.
///
/// Uses a simple hash of column names and type OIDs. This allows consumers
/// to detect schema drift by comparing fingerprints.
fn compute_fingerprint(rel: &RelationInfo) -> u64 {
  use std::hash::{Hash, Hasher};
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  rel.schema.hash(&mut hasher);
  rel.table.hash(&mut hasher);
  for col in &rel.columns {
    col.name.hash(&mut hasher);
    col.type_oid.hash(&mut hasher);
  }
  hasher.finish()
}
