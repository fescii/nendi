use serde::{Deserialize, Serialize};
use std::fmt;

/// Uniquely identifies a tenant within the Nendi daemon.
///
/// In single-tenant mode this is typically `"default"`. Multi-tenant
/// deployments use this to isolate RocksDB column families and consumer
/// namespaces.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    pub const DEFAULT: &'static str = "default";

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn default_tenant() -> Self {
        Self(Self::DEFAULT.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the RocksDB column family name for this tenant.
    pub fn cf_name(&self) -> String {
        format!("tenant_{}", self.0)
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for TenantId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Per-tenant configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    /// Tenant identifier.
    pub id: TenantId,
    /// Whether this tenant is active.
    #[serde(default = "default_true")]
    pub active: bool,
    /// Maximum events per second for this tenant (0 = unlimited).
    #[serde(default)]
    pub rate_limit: u64,
    /// RocksDB TTL override for this tenant's events, in seconds.
    #[serde(default)]
    pub ttl_secs: u64,
}

fn default_true() -> bool {
    true
}
