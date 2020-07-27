-- Rename the master_seed column to master_key
ALTER TABLE key_manager_states RENAME COLUMN master_seed TO master_key;

ALTER TABLE pending_transaction_outputs ADD COLUMN coinbase_block_height INTEGER NULL DEFAULT NULL;

ALTER TABLE completed_transactions ADD COLUMN coinbase_block_height INTEGER NULL DEFAULT NULL;
