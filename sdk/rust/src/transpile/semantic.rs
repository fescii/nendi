//! Semantic type-checking pass with constant folding.
//!
//! Traverses the raw `ExprKind` AST and:
//! 1. Resolves `row.field` references (no schema validation by default — done at runtime).
//! 2. Infers types for each node.
//! 3. Constant-folds binary operations on literals (e.g. `2 > 1` → `true`).

use crate::transpile::ast::{BinOp, ExprKind, Literal};
use crate::transpile::error::TranspileError;

/// Type annotation on an expression node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NendiType {
  Int,
  Float,
  Text,
  Bool,
  Null,
  /// Unknown — column value (resolved at evaluation time).
  Field,
}

/// An expression node annotated with its inferred type.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedExpr {
  Field {
    name: String,
    ty: NendiType,
  },
  Lit {
    value: Literal,
    ty: NendiType,
  },
  BinOp {
    op: BinOp,
    left: Box<TypedExpr>,
    right: Box<TypedExpr>,
    ty: NendiType,
  },
  Not(Box<TypedExpr>),
  In {
    field: String,
    values: Vec<Literal>,
  },
  NotIn {
    field: String,
    values: Vec<Literal>,
  },
}

impl TypedExpr {
  pub fn ty(&self) -> NendiType {
    match self {
      TypedExpr::Field { ty, .. } => *ty,
      TypedExpr::Lit { ty, .. } => *ty,
      TypedExpr::BinOp { ty, .. } => *ty,
      TypedExpr::Not(_) => NendiType::Bool,
      TypedExpr::In { .. } | TypedExpr::NotIn { .. } => NendiType::Bool,
    }
  }
}

/// Semantic pass runner.
pub struct SemanticPass;

impl SemanticPass {
  /// Type-check and constant-fold the AST.
  pub fn run(expr: ExprKind) -> Result<TypedExpr, TranspileError> {
    let typed = Self::check(expr)?;
    // Constant-fold the result
    Ok(Self::fold(typed))
  }

  fn check(expr: ExprKind) -> Result<TypedExpr, TranspileError> {
    match expr {
      ExprKind::Field(name) => Ok(TypedExpr::Field {
        name,
        ty: NendiType::Field,
      }),

      ExprKind::Lit(lit) => {
        let ty = lit_type(&lit);
        Ok(TypedExpr::Lit { value: lit, ty })
      }

      ExprKind::BinOp { op, left, right } => {
        let l = Self::check(*left)?;
        let r = Self::check(*right)?;

        // Type the result of the operation
        let result_ty = match op {
          BinOp::And | BinOp::Or => NendiType::Bool,
          BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            // Validate that mixing types is sane
            let lt = l.ty();
            let rt = r.ty();
            if lt != rt
              && lt != NendiType::Field
              && rt != NendiType::Field
              && !(lt == NendiType::Int && rt == NendiType::Float)
              && !(lt == NendiType::Float && rt == NendiType::Int)
            {
              return Err(TranspileError::TypeMismatch {
                left: type_name(lt),
                right: type_name(rt),
                op: op_str(op),
              });
            }
            NendiType::Bool
          }
        };

        Ok(TypedExpr::BinOp {
          op,
          left: Box::new(l),
          right: Box::new(r),
          ty: result_ty,
        })
      }

      ExprKind::Not(inner) => {
        let inner = Self::check(*inner)?;
        Ok(TypedExpr::Not(Box::new(inner)))
      }

      ExprKind::In { field, values } => Ok(TypedExpr::In { field, values }),
      ExprKind::NotIn { field, values } => Ok(TypedExpr::NotIn { field, values }),
    }
  }

  /// Constant folding: reduce compile-time-known expressions to their values.
  fn fold(expr: TypedExpr) -> TypedExpr {
    match expr {
      TypedExpr::BinOp {
        op,
        left,
        right,
        ty,
      } => {
        let l = Self::fold(*left);
        let r = Self::fold(*right);

        // Fold Int op Int → Bool literal
        if let (
          TypedExpr::Lit {
            value: Literal::Int(a),
            ..
          },
          TypedExpr::Lit {
            value: Literal::Int(b),
            ..
          },
        ) = (&l, &r)
        {
          if let Some(result) = fold_int_op(*a, *b, op) {
            return TypedExpr::Lit {
              value: Literal::Bool(result),
              ty: NendiType::Bool,
            };
          }
        }

        // Fold Bool AND/OR short-circuit
        if let TypedExpr::Lit {
          value: Literal::Bool(bval),
          ..
        } = &l
        {
          match op {
            BinOp::And if !bval => {
              return TypedExpr::Lit {
                value: Literal::Bool(false),
                ty: NendiType::Bool,
              }
            }
            BinOp::Or if *bval => {
              return TypedExpr::Lit {
                value: Literal::Bool(true),
                ty: NendiType::Bool,
              }
            }
            _ => {}
          }
        }

        TypedExpr::BinOp {
          op,
          left: Box::new(l),
          right: Box::new(r),
          ty,
        }
      }

      TypedExpr::Not(inner) => {
        let inner = Self::fold(*inner);
        if let TypedExpr::Lit {
          value: Literal::Bool(b),
          ..
        } = &inner
        {
          return TypedExpr::Lit {
            value: Literal::Bool(!b),
            ty: NendiType::Bool,
          };
        }
        TypedExpr::Not(Box::new(inner))
      }

      other => other,
    }
  }
}

