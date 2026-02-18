-- Nendi CDC: Migration 001 — Create nendi schema and metadata tables

CREATE SCHEMA IF NOT EXISTS nendi;

-- Tracks which consumers have committed offsets.
CREATE TABLE IF NOT EXISTS nendi.offsets (
  consumer TEXT PRIMARY KEY,
  committed BIGINT NOT NULL DEFAULT 0,
  updated TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Tracks schema fingerprints for drift detection.
CREATE TABLE IF NOT EXISTS nendi.fingerprints (
  tbl TEXT PRIMARY KEY,
  fingerprint BIGINT NOT NULL,
  columns INTEGER NOT NULL,
  updated TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Daemon metadata (singleton table).
CREATE TABLE IF NOT EXISTS nendi.daemon (
  id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  slot TEXT NOT NULL,
  confirmed BIGINT NOT NULL DEFAULT 0,
  started TIMESTAMPTZ NOT NULL DEFAULT now(),
  heartbeat TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Auto-update trigger for `updated` columns.
CREATE OR REPLACE FUNCTION set_updated()
RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated = now();
  RETURN NEW;
END;
$$;

-- Helper: sets up auto-updated timestamp for any table.
CREATE OR REPLACE FUNCTION manage_updated(tbl TEXT)
RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
  EXECUTE format(
    'CREATE TRIGGER set_updated BEFORE UPDATE ON %s
     FOR EACH ROW EXECUTE FUNCTION set_updated()',
    tbl
  );
END;
$$;

-- Apply to tables with `updated` column
SELECT manage_updated('nendi.offsets');
SELECT manage_updated('nendi.fingerprints');
