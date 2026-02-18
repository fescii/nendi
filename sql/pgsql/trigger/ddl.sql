-- Nendi CDC: DDL event trigger
-- Fires on ddl_command_end to capture schema changes.

CREATE EVENT TRIGGER nendi_ddl_trigger
  ON ddl_command_end
  EXECUTE FUNCTION nendi_capture_ddl();
