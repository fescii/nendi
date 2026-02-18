use super::backend::IoBackend;
use super::epoll::EpollBackend;
use super::uring::UringBackend;
use tracing::info;

/// Detect the best available IO backend for this platform.
///
/// Checks for io_uring support by probing the kernel. Falls back
/// to epoll if io_uring is not available.
pub fn detect_backend() -> Box<dyn IoBackend> {
  if is_uring_available() {
    info!("io_uring is available, using io_uring backend");
    Box::new(UringBackend)
  } else {
    info!("io_uring not available, using epoll backend");
    Box::new(EpollBackend)
  }
}

/// Check if io_uring is supported on this kernel.
fn is_uring_available() -> bool {
  // Probe by checking kernel version >= 5.6.
  // In a production implementation this would use the
  // io_uring_probe syscall.
  if let Ok(info) = std::fs::read_to_string("/proc/version") {
    // Parse "Linux version X.Y.Z ..."
    if let Some(version) = info.split_whitespace().nth(2) {
      let parts: Vec<&str> = version.split('.').collect();
      if parts.len() >= 2 {
        let major = parts[0].parse::<u32>().unwrap_or(0);
        let minor = parts[1].parse::<u32>().unwrap_or(0);
        return major > 5 || (major == 5 && minor >= 6);
      }
    }
  }
  false
}
