//! Operation kind enum.

/// The type of database operation that produced a change event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
  /// A new row was inserted.
  Insert,
  /// An existing row was updated.
  Update,
  /// A row was deleted.
  Delete,
  /// The table was truncated.
  Truncate,
  /// A DDL schema change occurred.
  Ddl,
}

impl std::fmt::Display for Op {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Op::Insert => write!(f, "INSERT"),
      Op::Update => write!(f, "UPDATE"),
      Op::Delete => write!(f, "DELETE"),
      Op::Truncate => write!(f, "TRUNCATE"),
      Op::Ddl => write!(f, "DDL"),
    }
  }
}
