
ALTER TABLE inbound_transactions
    DROP message;

ALTER TABLE outbound_transactions
    DROP message;

ALTER TABLE completed_transactions
    DROP message;
