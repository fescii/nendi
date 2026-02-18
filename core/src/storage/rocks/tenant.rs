use super::config::RocksConfig;
use crate::pipeline::batch::WriteBatch;
use rocksdb::{WriteBatch as RocksWriteBatch, WriteOptions, DB};
use shared::event::ChangeEvent;
use shared::lsn::Lsn;
use shared::tenant::TenantId;
use std::sync::Arc;
use tracing::{debug, error, info};

/// Per-tenant RocksDB store with column family isolation.
///
/// Each tenant gets its own column family, providing namespace isolation
/// without the overhead of multiple DB instances. The key format is:
///
/// ```text
/// [8 bytes LSN BE][4 bytes XID BE][table_hash]
/// ```
///
/// This ensures lexicographic order matches LSN order for efficient
/// range scans and replay.
pub struct TenantStore {
  db: Arc<DB>,
  tenant_id: TenantId,
}

impl TenantStore {
  /// Open or create a tenant store.
  pub fn open(config: &RocksConfig, tenant_id: TenantId) -> anyhow::Result<Self> {
    let opts = config.to_rocksdb_options();
    let cf_name = tenant_id.cf_name();

    // Open with column families
    let cfs = vec![cf_name.clone()];
    let db = match DB::open_cf(&opts, &config.data_dir, &cfs) {
      Ok(db) => db,
      Err(_) => {
        // First open — create without CFs, then add the CF.
        let mut db = DB::open(&opts, &config.data_dir)?;
        let cf_opts = rocksdb::Options::default();
        db.create_cf(&cf_name, &cf_opts)?;
        db
      }
    };

    info!(
        tenant = %tenant_id,
        cf = %cf_name,
        "opened tenant store"
    );

    Ok(Self {
      db: Arc::new(db),
      tenant_id,
    })
  }

  /// Write a batch of events atomically.
  pub fn write_batch(&self, batch: &WriteBatch) -> anyhow::Result<()> {
    let cf_name = self.tenant_id.cf_name();
    let cf = self
      .db
      .cf_handle(&cf_name)
      .ok_or_else(|| anyhow::anyhow!("column family '{}' not found", cf_name))?;

    let mut wb = RocksWriteBatch::default();

    for event in batch.events() {
      let key = make_key(event);
      let value = shared::serialize::proto::encode_event(event);
      wb.put_cf(cf, &key, &value);
    }

    let mut write_opts = WriteOptions::default();
    write_opts.set_sync(false); // WAL provides durability
    write_opts.disable_wal(false);

    self.db.write_opt(wb, &write_opts)?;

    debug!(
        tenant = %self.tenant_id,
        events = batch.len(),
        bytes = batch.total_bytes(),
        "batch written to RocksDB"
    );

    Ok(())
  }

  /// Read events in LSN order starting from the given LSN.
  pub fn read_from(&self, start_lsn: Lsn, limit: usize) -> anyhow::Result<Vec<ChangeEvent>> {
    let cf_name = self.tenant_id.cf_name();
    let cf = self
      .db
      .cf_handle(&cf_name)
      .ok_or_else(|| anyhow::anyhow!("column family '{}' not found", cf_name))?;

    let start_key = start_lsn.to_be_bytes();
    let iter = self.db.iterator_cf(
      cf,
      rocksdb::IteratorMode::From(&start_key, rocksdb::Direction::Forward),
    );

    let mut events = Vec::with_capacity(limit);
    for item in iter {
      let (_, value) = item?;
      match shared::serialize::proto::decode_event(&value) {
        Ok(event) => events.push(event),
        Err(e) => {
          error!("failed to decode stored event: {}", e);
          continue;
        }
      }
      if events.len() >= limit {
        break;
      }
    }

    Ok(events)
  }

  pub fn tenant_id(&self) -> &TenantId {
    &self.tenant_id
  }
}

/// Build a RocksDB key from a change event.
///
/// Format: [8 bytes LSN BE][4 bytes XID BE]
fn make_key(event: &ChangeEvent) -> Vec<u8> {
  let mut key = Vec::with_capacity(12);
  key.extend_from_slice(&event.lsn.to_be_bytes());
  key.extend_from_slice(&event.xid.to_be_bytes());
  key
}
