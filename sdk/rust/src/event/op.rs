//! Op enum — database operation kind.

/// The type of database operation that produced a change event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
  /// A row was inserted.
  Insert,
  /// A row was updated.
  Update,
  /// A row was deleted.
  Delete,
  /// A table was truncated.
  Truncate,
}

impl Op {
  /// Convert from a raw protobuf OpKind integer.
  ///
  /// This avoids a dependency on the proto crate (which is gated behind
  /// `feature = "grpc"`) so that `Op` is usable from the wasm SDK.
  pub(crate) fn from_proto(raw: i32) -> Self {
    // Matches the OpKind enum values defined in nendi.proto:
    //   0 = OP_UNSPECIFIED, 1 = INSERT, 2 = UPDATE, 3 = DELETE, 4 = TRUNCATE
    match raw {
      1 => Op::Insert,
      2 => Op::Update,
      3 => Op::Delete,
      4 => Op::Truncate,
      _ => Op::Insert, // safe default for unknown values
    }
  }
}

impl std::fmt::Display for Op {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Op::Insert => write!(f, "INSERT"),
      Op::Update => write!(f, "UPDATE"),
      Op::Delete => write!(f, "DELETE"),
      Op::Truncate => write!(f, "TRUNCATE"),
    }
  }
}
