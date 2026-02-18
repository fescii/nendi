-- Nendi CDC: Replication slot and publication setup

-- Create publication for all tables (can be refined later)
CREATE PUBLICATION nendi_pub FOR ALL TABLES;

-- Create logical replication slot using pgoutput
SELECT pg_create_logical_replication_slot('nendi_slot', 'pgoutput');
