use shared::lsn::Lsn;

/// Computes the GC watermark — the lowest LSN that is still needed.
///
/// The watermark is the MIN of all consumer confirmed offsets. Events
/// below this LSN have been consumed by ALL subscribers and can be
/// safely garbage collected.
pub struct WatermarkTracker {
  /// Consumer ID → last confirmed LSN.
  consumers: dashmap::DashMap<String, Lsn>,
}

impl WatermarkTracker {
  pub fn new() -> Self {
    Self {
      consumers: dashmap::DashMap::new(),
    }
  }

  /// Update a consumer's confirmed LSN.
  pub fn update(&self, consumer_id: &str, lsn: Lsn) {
    self.consumers.insert(consumer_id.to_string(), lsn);
  }

  /// Remove a consumer (e.g. on disconnect).
  pub fn remove(&self, consumer_id: &str) {
    self.consumers.remove(consumer_id);
  }

  /// Compute the current watermark.
  ///
  /// Returns the minimum confirmed LSN across all consumers, or
  /// `Lsn::ZERO` if there are no consumers.
  pub fn watermark(&self) -> Lsn {
    let mut min = Lsn::MAX;
    for entry in self.consumers.iter() {
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

  /// Returns the number of tracked consumers.
  pub fn consumer_count(&self) -> usize {
    self.consumers.len()
  }
}
