# Nendi

**Nendi** is a high-performance, standalone database streaming daemon engineered for sub-millisecond Change Data Capture (CDC). It connects directly to PostgreSQL via logical replication, stores change events in an embedded RocksDB log, and streams them to consumers via gRPC or HTTP webhooks.

It is designed to replace complex Kafka+Debezium pipelines for single-region or localized streaming workloads, reducing infrastructure footprint and tail latency.

## Core Architecture

Nendi consolidates ingestion, storage, and egress into a single binary:

1. **Ingestion**: Consumes PostgreSQL WAL via `pgoutput` logical decoding. Supports automatic publication management and schema evolution.
2. **Storage**: Embedded RocksDB engine tuned for sequential NVMe writes. Deduplicates storage by projecting columns before persistence.
3. **Egress**:
    * **gRPC**: Bidirectional streaming with HTTP/2 flow control.
    * **Webhooks**: Asynchronous HTTP dispatch for legacy integration.

## Features

* **Kafka-Free**: No external broker required. Nendi acts as the log.
* **Layered Filtering**:
  * **L1 (Publication)**: Filter at the source (PostgreSQL).
  * **L2 (Ingestion)**: Filter before storage (CEL predicates, column projection).
  * **L3 (Subscription)**: Filter at egress per consumer.
  * **L4 (Encryption)**: Column-level encryption with customer-managed keys.
* **Performance**: Targets 100k+ writes/sec on commodity NVMe hardware.
* **Safety**: Written in Rust. memory-safe, thread-per-core architecture using `io_uring` (where supported).

## Project Structure

* `cmd/nendi`: Main daemon entry point.
* `core`: The streaming engine (pipeline, RocksDB storage, offset registry).
* `services/pgsql`: PostgreSQL connector and logical decoding logic.
* `http`: gRPC server, Admin API, and Webhook dispatcher.
* `shared`: Common type definitions, configuration structs, and errors.
* `sdk`: Client libraries for consuming Nendi streams.
* `tests`: Integration tests and performance benchmarks.

## Quick Start

### Prerequisites

* Rust 1.75+
* PostgreSQL 14+ (with `wal_level = logical`)
* `clang` and `llvm` (for RocksDB compilation)

### Build & Run

```bash
# Build release binary
cargo build --release

# Run with default config
./target/release/nendi
```

### Configuration

Configuration is loaded from `config/default.toml`. See `shared/src/config.rs` for schema details.

```toml
[sources.primary]
type = "pgsql"
host = "localhost"
port = 5432
dbname = "app_db"
user = "replication_user"

[sources.primary.publication]
mode = "managed"
tables = ["public.users", "public.orders"]
```
