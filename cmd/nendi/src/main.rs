use shared::config;
use std::path::Path;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
  init_logging();

  info!("nendi CDC daemon starting");

  let env = std::env::var("NENDI_ENV").unwrap_or_else(|_| "development".to_string());
  let config_dir = Path::new("config");
  let cfg = config::load_config(config_dir, &env)?;
  info!(env = %env, "configuration loaded");

  let runtime_cfg = nendi_core::daemon::runtime::RuntimeConfig::default();
  let rt = runtime_cfg.build_runtime()?;

  rt.block_on(async move { run_daemon(cfg).await })
}

async fn run_daemon(cfg: shared::config::NendiConfig) -> anyhow::Result<()> {
  use nendi_core::daemon::lifecycle::{Lifecycle, LifecycleState};

  let mut lifecycle = Lifecycle::new();

  // Phase 1: Initialize
  lifecycle.transition(LifecycleState::Initializing);

  let metrics = nendi_core::metrics::MetricsRegistry::new()?;
  let memory = nendi_core::memory::MemoryBudget::from_config(&cfg.memory);
  metrics.memory_budget.set(memory.budget() as i64);
  info!(budget = memory.budget(), "memory budget set");

  let _gate = nendi_core::backpressure::gate::BackpressureGate::new();
  let ring = shared::queue::RingBuffer::new(cfg.pipeline.ring_buffer_capacity);
  let flush = nendi_core::pipeline::FlushPolicy::from_config(&cfg.pipeline);
  let _coordinator = nendi_core::pipeline::PipelineCoordinator::new(ring.clone(), flush);
  let _offsets = nendi_core::offset::OffsetManager::new();

  let io_backend = nendi_core::io::detect_backend();
  info!(backend = io_backend.name(), "IO backend selected");

  let health = nendi_http::health::HealthEndpoint::new();

  // Phase 2: Running
  lifecycle.transition(LifecycleState::Running);
  health.set_ready();
  info!("daemon is ready");

  Lifecycle::wait_for_shutdown().await;

  // Phase 3: Shutdown
  lifecycle.transition(LifecycleState::ShuttingDown);
  health.set_not_ready();
  info!("shutting down");

  lifecycle.transition(LifecycleState::Stopped);
  info!("nendi CDC daemon stopped");

  Ok(())
}

fn init_logging() {
  let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

  tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_target(true)
    .with_thread_ids(true)
    .init();
}
