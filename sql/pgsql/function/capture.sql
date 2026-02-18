-- Nendi CDC: DDL capture function
-- Called by event triggers to emit DDL changes as logical messages.
-- These messages appear in the WAL and are picked up by the decoder.

CREATE OR REPLACE FUNCTION nendi_capture_ddl()
RETURNS event_trigger
LANGUAGE plpgsql AS $$
DECLARE
  obj RECORD;
  ddl_type TEXT;
  tbl_name TEXT;
  stmt TEXT;
BEGIN
  -- Get the DDL command tag
  ddl_type := tg_tag;
  stmt := current_query();

  -- Iterate over affected objects
  FOR obj IN SELECT * FROM pg_event_trigger_ddl_commands()
  LOOP
    -- Only capture table-level DDL
    IF obj.object_type IN ('table', 'table column') THEN
      tbl_name := obj.schema_name || '.' || obj.object_identity;

      -- Emit as a logical decoding message
      -- Format: DDL_TYPE|schema.table|statement
      PERFORM pg_logical_emit_message(
        true,  -- transactional
        'nendi_ddl',
        ddl_type || '|' || tbl_name || '|' || stmt
      );
    END IF;
  END LOOP;
END;
$$;
