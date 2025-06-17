-- This file should undo anything in `up.sql`

-- Remove PayRef optimization index
DROP INDEX IF EXISTS idx_completed_tx_mined_block;

-- Remove output hash columns from inbound_transactions
ALTER TABLE inbound_transactions DROP COLUMN received_output_hashes;

-- Remove output hash columns from outbound_transactions  
ALTER TABLE outbound_transactions DROP COLUMN change_output_hashes;
ALTER TABLE outbound_transactions DROP COLUMN sent_output_hashes;

-- Remove output hash columns from completed_transactions
ALTER TABLE completed_transactions DROP COLUMN change_output_hashes;
ALTER TABLE completed_transactions DROP COLUMN received_output_hashes;
ALTER TABLE completed_transactions DROP COLUMN sent_output_hashes;
