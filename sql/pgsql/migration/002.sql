-- Nendi CDC: Migration 002 — Publication tracking

CREATE TABLE IF NOT EXISTS nendi.pubs (
  name TEXT PRIMARY KEY,
  tables TEXT[] NOT NULL DEFAULT '{}',
  created TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for fast consumer offset lookups
CREATE INDEX IF NOT EXISTS idx_offsets_committed
  ON nendi.offsets (committed);
