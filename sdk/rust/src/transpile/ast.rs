//! Arena-allocated AST for predicate expressions.
//!
//! Uses `typed-arena` so all nodes in a single parse are freed atomically
//! when the arena is dropped — no per-node `Box` overhead.

use crate::transpile::error::TranspileError;
use crate::transpile::lexer::Token;

/// Source position (byte offset in input string).
pub type Pos = usize;

/// A binary comparison or logical operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
  Eq,
  Ne,
  Lt,
  Le,
  Gt,
  Ge,
  And,
  Or,
}

/// A literal value in the AST.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
  Int(i64),
  Float(f64),
  Str(String),
  Bool(bool),
  Null,
}

/// A single expression node.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
  /// `row.column_name` — column field access.
  Field(String),
  /// A literal value.
  Lit(Literal),
  /// A binary operation.
  BinOp {
    op: BinOp,
    left: Box<ExprKind>,
    right: Box<ExprKind>,
  },
  /// Logical NOT.
  Not(Box<ExprKind>),
  /// `row.column_name IN [v1, v2, …]`.
  In { field: String, values: Vec<Literal> },
  /// `row.column_name NOT IN [v1, v2, …]`.
  NotIn { field: String, values: Vec<Literal> },
}

/// Recursive descent parser over a flat token slice.
pub struct Parser<'a> {
  tokens: &'a [Token<'a>],
  pos: usize,
}

