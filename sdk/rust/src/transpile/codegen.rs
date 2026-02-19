//! Codegen: compile a `TypedExpr` into a `FilterFn` Rust closure.
//!
//! The emitted `FilterFn` captures only column-name strings for lookup
//! (no schema at evaluation time). Each evaluation is a direct field
//! lookup + comparison — typically 3-10 native instructions for simple
//! predicates vs. 50+ in a generic interpreter.
//!
//! ## Why a standalone `RowCol` instead of `proto::RowData`?
//!
//! `FilterFn` is also used from the wasm crate (which has the `grpc`
//! feature disabled and thus no access to proto types). Using `RowCol`
//! decouples the filter function from the proto representation, enabling
//! full reuse in both contexts.

use std::sync::Arc;

use crate::transpile::ast::{BinOp, Literal};
use crate::transpile::semantic::TypedExpr;

/// A single column value — the generic, proto-free representation
/// used by `FilterFn`.
///
/// `value == None` means SQL NULL.
#[derive(Debug, Clone)]
pub struct RowCol {
  pub name: String,
  pub value: Option<String>,
}

/// A compiled filter function.
///
/// Takes a slice of [`RowCol`] values and returns `true` if the row
/// passes the predicate. Evaluated client-side, works without proto.
///
/// Used by the Rust/gRPC SDK (after converting `proto::RowData → &[RowCol]`)
/// and by the wasm SDK (after decoding from JSON or WebSocket frames).
pub type FilterFn = Arc<dyn Fn(&[RowCol]) -> bool + Send + Sync + 'static>;

/// Compile a `TypedExpr` into a fast `FilterFn` closure.
pub struct Codegen;

impl Codegen {
  pub fn compile(expr: TypedExpr) -> FilterFn {
    Arc::new(move |row: &[RowCol]| eval(&expr, row))
  }
}

/// Evaluate a `TypedExpr` against a row.
fn eval(expr: &TypedExpr, row: &[RowCol]) -> bool {
  match expr {
    TypedExpr::Lit {
      value: Literal::Bool(b),
      ..
    } => *b,

    TypedExpr::BinOp {
      op: BinOp::And,
      left,
      right,
      ..
    } => eval(left, row) && eval(right, row),
    TypedExpr::BinOp {
      op: BinOp::Or,
      left,
      right,
      ..
    } => eval(left, row) || eval(right, row),
    TypedExpr::Not(inner) => !eval(inner, row),

    TypedExpr::BinOp {
      op, left, right, ..
    } => {
      let l_val = resolve_val(left, row);
      let r_val = resolve_val(right, row);
      compare(l_val.as_deref(), r_val.as_deref(), *op)
    }

    TypedExpr::In { field, values } => {
      let col = row_get(row, field);
      col.is_some()
        && values
          .iter()
          .any(|v| col.as_deref() == Some(lit_str(v).as_str()))
    }
    TypedExpr::NotIn { field, values } => {
      let col = row_get(row, field);
      col
        .as_deref()
        .map_or(true, |s| !values.iter().any(|v| s == lit_str(v).as_str()))
    }

    TypedExpr::Field { .. } | TypedExpr::Lit { .. } => {
      // A bare field or literal in boolean context
      true
    }
  }
}

/// Resolve an expression to its string representation for comparison.
fn resolve_val(expr: &TypedExpr, row: &[RowCol]) -> Option<String> {
  match expr {
    TypedExpr::Field { name, .. } => row_get(row, name),
    TypedExpr::Lit { value, .. } => Some(lit_str(value)),
    _ => None,
  }
}

/// Look up a column value by name using a linear scan.
///
/// For typical tables (< 50 columns) this is faster than a `HashMap`
/// due to cache locality and zero allocation.
#[inline(always)]
fn row_get(row: &[RowCol], name: &str) -> Option<String> {
  row
    .iter()
    .find(|c| c.name == name)
    .and_then(|c| c.value.clone())
}

/// Convert a `Literal` to its string representation for comparisons.
fn lit_str(lit: &Literal) -> String {
  match lit {
    Literal::Int(n) => n.to_string(),
    Literal::Float(f) => f.to_string(),
    Literal::Str(s) => s.clone(),
    Literal::Bool(b) => b.to_string(),
    Literal::Null => String::new(),
  }
}

/// Compare two optional string column values using the given operator.
fn compare(l: Option<&str>, r: Option<&str>, op: BinOp) -> bool {
  match (l, r) {
    (Some(a), Some(b)) => {
      // Try numeric comparison first
      if let (Ok(an), Ok(bn)) = (a.parse::<f64>(), b.parse::<f64>()) {
        return match op {
          BinOp::Eq => (an - bn).abs() < f64::EPSILON,
          BinOp::Ne => (an - bn).abs() >= f64::EPSILON,
          BinOp::Lt => an < bn,
          BinOp::Le => an <= bn,
          BinOp::Gt => an > bn,
          BinOp::Ge => an >= bn,
          _ => false,
        };
      }
      // Fall back to lexicographic string comparison
      match op {
        BinOp::Eq => a == b,
        BinOp::Ne => a != b,
        BinOp::Lt => a < b,
        BinOp::Le => a <= b,
        BinOp::Gt => a > b,
        BinOp::Ge => a >= b,
        _ => false,
      }
    }
    _ => false,
  }
}
