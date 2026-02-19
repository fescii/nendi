//! CEL predicate → PostgreSQL WHERE clause emitter.
//!
//! Translates the `TypedExpr` AST into a SQL WHERE fragment that the Nendi
//! daemon can embed in its `pg_publication_tables` filter. This provides
//! Layer 1 push-down: rows that fail the predicate are rejected at the
//! replication origin and never cross the network.
//!
//! ## Translation rules
//!
//! | CEL                        | SQL                          |
//! |----------------------------|------------------------------|
//! | `row.amount > 10000`       | `amount > 10000`             |
//! | `row.status == "active"`   | `status = 'active'`          |
//! | `row.a > 1 && row.b < 5`  | `a > 1 AND b < 5`           |
//! | `row.x IN ["a", "b"]`     | `x IN ('a', 'b')`            |
//! | `!row.deleted`             | `NOT deleted`                |

use crate::transpile::ast::{BinOp, Literal};
use crate::transpile::semantic::TypedExpr;

/// Emits a PostgreSQL WHERE clause from a `TypedExpr`.
pub struct PgEmitter;

impl PgEmitter {
  pub fn emit(expr: &TypedExpr) -> String {
    let mut buf = String::with_capacity(128);
    emit_expr(expr, &mut buf);
    buf
  }
}

fn emit_expr(expr: &TypedExpr, buf: &mut String) {
  match expr {
    TypedExpr::Lit {
      value: Literal::Bool(true),
      ..
    } => buf.push_str("TRUE"),
    TypedExpr::Lit {
      value: Literal::Bool(false),
      ..
    } => buf.push_str("FALSE"),
    TypedExpr::Lit {
      value: Literal::Null,
      ..
    } => buf.push_str("NULL"),
    TypedExpr::Lit {
      value: Literal::Int(n),
      ..
    } => {
      buf.push_str(&n.to_string());
    }
    TypedExpr::Lit {
      value: Literal::Float(f),
      ..
    } => {
      buf.push_str(&f.to_string());
    }
    TypedExpr::Lit {
      value: Literal::Str(s),
      ..
    } => {
      // SQL string literal with single-quote escaping
      buf.push('\'');
      buf.push_str(&s.replace('\'', "''"));
      buf.push('\'');
    }

    TypedExpr::Field { name, .. } => {
      // Quote identifier to handle reserved words / mixed case
      buf.push('"');
      buf.push_str(name);
      buf.push('"');
    }

    TypedExpr::BinOp {
      op, left, right, ..
    } => {
      let needs_paren_left = matches!(
        **left,
        TypedExpr::BinOp { op: BinOp::Or, .. } | TypedExpr::BinOp { op: BinOp::And, .. }
      ) && !matches!(op, BinOp::Or);

      if needs_paren_left {
        buf.push('(');
        emit_expr(left, buf);
        buf.push(')');
      } else {
        emit_expr(left, buf);
      }

      buf.push(' ');
      buf.push_str(binop_to_sql(*op));
      buf.push(' ');

      let needs_paren_right = matches!(
        **right,
        TypedExpr::BinOp { op: BinOp::Or, .. } | TypedExpr::BinOp { op: BinOp::And, .. }
      ) && !matches!(op, BinOp::Or);

      if needs_paren_right {
        buf.push('(');
        emit_expr(right, buf);
        buf.push(')');
      } else {
        emit_expr(right, buf);
      }
    }

    TypedExpr::Not(inner) => {
      buf.push_str("NOT (");
      emit_expr(inner, buf);
      buf.push(')');
    }

    TypedExpr::In { field, values } => {
      buf.push('"');
      buf.push_str(field);
      buf.push_str("\" IN (");
      for (i, v) in values.iter().enumerate() {
        if i > 0 {
          buf.push_str(", ");
        }
        emit_literal(v, buf);
      }
      buf.push(')');
    }

    TypedExpr::NotIn { field, values } => {
      buf.push('"');
      buf.push_str(field);
      buf.push_str("\" NOT IN (");
      for (i, v) in values.iter().enumerate() {
        if i > 0 {
          buf.push_str(", ");
        }
        emit_literal(v, buf);
      }
      buf.push(')');
    }
  }
}

fn emit_literal(lit: &Literal, buf: &mut String) {
  match lit {
    Literal::Int(n) => buf.push_str(&n.to_string()),
    Literal::Float(f) => buf.push_str(&f.to_string()),
    Literal::Str(s) => {
      buf.push('\'');
      buf.push_str(&s.replace('\'', "''"));
      buf.push('\'');
    }
    Literal::Bool(true) => buf.push_str("TRUE"),
    Literal::Bool(false) => buf.push_str("FALSE"),
    Literal::Null => buf.push_str("NULL"),
  }
}

fn binop_to_sql(op: BinOp) -> &'static str {
  match op {
    BinOp::Eq => "=",
    BinOp::Ne => "<>",
    BinOp::Lt => "<",
    BinOp::Le => "<=",
    BinOp::Gt => ">",
    BinOp::Ge => ">=",
    BinOp::And => "AND",
    BinOp::Or => "OR",
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::transpile::{ast::Parser, lexer, semantic::SemanticPass};
  use typed_arena::Arena;

  fn parse_and_emit(expr: &str) -> String {
    let arena = Arena::new();
    let toks = lexer::tokenize(expr).unwrap();
    let raw = Parser::new(&arena, &toks).parse().unwrap();
    let typed = SemanticPass::run(raw).unwrap();
    PgEmitter::emit(&typed)
  }

  #[test]
  fn cel_to_pg_gt_int() {
    assert_eq!(parse_and_emit("row.amount > 10000"), "\"amount\" > 10000");
  }

  #[test]
  fn cel_to_pg_eq_string() {
    assert_eq!(
      parse_and_emit(r#"row.status == "active""#),
      "\"status\" = 'active'"
    );
  }

  #[test]
  fn cel_to_pg_and() {
    let sql = parse_and_emit("row.a > 1 && row.b < 5");
    assert!(sql.contains("AND"), "expected AND in: {sql}");
  }

  #[test]
  fn cel_to_pg_in_list() {
    let sql = parse_and_emit(r#"row.status IN ["active", "pending"]"#);
    assert!(sql.contains("IN ('active', 'pending')"), "got: {sql}");
  }

  #[test]
  fn cel_to_pg_not() {
    let sql = parse_and_emit("!row.deleted");
    assert!(sql.starts_with("NOT"), "got: {sql}");
  }

  #[test]
  fn string_escaping() {
    // Single quotes inside string literals must be escaped in SQL
    let sql = parse_and_emit(r#"row.name == "o'brian""#);
    assert!(sql.contains("o''brian"), "got: {sql}");
  }
}
