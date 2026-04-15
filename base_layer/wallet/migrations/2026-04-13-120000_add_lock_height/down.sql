-- Cannot drop columns in SQLite, so we recreate the table without lock_height.
-- This is destructive: all completed transaction data is preserved except lock_height.
CREATE TABLE completed_transactions_backup AS SELECT
    tx_id, source_address, destination_address, amount, fee, transaction_protocol,
    status, timestamp, cancelled, direction, send_count, last_send_timestamp,
    confirmations, mined_height, mined_in_block, mined_timestamp,
    transaction_signature_nonce, transaction_signature_key, payment_id,
    sent_output_hashes, received_output_hashes, change_output_hashes, user_payment_id
FROM completed_transactions;

DROP TABLE completed_transactions;

ALTER TABLE completed_transactions_backup RENAME TO completed_transactions;
