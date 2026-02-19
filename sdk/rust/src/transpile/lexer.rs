//! Zero-copy byte-slice lexer for Nendi predicate expressions.
//!
//! Produces a flat `Vec<Token<'_>>` in a single O(n) pass over the input.
//! No heap allocation per token — tokens borrow directly from the input slice.
//!
//! ## Supported syntax
//!
//! - Field access: `row.column_name`
//! - Integer / float literals: `42`, `3.14`
//! - String literals: `"hello"`, `'world'`
//! - Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
//! - Logical: `&&`, `||`, `!`
//! - Membership: `IN`, `NOT IN`
//! - Grouping: `(`, `)`, `[`, `]`, `,`

use crate::transpile::error::TranspileError;

/// A single lexical token borrowing from the source expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Token<'a> {
  Ident(&'a str),
  Integer(i64),
  Float(f64),
  Str(&'a str),
  Dot,
  // Comparison operators
  Eq,
  Ne,
  Lt,
  Le,
  Gt,
  Ge,
  // Logical operators
  And,
  Or,
  Not,
  // Keywords
  In,
  // Grouping / list
  LParen,
  RParen,
  LBracket,
  RBracket,
  Comma,
}

/// Tokenise the full input expression into a `Vec<Token<'_>>`.
///
/// Runs in O(n) with a single forward pass. No regex, no intermediate
/// heap allocations for tokens — identifiers and string values borrow
/// directly from `input`.
pub fn tokenize(input: &str) -> Result<Vec<Token<'_>>, TranspileError> {
  // Dispatch table: classify each ASCII byte into a broad category.
  // Using a lookup is branchless compared to if/else chains.
  let src = input.as_bytes();
  let mut pos = 0usize;
  let mut tokens = Vec::with_capacity(32);

  while pos < src.len() {
    let b = src[pos];

    // Skip whitespace
    if b.is_ascii_whitespace() {
      pos += 1;
      continue;
    }

    match b {
      b'(' => {
        tokens.push(Token::LParen);
        pos += 1;
      }
      b')' => {
        tokens.push(Token::RParen);
        pos += 1;
      }
      b'[' => {
        tokens.push(Token::LBracket);
        pos += 1;
      }
      b']' => {
        tokens.push(Token::RBracket);
        pos += 1;
      }
      b',' => {
        tokens.push(Token::Comma);
        pos += 1;
      }
      b'.' => {
        tokens.push(Token::Dot);
        pos += 1;
      }

      // Two-char operators
      b'=' if src.get(pos + 1) == Some(&b'=') => {
        tokens.push(Token::Eq);
        pos += 2;
      }
      b'!' if src.get(pos + 1) == Some(&b'=') => {
        tokens.push(Token::Ne);
        pos += 2;
      }
      b'<' if src.get(pos + 1) == Some(&b'=') => {
        tokens.push(Token::Le);
        pos += 2;
      }
      b'>' if src.get(pos + 1) == Some(&b'=') => {
        tokens.push(Token::Ge);
        pos += 2;
      }
      b'&' if src.get(pos + 1) == Some(&b'&') => {
        tokens.push(Token::And);
        pos += 2;
      }
      b'|' if src.get(pos + 1) == Some(&b'|') => {
        tokens.push(Token::Or);
        pos += 2;
      }

      // Single-char operators
      b'<' => {
        tokens.push(Token::Lt);
        pos += 1;
      }
      b'>' => {
        tokens.push(Token::Gt);
        pos += 1;
      }
      b'!' => {
        tokens.push(Token::Not);
        pos += 1;
      }

      // String literals — " or '
      b'"' | b'\'' => {
        let quote = b;
        pos += 1;
        let start = pos;
        while pos < src.len() && src[pos] != quote {
          pos += 1;
        }
        if pos >= src.len() {
          return Err(TranspileError::Lex {
            pos,
            msg: "unterminated string",
          });
        }
        // SAFETY: we're operating on valid UTF-8 from &str
        let s = &input[start..pos];
        tokens.push(Token::Str(s));
        pos += 1; // consume closing quote
      }

      // Integer / float literals
      b'0'..=b'9' | b'-'
        if (b == b'-' && src.get(pos + 1).map_or(false, |c| c.is_ascii_digit())) || b != b'-' =>
      {
        let start = pos;
        if b == b'-' {
          pos += 1;
        }
        while pos < src.len() && src[pos].is_ascii_digit() {
          pos += 1;
        }
        let is_float = pos < src.len() && src[pos] == b'.';
        if is_float {
          pos += 1;
          while pos < src.len() && src[pos].is_ascii_digit() {
            pos += 1;
          }
          let s = &input[start..pos];
          let v: f64 = s.parse().map_err(|_| TranspileError::Lex {
            pos,
            msg: "bad float",
          })?;
          tokens.push(Token::Float(v));
        } else {
          let s = &input[start..pos];
          let v: i64 = s.parse().map_err(|_| TranspileError::Lex {
            pos,
            msg: "bad integer",
          })?;
          tokens.push(Token::Integer(v));
        }
      }

      // Identifiers & keywords
      b if b.is_ascii_alphabetic() || b == b'_' => {
        let start = pos;
        while pos < src.len() && (src[pos].is_ascii_alphanumeric() || src[pos] == b'_') {
          pos += 1;
        }
        let word = &input[start..pos];
        match word {
          "IN" | "in" => tokens.push(Token::In),
          "NOT" | "not" => {
            // Peek: "NOT IN" → emit just NotIn as In, caller handles
            tokens.push(Token::Not)
          }
          _ => tokens.push(Token::Ident(word)),
        }
      }

      _other => {
        return Err(TranspileError::Lex {
          pos,
          msg: "unexpected character",
        });
      }
    }
  }

  Ok(tokens)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lex_simple_comparison() {
    let toks = tokenize("row.amount > 10000").unwrap();
    assert_eq!(
      toks,
      vec![
        Token::Ident("row"),
        Token::Dot,
        Token::Ident("amount"),
        Token::Gt,
        Token::Integer(10000),
      ]
    );
  }

  #[test]
  fn lex_string_eq() {
    let toks = tokenize(r#"row.status == "active""#).unwrap();
    assert_eq!(
      toks,
      vec![
        Token::Ident("row"),
        Token::Dot,
        Token::Ident("status"),
        Token::Eq,
        Token::Str("active"),
      ]
    );
  }

  #[test]
  fn lex_and_expr() {
    let toks = tokenize("row.a > 1 && row.b < 5").unwrap();
    assert!(toks.contains(&Token::And));
  }

  #[test]
  fn lex_unterminated_string() {
    assert!(tokenize(r#"row.x == "oops"#).is_err());
  }

  #[test]
  fn lex_in_list() {
    let toks = tokenize(r#"row.status IN ["a", "b"]"#).unwrap();
    assert!(toks.contains(&Token::In));
    assert!(toks.contains(&Token::LBracket));
  }
}
