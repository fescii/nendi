use shared::config::NendiConfig;
use tokio_postgres::NoTls;

/// Load test configuration from fixtures and environment.
pub fn load_test_config() -> NendiConfig {
  // Force loading of .env file
  dotenvy::from_path("../../env/.env").ok();

  // In a real scenario, we might merge default.toml + test.toml
  // For now, we rely on the loader to pick up defaults and env vars.
  // We can manually construct one or use the shared loader if public.
  // Let's rely on standard config loading or manual construction for control.

  // We'll construct a config that matches our .env settings for testing
  let source_conn = std::env::var("NENDI_SOURCE__CONNECTION_STRING")
    .expect("NENDI_SOURCE__CONNECTION_STRING must be set");

  let source = shared::config::SourceConfig {
    connection_string: source_conn,
    pool_size: 2,
    application_name: "nendi_integration_test".to_string(),
  };

  let slot = shared::config::SlotConfig {
    name: std::env::var("NENDI_SLOT__NAME").unwrap_or_else(|_| "nendi_test_slot".into()),
    output_plugin: "pgoutput".to_string(),
    auto_create: true,
    wal_lag_hard_cap_bytes: 1024 * 1024 * 100,
    lag_check_interval_secs: 1,
  };

  let publication = shared::config::PublicationConfig {
    name: std::env::var("NENDI_PUBLICATION__NAME").unwrap_or_else(|_| "nendi_test_pub".into()),
    tables: vec![], // All tables
  };

  // Other configs with defaults
  let storage = shared::config::StorageConfig {
    data_dir: std::env::var("NENDI_STORAGE__DATA_DIR").unwrap_or_else(|_| "/tmp/nendi-test".into()),
    write_buffer_size: 1024 * 1024,
    max_write_buffers: 2,
    block_cache_size: 1024 * 1024,
    direct_io: false, // Often better for tests
    compression: "none".to_string(),
    ttl_secs: 0,
  };

  let pipeline = shared::config::PipelineConfig {
    flush_max_bytes: 1024,
    flush_max_age_us: 10_000,
    ring_buffer_capacity: 1024,
    filters: Default::default(),
  };

  let grpc = shared::config::GrpcConfig {
    listen_addr: "[::1]:0".to_string(), // Random port
    credit_window: 100,
    max_consumers: 10,
  };

  let memory = shared::config::MemoryConfig {
    budget_bytes: 1024 * 1024 * 100,
    warn_threshold: 0.8,
  };

  let observability = shared::config::ObservabilityConfig {
    log_level: "debug".to_string(),
    metrics_addr: "127.0.0.1:0".to_string(),
  };

  NendiConfig {
    source,
    slot,
    publication,
    storage,
    pipeline,
    grpc,
    webhook: None,
    memory,
    observability,
  }
}

pub async fn connect_postgres(
  cfg: &NendiConfig,
) -> (
  tokio_postgres::Client,
  tokio_postgres::Connection<tokio_postgres::Socket, tokio_postgres::tls::NoTlsStream>,
) {
  tokio_postgres::connect(&cfg.source.connection_string, NoTls)
    .await
    .expect("Failed to connect to postgres")
}

pub async fn apply_fixtures(client: &tokio_postgres::Client) {
  // 1. Drop existing objects to start clean (scoped to this test run mostly)
  // Note: In real parallel tests, we'd use unique names or namespaces.
  client
    .batch_execute(
      "
         DROP TABLE IF EXISTS public.payments;
         DROP TABLE IF EXISTS public.orders;
         DROP TABLE IF EXISTS public.customers;
    ",
    )
    .await
    .expect("Failed to clean up DB tables");

  // 2. Load schema fixture
  let schema_sql = std::fs::read_to_string("../../tests/fixtures/pgsql/schema.sql")
    .expect("Failed to read schema.sql");
  client
    .batch_execute(&schema_sql)
    .await
    .expect("Failed to apply schema");

  // 3. Load seed fixture
  let seed_sql = std::fs::read_to_string("../../tests/fixtures/pgsql/seed.sql")
    .expect("Failed to read seed.sql");
  client
    .batch_execute(&seed_sql)
    .await
    .expect("Failed to apply seed data");
}

#[tokio::test]
async fn test_schema_setup() {
  let cfg = load_test_config();
  let (client, connection) = connect_postgres(&cfg).await;

  tokio::spawn(async move {
    if let Err(e) = connection.await {
      eprintln!("connection error: {}", e);
    }
  });

  client.batch_execute(&format!(
        "DROP PUBLICATION IF EXISTS {};
         SELECT pg_drop_replication_slot('{}') WHERE EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = '{}');",
         cfg.publication.name, cfg.slot.name, cfg.slot.name
    )).await.expect("Failed to clean up DB objects");

  apply_fixtures(&client).await;

  // 4. Verify data
  let row = client
    .query_one("SELECT count(*) FROM public.customers", &[])
    .await
    .expect("Query failed");
  let count: i64 = row.get(0);
  assert!(count >= 3, "Expected at least 3 customers, got {}", count);
}
