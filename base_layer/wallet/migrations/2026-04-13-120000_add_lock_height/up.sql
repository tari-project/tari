-- Add lock_height column to completed_transactions.
-- For existing outbound transactions, default to 0 (change outputs only).
-- For existing inbound transactions, we cannot retroactively determine maturity,
-- so we default to 0 as well; the value will be correct for all new transactions going forward.
ALTER TABLE completed_transactions ADD COLUMN lock_height BIGINT NOT NULL DEFAULT 0;
