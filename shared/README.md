# Nendi Shared

The `nendi-shared` crate contains common type definitions and utilities used across the workspace.

## Contents

* **Config**: `serde`-based configuration structs (TOML).
* **Event Model**: `ChangeEvent`, `RowData`, `OpType` structs that define the canonical data format.
* **LSN**: `Lsn` (Log Sequence Number) type for ordering and offsets.
* **Errors**: Centralized error types for the application.

This crate ensures that `core`, `http`, and specific service connectors all speak the same data language.
