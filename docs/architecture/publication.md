# Nendi — Config-Driven Publication Management

> **Document location:** `docs/architecture/publication.md`
> **Status:** Approved for implementation
> **Replaces:** Manual SQL publication setup described in earlier drafts
> **Principle:** The developer never writes publication SQL. The TOML config
> is the single source of truth. Nendi owns the publication lifecycle entirely.

---

## Table of Contents

1. [Design Philosophy](#design-philosophy)
2. [How It Works Internally](#how-it-works-internally)
3. [Configuration Reference](#configuration-reference)
4. [Lifecycle Management](#lifecycle-management)
5. [Schema Change Handling](#schema-change-handling)
6. [Conflict Detection and Recovery](#conflict-detection-and-recovery)
7. [Multi-Source Configuration](#multi-source-configuration)
8. [Environment-Specific Overrides](#environment-specific-overrides)
9. [Admin API for Runtime Changes](#admin-api-for-runtime-changes)
10. [What Gets Generated Under the Hood](#what-gets-generated-under-the-hood)
11. [Implementation Guide](#implementation-guide)

---

## Design Philosophy

The developer using Nendi knows their domain — their tables, their columns, their
business rules. They should not need to know PostgreSQL publication syntax, replication
slot internals, or WAL decoding semantics. All of that is Nendi's problem.

The contract is simple:

```
Developer declares intent in TOML
          ↓
Nendi figures out the SQL
          ↓
Nendi creates, updates, and validates the publication automatically
          ↓
Developer gets a stream
```

If the developer changes their TOML and restarts Nendi (or hits the reload endpoint),
Nendi reconciles the live PostgreSQL publication with the new config. The developer
never opens a psql session to manage publications.

---

## How It Works Internally

At startup, Nendi runs a **publication reconciler** as part of the connector
initialization sequence. The reconciler follows this logic:

```
1. Read [sources.*.publication] from config
2. Connect to PostgreSQL as the replication user
3. Check if the named publication exists
   ├─ Does not exist → CREATE PUBLICATION with config values
   └─ Exists → Compare live publication against config
       ├─ Matches → proceed, no action needed
       ├─ Diverges → ALTER PUBLICATION to match config
       └─ Irreconcilable → emit error, refuse to start
4. Validate replication slot exists for this publication
   ├─ Does not exist → CREATE_REPLICATION_SLOT
   └─ Exists → validate it references the correct plugin (pgoutput)
5. Begin streaming
```

This happens before any WAL bytes are consumed, so the developer's config is
always in effect before the first event is processed.

---

## Configuration Reference

### Minimal Config

The smallest possible publication config — stream everything from one table:

```toml
[sources.primary]
type     = "pgsql"
host     = "localhost"
port     = 5432
database = "myapp"
user     = "nendi"

  [sources.primary.publication]
  name   = "nendi_pub"
  tables = ["public.users"]
```

That is all. Nendi handles the rest.

---

### Full Config Reference

```toml
# -----------------------------------------------------------------------
# Source connection
# -----------------------------------------------------------------------
[sources.primary]
type     = "pgsql"
host     = "db.internal"
port     = 5432
database = "appdb"
user     = "nendi_replication"
# Password: set NENDI_SOURCE_PRIMARY_PASSWORD env variable
# Never put passwords in config files

  # -----------------------------------------------------------------------
  # Publication block — Nendi manages this in PostgreSQL automatically
  # -----------------------------------------------------------------------
  [sources.primary.publication]

  # Name of the PostgreSQL publication Nendi will create and own
  name = "nendi_pub"

  # "managed"  — Nendi creates, alters, and owns the publication (default)
  # "external" — Nendi assumes the publication exists and does not touch it
  #              Use this if your DBA manages publications separately
  mode = "managed"

  # Tables to include — use "schema.table" format
  # Add as many tables as needed; Nendi handles all of them
  tables = [
      "public.orders",
      "public.accounts",
      "public.payments",
      "public.products",
      "public.customers",
  ]

  # Operations to capture per table
  # Options: "insert" | "update" | "delete" | "truncate"
  # This applies to ALL tables unless overridden per-table below
  operations = ["insert", "update", "delete"]

  # -----------------------------------------------------------------------
  # Per-table overrides
  # Use this when different tables need different operation sets
  # or different column selections at the publication level
  # -----------------------------------------------------------------------
  [[sources.primary.publication.overrides]]
  table      = "public.audit_log"
  operations = ["insert"]           # Audit log is append-only — no updates/deletes

  [[sources.primary.publication.overrides]]
  table      = "public.sessions"
  operations = ["insert", "delete"] # Don't care about session updates

  [[sources.primary.publication.overrides]]
  table      = "public.products"
  operations = ["insert", "update", "delete"]

  # -----------------------------------------------------------------------
  # Row-level filters (PostgreSQL 15+ only)
  # Nendi detects PG version automatically and skips this block on PG14 and below
  # with a warning rather than an error
  # -----------------------------------------------------------------------
  [[sources.primary.publication.row_filters]]
  table  = "public.orders"
  where  = "status != 'draft'"       # Never stream draft orders

  [[sources.primary.publication.row_filters]]
  table  = "public.payments"
  where  = "amount > 0"              # Skip zero-amount payment records

  [[sources.primary.publication.row_filters]]
  table  = "public.customers"
  where  = "deleted_at IS NULL"      # Skip soft-deleted customers

  # -----------------------------------------------------------------------
  # Slot configuration
  # -----------------------------------------------------------------------
  [sources.primary.slot]
  name   = "nendi_slot"
  plugin = "pgoutput"               # "pgoutput" (default) or "wal2json" (debug only)

  # If true, Nendi drops and recreates the slot on startup if it detects
  # the slot's LSN is too far behind to be useful (WAL bloat prevention).
  # If false, Nendi halts with an error instead. Default: false (safer)
  allow_recreate = false
```

---

### Stream-Level Filter Config

The publication config controls what PostgreSQL decodes and sends. The stream
filter config controls what Nendi stores. They are separate concerns — the
publication is the outer gate, the stream filter is the inner gate.

```toml
# -----------------------------------------------------------------------
# Stream definition — one logical stream, one source
# -----------------------------------------------------------------------
[[streams]]
id     = "orders"
source = "primary"

  # What to store in RocksDB (subset of what the publication allows)
  [streams.filter]

  # Only ingest these tables from the source
  # Must be a subset of the tables in [sources.primary.publication]
  tables = ["public.orders", "public.payments"]

  # Only ingest these operation types
  operations = ["insert", "update"]

  # Row predicate — CEL expression
  # Evaluated after publication filter, before storage
  # Event dropped here is never written to RocksDB
  predicate = "row.status in ['completed', 'processing', 'failed']"

  # Column projection per table
  # Only listed columns are stored — all others are stripped before RocksDB write
  # Omit a table to store all its columns
  [streams.filter.columns]
  "public.orders" = [
      "id",
      "status",
      "amount",
      "currency",
      "customer",
      "created",
      "updated",
  ]
  "public.payments" = [
      "id",
      "order_id",
      "status",
      "amount",
      "method",
      "created",
  ]

# -----------------------------------------------------------------------
# A second stream from the same source — different tables, different focus
# -----------------------------------------------------------------------
[[streams]]
id     = "customers"
source = "primary"

  [streams.filter]
  tables     = ["public.customers", "public.accounts"]
  operations = ["insert", "update", "delete"]
  predicate  = ""   # Empty = no predicate, all rows pass

  [streams.filter.columns]
  "public.customers" = ["id", "email", "plan", "created", "updated"]
  "public.accounts"  = ["id", "customer", "balance", "currency", "status"]
```

---

## Lifecycle Management

### What Nendi Does on First Start

```
Config says: tables = ["public.orders", "public.accounts"]

Nendi executes internally:
  CREATE PUBLICATION nendi_pub
      FOR TABLE public.orders, public.accounts
      WITH (publish = 'insert, update, delete');

  -- If PG15+ and row_filters are configured:
  -- (Nendi rewrites as ALTER after initial CREATE)
  ALTER PUBLICATION nendi_pub
      SET TABLE
          public.orders WHERE (status != 'draft'),
          public.accounts;

  CREATE_REPLICATION_SLOT nendi_slot
      LOGICAL pgoutput
      NOEXPORT_SNAPSHOT;
```

The developer sees none of this. Nendi logs it at `DEBUG` level.

---

### What Nendi Does When Config Changes

When the developer adds a table to their TOML and either restarts Nendi or
hits the reload endpoint, Nendi reconciles the difference:

```
Old config: tables = ["public.orders", "public.accounts"]
New config: tables = ["public.orders", "public.accounts", "public.products"]

Nendi computes diff:
  Added:   ["public.products"]
  Removed: []

Nendi executes internally:
  ALTER PUBLICATION nendi_pub ADD TABLE public.products;

Log output:
  INFO [nendi::pgsql::publication] Reconciled publication 'nendi_pub':
       added 1 table(s) [public.products], removed 0 table(s).
```

Removing a table:

```
Old config: tables = ["public.orders", "public.accounts", "public.products"]
New config: tables = ["public.orders", "public.accounts"]

Nendi executes internally:
  ALTER PUBLICATION nendi_pub DROP TABLE public.products;

Log output:
  INFO [nendi::pgsql::publication] Reconciled publication 'nendi_pub':
       added 0 table(s), removed 1 table(s) [public.products].
  WARN [nendi::pgsql::publication] Table 'public.products' removed from publication.
       Events already stored in RocksDB for this table are NOT deleted.
       Consumers subscribed to 'public.products' will continue to receive
       stored events but will receive no new events going forward.
       To remove stored events, use the admin API: DELETE /admin/streams/{id}/tables/products
```

This warning is intentional — Nendi never silently deletes stored data.

---

### Startup Validation Errors

If Nendi detects a config that conflicts with the live PostgreSQL state in a
way it cannot automatically resolve, it refuses to start and explains exactly
what is wrong:

```
ERROR [nendi::pgsql::publication] Publication 'nendi_pub' exists in PostgreSQL
      but was not created by Nendi (no nendi_managed marker in publication comment).
      To let Nendi manage this publication, either:
        (a) Set mode = "external" in config to use it read-only, or
        (b) Drop the publication manually and restart Nendi to recreate it:
            DROP PUBLICATION nendi_pub;
      Refusing to start to avoid overwriting a publication Nendi does not own.
```

```
ERROR [nendi::pgsql::publication] Config declares table 'public.nonexistent'
      in source 'primary', but this table does not exist in database 'appdb'.
      Fix the table name in config or create the table before starting Nendi.
      Refusing to start with invalid publication config.
```

```
WARN  [nendi::pgsql::publication] Row filters are configured for source 'primary'
      but the connected PostgreSQL version is 14.8, which does not support
      publication row filters (requires 15+). Row filters will be ignored.
      All rows from configured tables will be ingested. Use stream-level
      predicates in [streams.filter] to filter rows after ingestion instead.
```

---

## Schema Change Handling

When the developer adds a column to a table in PostgreSQL, no Nendi config
change is required. PostgreSQL automatically includes new columns in the
publication for that table. Nendi's schema registry detects the new column
via the DDL event trigger and updates its fingerprint map.

The stream filter column whitelist in TOML determines whether the new column
is stored. If the column is not in the whitelist, it is silently dropped at
ingestion. If the developer wants the new column stored, they add it to the
TOML and reload — no PostgreSQL interaction required.

```
Developer adds column 'discount_amount' to public.orders in PostgreSQL
      │
      ├─ DDL trigger fires → Nendi receives DDL event → schema registry updated
      │
      ├─ Stream filter column list does NOT include 'discount_amount'
      │   └─ New column is stripped at ingestion. Nothing breaks.
      │
      └─ Developer adds 'discount_amount' to [streams.filter.columns] in TOML
          └─ Reload config → new column stored from this point forward
             (historical events before reload do not have this column)
```

---

## Conflict Detection and Recovery

### Slot Invalidation

If a replication slot is invalidated by PostgreSQL (WAL cleanup forced the
slot's LSN forward beyond what Nendi can catch up from), Nendi detects this
on startup and enters a controlled halt:

```
ERROR [nendi::pgsql::slot] Replication slot 'nendi_slot' has been invalidated
      by PostgreSQL. The slot's restart_lsn (0/3A4F210) is no longer available
      in the WAL. This typically means the slot fell too far behind and
      PostgreSQL cleaned up the WAL it was protecting.

      Options:
        (a) Set allow_recreate = true in [sources.primary.slot] to drop and
            recreate the slot. This will lose all events between the last stored
            LSN and the current DB head LSN. Consumers will need to resync.
        (b) Restore from a PostgreSQL backup that includes the missing WAL.

      Current head LSN:    0/4B2A110
      Last stored LSN:     0/3A4F200
      Gap:                 ~267MB of WAL

      Refusing to start. Set allow_recreate = true to proceed with data loss,
      or resolve the WAL gap manually.
```

### Publication Drift Detection

If a managed publication is found to have drifted from config at startup
(e.g., someone ran `ALTER PUBLICATION` manually on the database), Nendi
logs a warning, applies the reconciliation, and continues:

```
WARN [nendi::pgsql::publication] Live publication 'nendi_pub' diverges from config:
     Extra tables in live publication (not in config): [public.legacy_events]
     Missing tables from live publication (in config): []

     Reconciling: DROP TABLE public.legacy_events from publication.
     If this table was added intentionally, add it to the config TOML.
```

---

## Multi-Source Configuration

When Nendi connects to multiple PostgreSQL instances, each source gets
its own independent publication and slot. The config mirrors this naturally:

```toml
# -----------------------------------------------------------------------
# Source 1: Main application database
# -----------------------------------------------------------------------
[sources.app]
type     = "pgsql"
host     = "app-db.internal"
port     = 5432
database = "appdb"
user     = "nendi"

  [sources.app.publication]
  name   = "nendi_app_pub"
  mode   = "managed"
  tables = ["public.orders", "public.customers", "public.products"]

  [sources.app.slot]
  name = "nendi_app_slot"

# -----------------------------------------------------------------------
# Source 2: Analytics staging database (read replica)
# -----------------------------------------------------------------------
[sources.analytics]
type     = "pgsql"
host     = "analytics-db.internal"
port     = 5432
database = "analyticsdb"
user     = "nendi"

  [sources.analytics.publication]
  name   = "nendi_analytics_pub"
  mode   = "managed"
  tables = ["reporting.fact_sales", "reporting.dim_customers"]
  operations = ["insert"]   # Analytics tables are insert-only

  [sources.analytics.slot]
  name = "nendi_analytics_slot"

# -----------------------------------------------------------------------
# Source 3: Legacy database — DBA-managed publication, Nendi reads only
# -----------------------------------------------------------------------
[sources.legacy]
type     = "pgsql"
host     = "legacy-db.internal"
port     = 5432
database = "legacydb"
user     = "nendi_readonly"

  [sources.legacy.publication]
  name = "existing_pub"
  mode = "external"   # Nendi does not touch this publication

  [sources.legacy.slot]
  name = "nendi_legacy_slot"

# -----------------------------------------------------------------------
# Streams backed by different sources
# -----------------------------------------------------------------------
[[streams]]
id     = "live-orders"
source = "app"

[[streams]]
id     = "analytics-sales"
source = "analytics"

[[streams]]
id     = "legacy-data"
source = "legacy"
```

---

## Environment-Specific Overrides

The base config defines the full table list and structure. Environment
overrides narrow it down for dev/test without duplicating the entire config.

```toml
# config/default.toml — full production publication
[sources.primary.publication]
name   = "nendi_pub"
tables = [
    "public.orders",
    "public.accounts",
    "public.payments",
    "public.customers",
    "public.products",
    "public.inventory",
    "public.shipments",
]
```

```toml
# config/development.toml — narrowed for local dev
# Only override what changes; everything else inherits from default.toml
[sources.primary.publication]
tables = [
    "public.orders",
    "public.customers",
]
# Fewer tables means faster dev startup and less noise in local logs
```

```toml
# config/testing.toml — minimal for integration test harness
[sources.primary.publication]
name   = "nendi_test_pub"  # Separate publication name avoids conflicts
tables = ["public.orders"]

[sources.primary.slot]
name           = "nendi_test_slot"
allow_recreate = true  # Tests need clean slate on each run
```

Nendi loads configs in this order with later files winning:

```
default.toml → {environment}.toml → environment variables
```

The environment is set via:

```bash
NENDI_ENV=development ./nendi
NENDI_ENV=testing ./nendi
NENDI_ENV=production ./nendi   # default if unset
```

---

## Admin API for Runtime Changes

For production deployments where a restart is undesirable, config changes
can be applied live via the admin API. Nendi re-reads the config file and
reconciles any differences without interrupting active consumer streams.

```bash
# Reload config from disk and reconcile all publications
curl -X POST http://localhost:9090/admin/config/reload

# Response
{
  "status": "ok",
  "reconciled": {
    "sources": {
      "primary": {
        "publication": {
          "added_tables": ["public.refunds"],
          "removed_tables": [],
          "updated_row_filters": ["public.orders"]
        }
      }
    }
  },
  "streams_affected": ["orders", "customers"],
  "consumers_notified": 4,
  "duration_ms": 42
}
```

```bash
# Inspect the live publication state vs config without applying changes
curl http://localhost:9090/admin/sources/primary/publication/diff

# Response
{
  "source": "primary",
  "publication": "nendi_pub",
  "status": "diverged",
  "diff": {
    "in_config_not_in_pg": ["public.refunds"],
    "in_pg_not_in_config": [],
    "row_filter_mismatch": [
      {
        "table": "public.orders",
        "config_filter": "status != 'draft'",
        "live_filter":   "status != 'draft' AND amount > 0"
      }
    ]
  }
}
```

```bash
# Check publication health for all sources
curl http://localhost:9090/admin/sources/publication/status

# Response
{
  "sources": [
    {
      "id": "primary",
      "publication": "nendi_pub",
      "slot": "nendi_slot",
      "status": "healthy",
      "slot_lag_bytes": 2048,
      "tables_in_publication": 7,
      "tables_in_config": 7,
      "diverged": false,
      "pg_version": "16.2"
    }
  ]
}
```

---

## What Gets Generated Under the Hood

This section is for curiosity and debugging only. Developers should never
need to run these statements manually.

### What Nendi generates for a typical config

**Config input:**

```toml
[sources.primary.publication]
name       = "nendi_pub"
mode       = "managed"
tables     = ["public.orders", "public.accounts", "public.payments"]
operations = ["insert", "update", "delete"]

[[sources.primary.publication.row_filters]]
table = "public.orders"
where = "status != 'draft'"
```

**SQL generated (first start, PostgreSQL 15+):**

```sql
-- Step 1: Create publication with tables and operations
CREATE PUBLICATION nendi_pub
    FOR TABLE public.orders, public.accounts, public.payments
    WITH (publish = 'insert, update, delete');

-- Step 2: Apply row filters (ALTER required after initial CREATE)
ALTER PUBLICATION nendi_pub
    SET TABLE
        public.orders WHERE (status != 'draft'),
        public.accounts,
        public.payments;

-- Step 3: Mark publication as Nendi-managed (stored in publication comment)
COMMENT ON PUBLICATION nendi_pub IS 'nendi_managed:true source:primary';

-- Step 4: Create replication slot
SELECT pg_create_logical_replication_slot('nendi_slot', 'pgoutput');
```

**SQL generated (PostgreSQL 14 — no row filters):**

```sql
CREATE PUBLICATION nendi_pub
    FOR TABLE public.orders, public.accounts, public.payments
    WITH (publish = 'insert, update, delete');

COMMENT ON PUBLICATION nendi_pub IS 'nendi_managed:true source:primary';

SELECT pg_create_logical_replication_slot('nendi_slot', 'pgoutput');

-- Row filters declared in config are skipped with this warning:
-- WARN [nendi::pgsql::publication] PostgreSQL 14 does not support
--      publication row filters. Configure stream-level predicates
--      in [streams.filter.predicate] to achieve row-level filtering instead.
```

**SQL generated (config update — add one table):**

```sql
ALTER PUBLICATION nendi_pub ADD TABLE public.products;
```

**SQL generated (config update — remove a table):**

```sql
ALTER PUBLICATION nendi_pub DROP TABLE public.payments;
```

**SQL generated (config update — change row filter):**

```sql
-- Must SET TABLE for all tables to update one filter (PostgreSQL limitation)
ALTER PUBLICATION nendi_pub
    SET TABLE
        public.orders WHERE (status != 'draft' AND amount > 0),
        public.accounts,
        public.products;
```

---

## Implementation Guide

### Files Involved

```
services/pgsql/
└── src/
    └── publication/
        ├── mod.rs
        ├── manager.rs      # Top-level reconciler — owns the full lifecycle
        ├── reconcile.rs    # Diff computation: config vs live PostgreSQL state
        ├── generate.rs     # SQL statement generator from config structs
        ├── validate.rs     # Pre-flight validation (table exists, PG version, etc.)
        └── version.rs      # PostgreSQL version detection and feature gating
```

### Core Trait

```rust
#[async_trait]
pub trait PublicationManager: Send + Sync {
    /// Run on startup — create or reconcile the publication to match config
    async fn reconcile(&self, config: &PublicationConfig) -> Result<ReconcileReport, PubError>;

    /// Run on config reload — compute diff and apply minimal SQL changes
    async fn apply_diff(&self, old: &PublicationConfig, new: &PublicationConfig)
        -> Result<ReconcileReport, PubError>;

    /// Inspect live state vs config without applying changes
    async fn diff(&self, config: &PublicationConfig) -> Result<PublicationDiff, PubError>;

    /// Validate config before touching PostgreSQL
    async fn validate(&self, config: &PublicationConfig) -> Result<(), Vec<ValidationError>>;

    /// Drop the publication and slot — used in teardown and allow_recreate
    async fn teardown(&self, config: &PublicationConfig) -> Result<(), PubError>;
}
```

### Config Struct

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PublicationConfig {
    pub name: String,
    #[serde(default = "default_mode")]
    pub mode: PublicationMode,
    pub tables: Vec<String>,
    #[serde(default = "default_operations")]
    pub operations: Vec<OpFilter>,
    #[serde(default)]
    pub overrides: Vec<TableOverride>,
    #[serde(default)]
    pub row_filters: Vec<RowFilter>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub enum PublicationMode {
    #[default]
    Managed,
    External,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableOverride {
    pub table: String,
    pub operations: Vec<OpFilter>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RowFilter {
    pub table: String,
    #[serde(rename = "where")]
    pub predicate: String,
}

#[derive(Debug, Clone, Deserialize)]
pub enum OpFilter {
    #[serde(rename = "insert")]   Insert,
    #[serde(rename = "update")]   Update,
    #[serde(rename = "delete")]   Delete,
    #[serde(rename = "truncate")] Truncate,
}
```

### Reconcile Report

```rust
#[derive(Debug)]
pub struct ReconcileReport {
    pub publication_name: String,
    pub action_taken: ReconcileAction,
    pub added_tables: Vec<String>,
    pub removed_tables: Vec<String>,
    pub updated_filters: Vec<String>,
    pub pg_version: PgVersion,
    pub row_filters_skipped: bool,   // true if PG < 15
    pub duration: Duration,
}

#[derive(Debug)]
pub enum ReconcileAction {
    Created,      // Publication did not exist — created fresh
    Reconciled,   // Existed, changes applied
    NoOp,         // Existed, already matched config
    ExternalSkip, // mode = "external" — read-only, no changes made
}
```
