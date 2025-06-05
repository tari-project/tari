-- PayRef Performance Optimization Migration
-- Adds database indexes to improve PayRef calculation and lookup performance

-- Index on outputs.commitment for fast commitment-based lookups
-- This optimizes the GetOutputsByCommitments query used in PayRef calculations
CREATE INDEX IF NOT EXISTS idx_outputs_commitment ON outputs(commitment);

-- Index on outputs.mined_in_block for filtering mined outputs
-- PayRefs can only be calculated for outputs that have mined_in_block set
CREATE INDEX IF NOT EXISTS idx_outputs_mined_in_block ON outputs(mined_in_block);

-- Index on outputs.received_in_tx_id for transaction linkage
-- This optimizes lookups when linking outputs to their originating transactions
CREATE INDEX IF NOT EXISTS idx_outputs_received_in_tx_id ON outputs(received_in_tx_id);

-- Index on outputs.spent_in_tx_id for spent output queries
-- Helps with filtering spent vs unspent outputs
CREATE INDEX IF NOT EXISTS idx_outputs_spent_in_tx_id ON outputs(spent_in_tx_id);

-- Composite index on outputs status and mined_in_block for efficient PayRef queries
-- This optimizes the OutputBackendQuery filtering by status and mined state
CREATE INDEX IF NOT EXISTS idx_outputs_status_mined ON outputs(status, mined_in_block);

-- Index on completed_transactions.timestamp for chronological queries
-- Improves performance when fetching recent transactions for PayRef calculation
CREATE INDEX IF NOT EXISTS idx_completed_tx_timestamp ON completed_transactions(timestamp);

-- Index on completed_transactions.status for filtering by transaction status
-- Helps with queries that filter transactions by completion status
CREATE INDEX IF NOT EXISTS idx_completed_tx_status ON completed_transactions(status);
