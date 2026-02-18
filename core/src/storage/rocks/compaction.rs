use rocksdb::compaction_filter::CompactionFilter;

/// RocksDB compaction filter that removes expired events during
/// background compaction.
///
/// This is the GC mechanism for time-bounded retention. During
/// compaction, each key is checked against the watermark — events
/// with LSN below the watermark are discarded.
///
/// This avoids a separate GC scan and piggybacks on RocksDB's
/// existing compaction I/O.
pub struct EventGcFilter {
  /// Events with LSN below this value are eligible for GC.
  watermark_lsn_bytes: [u8; 8],
}

impl EventGcFilter {
  pub fn new(watermark_lsn: u64) -> Self {
    Self {
      watermark_lsn_bytes: watermark_lsn.to_be_bytes(),
    }
  }
}

impl CompactionFilter for EventGcFilter {
  fn name(&self) -> &std::ffi::CStr {
    c"nendi_event_gc"
  }

  fn filter(
    &mut self,
    _level: u32,
    key: &[u8],
    _value: &[u8],
  ) -> rocksdb::compaction_filter::Decision {
    // Keys are prefixed with 8-byte big-endian LSN.
    if key.len() >= 8 && key[..8] < self.watermark_lsn_bytes[..] {
      rocksdb::compaction_filter::Decision::Remove
    } else {
      rocksdb::compaction_filter::Decision::Keep
    }
  }
}
