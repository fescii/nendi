//! # Nendi SDK
//!
//! Official Rust client SDK for the Nendi change data capture daemon.
//!
//! Nendi is a standalone CDC daemon that captures row-level changes from
//! PostgreSQL and streams them to consumers via gRPC. This SDK provides
//! a typed, async Rust client for subscribing to those streams.
//!
//! ## Quick Start
//!
//! ```ignore
//! use nendi::{NendiClient, Op};
//!
//! let client = NendiClient::new("http://nendi:50051").await?;
//!
//! let mut stream = client.subscribe("orders").await?;
//!
//! while let Some(event) = stream.next().await? {
//!     println!("{} on {}.{}", event.op(), event.schema(), event.table());
//!     stream.commit(event.offset()).await?;
//! }
//! ```

pub mod client;
pub mod deserialize;
pub mod error;
pub mod event;
pub mod offset;
pub mod retry;
pub mod stream;
pub mod subscription;

// Re-export core types at the crate root for convenience.
pub use client::builder::NendiClient;
pub use deserialize::NendiDeserialize;
pub use error::NendiError;
pub use event::{ChangeEvent, DdlEvent, Op, TypedEvent};
pub use offset::{Offset, OffsetStore};
pub use retry::RetryPolicy;
pub use stream::{NendiStream, StreamMux, TypedStream};
pub use subscription::{Filter, FilterInspection, SubscriptionBuilder};

// Re-export the derive macros.
pub use nendi_derive::NendiDeserialize as NendiDeserializeMacro;
pub use nendi_derive::NendiEvent;
