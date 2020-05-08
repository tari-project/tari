UPDATE completed_transactions
SET status = 5
WHERE cancelled = 1;

ALTER TABLE completed_transactions
    DROP COLUMN cancelled;

ALTER TABLE inbound_transactions
    DROP COLUMN cancelled;

ALTER TABLE outbound_transactions
    DROP COLUMN cancelled;