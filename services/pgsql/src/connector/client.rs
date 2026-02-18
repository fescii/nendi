use super::config::ConnectorConfig;
use tokio_postgres::tls::NoTlsStream;
use tokio_postgres::{Client, Connection, NoTls, Socket};
use tracing::info;

/// Manages a connection to the source PostgreSQL database.
///
/// This client is used for:
/// 1. Schema introspection queries
/// 2. Publication management (CREATE/ALTER PUBLICATION)
/// 3. Replication slot management
///
/// The actual replication stream uses a separate connection in
/// replication mode (see `replication::stream`).
pub struct PgClient {
  client: Client,
  config: ConnectorConfig,
}

impl PgClient {
  /// Connect to the PostgreSQL database.
  pub async fn connect(
    config: ConnectorConfig,
  ) -> anyhow::Result<(Self, Connection<Socket, NoTlsStream>)> {
    let (client, connection) = tokio_postgres::connect(&config.connection_string, NoTls).await?;

    info!(
        app_name = %config.application_name,
        slot = %config.slot_name,
        "connected to PostgreSQL source"
    );

    Ok((Self { client, config }, connection))
  }

  /// Returns a reference to the underlying tokio-postgres client.
  pub fn inner(&self) -> &Client {
    &self.client
  }

  /// Returns the connector configuration.
  pub fn config(&self) -> &ConnectorConfig {
    &self.config
  }

  /// Execute a simple query (e.g. publication management).
  pub async fn execute(&self, query: &str) -> anyhow::Result<u64> {
    let rows = self.client.execute(query, &[]).await?;
    Ok(rows)
  }

  /// Query the current WAL LSN position.
  pub async fn current_wal_lsn(&self) -> anyhow::Result<shared::lsn::Lsn> {
    let row = self
      .client
      .query_one("SELECT pg_current_wal_lsn()::text", &[])
      .await?;
    let lsn_str: &str = row.get(0);
    shared::lsn::Lsn::from_pg_str(lsn_str)
      .map_err(|e| anyhow::anyhow!("failed to parse WAL LSN: {}", e))
  }

  /// Check if the replication slot exists.
  pub async fn slot_exists(&self, slot_name: &str) -> anyhow::Result<bool> {
    let row = self
      .client
      .query_one(
        "SELECT COUNT(*) FROM pg_replication_slots WHERE slot_name = $1",
        &[&slot_name],
      )
      .await?;
    let count: i64 = row.get(0);
    Ok(count > 0)
  }

  /// Check if the publication exists.
  pub async fn publication_exists(&self, pub_name: &str) -> anyhow::Result<bool> {
    let row = self
      .client
      .query_one(
        "SELECT COUNT(*) FROM pg_publication WHERE pubname = $1",
        &[&pub_name],
      )
      .await?;
    let count: i64 = row.get(0);
    Ok(count > 0)
  }

  /// Get the current WAL lag in bytes for a slot.
  pub async fn wal_lag_bytes(&self, slot_name: &str) -> anyhow::Result<u64> {
    let row = self
      .client
      .query_one(
        "SELECT COALESCE(pg_wal_lsn_diff(pg_current_wal_lsn(), confirmed_flush_lsn), 0)::bigint
                 FROM pg_replication_slots WHERE slot_name = $1",
        &[&slot_name],
      )
      .await?;
    let lag: i64 = row.get(0);
    Ok(lag.max(0) as u64)
  }

  /// List tables currently in the publication.
  pub async fn publication_tables(&self, pub_name: &str) -> anyhow::Result<Vec<String>> {
    let rows = self
      .client
      .query(
        "SELECT schemaname || '.' || tablename
                 FROM pg_publication_tables WHERE pubname = $1
                 ORDER BY schemaname, tablename",
        &[&pub_name],
      )
      .await?;
    Ok(rows.iter().map(|r| r.get(0)).collect())
  }
}
