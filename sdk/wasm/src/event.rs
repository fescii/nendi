//! `WasmChangeEvent` — the JS-visible change event type.

use serde_json::Value as JsonValue;
use wasm_bindgen::prelude::*;

/// A single CDC change event delivered to JavaScript consumers.
///
/// All fields are eagerly decoded; the JSON payload is decoded
/// only when `.payload_json()` is called.
#[wasm_bindgen]
pub struct WasmChangeEvent {
  pub(crate) lsn: u64,
  pub(crate) op: String,
  pub(crate) xid: u32,
  pub(crate) schema: String,
  pub(crate) table: String,
  /// µs since the Unix epoch (from PostgreSQL commit timestamp).
  pub(crate) committed: i64,
  pub(crate) payload_json: JsonValue,
}

#[wasm_bindgen]
impl WasmChangeEvent {
  /// The LSN (log-sequence number) of this event.
  ///
  /// Returned as an `f64` (JS `number`) which safely represents all u64
  /// values up to 2^53 (≅ 9 PiB of WAL, well beyond any practical limit).
  pub fn lsn(&self) -> f64 {
    self.lsn as f64
  }

  /// The operation kind: `"insert"`, `"update"`, `"delete"`, or `"truncate"`.
  pub fn op(&self) -> String {
    self.op.clone()
  }

  /// The PostgreSQL transaction ID.
  pub fn xid(&self) -> u32 {
    self.xid
  }

  /// The source schema name (e.g. `"public"`).
  pub fn schema(&self) -> String {
    self.schema.clone()
  }

  /// The source table name (e.g. `"orders"`).
  pub fn table(&self) -> String {
    self.table.clone()
  }

  /// Wall-clock commit time as milliseconds since the Unix epoch.
  pub fn committed_ms(&self) -> f64 {
    // Convert µs → ms for JS `Date` compatibility.
    (self.committed / 1_000) as f64
  }

  /// The row payload as a JS object (`{ col: value, … }`).
  ///
  /// For INSERT/UPDATE this is the **after** state; for DELETE it is the
  /// **before** state.  
  /// Returns `null` for TRUNCATE events.
  pub fn payload_json(&self) -> JsValue {
    serde_wasm_bindgen::to_value(&self.payload_json).unwrap_or(JsValue::NULL)
  }

  /// Convenience: fully qualified table name `"schema.table"`.
  pub fn qualified_table(&self) -> String {
    format!("{}.{}", self.schema, self.table)
  }
}
