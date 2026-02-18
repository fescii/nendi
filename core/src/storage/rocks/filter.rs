use shared::event::ChangeEvent;
use shared::lsn::Lsn;

/// Content-based filter for RocksDB reads.
///
/// Used during consumer replay to skip events that don't match the
/// consumer's subscription filter. This avoids deserializing events
/// that will be discarded anyway.
pub struct ReadFilter {
  /// Only return events from these tables.
  tables: Option<Vec<String>>,
  /// Only return events at or after this LSN.
  min_lsn: Lsn,
  /// Only return events at or before this LSN.
  max_lsn: Lsn,
}

impl ReadFilter {
  pub fn new() -> Self {
    Self {
      tables: None,
      min_lsn: Lsn::ZERO,
      max_lsn: Lsn::MAX,
    }
  }

  pub fn with_tables(mut self, tables: Vec<String>) -> Self {
    self.tables = Some(tables);
    self
  }

  pub fn with_lsn_range(mut self, min: Lsn, max: Lsn) -> Self {
    self.min_lsn = min;
    self.max_lsn = max;
    self
  }

  /// Check if an event passes this filter.
  pub fn accept(&self, event: &ChangeEvent) -> bool {
    let lsn = event.lsn();

    if lsn < self.min_lsn || lsn > self.max_lsn {
      return false;
    }

    if let Some(ref tables) = self.tables {
      let qualified = event.table.qualified();
      if !tables.iter().any(|t| t == &qualified) {
        return false;
      }
    }

    true
  }

  /// Check if a key (LSN prefix) could potentially match.
  ///
  /// Used for key-level filtering before deserialization.
  pub fn key_might_match(&self, key: &[u8]) -> bool {
    if key.len() < 8 {
      return false;
    }
    let lsn_bytes: [u8; 8] = key[..8].try_into().unwrap_or([0; 8]);
    let lsn = Lsn::from_be_bytes(lsn_bytes);
    lsn >= self.min_lsn && lsn <= self.max_lsn
  }
}
