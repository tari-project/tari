CREATE TABLE outputs (
    spending_key BLOB PRIMARY KEY NOT NULL,
    value INTEGER NOT NULL,
    flags INTEGER NOT NULL,
    maturity INTEGER NOT NULL,
    spent INTEGER NOT NULL DEFAULT 0,
    to_be_received INTEGER NOT NULL DEFAULT 0,
    encumbered INTEGER NOT NULL DEFAULT 0,
    tx_id INTEGER NULL,
    FOREIGN KEY(tx_id) REFERENCES pending_transaction_outputs(tx_id)
);

CREATE TABLE pending_transaction_outputs (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    timestamp DATETIME NOT NULL
);