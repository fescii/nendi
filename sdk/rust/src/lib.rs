//! # Nendi SDK
//!
//! Official Rust client SDK for the Nendi change data capture daemon.
//!
//! ## Quick Start
//!
//! ```ignore
//! use nendi_sdk::{NendiClient, Op};
//!
//! let client = NendiClient::new("http://nendi:50051").await?;
//! let mut stream = client.subscribe("orders").await?;
//! while let Some(event) = stream.next().await? {
//!     println!("{} on {}.{}", event.op(), event.schema(), event.table());
//!     stream.commit(event.offset()).await?;
//! }
//! ```

// ─── Modules always compiled (wasm-safe, pure logic) ─────────────────────────
pub mod deserialize;
pub mod error;
pub mod event;
pub mod offset;
pub mod transpile;

// ─── Modules only available with `grpc` feature (use tonic / tokio TCP) ──────
#[cfg(feature = "grpc")]
pub mod client;
#[cfg(feature = "grpc")]
pub mod retry;
#[cfg(feature = "grpc")]
pub mod stream;
#[cfg(feature = "grpc")]
pub mod subscription;

// Internal proto bindings — only needed for gRPC transport.
#[cfg(feature = "grpc")]
pub(crate) mod proto;

// ─── Re-exports ───────────────────────────────────────────────────────────────

// Always available
pub use deserialize::NendiDeserialize;
pub use error::NendiError;
pub use event::{ChangeEvent, DdlEvent, Op, TypedEvent};
pub use offset::Offset;
#[cfg(feature = "grpc")]
pub use offset::OffsetStore;
pub use transpile::{FilterFn, TranspileError, Transpiler};

// gRPC only
#[cfg(feature = "grpc")]
pub use client::builder::NendiClient;
#[cfg(feature = "grpc")]
pub use retry::RetryPolicy;
#[cfg(feature = "grpc")]
pub use stream::{NendiStream, StreamMux, TypedStream};
#[cfg(feature = "grpc")]
pub use subscription::{Filter, FilterInspection, SubscriptionBuilder};

// Derive macros (always available)
pub use nendi_derive::NendiDeserialize as NendiDeserializeMacro;
pub use nendi_derive::NendiEvent;
