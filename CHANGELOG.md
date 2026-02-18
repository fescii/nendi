# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- **Core Engine**: Implemented `nendi-core` with ingestion pipeline, RocksDB storage backend, and offset management.
- **PostgreSQL Connector**: Created `nendi-pgsql` for logical replication consumption using `pgoutput` plugin.
- **HTTP/gRPC Layer**: Implemented `nendi-http` providing:
  - gRPC `StreamService` for high-throughput event streaming.
  - Webhook dispatcher for legacy integrations.
  - Admin API for configuration and health checks.
- **SDK**: Released `nendi-sdk` (Rust) with `ConsumerBuilder`, automatic reconnects, and type-safe event handling.
- **Configuration**: Added `config/default.toml` and environment variable overrides via `nendi-shared`.
- **Testing**:
  - **Integration**: Added end-to-end tests for schema application and replication slot management.
  - **Benchmarks**: Added `csv_report` runner for JSON serialization, ingestion throughput, and storage I/O.
- **Documentation**: Added comprehensive `README.md` files for all major components (`core`, `http`, `services/pgsql`, `sdk`).

### Changed

- **Refactor**: Split monolithic codebase into workspace crates (`core`, `http`, `services`, `shared`) for better modularity.
- **Build**: Optimized release profile with LTO and strip for smaller binary size.
- **Deps**: Removed `wasm` crate from workspace to streamline the build process (SDK supports Wasm targets directly).

### Fixed

- **Benchmarks**: Resolved hanging issues in `criterion` by switching to a custom CSV-based benchmark runner.
- **Connectors**: Fixed relative path issues in benchmark output generation.
