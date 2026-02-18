use super::manager::LagStatus;

/// WAL lag policy enforcing the hard cap described in the architecture docs.
///
/// The hard cap prevents unbounded `pg_wal/` growth when the daemon
/// falls behind. When lag exceeds the cap, replication is paused until
/// the pipeline catches up.
pub struct LagPolicy {
  /// Hard cap in bytes.
  hard_cap_bytes: u64,
  /// Warning threshold = 85% of hard cap.
  warn_threshold: f64,
}

impl LagPolicy {
  pub fn new(hard_cap_bytes: u64) -> Self {
    Self {
      hard_cap_bytes,
      warn_threshold: 0.85,
    }
  }

  /// Evaluate the current lag against the policy.
  pub fn evaluate(&self, lag_bytes: u64) -> LagStatus {
    if lag_bytes > self.hard_cap_bytes {
      LagStatus::Critical {
        lag_bytes,
        cap_bytes: self.hard_cap_bytes,
      }
    } else if lag_bytes as f64 > self.hard_cap_bytes as f64 * self.warn_threshold {
      LagStatus::Warning {
        lag_bytes,
        cap_bytes: self.hard_cap_bytes,
      }
    } else {
      LagStatus::Ok { lag_bytes }
    }
  }

  /// Returns whether replication should be paused.
  pub fn should_pause(&self, lag_bytes: u64) -> bool {
    lag_bytes > self.hard_cap_bytes
  }

  /// Returns whether replication can resume (lag dropped below 50% of cap).
  pub fn can_resume(&self, lag_bytes: u64) -> bool {
    lag_bytes < self.hard_cap_bytes / 2
  }
}
