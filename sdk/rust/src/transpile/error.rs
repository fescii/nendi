//! Transpiler error types.

use std::fmt;

/// Error produced during transpilation of a predicate expression.
#[derive(Debug, Clone)]
pub enum TranspileError {
  /// The input string had an unrecognised token or illegal character.
  Lex { pos: usize, msg: &'static str },
  /// The token stream did not form a valid expression.
  Parse { pos: usize, msg: String },
  /// The expression referred to an unknown column name.
  UnknownColumn(String),
  /// Type mismatch between operands.
  TypeMismatch {
    left: &'static str,
    right: &'static str,
    op: &'static str,
  },
}

impl fmt::Display for TranspileError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Lex { pos, msg } => write!(f, "lexer error at byte {pos}: {msg}"),
      Self::Parse { pos, msg } => write!(f, "parse error at byte {pos}: {msg}"),
      Self::UnknownColumn(c) => write!(f, "unknown column: `{c}`"),
      Self::TypeMismatch { left, right, op } => {
        write!(f, "type mismatch: {left} {op} {right}")
      }
    }
  }
}

impl std::error::Error for TranspileError {}
