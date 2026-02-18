# Nendi Tests & Benchmarks

This directory contains the integration test suite and performance benchmarks for Nendi.

## Integration Tests

Located in `integration/`. These tests spin up real PostgreSQL and Nendi instances to verify end-to-end behavior.

### Running Tests

```bash
cargo test --workspace
```

Generates reports in `tests/csvs/tests/`.

## Benchmarks

Located in `bench/`. Focused on micro-benchmarks for critical path components (serialization, storage I/O, filtering).

### Running Benchmarks

```bash
cargo bench --bench csv_report
```

Generates detailed CSV reports in `tests/csvs/{json,ingest,wal}/`.

* `json`: Serialization throughput.
* `ingest`: Filter evaluation speed.
* `wal`: RocksDB append throughput.
