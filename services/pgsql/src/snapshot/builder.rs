use shared::event::{ChangeEvent, ColumnValue, OpType, RowData, TableId};
use tracing::info;

/// Builds an initial snapshot of the source tables for consumers that
/// need the full current state before streaming incremental changes.
///
/// The snapshot uses the consistent LSN from the replication slot to
/// ensure that streaming can resume exactly where the snapshot left off.
pub struct SnapshotBuilder {
  /// Tables to snapshot.
  tables: Vec<String>,
}

impl SnapshotBuilder {
  pub fn new(tables: Vec<String>, _batch_size: usize) -> Self {
    Self { tables }
  }

  /// Execute the snapshot for a single table.
  ///
  /// Returns the rows as synthetic INSERT ChangeEvents so they can be
  /// fed into the same pipeline as streaming events.
  pub async fn snapshot_table(
    &self,
    client: &crate::connector::PgClient,
    qualified_table: &str,
    snapshot_lsn: u64,
  ) -> anyhow::Result<Vec<ChangeEvent>> {
    let parts = qualified_table
      .split_once('.')
      .ok_or_else(|| anyhow::anyhow!("invalid table name: {}", qualified_table))?;

    // Get the column names first.
    let col_rows = client
      .inner()
      .query(
        "SELECT column_name, udt_name
                 FROM information_schema.columns
                 WHERE table_schema = $1 AND table_name = $2
                 ORDER BY ordinal_position",
        &[&parts.0, &parts.1],
      )
      .await?;

    let col_info: Vec<(String, String)> = col_rows
      .iter()
      .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
      .collect();

    if col_info.is_empty() {
      anyhow::bail!("no columns found for table {}", qualified_table);
    }

    let col_names: Vec<&str> = col_info.iter().map(|(n, _)| n.as_str()).collect();
    let select = format!(
      "SELECT {} FROM {}.{}",
      col_names.join(", "),
      parts.0,
      parts.1
    );

    info!(
      table = qualified_table,
      columns = col_names.len(),
      "starting snapshot"
    );

    let rows = client.inner().query(&select, &[]).await?;
    let mut events = Vec::with_capacity(rows.len());

    for row in &rows {
      let mut columns = Vec::with_capacity(col_info.len());
      for (i, (name, _type_name)) in col_info.iter().enumerate() {
        // Extract as text representation
        let value: Option<String> = row.try_get(i).ok();
        columns.push(ColumnValue {
          name: name.clone(),
          type_oid: 0,
          value,
        });
      }

      events.push(ChangeEvent {
        lsn: snapshot_lsn,
        xid: 0,
        op: OpType::Insert, // Snapshot rows are synthetic INSERTs
        table: TableId::new(parts.0, parts.1),
        schema_fingerprint: 0,
        after: Some(RowData { columns }),
        before: None,
        commit_timestamp_us: 0,
      });
    }

    info!(
      table = qualified_table,
      rows = events.len(),
      "snapshot complete"
    );

    Ok(events)
  }

  /// Snapshot all configured tables.
  pub async fn snapshot_all(
    &self,
    client: &crate::connector::PgClient,
    snapshot_lsn: u64,
  ) -> anyhow::Result<Vec<ChangeEvent>> {
    let mut all_events = Vec::new();
    for table in &self.tables {
      let events = self.snapshot_table(client, table, snapshot_lsn).await?;
      all_events.extend(events);
    }
    Ok(all_events)
  }
}
