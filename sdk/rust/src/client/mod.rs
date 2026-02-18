//! Nendi client for connecting to the daemon.

pub mod builder;
pub mod config;
pub mod health;

pub use builder::ClientBuilder;
pub use config::ClientConfig;
