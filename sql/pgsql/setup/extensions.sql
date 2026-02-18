-- Nendi CDC: Required PostgreSQL extensions
-- Run this as a superuser before starting the daemon.

-- Logical replication support (built-in but needs wal_level = logical)
-- No CREATE EXTENSION needed, but verify:
-- ALTER SYSTEM SET wal_level = logical;

-- pgcrypto for UUID generation (optional)
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- pg_stat_statements for query performance monitoring (optional)
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
