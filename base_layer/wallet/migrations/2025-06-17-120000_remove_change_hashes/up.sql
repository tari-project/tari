-- Migration to add PayRef support by adding output hash storage to transaction tables


-- Add output hash storage to outbound_transactions table
ALTER TABLE outbound_transactions DROP COLUMN change_output_hashes;
