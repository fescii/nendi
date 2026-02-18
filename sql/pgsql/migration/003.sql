-- Nendi CDC: Migration 003 — Dead letter queue

CREATE TABLE IF NOT EXISTS nendi.dlq (
  id BIGSERIAL PRIMARY KEY,
  consumer TEXT NOT NULL,
  lsn BIGINT NOT NULL,
  tbl TEXT NOT NULL,
  error TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  payload JSONB,
  created TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_dlq_consumer
  ON nendi.dlq (consumer, created);
