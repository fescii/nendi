use shared::event::{ChangeEvent, OpType};
use std::collections::HashMap;
use tracing::debug;

/// Layer 2 ingestion filter — evaluates events BEFORE they enter storage.
///
/// This is the second filtering layer (after PG publication filtering).
/// It evaluates simple predicate expressions per table to decide whether
/// an event should be stored in RocksDB at all.
///
/// Events that fail the filter are discarded, saving storage I/O and
/// disk space. This is particularly useful for high-volume tables where
/// only a subset of changes are interesting.
pub struct IngestionFilter {
  /// Filter expressions keyed by fully-qualified table name.
  /// An empty map means "accept all".
  filters: HashMap<String, FilterRule>,
}

/// A simple filter rule for a table.
#[derive(Debug, Clone)]
pub struct FilterRule {
  /// Operations to accept (empty = accept all).
  pub ops: Vec<OpType>,
  /// Simple column-value predicates (all must match).
  pub predicates: Vec<ColumnPredicate>,
}

/// A simple column-value predicate.
#[derive(Debug, Clone)]
pub struct ColumnPredicate {
  pub column: String,
  pub op: PredicateOp,
  pub value: String,
}

/// Predicate comparison operator.
#[derive(Debug, Clone)]
pub enum PredicateOp {
  Eq,
  Neq,
  Gt,
  Lt,
  Gte,
  Lte,
  Contains,
  StartsWith,
  IsNull,
  IsNotNull,
}

impl IngestionFilter {
  pub fn new() -> Self {
    Self {
      filters: HashMap::new(),
    }
  }

  /// Build from the pipeline config filter expressions.
  pub fn from_config(cfg: &shared::config::PipelineConfig) -> Self {
    let mut filter = Self::new();
    for (table, expr) in &cfg.filters {
      if let Some(rule) = parse_filter_expr(expr) {
        filter.filters.insert(table.clone(), rule);
      }
    }
    filter
  }

  /// Add a filter rule for a table.
  pub fn add_rule(&mut self, table: &str, rule: FilterRule) {
    self.filters.insert(table.to_string(), rule);
  }

  /// Evaluate whether an event should be ingested.
  ///
  /// Returns `true` if the event passes the filter (should be stored).
  pub fn accept(&self, event: &ChangeEvent) -> bool {
    let table_name = event.table.qualified();

    let rule = match self.filters.get(&table_name) {
      Some(r) => r,
      None => return true, // No filter = accept all
    };

    // Check operation filter
    if !rule.ops.is_empty() && !rule.ops.contains(&event.op) {
      debug!(
          table = %table_name,
          op = %event.op,
          "ingestion filter: rejected by op filter"
      );
      return false;
    }

    // Check column predicates against the after row
    if let Some(ref row) = event.after {
      for pred in &rule.predicates {
        let val = row.get_text(&pred.column);
        if !evaluate_predicate(val, &pred.op, &pred.value) {
          debug!(
              table = %table_name,
              column = %pred.column,
              "ingestion filter: rejected by predicate"
          );
          return false;
        }
      }
    }

    true
  }
}

fn evaluate_predicate(actual: Option<&str>, op: &PredicateOp, expected: &str) -> bool {
  match op {
    PredicateOp::IsNull => actual.is_none(),
    PredicateOp::IsNotNull => actual.is_some(),
    _ => {
      let actual = match actual {
        Some(v) => v,
        None => return false,
      };
      match op {
        PredicateOp::Eq => actual == expected,
        PredicateOp::Neq => actual != expected,
        PredicateOp::Gt => actual > expected,
        PredicateOp::Lt => actual < expected,
        PredicateOp::Gte => actual >= expected,
        PredicateOp::Lte => actual <= expected,
        PredicateOp::Contains => actual.contains(expected),
        PredicateOp::StartsWith => actual.starts_with(expected),
        _ => true,
      }
    }
  }
}

/// Parse a simple filter expression string.
///
/// Format: `op:insert,update; column_name == value; other_col != val2`
fn parse_filter_expr(expr: &str) -> Option<FilterRule> {
  let mut ops = Vec::new();
  let mut predicates = Vec::new();

  for part in expr.split(';') {
    let part = part.trim();
    if part.is_empty() {
      continue;
    }

    if part.starts_with("op:") {
      let op_str = &part[3..];
      for op in op_str.split(',') {
        if let Some(op_type) = OpType::from_str_tag(op.trim()) {
          ops.push(op_type);
        }
      }
    } else if let Some(pred) = parse_predicate(part) {
      predicates.push(pred);
    }
  }

  Some(FilterRule { ops, predicates })
}

fn parse_predicate(expr: &str) -> Option<ColumnPredicate> {
  let operators = [
    ("!=", PredicateOp::Neq),
    (">=", PredicateOp::Gte),
    ("<=", PredicateOp::Lte),
    ("==", PredicateOp::Eq),
    (">", PredicateOp::Gt),
    ("<", PredicateOp::Lt),
  ];

  for (sym, op) in &operators {
    if let Some(pos) = expr.find(sym) {
      let column = expr[..pos].trim().to_string();
      let value = expr[pos + sym.len()..].trim().to_string();
      return Some(ColumnPredicate {
        column,
        op: op.clone(),
        value,
      });
    }
  }

  None
}
