-- This file should undo anything in `up.sql`
-- Remove output hash columns from outbound_transactions
ALTER TABLE outbound_transactions ADD COLUMN change_output_hashes BLOB NULL;