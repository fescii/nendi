-- Nendi CDC: Teardown — Drop replication slot and publication

SELECT pg_drop_replication_slot('nendi_slot')
  WHERE EXISTS (
    SELECT 1 FROM pg_replication_slots WHERE slot_name = 'nendi_slot'
  );

DROP PUBLICATION IF EXISTS nendi_pub;

-- Drop event triggers
DROP EVENT TRIGGER IF EXISTS nendi_ddl_trigger;
DROP EVENT TRIGGER IF EXISTS nendi_drop_trigger;

-- Drop functions
DROP FUNCTION IF EXISTS nendi_capture_ddl();
DROP FUNCTION IF EXISTS nendi_capture_drop();
DROP FUNCTION IF EXISTS nendi_emit(TEXT, TEXT, BOOLEAN);
DROP FUNCTION IF EXISTS nendi_schema_fingerprint(TEXT, TEXT);
