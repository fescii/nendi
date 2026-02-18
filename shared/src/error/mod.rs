pub mod types;

pub use types::NendiError;

/// Shorthand Result type used throughout Nendi.
pub type Result<T> = std::result::Result<T, NendiError>;
