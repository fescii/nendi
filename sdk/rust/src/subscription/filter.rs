//! Filter types for subscription filtering.

use std::collections::HashMap;

use crate::event::Op;

/// A set of filters applied to a subscription.
///
/// Filters are evaluated server-side by the Nendi daemon.
/// Events that don't match are never sent over the network.
#[derive(Debug, Clone, Default)]
pub struct Filter {
  /// Only receive events for these tables.
  pub tables: Vec<String>,

  /// Only receive events for these operations.
  pub operations: Vec<Op>,

  /// Column projections per table.
  pub columns: HashMap<String, Vec<String>>,

  /// CEL row predicate expression.
  pub predicate: Option<String>,
}
