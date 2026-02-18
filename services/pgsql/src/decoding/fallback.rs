use bytes::Bytes;
use shared::event::ChangeEvent;
use tracing::warn;

/// Fallback decoder for non-pgoutput output plugins.
///
/// This is a safety net. If the configured output plugin is not pgoutput,
/// this decoder logs a warning and returns None. In the future, additional
/// plugins (e.g. wal2json) could be supported here.
pub struct FallbackDecoder {
  plugin_name: String,
}

impl FallbackDecoder {
  pub fn new(plugin_name: &str) -> Self {
    Self {
      plugin_name: plugin_name.to_string(),
    }
  }

  /// Attempt to decode — always returns None for unsupported plugins.
  pub fn decode(&self, _lsn: u64, _data: &Bytes) -> anyhow::Result<Option<ChangeEvent>> {
    warn!(
        plugin = %self.plugin_name,
        "unsupported output plugin; only 'pgoutput' is supported"
    );
    Ok(None)
  }
}
