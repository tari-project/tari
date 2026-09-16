-- Any old 'outputs' will not be valid due to the removal of 'coinbase_block_height' and removal of default value for
-- 'spending_priority', so we drop and recreate the table.
ALTER TABLE outputs
    ADD user_payment_id BLOB NULL;

ALTER TABLE completed_transactions
    ADD user_payment_id BLOB NULL;

ALTER TABLE inbound_transactions
    ADD user_payment_id BLOB NULL;

ALTER TABLE outbound_transactions
    ADD user_payment_id BLOB NULL;
