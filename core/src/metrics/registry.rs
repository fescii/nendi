use prometheus::{Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, Opts, Registry};

/// Central Prometheus metrics registry for the Nendi daemon.
pub struct MetricsRegistry {
  pub registry: Registry,
  // Ingestion metrics
  pub events_ingested: IntCounter,
  pub events_filtered: IntCounter,
  pub batches_flushed: IntCounter,
  // Storage metrics
  pub storage_writes: IntCounter,
  pub storage_bytes_written: IntCounter,
  // Egress metrics
  pub events_delivered: IntCounterVec,
  pub delivery_errors: IntCounterVec,
  // Pipeline metrics
  pub ring_buffer_size: IntGauge,
  pub ring_buffer_capacity: IntGauge,
  pub flush_latency: Histogram,
  // Consumer metrics
  pub active_consumers: IntGauge,
  pub consumer_lag: IntGauge,
  // Memory metrics
  pub memory_used: IntGauge,
  pub memory_budget: IntGauge,
}

impl MetricsRegistry {
  pub fn new() -> anyhow::Result<Self> {
    let registry = Registry::new();

    let events_ingested = IntCounter::new(
      "nendi_events_ingested_total",
      "Total events ingested from source",
    )?;
    let events_filtered = IntCounter::new(
      "nendi_events_filtered_total",
      "Total events discarded by ingestion filter",
    )?;
    let batches_flushed = IntCounter::new(
      "nendi_batches_flushed_total",
      "Total batches flushed to storage",
    )?;

    let storage_writes = IntCounter::new(
      "nendi_storage_writes_total",
      "Total storage write operations",
    )?;
    let storage_bytes_written = IntCounter::new(
      "nendi_storage_bytes_written_total",
      "Total bytes written to storage",
    )?;

    let events_delivered = IntCounterVec::new(
      Opts::new(
        "nendi_events_delivered_total",
        "Total events delivered to consumers",
      ),
      &["consumer", "egress_type"],
    )?;
    let delivery_errors = IntCounterVec::new(
      Opts::new("nendi_delivery_errors_total", "Total delivery errors"),
      &["consumer", "error"],
    )?;

    let ring_buffer_size = IntGauge::new("nendi_ring_buffer_size", "Current ring buffer length")?;
    let ring_buffer_capacity = IntGauge::new("nendi_ring_buffer_capacity", "Ring buffer capacity")?;
    let flush_latency = Histogram::with_opts(
      HistogramOpts::new("nendi_flush_latency_seconds", "Batch flush latency")
        .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]),
    )?;

    let active_consumers = IntGauge::new(
      "nendi_active_consumers",
      "Number of active consumer streams",
    )?;
    let consumer_lag = IntGauge::new(
      "nendi_consumer_lag_events",
      "Consumer lag in number of events",
    )?;

    let memory_used = IntGauge::new("nendi_memory_used_bytes", "Current memory usage")?;
    let memory_budget = IntGauge::new("nendi_memory_budget_bytes", "Total memory budget")?;

    // Register all metrics
    registry.register(Box::new(events_ingested.clone()))?;
    registry.register(Box::new(events_filtered.clone()))?;
    registry.register(Box::new(batches_flushed.clone()))?;
    registry.register(Box::new(storage_writes.clone()))?;
    registry.register(Box::new(storage_bytes_written.clone()))?;
    registry.register(Box::new(events_delivered.clone()))?;
    registry.register(Box::new(delivery_errors.clone()))?;
    registry.register(Box::new(ring_buffer_size.clone()))?;
    registry.register(Box::new(ring_buffer_capacity.clone()))?;
    registry.register(Box::new(flush_latency.clone()))?;
    registry.register(Box::new(active_consumers.clone()))?;
    registry.register(Box::new(consumer_lag.clone()))?;
    registry.register(Box::new(memory_used.clone()))?;
    registry.register(Box::new(memory_budget.clone()))?;

    Ok(Self {
      registry,
      events_ingested,
      events_filtered,
      batches_flushed,
      storage_writes,
      storage_bytes_written,
      events_delivered,
      delivery_errors,
      ring_buffer_size,
      ring_buffer_capacity,
      flush_latency,
      active_consumers,
      consumer_lag,
      memory_used,
      memory_budget,
    })
  }

  /// Encode all metrics in Prometheus text format.
  pub fn encode(&self) -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let families = self.registry.gather();
    let mut buf = Vec::new();
    encoder.encode(&families, &mut buf).unwrap_or_default();
    String::from_utf8(buf).unwrap_or_default()
  }
}
