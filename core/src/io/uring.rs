use super::backend::IoBackend;

/// io_uring backend stub — requires Linux 5.6+.
///
/// This is a placeholder for future io_uring integration. When the
/// kernel supports it, this backend provides zero-copy I/O submission
/// for RocksDB flush and WAL writes.
pub struct UringBackend;

impl IoBackend for UringBackend {
  fn name(&self) -> &'static str {
    "io_uring"
  }

  fn supports_async_submit(&self) -> bool {
    true
  }
}
