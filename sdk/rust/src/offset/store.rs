//! OffsetStore trait for external offset persistence.

use async_trait::async_trait;

use crate::error::NendiError;
use crate::offset::Offset;

/// Trait for persisting stream offsets to an external store.
///
/// Implement this trait to store offsets in your own database,
/// Redis, or any other durable storage. This allows your application
/// to resume from where it left off after a restart, independently
/// of the Nendi daemon's internal offset tracking.
#[async_trait]
pub trait OffsetStore: Send + Sync {
    /// Load the last committed offset for the given stream.
    ///
    /// Returns `None` if no offset has been committed yet (first run).
    async fn load(&self, stream_id: &str) -> Result<Option<Offset>, NendiError>;

    /// Save (commit) an offset for the given stream.
    ///
    /// This should be an upsert — create if not exists, update if exists.
    async fn save(&self, stream_id: &str, offset: &Offset) -> Result<(), NendiError>;
}
