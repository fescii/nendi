//! `WasmTranspiler` — JS wrapper around `nendi_sdk::Transpiler`.
//!
//! Compiles a CEL-like predicate expression into:
//! - A PostgreSQL `WHERE` clause (for server-side push-down).
//! - A JS-callable `.test(payload_json)` that evaluates the filter client-side.

use nendi_sdk::transpile::{FilterFn, RowCol};
use nendi_sdk::Transpiler;
use wasm_bindgen::prelude::*;

/// A compiled predicate expression, ready to emit SQL or test events.
///
/// ```js
/// const t = WasmTranspiler.parse("row.amount > 100 && row.status == 'paid'");
/// console.log(t.to_sql());
/// // → amount > 100 AND status = 'paid'
/// t.test('{"amount": 200, "status": "paid"}'); // → true
/// ```
#[wasm_bindgen]
pub struct WasmTranspiler {
  inner: Transpiler,
  filter: FilterFn,
}

#[wasm_bindgen]
impl WasmTranspiler {
  /// Parse and type-check a predicate expression.
  ///
  /// Throws a JS `Error` on syntax / type errors.
  #[wasm_bindgen]
  pub fn parse(expr: &str) -> Result<WasmTranspiler, JsError> {
    let inner = Transpiler::parse(expr).map_err(|e| JsError::new(&e.to_string()))?;
    // compile_filter() consumes inner — clone the typed expr via a re-parse
    // (parse is cheap, < 1µs for typical predicates)
    let inner2 = Transpiler::parse(expr).map_err(|e| JsError::new(&e.to_string()))?;
    let filter = inner2.compile_filter();
    Ok(Self { inner, filter })
  }

  /// Emit a PostgreSQL `WHERE` clause for server-side push-down.
  ///
  /// ```js
  /// t.to_sql(); // → "amount > 100 AND status = 'paid'"
  /// ```
  pub fn to_sql(&self) -> String {
    self.inner.to_sql()
  }

  /// Test a JSON payload string against the compiled filter.
  ///
  /// `payload_json` should be a JSON string `{ "col": value, … }`.
  /// Returns `true` if the row passes the predicate, `false` otherwise.
  ///
  /// ```js
  /// t.test('{"amount": 200, "status": "paid"}'); // → true
  /// ```
  pub fn test(&self, payload_json: &str) -> bool {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(payload_json) else {
      return false;
    };

    let Some(map) = val.as_object() else {
      return false;
    };

    let cols: Vec<RowCol> = map
      .iter()
      .map(|(k, v)| RowCol {
        name: k.clone(),
        value: match v {
          serde_json::Value::Null => None,
          serde_json::Value::String(s) => Some(s.clone()),
          other => Some(other.to_string()),
        },
      })
      .collect();

    (self.filter)(&cols)
  }
}
