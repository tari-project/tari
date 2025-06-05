-- Rollback PayRef Performance Optimization Migration
-- Removes the indexes added for PayRef performance improvements

DROP INDEX IF EXISTS idx_outputs_commitment;
DROP INDEX IF EXISTS idx_outputs_mined_in_block;
DROP INDEX IF EXISTS idx_outputs_received_in_tx_id;
DROP INDEX IF EXISTS idx_outputs_spent_in_tx_id;
DROP INDEX IF EXISTS idx_outputs_status_mined;
DROP INDEX IF EXISTS idx_completed_tx_timestamp;
DROP INDEX IF EXISTS idx_completed_tx_status;
