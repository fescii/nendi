//! Semantic transpiler for Nendi predicate expressions.
//!
//! Converts a CEL-like predicate string (e.g. `row.amount > 10000`) into:
//! - A compiled Rust closure (`FilterFn`) for fast client-side evaluation.
//! - A PostgreSQL `WHERE` clause fragment for server-side push-down.
//!
//! # Pipeline
//!
//! ```text
//! &str
//!  └─► Lexer (zero-alloc byte scanner)
//!       └─► Token<'_> stream
//!            └─► Parser (recursive descent, arena-allocated AST)
//!                 └─► Ast
//!                      └─► SemanticPass (type-check + constant fold)
//!                           └─► TypedAst
//!                                ├─► Codegen   → FilterFn (compiled closure)
//!                                └─► PgEmitter → SQL WHERE string
//! ```

pub mod ast;
pub mod cel_to_pg;
pub mod codegen;
pub mod error;
pub mod lexer;
// proto_to_row depends on crate::proto::RowData — only available with `grpc`.
#[cfg(feature = "grpc")]
pub mod proto_to_row;
pub mod semantic;

pub use codegen::{FilterFn, RowCol};
pub use error::TranspileError;

use cel_to_pg::PgEmitter;
use codegen::Codegen;
use semantic::SemanticPass;

/// A compiled predicate ready to be evaluated or emitted as SQL.
pub struct Transpiler {
  typed_expr: semantic::TypedExpr,
}

impl Transpiler {
  /// Parse and type-check a predicate expression.
  ///
  /// # Errors
  /// Returns `TranspileError` if the expression has a syntax or type error.
  pub fn parse(expr: &str) -> Result<Self, TranspileError> {
    let arena = typed_arena::Arena::new();
    let tokens = lexer::tokenize(expr)?;
    let raw_expr = ast::Parser::new(&arena, &tokens).parse()?;
    let typed_expr = SemanticPass::run(raw_expr)?;
    Ok(Self { typed_expr })
  }

  /// Compile into a `FilterFn` closure for fast client-side evaluation.
  ///
  /// The returned closure takes `&[RowCol]` — compatible with both the
  /// gRPC path (convert proto RowData → RowCol slice) and the wasm path
  /// (build RowCol from JSON).
  pub fn compile_filter(self) -> FilterFn {
    Codegen::compile(self.typed_expr)
  }

  /// Alias kept for backwards compatibility with the gRPC SDK.
  #[inline]
  pub fn to_filter_fn(self) -> FilterFn {
    self.compile_filter()
  }

  /// Emit a PostgreSQL `WHERE` clause fragment for server-side push-down.
  ///
  /// Example: `row.amount > 10000` → `amount > 10000`
  pub fn to_pg_where(&self) -> String {
    PgEmitter::emit(&self.typed_expr)
  }

  /// Emit SQL — alias matching the wasm API name.
  #[inline]
  pub fn to_sql(&self) -> String {
    self.to_pg_where()
  }
}
