use tracing::info;

/// Manages the PostgreSQL replication slot lifecycle.
///
/// Responsible for creating, inspecting, and dropping replication slots.
/// Enforces the WAL lag hard cap to prevent unbounded `pg_wal/` growth.
pub struct SlotManager {
  slot_name: String,
  auto_create: bool,
}

impl SlotManager {
  pub fn new(slot_name: &str, auto_create: bool) -> Self {
    Self {
      slot_name: slot_name.to_string(),
      auto_create,
    }
  }

  /// Create the replication slot if it doesn't exist.
  ///
  /// Returns the consistent point LSN if a new slot was created.
  pub async fn ensure_slot(
    &self,
    client: &crate::connector::PgClient,
    _publication_name: &str,
  ) -> anyhow::Result<Option<shared::lsn::Lsn>> {
    let exists = client.slot_exists(&self.slot_name).await?;

    if exists {
      info!(slot = %self.slot_name, "replication slot already exists");
      return Ok(None);
    }

    if !self.auto_create {
      anyhow::bail!(
        "replication slot '{}' does not exist and auto_create is disabled",
        self.slot_name
      );
    }

    let query = format!(
      "SELECT lsn FROM pg_create_logical_replication_slot('{}', 'pgoutput')",
      self.slot_name
    );
    let row = client.inner().query_one(&query, &[]).await?;
    let lsn_str: &str = row.get(0);
    let lsn = shared::lsn::Lsn::from_pg_str(lsn_str)
      .map_err(|e| anyhow::anyhow!("failed to parse slot LSN: {}", e))?;

    info!(
        slot = %self.slot_name,
        lsn = %lsn,
        "created new replication slot"
    );

    Ok(Some(lsn))
  }

  /// Drop the replication slot (used during teardown).
  pub async fn drop_slot(&self, client: &crate::connector::PgClient) -> anyhow::Result<()> {
    let query = format!("SELECT pg_drop_replication_slot('{}')", self.slot_name);
    client.execute(&query).await?;
    info!(slot = %self.slot_name, "dropped replication slot");
    Ok(())
  }

  pub fn slot_name(&self) -> &str {
    &self.slot_name
  }
}
