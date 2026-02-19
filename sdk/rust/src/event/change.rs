//! `change.rs` — core change event type.

use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};

use crate::error::NendiError;
use crate::event::Op;
use crate::offset::Offset;

/// A single change event received from a Nendi stream.
#[derive(Debug, Clone)]
pub struct ChangeEvent {
  /// Unique position of this event in the stream (LSN).
  offset: Offset,
  /// The type of database operation.
  op: Op,
  /// Source schema name (e.g. `"public"`).
  schema: String,
  /// Source table name (e.g. `"orders"`).
  table: String,
  /// Raw row payload — compact TLV bytes.
  raw: Bytes,
  /// Previous row state — only populated for UPDATE events.
  raw_old: Option<Bytes>,
  /// Schema fingerprint at time of this event (u64 → 8-byte array, big-endian).
  schema_fingerprint: [u8; 8],
  /// Wall-clock time the event was committed in PostgreSQL (microseconds since epoch).
  committed: DateTime<Utc>,
  /// PostgreSQL transaction ID.
  xid: u32,
}

impl ChangeEvent {
  /// Convert a protobuf `ChangeEvent` into the SDK's typed event.
  ///
  /// This is only available with the `grpc` feature.
  #[cfg(feature = "grpc")]
  pub(crate) fn from_proto(p: crate::proto::ChangeEvent) -> Result<Self, NendiError> {
    let op = Op::from_proto(p.op);
    let table_id = p.table.unwrap_or_default();

    let raw = encode_row_data(p.after.as_ref().or(p.before.as_ref()));
    let raw_old = if op == Op::Update {
      p.before.as_ref().map(encode_row_data_owned)
    } else {
      None
    };

    let committed = Utc
      .timestamp_micros(p.committed as i64)
      .single()
      .unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap());

    let mut fingerprint = [0u8; 8];
    fingerprint.copy_from_slice(&p.fingerprint.to_be_bytes());

    Ok(Self {
      offset: Offset::from_lsn(p.lsn),
      op,
      schema: table_id.schema,
      table: table_id.table,
      raw,
      raw_old,
      schema_fingerprint: fingerprint,
      committed,
      xid: p.xid,
    })
  }

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

  /// Returns the schema fingerprint bytes.
  pub fn schema_fingerprint(&self) -> &[u8; 8] {
    &self.schema_fingerprint
  }

  /// Returns the raw payload bytes.
  pub fn raw_bytes(&self) -> &[u8] {
    &self.raw
  }

  /// Deserialize the event payload into a typed struct using serde.
  pub fn payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, NendiError> {
    let json_val = decode_to_json(&self.raw).map_err(|e| NendiError::Deserialize {
      table: format!("{}.{}", self.schema, self.table),
      source: Box::new(e),
    })?;
    serde_json::from_value(json_val).map_err(|e| NendiError::Deserialize {
      table: format!("{}.{}", self.schema, self.table),
      source: Box::new(e),
    })
  }

  /// Deserialize the previous row state (UPDATE events only).
  pub fn old_payload<T: serde::de::DeserializeOwned>(&self) -> Result<Option<T>, NendiError> {
    match &self.raw_old {
      Some(raw) => {
        let json_val = decode_to_json(raw).map_err(|e| NendiError::Deserialize {
          table: format!("{}.{}", self.schema, self.table),
          source: Box::new(e),
        })?;
        let val = serde_json::from_value(json_val).map_err(|e| NendiError::Deserialize {
          table: format!("{}.{}", self.schema, self.table),
          source: Box::new(e),
        })?;
        Ok(Some(val))
      }
      None => Ok(None),
    }
  }
}

// ─── Internal encoding helpers ────────────────────────────────────────────────

/// Encode proto `RowData` into a compact TLV byte format.
///
/// Format (column order preserved):
///   `<name_len:u16><name_bytes><is_null:u8><value_len:u32><value_bytes> ...`
#[cfg(feature = "grpc")]
fn encode_row_data(row: Option<&crate::proto::RowData>) -> Bytes {
  let Some(row) = row else {
    return Bytes::new();
  };
  encode_row_data_inner(&row.columns)
}

#[cfg(feature = "grpc")]
fn encode_row_data_owned(row: &crate::proto::RowData) -> Bytes {
  encode_row_data_inner(&row.columns)
}

#[cfg(feature = "grpc")]
fn encode_row_data_inner(cols: &[crate::proto::ColumnValue]) -> Bytes {
  let mut buf = Vec::with_capacity(cols.len() * 32);
  for col in cols {
    let name = col.name.as_bytes();
    let name_len = (name.len() as u16).to_le_bytes();
    buf.extend_from_slice(&name_len);
    buf.extend_from_slice(name);
    match &col.value {
      Some(v) => {
        buf.push(0u8);
        let vb = v.as_bytes();
        let vlen = (vb.len() as u32).to_le_bytes();
        buf.extend_from_slice(&vlen);
        buf.extend_from_slice(vb);
      }
      None => {
        buf.push(1u8);
        buf.extend_from_slice(&0u32.to_le_bytes());
      }
    }
  }
  Bytes::from(buf)
}

/// Decode compact TLV bytes back into a `serde_json::Value` (Map).
///
/// Called on demand from `payload<T>()` and from the wasm decode path.
/// Exposed as `pub` so `sdk/wasm` can reuse this without duplication.
pub fn decode_to_json(data: &[u8]) -> Result<serde_json::Value, std::io::Error> {
  use std::io::{Error, ErrorKind};

  let mut map = serde_json::Map::new();
  let mut pos = 0usize;

  while pos < data.len() {
    if pos + 2 > data.len() {
      return Err(Error::new(ErrorKind::UnexpectedEof, "short name_len"));
    }
    let name_len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;
    if pos + name_len > data.len() {
      return Err(Error::new(ErrorKind::UnexpectedEof, "short name"));
    }
    let name = std::str::from_utf8(&data[pos..pos + name_len])
      .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
      .to_string();
    pos += name_len;

    if pos + 1 > data.len() {
      return Err(Error::new(ErrorKind::UnexpectedEof, "short null_flag"));
    }
    let is_null = data[pos] != 0;
    pos += 1;

    if pos + 4 > data.len() {
      return Err(Error::new(ErrorKind::UnexpectedEof, "short value_len"));
    }
    let value_len =
      u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;

    let json_val = if is_null {
      serde_json::Value::Null
    } else {
      if pos + value_len > data.len() {
        return Err(Error::new(ErrorKind::UnexpectedEof, "short value"));
      }
      let s = std::str::from_utf8(&data[pos..pos + value_len])
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
      if let Ok(n) = s.parse::<i64>() {
        serde_json::Value::Number(n.into())
      } else if let Ok(n) = s.parse::<f64>() {
        serde_json::Value::Number(
          serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)),
        )
      } else if s == "t" || s == "true" {
        serde_json::Value::Bool(true)
      } else if s == "f" || s == "false" {
        serde_json::Value::Bool(false)
      } else {
        serde_json::Value::String(s.to_owned())
      }
    };
    pos += value_len;
    map.insert(name, json_val);
  }

  Ok(serde_json::Value::Object(map))
}
