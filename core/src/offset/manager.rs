use super::registry::OffsetRegistry;
use shared::lsn::Lsn;
use tracing::{debug, info};

/// High-level offset manager coordinating commit, resume, and GC.
pub struct OffsetManager {
  registry: OffsetRegistry,
}

impl OffsetManager {
  pub fn new() -> Self {
    Self {
      registry: OffsetRegistry::new(),
    }
  }

  /// Commit an offset for a consumer.
  pub fn commit(&self, consumer_id: &str, lsn: Lsn) -> Result<(), shared::error::NendiError> {
    self.registry.commit(consumer_id, lsn)?;
    debug!(consumer = consumer_id, lsn = %lsn, "offset committed");
    Ok(())
  }

  /// Get the resume LSN for a consumer (where to start streaming from).
  pub fn resume_lsn(&self, consumer_id: &str) -> Lsn {
    self
      .registry
      .get(consumer_id)
      .map(|lsn| lsn.saturating_add(1))
      .unwrap_or(Lsn::ZERO)
  }

  /// Get the GC watermark (min offset across all consumers).
  pub fn watermark(&self) -> Lsn {
    self.registry.min_offset()
  }

  /// Register a new consumer (starts from ZERO if no prior offset).
  pub fn register_consumer(&self, consumer_id: &str) {
    if self.registry.get(consumer_id).is_none() {
      info!(consumer = consumer_id, "new consumer registered");
    }
  }

  /// Unregister a consumer.
  pub fn unregister_consumer(&self, consumer_id: &str) {
    self.registry.remove(consumer_id);
    info!(consumer = consumer_id, "consumer unregistered");
  }

  pub fn registry(&self) -> &OffsetRegistry {
    &self.registry
  }
}
