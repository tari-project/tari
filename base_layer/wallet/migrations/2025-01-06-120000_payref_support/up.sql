-- Migration to add PayRef support by adding output hash storage to transaction tables

-- Add output hash storage to completed_transactions table
ALTER TABLE completed_transactions ADD COLUMN sent_output_hashes BLOB NULL;
ALTER TABLE completed_transactions ADD COLUMN received_output_hashes BLOB NULL;
ALTER TABLE completed_transactions ADD COLUMN change_output_hashes BLOB NULL;

-- Add output hash storage to outbound_transactions table  
ALTER TABLE outbound_transactions ADD COLUMN sent_output_hashes BLOB NULL;
ALTER TABLE outbound_transactions ADD COLUMN change_output_hashes BLOB NULL;

-- Add output hash storage to inbound_transactions table
ALTER TABLE inbound_transactions ADD COLUMN received_output_hashes BLOB NULL;

-- Create index for faster PayRef lookups (future optimization)
CREATE INDEX IF NOT EXISTS idx_completed_tx_mined_block ON completed_transactions(mined_in_block);