fn lit_type(lit: &Literal) -> NendiType {
  match lit {
    Literal::Int(_) => NendiType::Int,
    Literal::Float(_) => NendiType::Float,
    Literal::Str(_) => NendiType::Text,
    Literal::Bool(_) => NendiType::Bool,
    Literal::Null => NendiType::Null,
  }
}

fn fold_int_op(a: i64, b: i64, op: BinOp) -> Option<bool> {
  Some(match op {
    BinOp::Eq => a == b,
    BinOp::Ne => a != b,
    BinOp::Lt => a < b,
    BinOp::Le => a <= b,
    BinOp::Gt => a > b,
    BinOp::Ge => a >= b,
    _ => return None,
  })
}

fn type_name(ty: NendiType) -> &'static str {
  match ty {
    NendiType::Int => "Int",
    NendiType::Float => "Float",
    NendiType::Text => "Text",
    NendiType::Bool => "Bool",
    NendiType::Null => "Null",
    NendiType::Field => "Field",
  }
}

fn op_str(op: BinOp) -> &'static str {
  match op {
    BinOp::Eq => "==",
    BinOp::Ne => "!=",
    BinOp::Lt => "<",
    BinOp::Le => "<=",
    BinOp::Gt => ">",
    BinOp::Ge => ">=",
    BinOp::And => "&&",
    BinOp::Or => "||",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn constant_fold_int_gt() {
    let expr = ExprKind::BinOp {
      op: BinOp::Gt,
      left: Box::new(ExprKind::Lit(Literal::Int(2))),
      right: Box::new(ExprKind::Lit(Literal::Int(1))),
    };
    let typed = SemanticPass::run(expr).unwrap();
    assert_eq!(
      typed,
      TypedExpr::Lit {
        value: Literal::Bool(true),
        ty: NendiType::Bool
      }
    );
  }

  #[test]
  fn constant_fold_false_and_short_circuits() {
    // false && row.x > 0 → false
    let expr = ExprKind::BinOp {
      op: BinOp::And,
      left: Box::new(ExprKind::Lit(Literal::Bool(false))),
      right: Box::new(ExprKind::BinOp {
        op: BinOp::Gt,
        left: Box::new(ExprKind::Field("x".into())),
        right: Box::new(ExprKind::Lit(Literal::Int(0))),
      }),
    };
    let typed = SemanticPass::run(expr).unwrap();
    assert_eq!(
      typed,
      TypedExpr::Lit {
        value: Literal::Bool(false),
        ty: NendiType::Bool
      }
    );
  }
}
