-- Add rejection_detail column to store the human-readable reason when the mempool rejects a transaction.
-- NULL means the transaction was not rejected or no detail was available.
ALTER TABLE completed_transactions ADD COLUMN rejection_detail TEXT NULL;
