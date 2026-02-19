//! Offset types and stores.

pub mod types;

// Stores require async_trait + tokio — only available with `grpc` feature.
#[cfg(feature = "grpc")]
pub mod file;
#[cfg(feature = "grpc")]
pub mod nendi;
#[cfg(feature = "grpc")]
pub mod store;

pub use types::Offset;

#[cfg(feature = "grpc")]
pub use store::OffsetStore;
