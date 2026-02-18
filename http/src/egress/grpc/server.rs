/// gRPC streaming server for pushing change events to consumers.
///
/// Each consumer gets a dedicated `mpsc` channel. The server task
/// reads from the channel and sends events via the gRPC bidirectional
/// stream.
pub struct GrpcServer {
  /// Bind address for the gRPC server.
  bind_addr: String,
  /// Maximum concurrent consumer streams.
  max_consumers: usize,
}

impl GrpcServer {
  pub fn new(bind_addr: &str, max_consumers: usize) -> Self {
    Self {
      bind_addr: bind_addr.to_string(),
      max_consumers,
    }
  }

  pub fn from_config(cfg: &shared::config::GrpcConfig) -> Self {
    Self::new(&cfg.listen_addr, cfg.max_consumers)
  }

  pub fn bind_addr(&self) -> &str {
    &self.bind_addr
  }

  pub fn max_consumers(&self) -> usize {
    self.max_consumers
  }
}
