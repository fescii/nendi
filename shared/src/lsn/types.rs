use std::fmt;

/// PostgreSQL Log Sequence Number — a monotonically increasing 64-bit value
/// representing a position in the write-ahead log.
///
/// Stored as big-endian bytes in RocksDB so that lexicographic key order
/// matches LSN order, enabling efficient range scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lsn(pub u64);

impl Lsn {
    pub const ZERO: Lsn = Lsn(0);
    pub const MAX: Lsn = Lsn(u64::MAX);

    /// Create a new LSN from a raw u64 value.
    #[inline]
    pub fn new(val: u64) -> Self {
        Self(val)
    }

    /// Returns the raw u64 value.
    #[inline]
    pub fn get(self) -> u64 {
        self.0
    }

    /// Encode as 8-byte big-endian slice — used as RocksDB key prefix.
    #[inline]
    pub fn to_be_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// Decode from an 8-byte big-endian slice.
    #[inline]
    pub fn from_be_bytes(bytes: [u8; 8]) -> Self {
        Self(u64::from_be_bytes(bytes))
    }

    /// Parse from PostgreSQL's `X/Y` hex format (e.g. `0/16B3740`).
    pub fn from_pg_str(s: &str) -> Result<Self, LsnParseError> {
        let (hi, lo) = s
            .split_once('/')
            .ok_or_else(|| LsnParseError(s.to_string()))?;
        let hi = u32::from_str_radix(hi, 16).map_err(|_| LsnParseError(s.to_string()))?;
        let lo = u32::from_str_radix(lo, 16).map_err(|_| LsnParseError(s.to_string()))?;
        Ok(Self(((hi as u64) << 32) | lo as u64))
    }

    /// Format as PostgreSQL's `X/Y` hex format.
    pub fn to_pg_string(self) -> String {
        let hi = (self.0 >> 32) as u32;
        let lo = self.0 as u32;
        format!("{:X}/{:X}", hi, lo)
    }

    /// Returns true if this LSN is ahead of `other`.
    #[inline]
    pub fn is_ahead_of(self, other: Lsn) -> bool {
        self.0 > other.0
    }

    /// Saturating addition.
    #[inline]
    pub fn saturating_add(self, delta: u64) -> Self {
        Self(self.0.saturating_add(delta))
    }
}

impl fmt::Display for Lsn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_pg_string())
    }
}

impl From<u64> for Lsn {
    #[inline]
    fn from(val: u64) -> Self {
        Self(val)
    }
}

impl From<Lsn> for u64 {
    #[inline]
    fn from(lsn: Lsn) -> Self {
        lsn.0
    }
}

/// Error returned when an LSN string cannot be parsed.
#[derive(Debug, Clone)]
pub struct LsnParseError(pub String);

impl fmt::Display for LsnParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid LSN format: '{}'", self.0)
    }
}

impl std::error::Error for LsnParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_pg_format() {
        let lsn = Lsn::from_pg_str("0/16B3740").unwrap();
        assert_eq!(lsn.to_pg_string(), "0/16B3740");
    }

    #[test]
    fn big_endian_ordering() {
        let a = Lsn::new(100);
        let b = Lsn::new(200);
        assert!(a.to_be_bytes() < b.to_be_bytes());
    }

    #[test]
    fn ordering() {
        let a = Lsn::new(42);
        let b = Lsn::new(99);
        assert!(a < b);
        assert!(b.is_ahead_of(a));
    }
}
