use shared::lsn::Lsn;
use tracing::info;

/// Manages the export of a consistent snapshot from a replication slot.
///
/// When a new replication slot is created, PostgreSQL provides a
/// "consistent point" and a snapshot ID. The exporter uses this to
/// perform a `SET TRANSACTION SNAPSHOT` for consistent reads across
/// all tables.
pub struct SnapshotExporter {
    slot_name: String,
    snapshot_lsn: Lsn,
}

impl SnapshotExporter {
    pub fn new(slot_name: &str, snapshot_lsn: Lsn) -> Self {
        Self {
            slot_name: slot_name.to_string(),
            snapshot_lsn,
        }
    }

    /// Begin the snapshot export transaction.
    ///
    /// This sets the transaction to REPEATABLE READ and binds to the
    /// replication slot's snapshot for consistent reads.
    pub async fn begin_export(
        &self,
        client: &crate::connector::PgClient,
    ) -> anyhow::Result<()> {
        // Get the snapshot name from the slot
        let row = client.inner()
            .query_one(
                "SELECT snapshot_name FROM pg_replication_slots WHERE slot_name = $1",
                &[&self.slot_name],
            )
            .await?;

        let snapshot_name: Option<String> = row.get(0);

        if let Some(snap) = snapshot_name {
            client.execute("BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ").await?;
            client.execute(&format!("SET TRANSACTION SNAPSHOT '{}'", snap)).await?;
            info!(
                slot = %self.slot_name,
                snapshot = %snap,
                lsn = %self.snapshot_lsn,
                "snapshot export transaction started"
            );
        } else {
            // No snapshot available — use a regular consistent read.
            client.execute("BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ").await?;
            info!(
                slot = %self.slot_name,
                lsn = %self.snapshot_lsn,
                "snapshot export started (no named snapshot)"
            );
        }

        Ok(())
    }

    /// Commit the snapshot export transaction.
    pub async fn commit_export(
        &self,
        client: &crate::connector::PgClient,
    ) -> anyhow::Result<()> {
        client.execute("COMMIT").await?;
        info!("snapshot export committed");
        Ok(())
    }

    pub fn snapshot_lsn(&self) -> Lsn {
        self.snapshot_lsn
    }
}
