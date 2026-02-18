# Nendi Naming & Design Guide

## Naming Conventions

### General

- **Short names**: Use `orgs` not `organizations`, `pub` not `publication`
- **No underscores in timestamps**: Use `created`, `updated`, `started`, `completed`
- **Direct FK names**: Reference table name directly — `consumer`, `user`, `org`
- **Use `kind` not `type`**: Enum fields use `kind` suffix — `op_kind`, `variant_kind`
- **Generated hex IDs**: 64-char hex with 3-char prefix (e.g., `U0X`, `F0X`)

### PostgreSQL Columns

| ✗ Avoid | ✓ Prefer |
|---------|----------|
| `consumer_id` | `consumer` |
| `created_at` | `created` |
| `updated_at` | `updated` |
| `started_at` | `started` |
| `table_name` | `table` |
| `slot_name` | `slot` |
| `event_type` | `event_kind` |
| `column_count` | `columns` |

### Rust

- Follow standard `snake_case` for variables/functions (language convention)
- Use `pub mod name { ... }` with functions inside where it reduces file nesting
- Avoid gratuitous underscored names — prefer shorter identifiers

### Config (TOML)

- Use dotted sub-sections for grouping
- Prefer short keys: `addr` not `listen_addr`, `cap` not `hard_cap_bytes`

### Proto

- Follow protobuf `snake_case` convention but keep field names short
- `consumer` not `consumer_id`, `committed` not `committed_lsn`

## SQL Functions

### `manage_updated(tbl TEXT)`

Sets up automatic timestamp management for a table's `updated` column.

```sql
SELECT manage_updated('platform.users');
```

### `set_updated()`

Trigger function that sets `updated = NOW()` on row updates.

## Design Principles

### Performance

- **Strategic denormalization**: Store complete paths to avoid joins
- **JSONB flexibility**: Use `meta`, `params`, `caps` for structured data
- **Full-text search**: `tsvector` fields with weighted search on `name` + `desc`
- **Table partitioning**: Files by date, jobs by status, activities by date
- **GIN indexes**: For JSONB and full-text search
- **Partial indexes**: For frequent filters (e.g., `WHERE active = TRUE`), max 20 per table
