use shared::lsn::Lsn;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tracing::debug;

/// Write-ahead log writer for crash recovery.
///
/// Before events are written to RocksDB, they are first appended to
/// the WAL. If the daemon crashes mid-batch, the WAL can be replayed
/// to recover uncommitted events.
///
/// The WAL uses a simple binary format:
/// ```text
/// [4 bytes record_len][8 bytes LSN][payload...]
/// ```
pub struct WalWriter {
  /// Path to the WAL directory.
  dir: PathBuf,
  /// Current WAL segment file.
  current_file: Option<File>,
  /// Current segment number.
  segment: u64,
  /// Bytes written to the current segment.
  segment_bytes: usize,
  /// Maximum segment size before rotation.
  max_segment_bytes: usize,
  /// Last LSN written.
  last_lsn: Lsn,
}

impl WalWriter {
  pub fn new(dir: PathBuf, max_segment_bytes: usize) -> anyhow::Result<Self> {
    fs::create_dir_all(&dir)?;

    Ok(Self {
      dir,
      current_file: None,
      segment: 0,
      segment_bytes: 0,
      max_segment_bytes,
      last_lsn: Lsn::ZERO,
    })
  }

  /// Append an event payload to the WAL.
  pub fn append(&mut self, lsn: Lsn, payload: &[u8]) -> anyhow::Result<()> {
    if self.current_file.is_none() || self.segment_bytes >= self.max_segment_bytes {
      self.rotate()?;
    }

    let file = self.current_file.as_mut().unwrap();
    let record_len = (8 + payload.len()) as u32;

    file.write_all(&record_len.to_be_bytes())?;
    file.write_all(&lsn.to_be_bytes())?;
    file.write_all(payload)?;

    self.segment_bytes += 4 + 8 + payload.len();
    self.last_lsn = lsn;

    Ok(())
  }

  /// Sync the WAL to disk.
  pub fn sync(&mut self) -> anyhow::Result<()> {
    if let Some(ref file) = self.current_file {
      file.sync_all()?;
    }
    Ok(())
  }

  /// Rotate to a new WAL segment.
  fn rotate(&mut self) -> anyhow::Result<()> {
    self.segment += 1;
    let path = self.dir.join(format!("wal_{:08}.log", self.segment));

    let file = OpenOptions::new()
      .create(true)
      .write(true)
      .append(true)
      .open(&path)?;

    debug!(
        segment = self.segment,
        path = %path.display(),
        "WAL segment rotated"
    );

    self.current_file = Some(file);
    self.segment_bytes = 0;
    Ok(())
  }

  /// Remove WAL segments that are fully durable (below watermark).
  pub fn gc_segments(&self, _watermark_lsn: Lsn) -> anyhow::Result<usize> {
    let mut removed = 0;
    for entry in fs::read_dir(&self.dir)? {
      let entry = entry?;
      let path = entry.path();
      if path.extension().and_then(|e| e.to_str()) == Some("log") {
        // Simple strategy: remove segments older than current - 2.
        // A production implementation would track the max LSN per segment.
        if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
          if let Ok(seg_num) = name.trim_start_matches("wal_").parse::<u64>() {
            if seg_num + 2 < self.segment {
              fs::remove_file(&path)?;
              removed += 1;
            }
          }
        }
      }
    }
    Ok(removed)
  }

  pub fn last_lsn(&self) -> Lsn {
    self.last_lsn
  }
}
