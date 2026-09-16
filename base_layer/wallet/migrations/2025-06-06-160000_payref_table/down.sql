-- This file should undo anything in `up.sql`
-- Remove table payref
DROP INDEX IF EXISTS idx_payrefs_in_payref;
DROP INDEX IF EXISTS idx_payrefs_in_tx_id;
DROP TABLE payrefs;
