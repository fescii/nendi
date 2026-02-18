-- DDL mutation fixtures for testing schema change detection

-- Add a new column
ALTER TABLE public.orders ADD COLUMN IF NOT EXISTS notes TEXT;

-- Rename a column (PostgreSQL 9.4+)
-- ALTER TABLE public.orders RENAME COLUMN notes TO memo;

-- Change a column type
-- ALTER TABLE public.payments ALTER COLUMN method TYPE VARCHAR(50);
