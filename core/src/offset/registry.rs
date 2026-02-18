use dashmap::DashMap;
use shared::lsn::Lsn;

/// Tracks per-consumer committed offsets (LSNs).
///
/// When a consumer sends a `CommitOffset` RPC, the registry updates
/// the consumer's position. This feeds into the GC watermark computation
/// and enables resumption from the last committed position.
pub struct OffsetRegistry {
  /// Consumer ID → committed LSN.
  offsets: DashMap<String, Lsn>,
}

impl OffsetRegistry {
  pub fn new() -> Self {
    Self {
      offsets: DashMap::new(),
    }
  }

  /// Get the committed offset for a consumer.
  pub fn get(&self, consumer_id: &str) -> Option<Lsn> {
    self.offsets.get(consumer_id).map(|v| *v)
  }

  /// Commit an offset for a consumer.
  ///
  /// Returns `Err` if the new LSN is behind the current committed LSN
  /// (regression protection).
  pub fn commit(&self, consumer_id: &str, lsn: Lsn) -> Result<(), shared::error::NendiError> {
    let current = self.get(consumer_id);

    if let Some(current) = current {
      if current.is_ahead_of(lsn) {
        return Err(shared::error::NendiError::OffsetRegression {
          consumer: consumer_id.to_string(),
          attempted: lsn.get(),
          current: current.get(),
        });
      }
    }

    self.offsets.insert(consumer_id.to_string(), lsn);
    Ok(())
  }

  /// Remove a consumer's offset (e.g. on unsubscribe).
  pub fn remove(&self, consumer_id: &str) {
    self.offsets.remove(consumer_id);
  }

  /// Get the minimum committed LSN across all consumers (watermark).
  pub fn min_offset(&self) -> Lsn {
    let mut min = Lsn::MAX;
    for entry in self.offsets.iter() {
      if *entry.value() < min {
        min = *entry.value();
      }
    }
    if min == Lsn::MAX {
      Lsn::ZERO
    } else {
      min
    }
  }

  /// List all consumer IDs.
  pub fn consumer_ids(&self) -> Vec<String> {
    self.offsets.iter().map(|e| e.key().clone()).collect()
  }

  /// Number of tracked consumers.
  pub fn len(&self) -> usize {
    self.offsets.len()
  }
}
