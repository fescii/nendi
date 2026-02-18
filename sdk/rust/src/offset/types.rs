//! Offset newtype — opaque bytes wrapper.

/// An opaque stream offset used for resumption and acknowledgment.
///
/// Offsets are ordered — a later offset is always greater than an earlier one.
/// The internal representation is a byte array whose format is an
/// implementation detail of the Nendi daemon.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset(Vec<u8>);

impl Offset {
  /// Create an offset representing the beginning of the stream.
  pub fn beginning() -> Self {
    Self(Vec::new())
  }

  /// Create an offset from raw bytes (received from the daemon).
  pub fn from_bytes(bytes: Vec<u8>) -> Self {
    Self(bytes)
  }

  /// Returns the raw byte representation of this offset.
  pub fn as_bytes(&self) -> &[u8] {
    &self.0
  }

  /// Returns whether this is a beginning-of-stream offset.
  pub fn is_beginning(&self) -> bool {
    self.0.is_empty()
  }

  /// Consume the offset and return the inner bytes.
  pub fn into_bytes(self) -> Vec<u8> {
    self.0
  }
}

impl std::fmt::Display for Offset {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if self.0.is_empty() {
      write!(f, "beginning")
    } else {
      // Display as hex for readability
      for byte in &self.0 {
        write!(f, "{byte:02x}")?;
      }
      Ok(())
    }
  }
}
