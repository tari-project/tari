ALTER TABLE outbound_transactions
    ADD COLUMN direct_send_success INTEGER NOT NULL DEFAULT 0;

ALTER TABLE inbound_transactions
    ADD COLUMN direct_send_success INTEGER NOT NULL DEFAULT 0;