/// Connection configuration for the PostgreSQL source database.
#[derive(Debug, Clone)]
pub struct ConnectorConfig {
    /// PostgreSQL connection string.
    pub connection_string: String,
    /// Application name visible in `pg_stat_activity`.
    pub application_name: String,
    /// Replication slot name.
    pub slot_name: String,
    /// Publication name to subscribe to.
    pub publication_name: String,
}

impl ConnectorConfig {
    /// Build from the shared NendiConfig.
    pub fn from_nendi_config(cfg: &shared::config::NendiConfig) -> Self {
        Self {
            connection_string: cfg.source.connection_string.clone(),
            application_name: cfg.source.application_name.clone(),
            slot_name: cfg.slot.name.clone(),
            publication_name: cfg.publication.name.clone(),
        }
    }
}
