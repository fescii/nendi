-- Nendi CDC: Emit function for custom logical messages
-- Wrapper around pg_logical_emit_message for application-level events.

CREATE OR REPLACE FUNCTION nendi_emit(
  prefix TEXT,
  content TEXT,
  transactional BOOLEAN DEFAULT true
)
RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM pg_logical_emit_message(transactional, prefix, content);
END;
$$;

-- Grant execute to the replication role
GRANT EXECUTE ON FUNCTION nendi_emit(TEXT, TEXT, BOOLEAN) TO nendi_repl;
