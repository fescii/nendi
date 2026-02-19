//! Binary frame decoder for Nendi WebSocket messages.
//!
//! The Nendi gateway sends events in the same compact TLV format as the
//! internal proto encoding, letting us reuse `nendi_sdk::event::decode_to_json`
//! from the Rust SDK without duplicating the 60-line parser.
//!
//! ## Wire format (little-endian)
//! ```text
//! ┌──────────┬───────┬──────────────┬───────────────┬──────────────┬──────────────┬───────────────┐
//! │ lsn:u64  │ op:u8 │ xid:u32      │ schema_len:u16│ schema bytes │ table_len:u16│ table bytes   │
//! ├──────────┴───────┴──────────────┴───────────────┴──────────────┴──────────────┴───────────────┤
//! │ committed:i64 (µs since epoch)                                                                │
//! ├───────────────────────────────────────────────────────────────────────────────────────────────┤
//! │ row data (TLV, same format as nendi-sdk ChangeEvent::raw):                                    │
//! │  [name_len:u16][name bytes][is_null:u8][value_len:u32][value bytes] …                        │
//! └───────────────────────────────────────────────────────────────────────────────────────────────┘
//! ```

use crate::event::WasmChangeEvent;

/// Errors that can occur while decoding a frame.
#[derive(Debug)]
pub enum DecodeError {
  TooShort { need: usize, have: usize },
  Utf8 { field: &'static str },
  UnknownOp(u8),
  Json(String),
}

impl std::fmt::Display for DecodeError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      DecodeError::TooShort { need, have } => {
        write!(f, "frame too short: need {need} bytes, have {have}")
      }
      DecodeError::Utf8 { field } => write!(f, "invalid UTF-8 in field `{field}`"),
      DecodeError::UnknownOp(b) => write!(f, "unknown op byte: {b:#04x}"),
      DecodeError::Json(e) => write!(f, "json decode error: {e}"),
    }
  }
}

/// Decode a raw WebSocket binary frame into a `WasmChangeEvent`.
pub fn decode_frame(buf: &[u8]) -> Result<WasmChangeEvent, DecodeError> {
  let mut pos = 0usize;

  macro_rules! need {
    ($n:expr, $field:literal) => {{
      let n = $n;
      if pos + n > buf.len() {
        return Err(DecodeError::TooShort {
          need: pos + n,
          have: buf.len(),
        });
      }
      let slice = &buf[pos..pos + n];
      pos += n;
      slice
    }};
  }

  // ── Fixed-width header ────────────────────────────────────────────────────
  let lsn = u64::from_le_bytes(need!(8, "lsn").try_into().unwrap());
  let op_byte = need!(1, "op")[0];
  let xid = u32::from_le_bytes(need!(4, "xid").try_into().unwrap());
  let committed = i64::from_le_bytes(need!(8, "committed").try_into().unwrap());

  // ── Schema / table strings ────────────────────────────────────────────────
  let schema_len = u16::from_le_bytes(need!(2, "schema_len").try_into().unwrap()) as usize;
  let schema = std::str::from_utf8(need!(schema_len, "schema"))
    .map_err(|_| DecodeError::Utf8 { field: "schema" })?
    .to_owned();

  let table_len = u16::from_le_bytes(need!(2, "table_len").try_into().unwrap()) as usize;
  let table = std::str::from_utf8(need!(table_len, "table"))
    .map_err(|_| DecodeError::Utf8 { field: "table" })?
    .to_owned();

  // ── Op byte ───────────────────────────────────────────────────────────────
  let op = match op_byte {
    1 => "insert",
    2 => "update",
    3 => "delete",
    4 => "truncate",
    b => return Err(DecodeError::UnknownOp(b)),
  };

  // ── Row data (rest of the frame) — reuse nendi_sdk's TLV decoder ─────────
  let row_bytes = &buf[pos..];
  let payload_json = if row_bytes.is_empty() {
    serde_json::Value::Object(serde_json::Map::new())
  } else {
    nendi_sdk::event::decode_to_json(row_bytes).map_err(|e| DecodeError::Json(e.to_string()))?
  };

  Ok(WasmChangeEvent {
    lsn,
    op: op.to_owned(),
    xid,
    schema,
    table,
    committed,
    payload_json,
  })
}
