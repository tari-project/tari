ALTER TABLE completed_transactions
    ADD COLUMN send_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE completed_transactions
    ADD COLUMN last_send_timestamp DATETIME NULL DEFAULT NULL;

ALTER TABLE inbound_transactions
    ADD COLUMN send_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inbound_transactions
    ADD COLUMN last_send_timestamp DATETIME NULL DEFAULT NULL;

ALTER TABLE outbound_transactions
    ADD COLUMN send_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE outbound_transactions
    ADD COLUMN last_send_timestamp DATETIME NULL DEFAULT NULL;
