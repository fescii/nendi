-- Nendi CDC: Table rename detection
-- Note: PostgreSQL fires ddl_command_end for ALTER TABLE ... RENAME.
-- This file documents the pattern but the main ddl trigger handles it.

-- The rename is captured by the nendi_capture_ddl() function via
-- the ddl_command_end event trigger. No separate trigger needed.

-- To verify rename detection works:
-- ALTER TABLE public.old_name RENAME TO new_name;
-- This will emit: RENAME_TABLE|public.old_name|ALTER TABLE ... RENAME TO new_name