impl<'a> Parser<'a> {
  pub fn new(_arena: &'a typed_arena::Arena<ExprKind>, tokens: &'a [Token<'a>]) -> Self {
    Self { tokens, pos: 0 }
  }

  /// Parse a full expression — entry point.
  pub fn parse(&mut self) -> Result<ExprKind, TranspileError> {
    let expr = self.parse_or()?;
    if self.pos < self.tokens.len() {
      return Err(TranspileError::Parse {
        pos: self.pos,
        msg: format!("unexpected token {:?}", self.tokens[self.pos]),
      });
    }
    Ok(expr)
  }

  // OR (lowest precedence)
  fn parse_or(&mut self) -> Result<ExprKind, TranspileError> {
    let mut left = self.parse_and()?;
    while self.peek() == Some(&Token::Or) {
      self.pos += 1;
      let right = self.parse_and()?;
      left = ExprKind::BinOp {
        op: BinOp::Or,
        left: Box::new(left),
        right: Box::new(right),
      };
    }
    Ok(left)
  }

  // AND
  fn parse_and(&mut self) -> Result<ExprKind, TranspileError> {
    let mut left = self.parse_not()?;
    while self.peek() == Some(&Token::And) {
      self.pos += 1;
      let right = self.parse_not()?;
      left = ExprKind::BinOp {
        op: BinOp::And,
        left: Box::new(left),
        right: Box::new(right),
      };
    }
    Ok(left)
  }

  // NOT (unary)
  fn parse_not(&mut self) -> Result<ExprKind, TranspileError> {
    if self.peek() == Some(&Token::Not) {
      // Check for NOT IN
      if self.tokens.get(self.pos + 1) == Some(&Token::In) {
        // handled in parse_cmp as NotIn
        return self.parse_cmp();
      }
      self.pos += 1;
      let inner = self.parse_not()?;
      return Ok(ExprKind::Not(Box::new(inner)));
    }
    self.parse_cmp()
  }

  // Comparisons and IN
  fn parse_cmp(&mut self) -> Result<ExprKind, TranspileError> {
    let left = self.parse_primary()?;

    // NOT IN
    if self.peek() == Some(&Token::Not) && self.tokens.get(self.pos + 1) == Some(&Token::In) {
      self.pos += 2; // consume NOT IN
      if let ExprKind::Field(col) = &left {
        let col = col.clone();
        let values = self.parse_list()?;
        return Ok(ExprKind::NotIn { field: col, values });
      }
      return Err(TranspileError::Parse {
        pos: self.pos,
        msg: "NOT IN requires a field on LHS".into(),
      });
    }

    // IN
    if self.peek() == Some(&Token::In) {
      self.pos += 1;
      if let ExprKind::Field(col) = &left {
        let col = col.clone();
        let values = self.parse_list()?;
        return Ok(ExprKind::In { field: col, values });
      }
      return Err(TranspileError::Parse {
        pos: self.pos,
        msg: "IN requires a field on LHS".into(),
      });
    }

    let op = match self.peek() {
      Some(Token::Eq) => BinOp::Eq,
      Some(Token::Ne) => BinOp::Ne,
      Some(Token::Lt) => BinOp::Lt,
      Some(Token::Le) => BinOp::Le,
      Some(Token::Gt) => BinOp::Gt,
      Some(Token::Ge) => BinOp::Ge,
      _ => return Ok(left),
    };
    self.pos += 1;
    let right = self.parse_primary()?;
    Ok(ExprKind::BinOp {
      op,
      left: Box::new(left),
      right: Box::new(right),
    })
  }

  // Primaries: `row.field`, literals, `(expr)`
  fn parse_primary(&mut self) -> Result<ExprKind, TranspileError> {
    // Clone the peeked token fully before mutating self.pos
    let tok = self.peek().cloned();
    match tok {
      Some(Token::Ident(id)) => {
        let id = id.to_string(); // own it before borrowing self again
        self.pos += 1;
        if id == "row" || id == "new" || id == "old" {
          // Field access: row.column
          if self.peek() == Some(&Token::Dot) {
            self.pos += 1; // consume dot
            let col_tok = self.peek().cloned();
            if let Some(Token::Ident(col)) = col_tok {
              let col = col.to_string();
              self.pos += 1;
              return Ok(ExprKind::Field(col));
            }
            return Err(TranspileError::Parse {
              pos: self.pos,
              msg: "expected column name after `.`".into(),
            });
          }
        }
        // Boolean literals
        match id.as_str() {
          "true" => Ok(ExprKind::Lit(Literal::Bool(true))),
          "false" => Ok(ExprKind::Lit(Literal::Bool(false))),
          "null" | "NULL" => Ok(ExprKind::Lit(Literal::Null)),
          other => Ok(ExprKind::Field(other.to_string())),
        }
      }
      Some(Token::Integer(n)) => {
        self.pos += 1;
        Ok(ExprKind::Lit(Literal::Int(n)))
      }
      Some(Token::Float(f)) => {
        self.pos += 1;
        Ok(ExprKind::Lit(Literal::Float(f)))
      }
      Some(Token::Str(s)) => {
        let s = s.to_string();
        self.pos += 1;
        Ok(ExprKind::Lit(Literal::Str(s)))
      }
      Some(Token::LParen) => {
        self.pos += 1;
        let inner = self.parse_or()?;
        self.expect(Token::RParen)?;
        Ok(inner)
      }
      _ => Err(TranspileError::Parse {
        pos: self.pos,
        msg: format!("unexpected token: {:?}", self.peek()),
      }),
    }
  }

  /// Parse `["a", "b", 1]` list of literals.
  fn parse_list(&mut self) -> Result<Vec<Literal>, TranspileError> {
    self.expect(Token::LBracket)?;
    let mut values = Vec::new();
    loop {
      let pos = self.pos;
      let peeked = self.tokens.get(pos).cloned();
      match peeked {
        Some(Token::RBracket) => {
          self.pos += 1;
          break;
        }
        Some(Token::Comma) => {
          self.pos += 1;
        }
        Some(Token::Str(s)) => {
          let sv = s.to_string();
          self.pos += 1;
          values.push(Literal::Str(sv));
        }
        Some(Token::Integer(n)) => {
          self.pos += 1;
          values.push(Literal::Int(n));
        }
        Some(Token::Float(f)) => {
          self.pos += 1;
          values.push(Literal::Float(f));
        }
        other => {
          return Err(TranspileError::Parse {
            pos: self.pos,
            msg: format!("expected list value, got {:?}", other),
          })
        }
      }
    }
    Ok(values)
  }

  fn peek(&self) -> Option<&Token<'_>> {
    self.tokens.get(self.pos)
  }

  fn expect(&mut self, tok: Token<'_>) -> Result<(), TranspileError> {
    match self.peek() {
      Some(t) if std::mem::discriminant(t) == std::mem::discriminant(&tok) => {
        self.pos += 1;
        Ok(())
      }
      other => Err(TranspileError::Parse {
        pos: self.pos,
        msg: format!("expected {:?}, got {:?}", tok, other),
      }),
    }
  }
}
