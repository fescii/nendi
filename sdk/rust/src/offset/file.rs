//! File-based offset store.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::error::NendiError;
use crate::offset::{Offset, OffsetStore};

/// Stores offsets as files on the local filesystem.
///
/// Each stream gets its own file in the configured directory.
/// Simple and dependency-free — suitable for development and
/// single-instance deployments.
pub struct FileOffsetStore {
    dir: PathBuf,
}

impl FileOffsetStore {
    /// Create a new file offset store that writes to the given directory.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path_for(&self, stream_id: &str) -> PathBuf {
        self.dir.join(format!("{stream_id}.offset"))
    }
}

#[async_trait]
impl OffsetStore for FileOffsetStore {
    async fn load(&self, stream_id: &str) -> Result<Option<Offset>, NendiError> {
        let path = self.path_for(stream_id);
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(Offset::from_bytes(bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(NendiError::OffsetStore(Box::new(e))),
        }
    }

    async fn save(&self, stream_id: &str, offset: &Offset) -> Result<(), NendiError> {
        let path = self.path_for(stream_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| NendiError::OffsetStore(Box::new(e)))?;
        }
        tokio::fs::write(&path, offset.as_bytes())
            .await
            .map_err(|e| NendiError::OffsetStore(Box::new(e)))?;
        Ok(())
    }
}
