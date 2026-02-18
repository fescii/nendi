# Nendi PostgreSQL Connector

The `nendi-pgsql` crate implements the `SourceConnector` trait for PostgreSQL databases. It handles the low-level details of the streaming replication protocol and logical decoding.

## Features

* **Logical Decoding**: Consumes WAL stream using the `pgoutput` plugin (standard since PG 10).
* **Publication Management**: Automatically creates and updates PostgreSQL publications (`CREATE PUBLICATION`) based on Nendi configuration.
  * Supports table addition/removal.
  * Supports PG15+ row filtering.
  * Reconciles configuration drift on startup.
* **Type Conversion**: Maps PostgreSQL Protocol types to Nendi's internal `RowData` format.
* **Snapshotting**: (Planned) Mechanisms for initial backfill.

## Configuration

This crate reads the `[sources.*]` block from the main config.

```toml
[sources.primary]
type = "pgsql"
host = "localhost"
port = 5432
user = "nendi"
# Password via env var: NENDI_SOURCE_PRIMARY_PASSWORD

[sources.primary.publication]
name = "nendi_pub"
mode = "managed" # Nendi owns this publication
tables = ["public.users", "public.orders"]
```
