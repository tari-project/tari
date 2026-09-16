
ALTER TABLE inbound_transactions
    ADD payment_id BLOB NULL;

ALTER TABLE outbound_transactions
    ADD payment_id BLOB NULL;
