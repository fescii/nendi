//! Offset newtype — LSN-based stream position.

/// An opaque stream offset used for resumption and acknowledgment.
///
/// Internally maps 1:1 to a PostgreSQL Log Sequence Number (LSN, u64).
/// Offsets are monotonically increasing — a later offset is always greater.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset(u64);

impl Offset {
  /// Create an offset representing the beginning of the stream (LSN 0).
  pub fn beginning() -> Self {
    Self(0)
  }

  /// Create an offset from a raw LSN value (received from the daemon).
  pub fn from_lsn(lsn: u64) -> Self {
    Self(lsn)
  }

  /// Create an offset from raw bytes (big-endian u64).
  pub fn from_bytes(bytes: Vec<u8>) -> Self {
    if bytes.len() >= 8 {
      let mut arr = [0u8; 8];
      arr.copy_from_slice(&bytes[..8]);
      Self(u64::from_be_bytes(arr))
    } else {
      Self(0)
    }
  }

  /// Returns the raw LSN as a u64.
  pub fn as_lsn(&self) -> u64 {
    self.0
  }

  /// Returns the LSN encoded as big-endian bytes.
  pub fn as_bytes(&self) -> [u8; 8] {
    self.0.to_be_bytes()
  }

  /// Returns whether this is a beginning-of-stream offset.
  pub fn is_beginning(&self) -> bool {
    self.0 == 0
  }

  /// Consume the offset and return the inner LSN.
  pub fn into_lsn(self) -> u64 {
    self.0
  }
}

impl std::fmt::Display for Offset {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    // Display in PostgreSQL LSN notation: XXXXXXXX/YYYYYYYY
    write!(f, "{:08X}/{:08X}", self.0 >> 32, self.0 & 0xFFFF_FFFF)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn offset_ordering() {
    let a = Offset::from_lsn(100);
    let b = Offset::from_lsn(200);
    assert!(a < b);
    assert!(b > a);
    assert_eq!(a, Offset::from_lsn(100));
  }

  #[test]
  fn offset_beginning() {
    let o = Offset::beginning();
    assert!(o.is_beginning());
    assert_eq!(o.as_lsn(), 0);
  }

  #[test]
  fn offset_roundtrip_bytes() {
    let original = Offset::from_lsn(0x0000_0001_0000_0000u64);
    let bytes = original.as_bytes().to_vec();
    let recovered = Offset::from_bytes(bytes);
    assert_eq!(original, recovered);
  }

  #[test]
  fn offset_display_pg_format() {
    let o = Offset::from_lsn(0x0000_0001_0000_0000u64);
    assert_eq!(format!("{o}"), "00000001/00000000");
  }
}
