ALTER TABLE completed_transactions
    ADD COLUMN cancelled INTEGER NOT NULL DEFAULT 0;

ALTER TABLE inbound_transactions
    ADD COLUMN cancelled INTEGER NOT NULL DEFAULT 0;

ALTER TABLE outbound_transactions
    ADD COLUMN cancelled INTEGER NOT NULL DEFAULT 0;

UPDATE completed_transactions
SET cancelled = 1, status = 1
WHERE status = 5;