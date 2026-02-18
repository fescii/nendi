use tracing::{info, warn};

/// High-level slot manager that combines slot lifecycle with WAL lag
/// monitoring and the hard cap policy.
pub struct SlotLifecycleManager {
  slot_name: String,
  publication_name: String,
  auto_create: bool,
  wal_lag_hard_cap_bytes: u64,
  _lag_check_interval_secs: u64,
}

impl SlotLifecycleManager {
  pub fn from_config(cfg: &shared::config::NendiConfig) -> Self {
    Self {
      slot_name: cfg.slot.name.clone(),
      publication_name: cfg.publication.name.clone(),
      auto_create: cfg.slot.auto_create,
      wal_lag_hard_cap_bytes: cfg.slot.wal_lag_hard_cap_bytes,
      _lag_check_interval_secs: cfg.slot.lag_check_interval_secs,
    }
  }

  /// Initialize the slot: ensure it exists and validate its state.
  pub async fn init(&self, client: &crate::connector::PgClient) -> anyhow::Result<()> {
    let slot_mgr = crate::replication::slot::SlotManager::new(&self.slot_name, self.auto_create);
    slot_mgr.ensure_slot(client, &self.publication_name).await?;

    // Check initial WAL lag
    let lag = client.wal_lag_bytes(&self.slot_name).await?;
    info!(
        slot = %self.slot_name,
        lag_bytes = lag,
        cap_bytes = self.wal_lag_hard_cap_bytes,
        "slot initialized"
    );

    if lag > self.wal_lag_hard_cap_bytes {
      warn!(
        lag_bytes = lag,
        cap_bytes = self.wal_lag_hard_cap_bytes,
        "WAL lag exceeds hard cap at startup"
      );
    }

    Ok(())
  }

  /// Check if WAL lag is within the acceptable range.
  pub async fn check_lag(&self, client: &crate::connector::PgClient) -> anyhow::Result<LagStatus> {
    let lag = client.wal_lag_bytes(&self.slot_name).await?;
    let policy = super::policy::LagPolicy::new(self.wal_lag_hard_cap_bytes);
    Ok(policy.evaluate(lag))
  }

  pub fn slot_name(&self) -> &str {
    &self.slot_name
  }
}

/// Result of a WAL lag check.
#[derive(Debug)]
pub enum LagStatus {
  /// Lag is within normal bounds.
  Ok { lag_bytes: u64 },
  /// Lag exceeds the warning threshold (85% of hard cap).
  Warning { lag_bytes: u64, cap_bytes: u64 },
  /// Lag exceeds the hard cap — replication must be paused.
  Critical { lag_bytes: u64, cap_bytes: u64 },
}
