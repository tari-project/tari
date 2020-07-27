-- This file should undo anything in `up.sql`
ALTER TABLE key_manager_states RENAME COLUMN master_key TO master_seed;

PRAGMA foreign_keys=off;
ALTER TABLE pending_transaction_outputs RENAME TO pending_transaction_outputs_old;
CREATE TABLE pending_transaction_outputs (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    short_term INTEGER NOT NULL,
    timestamp DATETIME NOT NULL
);
INSERT INTO pending_transaction_outputs (tx_id, short_term, timestamp) SELECT tx_id, short_term, timestamp  FROM pending_transaction_outputs_old;
DROP TABLE pending_transaction_outputs_old;
PRAGMA foreign_keys=on;

PRAGMA foreign_keys=off;
ALTER TABLE completed_transactions RENAME TO completed_transactions_old;
CREATE TABLE completed_transactions (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    source_public_key BLOB NOT NULL,
    destination_public_key BLOB NOT NULL,
    amount INTEGER NOT NULL,
    fee INTEGER NOT NULL,
    transaction_protocol TEXT NOT NULL,
    status INTEGER NOT NULL,
    message TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    cancelled INTEGER NOT NULL DEFAULT 0,
    direction INTEGER NULL DEFAULT NULL
);
INSERT INTO completed_transactions (tx_id, source_public_key, destination_public_key, amount, fee, transaction_protocol, status, message, timestamp, cancelled, direction)
SELECT tx_id, source_public_key, destination_public_key, amount, fee, transaction_protocol, status, message, timestamp, cancelled, direction
FROM completed_transactions_old;

DROP TABLE completed_transactions_old;
PRAGMA foreign_keys=on;