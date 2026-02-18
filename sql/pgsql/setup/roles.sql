-- Nendi CDC: Database role setup
-- Creates a dedicated replication user with minimum required privileges.

DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'nendi_repl') THEN
    CREATE ROLE nendi_repl WITH LOGIN REPLICATION PASSWORD 'changeme';
  END IF;
END $$;

-- Grant connect to the target database
GRANT CONNECT ON DATABASE current_database() TO nendi_repl;

-- Grant usage on schemas containing tables to replicate
GRANT USAGE ON SCHEMA public TO nendi_repl;

-- Grant SELECT on all tables (for initial snapshot)
GRANT SELECT ON ALL TABLES IN SCHEMA public TO nendi_repl;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
  GRANT SELECT ON TABLES TO nendi_repl;
