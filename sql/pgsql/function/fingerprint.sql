-- Nendi CDC: Schema fingerprint function
-- Computes a stable hash of a table's column definitions.
-- Used to detect schema drift between producers and consumers.

CREATE OR REPLACE FUNCTION nendi_schema_fingerprint(
  p_schema TEXT,
  p_table TEXT
)
RETURNS BIGINT
LANGUAGE plpgsql AS $$
DECLARE
  col RECORD;
  hash BIGINT := x'cbf29ce484222325'::BIGINT;  -- FNV-1a offset basis
  prime BIGINT := x'100000001b3'::BIGINT;       -- FNV-1a prime
  col_str TEXT;
BEGIN
  FOR col IN
    SELECT column_name, udt_name, ordinal_position, is_nullable
    FROM information_schema.columns
    WHERE table_schema = p_schema AND table_name = p_table
    ORDER BY ordinal_position
  LOOP
    col_str := col.column_name || ':' || col.udt_name || ':' || col.is_nullable;
    -- Hash each byte of the column string
    FOR i IN 1..length(col_str)
    LOOP
      hash := hash # ascii(substr(col_str, i, 1));
      hash := hash * prime;
    END LOOP;
  END LOOP;

  RETURN hash;
END;
$$;
