pub mod loader;

pub use loader::{
    GrpcConfig, MemoryConfig, NendiConfig, ObservabilityConfig, PipelineConfig, PublicationConfig,
    PublicationTable, SlotConfig, SourceConfig, StorageConfig, WebhookConfig, WebhookTarget,
};

use std::path::Path;

/// Load configuration from a TOML file with environment-variable overrides.
///
/// Resolution order:
/// 1. `config/default.toml` — base configuration
/// 2. `config/{env}.toml` — environment overlay (development, testing, production)
/// 3. Environment variables with prefix `NENDI_` (double underscore for nesting)
///
/// # Example
///
/// `NENDI_SOURCE__CONNECTION_STRING=postgres://...` overrides
/// `source.connection_string`.
pub fn load_config(config_dir: &Path, env: &str) -> anyhow::Result<NendiConfig> {
    let settings = config::Config::builder()
        .add_source(config::File::from(config_dir.join("default.toml")).required(true))
        .add_source(config::File::from(config_dir.join(format!("{}.toml", env))).required(false))
        .add_source(
            config::Environment::with_prefix("NENDI")
                .separator("__")
                .try_parsing(true),
        )
        .build()?;

    let cfg: NendiConfig = settings.try_deserialize()?;
    Ok(cfg)
}
