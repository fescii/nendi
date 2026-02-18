-- Nendi CDC: SQL drop event trigger
-- Fires when tables are dropped.

CREATE OR REPLACE FUNCTION nendi_capture_drop()
RETURNS event_trigger
LANGUAGE plpgsql AS $$
DECLARE
  obj RECORD;
BEGIN
  FOR obj IN SELECT * FROM pg_event_trigger_dropped_objects()
  LOOP
    IF obj.object_type = 'table' THEN
      PERFORM pg_logical_emit_message(
        true,
        'nendi_ddl',
        'DROP_TABLE|' || obj.schema_name || '.' || obj.object_name || '|DROP TABLE'
      );
    END IF;
  END LOOP;
END;
$$;

CREATE EVENT TRIGGER nendi_drop_trigger
  ON sql_drop
  EXECUTE FUNCTION nendi_capture_drop();
