ALTER TABLE completed_transactions
    ADD mined_timestamp DATETIME NULL;

ALTER TABLE outputs
    ADD mined_timestamp DATETIME NULL;