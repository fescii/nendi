//! Fast proto `RowData` → SDK `Row` mapping.
//!
//! This module lives on the hot receive path for typed streams. It converts
//! proto `RowData` into a thin `Row` wrapper that column-indexes by name
//! using a single linear scan — no `HashMap` allocation, no JSON round-trip.
//!
//! For user-facing `event.payload::<T>()`, deserialization falls back to
//! the JSON path in `event/change.rs`.

use crate::proto::RowData;

/// A lightweight view of a row's column values.
///
/// Wraps the proto `RowData` directly — zero copy on construction.
pub struct Row<'a> {
  inner: &'a RowData,
}

impl<'a> Row<'a> {
  /// Create a view over a proto `RowData`.
  pub fn new(row: &'a RowData) -> Self {
    Self { inner: row }
  }

  /// Get a column value by name.
  ///
  /// Performs a linear scan — O(columns). For typical tables (< 50 columns)
  /// this is faster than a `HashMap` due to data locality and zero allocation.
  #[inline]
  pub fn get(&self, name: &str) -> Option<&str> {
    self
      .inner
      .columns
      .iter()
      .find(|c| c.name == name)
      .and_then(|c| c.value.as_deref())
  }

  /// Get a column value parsed as a specific type.
  #[inline]
  pub fn get_parsed<T: std::str::FromStr>(&self, name: &str) -> Option<T> {
    self.get(name)?.parse().ok()
  }

  /// Returns true if the column exists and is non-NULL.
  #[inline]
  pub fn has(&self, name: &str) -> bool {
    self
      .inner
      .columns
      .iter()
      .any(|c| c.name == name && c.value.is_some())
  }

  /// Number of columns in this row.
  #[inline]
  pub fn len(&self) -> usize {
    self.inner.columns.len()
  }

  /// Returns true if the row has no columns.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.inner.columns.is_empty()
  }

  /// Iterate over all (name, value) pairs in column order.
  pub fn iter(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
    self
      .inner
      .columns
      .iter()
      .map(|c| (c.name.as_str(), c.value.as_deref()))
  }
}
