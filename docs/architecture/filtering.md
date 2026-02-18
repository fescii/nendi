# Nendi — Selective Streaming & Filtering Architecture

> **Document location:** `docs/architecture/filtering.md`
> **Status:** Approved for implementation
> **Covers:** All four filtering layers — PostgreSQL publication, ingestion predicate,
> consumer subscription, and column-level encryption

---

## Table of Contents

1. [Overview & Design Philosophy](#overview--design-philosophy)
2. [Layer Architecture Diagram](#layer-architecture-diagram)
3. [Layer 1 — PostgreSQL Publication Filter](#layer-1--postgresql-publication-filter)
4. [Layer 2 — Nendi Ingestion Filter](#layer-2--nendi-ingestion-filter)
5. [Layer 3 — Consumer Subscription Filter](#layer-3--consumer-subscription-filter)
6. [Layer 4 — Column-Level Encryption](#layer-4--column-level-encryption)
7. [Layer Interaction & Precedence Rules](#layer-interaction--precedence-rules)
8. [Performance Characteristics](#performance-characteristics)
9. [Configuration Reference](#configuration-reference)
10. [Predicate Expression Language](#predicate-expression-language)
11. [Implementation Roadmap](#implementation-roadmap)
12. [Testing Strategy](#testing-strategy)

---

## Overview & Design Philosophy

Nendi's filtering system is built around a single guiding principle: **filter as early and as cheaply as possible, store once, serve variably.** Every layer removes work from the layers that follow it. No layer duplicates data in storage to serve different consumer views. All four layers compose cleanly — enabling a stack where PostgreSQL never emits irrelevant WAL bytes, Nendi never persists irrelevant events, and each consumer receives only the exact fields and rows it declared interest in, optionally with sensitive fields encrypted behind per-consumer keys.

The four layers are:

| # | Name | Where It Runs | Removes Work From |
|---|------|---------------|-------------------|
| 1 | Publication Filter | Inside PostgreSQL | Nendi network ingestion |
| 2 | Ingestion Predicate | Nendi hot path, pre-storage | RocksDB, egress, all consumers |
| 3 | Consumer Subscription Filter | Nendi egress, per-consumer | Network bandwidth, consumer CPU |
| 4 | Column Encryption | Nendi ingestion + egress | Authorization infrastructure |

Layers 1 and 2 reduce what is **stored**. Layers 3 and 4 control what is **delivered**. This distinction is fundamental to the design — do not conflate them.

---

## Layer Architecture Diagram

```
  PostgreSQL WAL
       │
       │  ← Layer 1: Publication filter applied here
       │    (table selection, operation type, row predicate)
       │    Filtered rows are NEVER decoded or sent
       ▼
  Nendi Replication Socket
       │
       │  ← Layer 2: Ingestion predicate applied here
       │    (column projection, row predicate, type coercion)
       │    Filtered rows are NEVER written to RocksDB
       ▼
  RocksDB Log Store
  (stores only what passed Layers 1 and 2)
       │
       ├──────────────────────────────────────┐
       │                                      │
       │  Consumer A                          │  Consumer B
       │  ← Layer 3: Subscription filter      │  ← Layer 3: Subscription filter
       │    (different projection, predicate)  │    (different projection, predicate)
       │  ← Layer 4: Encryption               │  ← Layer 4: Encryption
       │    (Consumer A key — sees PII)        │    (Consumer B key — sees ciphertext)
       ▼                                      ▼
  gRPC stream to Consumer A             gRPC stream to Consumer B
  (full row with decrypted fields)       (partial row with encrypted fields)
```

---

## Layer 1 — PostgreSQL Publication Filter

### What It Does

A PostgreSQL **publication** defines which tables, operations, and (since PG15) which rows are eligible for logical decoding. Anything outside the publication is never transformed into logical change records — the WAL bytes for those rows are decoded only up to the point of checking the publication, then discarded by PostgreSQL itself. Nendi never sees these events. There is no network cost, no CPU cost on the Nendi side, and no storage cost.

This is the highest-leverage filter in the entire stack. Configure it aggressively.

### Setup

Nendi manages publication creation automatically during slot initialization using the config declared in `config/default.toml`. The SQL it executes is generated from the `sql/pgsql/setup/` directory.

**Automatic publication creation (Nendi-managed):**

Nendi reads the `[publication]` block in config and issues the appropriate `CREATE PUBLICATION` statement during the `connect()` phase of the `SourceConnector` trait implementation. If the publication already exists, Nendi validates it against the config and emits a warning if there is a mismatch — it does not silently overwrite.

**Manual publication creation (operator-managed):**

```sql
-- Minimal publication: all operations on specific tables
CREATE PUBLICATION nendi_pub
    FOR TABLE public.orders,
              public.accounts,
              public.payments,
              public.events;

-- Operation-scoped publication: only inserts and updates, no deletes
CREATE PUBLICATION nendi_pub
    FOR TABLE public.orders
    WITH (publish = 'insert, update');

-- Row-filtered publication (PostgreSQL 15+ only)
-- Rows not matching the WHERE clause are never decoded
CREATE PUBLICATION nendi_pub
    FOR TABLE public.orders WHERE (status != 'draft'),
              public.payments WHERE (amount > 0);

-- Full replication of a schema (use with caution — includes all future tables)
CREATE PUBLICATION nendi_pub
    FOR TABLES IN SCHEMA public;
```

### Updating a Live Publication

Publications can be altered without restarting Nendi or dropping the replication slot. The change takes effect for all subsequent WAL records.

```sql
-- Add a table
ALTER PUBLICATION nendi_pub ADD TABLE public.refunds;

-- Remove a table
ALTER PUBLICATION nendi_pub DROP TABLE public.legacy_events;

-- Change row filter on an existing table (PG15+)
ALTER PUBLICATION nendi_pub SET TABLE public.orders WHERE (status != 'draft' AND amount > 100);
```

After altering a publication, emit a notification to Nendi's admin API so it can update its internal schema registry:

```bash
curl -X POST http://localhost:9090/admin/publication/refresh \
  -H "Content-Type: application/json" \
  -d '{"source": "pgsql-prod"}'
```

### What PostgreSQL Cannot Filter at This Layer

- Column-level projection (all columns of a published row are always decoded)
- Dynamic per-consumer predicates (one publication serves all of Nendi's ingestion)
- Computed or derived values
- Encryption

These are handled by Layers 2, 3, and 4.

### Configuration Block

```toml
[sources.pgsql-prod.publication]
name            = "nendi_pub"
# "managed" means Nendi creates/validates the publication automatically
# "external" means the operator manages it manually
mode            = "managed"

# Tables included in the publication
tables = [
    "public.orders",
    "public.accounts",
    "public.payments",
]

# Operation types to include: insert | update | delete | truncate
operations = ["insert", "update", "delete"]

# Row-level filter (PG15+ only); set to "" to disable
# Applied per-table; must be valid PostgreSQL boolean expression
[sources.pgsql-prod.publication.row_filters]
"public.orders"   = "status != 'draft'"
"public.payments" = "amount > 0"
```

---

## Layer 2 — Nendi Ingestion Filter

### What It Does

After WAL bytes arrive at Nendi's replication socket and are decoded by `services/pgsql/decoding/pgoutput.rs`, but **before the event is written to RocksDB**, the ingestion filter is applied. It has two sub-components:

**Row predicate:** A boolean expression evaluated against the fully decoded row. If false, the event is dropped entirely and never stored. This is more flexible than the PostgreSQL publication filter because it can reference computed values, apply to multiple sources with different schemas uniformly, and be changed without altering the PostgreSQL publication.

**Column projection:** A whitelist of columns to retain. All other columns are stripped from the event before serialization to RocksDB. This reduces storage size, write amplification, and the downstream exposure of sensitive data — even to Nendi's own storage layer.

### Where It Runs in the Pipeline

```
pgoutput decoder
      │
      ▼
ChangeEvent (full row, all columns)
      │
      ├─ Row predicate evaluation
      │   └─ false? → drop, update metrics counter, continue
      │
      ├─ Column projection
      │   └─ Strip columns not in whitelist
      │
      ▼
rkyv serialization of projected ChangeEvent
      │
      ▼
WriteBatch accumulator → RocksDB
```

### Performance Characteristics

At 100,000 messages/second:

- Simple predicate (enum comparison, integer range): ~20–50ns per message = 2–5ms CPU/sec. Negligible.
- Complex predicate (3–5 conditions, string comparison): ~100–200ns per message = 10–20ms CPU/sec. Still negligible against I/O budget.
- Column projection (stripping 5 of 20 columns): ~30ns per message + reduced serialization time. Net positive — saves more on write I/O than it costs in CPU.

The ingestion predicate runs on the same thread as the replication socket reader, so it must never block or allocate unboundedly. The CEL evaluator (see [Predicate Expression Language](#predicate-expression-language)) is bounded by construction.

### Configuration Block

```toml
# Ingestion-level filter — applied before storage
# This reduces what is persisted to RocksDB

[[streams]]
id     = "orders-active"
source = "pgsql-prod"

  [streams.filter]
  # Tables this stream ingests — subset of the publication
  tables = ["public.orders", "public.accounts"]

  # Operations to ingest
  operations = ["insert", "update"]

  # Row predicate — CEL expression
  # Event is dropped if this evaluates to false
  predicate = "row.status in ['completed', 'failed', 'processing'] && row.amount > 0"

  # Column projection — per table
  # Only these columns are persisted to RocksDB
  # Omit a table entry to retain all its columns
  [streams.filter.columns]
  "public.orders" = [
      "id", "status", "amount", "currency",
      "customer", "created", "updated"
  ]
  "public.accounts" = [
      "id", "balance", "currency", "status", "updated"
  ]
```

### Schema Evolution Handling

When a column is added to a table in PostgreSQL and that column is not in Nendi's projection whitelist, Nendi ignores it silently. When a column is removed from PostgreSQL that is in Nendi's whitelist, Nendi emits a `WARN` log and a `schema_projection_mismatch_total` metric increment, then stores the event without the removed column rather than dropping it. This prevents a DDL change from silently breaking the stream.

```
WARN [nendi::storage::filter] Column 'legacy_field' in projection whitelist
     for 'public.orders' not found in decoded event at LSN 0/3A4F210.
     Storing event without this column. Update stream filter config to resolve.
```

---

## Layer 3 — Consumer Subscription Filter

### What It Does

Layer 3 is evaluated at the **egress boundary** — when Nendi reads events from RocksDB and sends them to a specific consumer over gRPC. Each consumer declares its own filter independently in the `SubscribeRequest`. Nendi applies the filter during the RocksDB range scan before serializing the event for the wire.

This means:

- The full (Layer 2-projected) event is stored once in RocksDB
- Each consumer receives a view of that event shaped to its own declaration
- Ten consumers with ten different filters cost ten filter evaluations per event delivered — not ten storage writes
- Consumers with no filter receive the full stored event at maximum throughput

### Extended SubscribeRequest Proto

```protobuf
syntax = "proto3";
package nendi.stream.v1;

service StreamService {
    // Start a filtered stream from a given offset
    rpc Subscribe(SubscribeRequest) returns (stream ChangeEvent);

    // Acknowledge processed offset — updates registry, enables GC
    rpc CommitOffset(CommitRequest) returns (CommitResponse);

    // Inspect what a consumer's effective filter produces (dry run)
    rpc InspectFilter(SubscribeRequest) returns (FilterInspectResponse);
}

message SubscribeRequest {
    // Stream to subscribe to (must match a configured [[streams]] id)
    string stream_id = 1;

    // Resume from this offset; empty = start from head
    bytes start_offset = 2;

    // Table-level filter — only receive events for these tables
    // Empty list = all tables in the stream
    repeated string table_filters = 3;

    // Column projection per table — consumer only receives listed columns
    // Key: "schema.table", Value: list of column names
    // Empty map = all columns
    map<string, ColumnList> column_projection = 4;

    // Row-level predicate — CEL expression evaluated against each event
    // Empty string = no predicate (all rows pass)
    string row_predicate = 5;

    // Operation types to receive
    // Empty list = all operations
    repeated OpKind operations = 6;

    // Encryption config — if set, sensitive columns are decrypted
    // before delivery using the provided key material
    EncryptionConfig encryption = 7;

    // Delivery mode for this consumer
    DeliveryMode delivery_mode = 8;

    // Maximum events per gRPC message (batching)
    // 0 = server decides (recommended)
    uint32 batch_size = 9;
}

message ColumnList {
    repeated string columns = 1;
}

enum OpKind {
    OP_UNKNOWN  = 0;
    OP_INSERT   = 1;
    OP_UPDATE   = 2;
    OP_DELETE   = 3;
    OP_DDL      = 4;
    OP_TRUNCATE = 5;
}

enum DeliveryMode {
    // Default: deliver as fast as possible; backpressure via HTTP/2 flow control
    DELIVERY_STREAM = 0;
    // Deliver in micro-batches; lower overhead for slow consumers
    DELIVERY_BATCH  = 1;
}

message EncryptionConfig {
    // Encryption scheme for this consumer
    EncryptionScheme scheme = 1;
    // Consumer's public key (for asymmetric schemes)
    bytes public_key = 2;
    // Key ID for key rotation tracking
    string key_id = 3;
}

enum EncryptionScheme {
    SCHEME_NONE        = 0;
    SCHEME_AES256_GCM  = 1;   // Symmetric — key pre-shared out of band
    SCHEME_CHACHA20    = 2;   // Symmetric — preferred for high throughput
    SCHEME_X25519      = 3;   // Asymmetric — key exchange via Curve25519
}

message FilterInspectResponse {
    // Human-readable description of the effective filter
    string description = 1;
    // Estimated percentage of events that will pass this filter
    // based on a sample of the last 10,000 stored events
    float estimated_pass_rate = 2;
    // Any warnings about the filter (e.g., column not in storage projection)
    repeated string warnings = 3;
}
```

### Consumer Filter Evaluation in the Egress Path

```
RocksDB range scan (Seek to start_offset)
      │
      ▼
Raw stored event (rkyv-deserialized)
      │
      ├─ Table filter check (O(1) hashset lookup)
      │   └─ Not in set? → skip, advance iterator
      │
      ├─ Operation type filter (bitmask check)
      │   └─ Not in mask? → skip
      │
      ├─ Row predicate evaluation (CEL)
      │   └─ false? → skip
      │
      ├─ Column projection (field masking)
      │   └─ Build projected view without allocating new event struct
      │      (mask applied over rkyv archived data)
      │
      ├─ Layer 4: Decrypt sensitive columns if EncryptionConfig present
      │
      ▼
Protobuf serialization of projected, decrypted event
      │
      ▼
gRPC send buffer → consumer
```

### Example: Multiple Consumers, One Stream

```
Stream "orders-active" in RocksDB:
  { id, status, amount, currency, customer, email, ssn, created, updated }

Consumer A — Fraud Detection Service
  table_filters:    ["public.orders"]
  operations:       [INSERT, UPDATE]
  column_projection: { "public.orders": ["id", "amount", "currency", "customer"] }
  row_predicate:    "row.amount > 10000"
  encryption:       NONE
  → Receives: high-value orders only, 4 fields, no PII

Consumer B — Compliance Audit Service
  table_filters:    ["public.orders"]
  operations:       [INSERT, UPDATE, DELETE]
  column_projection: {} (empty = all columns)
  row_predicate:    ""  (empty = all rows)
  encryption:       SCHEME_AES256_GCM (has the key — decrypts email, ssn)
  → Receives: all orders, all fields including decrypted PII

Consumer C — Analytics Pipeline
  table_filters:    ["public.orders"]
  operations:       [INSERT]
  column_projection: { "public.orders": ["id", "amount", "currency", "created"] }
  row_predicate:    "row.status == 'completed'"
  encryption:       NONE
  → Receives: completed inserts only, 4 fields, no PII

All three consumers read the same RocksDB log. Zero data duplication.
```

### Filter Inspection Endpoint

Before connecting a production consumer, operators can dry-run a filter against recent stored events to validate it returns what they expect:

```bash
grpcurl -plaintext -d '{
  "stream_id": "orders-active",
  "row_predicate": "row.amount > 10000 && row.status == \"completed\"",
  "column_projection": {
    "public.orders": { "columns": ["id", "amount", "status"] }
  }
}' localhost:50051 nendi.stream.v1.StreamService/InspectFilter

# Response:
# {
#   "description": "Table: public.orders | Ops: ALL | Predicate: row.amount > 10000 && row.status == 'completed' | Columns: [id, amount, status]",
#   "estimated_pass_rate": 0.034,
#   "warnings": []
# }
```

---

## Layer 4 — Column-Level Encryption

### What It Does

Layer 4 allows Nendi to encrypt specific columns **at ingestion time** before storing them in RocksDB, and decrypt them **selectively at egress time** for consumers that hold the appropriate key. Consumers without the key receive the ciphertext — the column is present in their event payload but unreadable. This means:

- Nendi's RocksDB store itself never holds plaintext PII
- Authorization is enforced by key possession, not by ACL configuration in Nendi
- Multiple consumers can have different access levels to the same stored event
- Key rotation is possible without re-streaming historical data (re-encryption of stored events is a background operation)

### Encryption Architecture

```
Ingestion path:
  Decoded ChangeEvent (plaintext row)
        │
        ├─ Identify sensitive columns (from stream config)
        ├─ Encrypt each sensitive column value with the stream's data key (DEK)
        │   AES-256-GCM or ChaCha20-Poly1305
        │   Nonce: derived from LSN + column name (deterministic, unique)
        ├─ Store: { ..., email: <ciphertext>, ssn: <ciphertext>, ... }
        ▼
  RocksDB (no plaintext PII at rest)

Egress path — Consumer WITH key:
  RocksDB event (ciphertext columns)
        │
        ├─ Consumer presents EncryptionConfig with key_id + key material
        ├─ Nendi retrieves DEK from key store (encrypted with consumer's key)
        ├─ Decrypt sensitive columns
        ├─ Deliver plaintext event to consumer
        ▼
  Consumer receives: { ..., email: "user@example.com", ssn: "123-45-6789", ... }

Egress path — Consumer WITHOUT key:
  RocksDB event (ciphertext columns)
        │
        ├─ No EncryptionConfig or wrong key_id
        ├─ Sensitive columns delivered as-is (ciphertext bytes)
        ▼
  Consumer receives: { ..., email: <bytes>, ssn: <bytes>, ... }
```

### Key Management Model

Nendi uses an **envelope encryption** model — the same pattern used by AWS KMS, Google Cloud KMS, and HashiCorp Vault:

```
Key Hierarchy:

  Master Key (KMK)
  — Never stored in Nendi; lives in external KMS or HSM
  — Used only to wrap/unwrap DEKs

  Data Encryption Key (DEK)
  — One per stream (not per consumer, not per row)
  — Encrypted with KMK and stored in Nendi's key column family in RocksDB
  — Rotated on schedule or on demand

  Consumer Key
  — One per consumer
  — Asymmetric (X25519) or pre-shared symmetric key
  — Used to encrypt the DEK for that specific consumer
  — Stored outside Nendi (consumer's responsibility)
```

```toml
# Encryption configuration in stream config

[[streams]]
id     = "orders-active"
source = "pgsql-prod"

  [streams.encryption]
  # Which columns are encrypted at ingestion
  sensitive_columns = ["email", "ssn", "card_number", "phone"]

  # Key management backend
  # "local" = DEK stored in RocksDB (dev/test only — not for production)
  # "vault"  = HashiCorp Vault transit engine
  # "kms"    = AWS KMS or GCP KMS
  backend = "vault"

  [streams.encryption.vault]
  address    = "https://vault.internal:8200"
  mount      = "transit"
  key_name   = "nendi-orders-dek"
  # Token sourced from VAULT_TOKEN environment variable or agent socket

  # Rotation policy
  [streams.encryption.rotation]
  interval_days     = 90
  # When rotated, re-encrypt stored events in background
  reencrypt_stored  = true
  # Keep old key versions for N days to allow in-flight consumers to finish
  retain_old_versions_days = 7
```

### Consumer Key Registration

Consumers register their public key (for asymmetric schemes) or a key ID (for pre-shared symmetric) with Nendi's admin API. Nendi wraps the stream DEK with the consumer's key and stores the wrapped DEK in the consumer registry.

```bash
# Register a consumer's X25519 public key
curl -X POST http://localhost:9090/admin/consumers/fraud-service/keys \
  -H "Content-Type: application/json" \
  -d '{
    "scheme": "X25519",
    "public_key_base64": "base64encodedpublickey==",
    "streams": ["orders-active"],
    "sensitive_columns": ["email"]
  }'

# Response
{
  "consumer": "fraud-service",
  "key_id": "key-20240301-001",
  "wrapped_dek_base64": "base64wrappeddek==",
  "streams": ["orders-active"],
  "granted_columns": ["email"],
  "denied_columns": ["ssn", "card_number", "phone"]
}
```

The consumer stores the `wrapped_dek_base64` and presents `key_id` in its `SubscribeRequest`. Nendi unwraps the DEK using the consumer's public key at connection time, holds it in memory for the session, and uses it to decrypt eligible columns during egress.

### Encryption Scheme Selection

| Scheme | Use Case | Notes |
|---|---|---|
| `AES_256_GCM` | Standard symmetric encryption | Hardware-accelerated on AES-NI CPUs. Good default. |
| `CHACHA20_POLY1305` | High throughput, no AES hardware | ~15% faster than AES-GCM on ARM without AES-NI. Preferred for Graviton/Ampere. |
| `X25519` | Asymmetric key exchange | Consumer never shares a secret with Nendi. Strongest security model. Slightly higher key-wrap cost at session start only. |

Performance impact of encryption on the hot path is approximately 50–150ns per sensitive column per event depending on scheme and CPU. For a row with 3 sensitive columns at 100k events/sec this is 15–45ms of CPU per second — well within budget and largely hidden by I/O wait time.

---

## Layer Interaction & Precedence Rules

When multiple layers are configured, they compose as an AND chain — an event must pass every applicable layer to be delivered to a consumer. Nendi enforces the following precedence rules to prevent configuration conflicts:

**Rule 1: Layer 2 cannot grant what Layer 1 has denied.**
If a table is excluded from the PostgreSQL publication, it cannot be added back by a Nendi stream filter. The publication is the ground truth for what Nendi receives.

**Rule 2: Layer 3 cannot grant what Layer 2 has removed.**
If Layer 2 projects away column `ssn` before storage, Layer 3 cannot deliver `ssn` to a consumer because it was never stored. Nendi detects this at subscription time and returns a `FILTER_CONFLICT` error with a descriptive message.

```
ERROR [nendi::egress::grpc] Consumer 'audit-service' requested column 'ssn'
      in subscription filter for stream 'orders-active', but 'ssn' is not
      present in the stream's storage projection. Either add 'ssn' to the
      stream's ingestion column whitelist, or remove it from the subscription filter.
      Consumer connection rejected.
```

**Rule 3: Layer 4 encryption is applied after Layer 3 projection.**
If Layer 3 projects away a sensitive column, Layer 4 does not attempt to decrypt it — there is nothing to decrypt. If Layer 3 includes a sensitive column, Layer 4 decrypts it if the consumer holds the correct key, otherwise delivers ciphertext.

**Rule 4: Ingestion predicates are evaluated on the full pre-projection row.**
Layer 2's row predicate has access to all decoded columns including those that will be projected away. This allows filtering on a column like `internal_flag` without storing `internal_flag` in RocksDB.

```toml
# Example: filter on a column that is not stored
[streams.filter]
predicate = "row.internal_flag == false && row.status == 'active'"

[streams.filter.columns]
"public.orders" = ["id", "status", "amount", "updated"]
# internal_flag is used in predicate but NOT in the column list
# It is checked before projection, then discarded
```

---

## Performance Characteristics

### End-to-End Cost at 100,000 Messages/Second

| Layer | Operation | Cost Per Message | Total CPU/sec |
|---|---|---|---|
| 1 | PostgreSQL publication check | 0 (PostgreSQL internal) | 0 |
| 2 | Row predicate (simple, 2 conditions) | ~30ns | 3ms |
| 2 | Column projection (strip 5 of 15 cols) | ~25ns | 2.5ms |
| 2 | Reduced rkyv serialization size | −40ns (net saving) | −4ms |
| 3 | Table filter (hashset lookup) | ~5ns | 0.5ms |
| 3 | Op type filter (bitmask) | ~2ns | 0.2ms |
| 3 | Row predicate (per consumer) | ~30ns × N consumers | 3ms × N |
| 3 | Column projection at egress | ~20ns | 2ms |
| 4 | AES-256-GCM per sensitive column | ~60ns × C columns | 6ms × C |

For a typical deployment with 10 consumers, 2 sensitive columns, and simple predicates, the total filtering overhead across all layers is approximately **35–50ms of CPU per second** against a baseline of several hundred milliseconds for I/O operations. The filtering stack is not the bottleneck.

### Storage Reduction from Layers 1 and 2

The combination of publication filtering and ingestion column projection typically reduces stored event size by 30–70% compared to storing full rows. This directly reduces:

- RocksDB write amplification (fewer bytes per compaction pass)
- Block cache pressure (more events fit in the same cache size)
- Egress serialization time (smaller payloads to frame and send)
- Network bandwidth to consumers (even before Layer 3 projection)

---

## Configuration Reference

### Complete Example

```toml
# config/default.toml

# -------------------------------------------------------------------
# Source database connection
# -------------------------------------------------------------------
[sources.pgsql-prod]
type     = "pgsql"
host     = "db.internal"
port     = 5432
database = "appdb"
user     = "nendi_replication"
# Password sourced from environment: NENDI_PGSQL_PROD_PASSWORD

  # Layer 1: Publication configuration
  [sources.pgsql-prod.publication]
  name       = "nendi_pub"
  mode       = "managed"
  tables     = ["public.orders", "public.accounts", "public.payments"]
  operations = ["insert", "update", "delete"]

  [sources.pgsql-prod.publication.row_filters]
  "public.orders"   = "status != 'draft'"
  "public.payments" = "amount > 0"

# -------------------------------------------------------------------
# Stream definition — one logical stream backed by one source
# -------------------------------------------------------------------
[[streams]]
id     = "orders-active"
source = "pgsql-prod"

  # Layer 2: Ingestion filter (pre-storage)
  [streams.filter]
  tables     = ["public.orders", "public.accounts"]
  operations = ["insert", "update"]
  predicate  = "row.status in ['completed', 'failed', 'processing']"

  [streams.filter.columns]
  "public.orders" = [
      "id", "status", "amount", "currency",
      "customer", "email", "ssn", "created", "updated"
  ]
  "public.accounts" = [
      "id", "balance", "currency", "status", "updated"
  ]

  # Layer 4: Encryption (applied at ingestion — stored encrypted)
  [streams.encryption]
  sensitive_columns = ["email", "ssn"]
  backend           = "vault"

  [streams.encryption.vault]
  address  = "https://vault.internal:8200"
  mount    = "transit"
  key_name = "nendi-orders-dek"

  [streams.encryption.rotation]
  interval_days            = 90
  reencrypt_stored         = true
  retain_old_versions_days = 7

# -------------------------------------------------------------------
# Consumer registry
# Layer 3 filters are declared by the consumer in SubscribeRequest
# Consumer key grants (Layer 4) are registered via admin API
# -------------------------------------------------------------------
[consumers]

  [consumers.fraud-service]
  stream = "orders-active"
  # Layer 4 key grant — managed via admin API
  # Columns this consumer is authorized to decrypt
  decrypt_columns = ["email"]

  [consumers.audit-service]
  stream = "orders-active"
  decrypt_columns = ["email", "ssn"]

  [consumers.analytics-pipeline]
  stream = "orders-active"
  decrypt_columns = []  # no PII access
```

---

## Predicate Expression Language

Nendi uses **Google Common Expression Language (CEL)** for all row predicates across Layers 2 and 3. CEL is chosen because it is safe by construction (no side effects, deterministic, bounded execution), portable, and has an existing Rust implementation via the `cel-interpreter` crate.

### Available Variables

Inside any predicate expression, the following variables are in scope:

| Variable | Type | Description |
|---|---|---|
| `row` | map | The decoded row as a key-value map. Access fields as `row.field_name`. |
| `op` | string | Operation type: `"insert"`, `"update"`, `"delete"`, `"ddl"` |
| `table` | string | Fully qualified table name: `"public.orders"` |
| `lsn` | uint | The LSN of this event |
| `old` | map | Previous row values (only populated for UPDATE events) |

### Type Coercion

PostgreSQL types are coerced to CEL types as follows:

| PostgreSQL Type | CEL Type | Notes |
|---|---|---|
| `integer`, `bigint`, `smallint` | `int` | |
| `numeric`, `decimal`, `float` | `double` | Precision may be lost for very large numerics |
| `text`, `varchar`, `char` | `string` | |
| `boolean` | `bool` | |
| `timestamp`, `timestamptz` | `timestamp` | CEL native timestamp type |
| `uuid` | `string` | Represented as UUID string format |
| `jsonb`, `json` | `map` / `list` | Parsed into CEL map or list |
| `enum` | `string` | Enum label as string |
| `array` | `list` | |
| `null` | `null` | Check with `row.field == null` |

### Example Predicates

```cel
# Simple value check
row.status == "completed"

# Numeric range
row.amount > 1000 && row.amount < 1000000

# Null check
row.deleted_at == null

# Enum membership
row.status in ["completed", "failed", "refunded"]

# String operations
row.currency.startsWith("US")

# Timestamp comparison
row.created > timestamp("2024-01-01T00:00:00Z")

# Update-only: check if a specific field changed
op == "update" && row.status != old.status

# JSON field access
row.metadata["source"] == "mobile_app"

# Combined cross-table predicate (evaluated independently per event)
table == "public.orders" && row.amount > 10000
|| table == "public.accounts" && row.status == "suspended"
```

### CEL Safety Guarantees

All predicates in Nendi are compiled and type-checked at stream/consumer registration time, not at evaluation time. If a predicate references a column that does not exist in the table schema, Nendi rejects the configuration at startup with a descriptive error. At evaluation time, the compiled predicate runs in a sandboxed context with:

- No access to external state or I/O
- Bounded iteration (no unbounded loops)
- Maximum evaluation time of 1ms (configurable); predicate is treated as `false` if exceeded and a metric is incremented
- No allocations beyond the predicate's own output value

---

## Implementation Roadmap

### Phase 1 — Foundation (Months 1–2)

Deliver table-level filtering and operation-type filtering only. This is the minimal viable selective streaming.

- Implement `table_filters` and `operations` in `SubscribeRequest` proto
- Implement table-level filter evaluation in `core/src/egress/grpc/stream.rs`
- Implement publication management in `services/pgsql/replication/slot.rs`
- Add `[sources.*.publication]` config block support
- Add `filter_pass_total` and `filter_drop_total` Prometheus metrics per stream

**Files to create or modify:**

```
proto/stream/service.proto          # Add table_filters, operations fields
proto/stream/types.proto            # Add OpKind enum
services/pgsql/replication/slot.rs  # Publication create/validate logic
core/src/egress/grpc/stream.rs      # Table + op filter evaluation
config/default.toml                 # Publication config block
```

### Phase 2 — Column Projection (Month 3)

Add column-level projection at both ingestion (Layer 2) and egress (Layer 3).

- Implement `[streams.filter.columns]` config parsing
- Apply column projection in `core/src/pipeline/ingestion.rs` before `WriteBatch`
- Implement `column_projection` field in `SubscribeRequest`
- Apply egress projection in `core/src/egress/grpc/stream.rs`
- Add `FilterInspectResponse` and `InspectFilter` RPC
- Emit `schema_projection_mismatch_total` metric on column-not-found

**Files to create or modify:**

```
core/src/pipeline/ingestion.rs      # Pre-storage column projection
core/src/egress/grpc/stream.rs      # Egress column projection
shared/src/event/envelope.rs        # ProjectedChangeEvent type
```

### Phase 3 — Row Predicates (Months 4–5)

Integrate CEL predicate evaluation at both ingestion and egress.

- Add `cel-interpreter` crate dependency
- Implement `core/src/pipeline/filter.rs` — predicate compiler and evaluator
- Apply ingestion predicate in `core/src/pipeline/ingestion.rs`
- Apply egress predicate in `core/src/egress/grpc/stream.rs`
- Implement predicate type-checking against schema registry at startup
- Add predicate evaluation timeout (1ms) with metric on timeout

**Files to create:**

```
core/src/pipeline/filter.rs         # CEL engine integration
core/src/pipeline/predicate.rs      # Type coercion PostgreSQL → CEL
```

### Phase 4 — Column Encryption (Months 5–6)

Implement envelope encryption with Vault backend and per-consumer key grants.

- Implement `core/src/storage/encryption/` module
- Implement DEK generation, wrapping, storage in key column family
- Implement Vault transit backend client
- Apply encryption in ingestion path, decryption in egress path
- Implement consumer key registration via admin API
- Add key rotation background task
- Add `CHACHA20_POLY1305` and `X25519` scheme implementations

**Files to create:**

```
core/src/storage/
  encryption/
    mod.rs
    dek.rs              # Data encryption key management
    envelope.rs         # Envelope encryption (wrap/unwrap DEK)
    vault.rs            # Vault transit backend
    kms.rs              # AWS/GCP KMS backend (stub for later)

core/src/egress/grpc/
  decrypt.rs            # Per-consumer column decryption at egress
```

---

## Testing Strategy

### Layer 1 — Integration Tests

```
tests/integration/src/pgsql/publication.rs

- test_publication_created_on_connect
- test_table_not_in_publication_never_received
- test_operation_filter_excludes_deletes
- test_pg15_row_filter_excludes_draft_orders
- test_publication_alter_takes_effect_without_restart
```

### Layer 2 — Unit + Integration Tests

```
tests/integration/src/pipeline/filter.rs

- test_ingestion_predicate_drops_non_matching_rows
- test_column_projection_strips_excluded_fields
- test_predicate_uses_pre_projection_column
- test_schema_mismatch_emits_warn_not_drop
- test_null_column_in_predicate_evaluates_safely
```

### Layer 3 — Integration Tests

```
tests/integration/src/egress/filter.rs

- test_consumer_table_filter_receives_only_subscribed_tables
- test_consumer_column_projection_strips_fields
- test_consumer_predicate_filters_rows
- test_two_consumers_different_projections_same_stream
- test_inspect_filter_returns_accurate_pass_rate
- test_projection_conflict_returns_error_on_subscribe
```

### Layer 4 — Integration Tests

```
tests/integration/src/storage/encryption.rs

- test_sensitive_columns_stored_as_ciphertext
- test_consumer_with_key_receives_plaintext
- test_consumer_without_key_receives_ciphertext
- test_wrong_key_id_rejected_on_subscribe
- test_key_rotation_reencrypts_stored_events
- test_chacha20_and_aes256_both_produce_decryptable_output
```

### Benchmark Targets

```
tests/bench/src/filter.rs

- bench_ingestion_predicate_simple        # Target: < 50ns/msg
- bench_ingestion_predicate_complex       # Target: < 200ns/msg
- bench_column_projection_10_of_20_cols   # Target: < 30ns/msg
- bench_egress_predicate_10_consumers     # Target: < 500ns total/msg
- bench_aes256_gcm_per_column             # Target: < 80ns/col
- bench_chacha20_per_column               # Target: < 60ns/col
```
