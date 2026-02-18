/// IO backend trait abstracting over epoll and io_uring.
///
/// This allows the daemon to use the optimal I/O subsystem for the
/// platform. io_uring provides better performance on Linux 5.6+ kernels,
/// while epoll is the portable fallback.
pub trait IoBackend: Send + Sync {
  /// Name of this backend (for logging).
  fn name(&self) -> &'static str;

  /// Whether this backend supports async submission.
  fn supports_async_submit(&self) -> bool;
}
