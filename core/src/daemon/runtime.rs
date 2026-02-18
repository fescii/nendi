use tracing::info;

/// Daemon runtime configuration.
pub struct RuntimeConfig {
  /// Number of Tokio worker threads.
  pub worker_threads: usize,
  /// Thread name prefix.
  pub thread_name: String,
  /// Stack size per worker thread.
  pub thread_stack_size: usize,
}

impl Default for RuntimeConfig {
  fn default() -> Self {
    Self {
      worker_threads: num_cpus(),
      thread_name: "nendi-worker".to_string(),
      thread_stack_size: 4 * 1024 * 1024, // 4 MiB
    }
  }
}

impl RuntimeConfig {
  /// Build a Tokio runtime from this config.
  pub fn build_runtime(&self) -> anyhow::Result<tokio::runtime::Runtime> {
    let rt = tokio::runtime::Builder::new_multi_thread()
      .worker_threads(self.worker_threads)
      .thread_name(&self.thread_name)
      .thread_stack_size(self.thread_stack_size)
      .enable_all()
      .build()?;

    info!(
      workers = self.worker_threads,
      stack_size = self.thread_stack_size,
      "tokio runtime configured"
    );

    Ok(rt)
  }
}

fn num_cpus() -> usize {
  std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4)
}
