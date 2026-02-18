use super::backend::IoBackend;

/// Epoll-based IO backend (portable fallback).
pub struct EpollBackend;

impl IoBackend for EpollBackend {
  fn name(&self) -> &'static str {
    "epoll"
  }

  fn supports_async_submit(&self) -> bool {
    false
  }
}
