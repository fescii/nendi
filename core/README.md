# Nendi Core

The `nendi-core` crate implements the central streaming engine of the daemon. It allows data to flow from ingestion sources to persistent storage, and from storage to egress interfaces.

## Modules

### `pipeline`

Orchestrates the flow of data.

* **Ingestion Ring Buffer**: A lock-free ring buffer that decouples database I/O threads from storage threads.
* **Flush Policy**: Controls when batched events are committed to RocksDB (time-based or size-based).

### `storage`

Manages the embedded RocksDB instance.

* **WAL Writer**: Appends change events to the LSM-tree.
* **Filter**: Applies Layer 2 (Ingestion) filters using CEL predicates and column projection before writing to disk.
* **Compaction**: Tuning configurations for sequential write performance.

### `offset`

Tracks replication progress.

* **Offset Registry**: Maps consumer acknowledgments and source database LSNs to RocksDB keys.
* **Persistence**: Ensures restart-safety by persisting offsets atomically with data batches.

## Usage

This crate is library code used by `cmd/nendi`. It is not intended to be run standalone.
