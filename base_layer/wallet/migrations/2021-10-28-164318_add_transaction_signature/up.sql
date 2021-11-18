ALTER TABLE completed_transactions
    ADD transaction_signature_nonce BLOB NOT NULL DEFAULT 0;

ALTER TABLE completed_transactions
    ADD transaction_signature_key BLOB NOT NULL DEFAULT 0;
