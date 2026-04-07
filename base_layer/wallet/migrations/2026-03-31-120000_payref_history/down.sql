-- Undo payref history table
DROP INDEX IF EXISTS idx_payref_history_payref;
DROP INDEX IF EXISTS idx_payref_history_tx_id;
DROP TABLE payref_history;
