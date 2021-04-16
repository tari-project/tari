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
    direction INTEGER NULL,
    coinbase_block_height INTEGER NULL
);
INSERT INTO completed_transactions (tx_id, source_public_key, destination_public_key, amount, fee, transaction_protocol, status, message, timestamp, cancelled, direction, coinbase_block_height)
SELECT tx_id, source_public_key, destination_public_key, amount, fee, transaction_protocol, status, message, timestamp, cancelled, direction, coinbase_block_height
FROM completed_transactions_old;

DROP TABLE completed_transactions_old;

ALTER TABLE inbound_transactions RENAME TO inbound_transactions_old;
CREATE TABLE inbound_transactions (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    source_public_key BLOB NOT NULL,
    amount INTEGER NOT NULL,
    receiver_protocol TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    cancelled INTEGER NOT NULL DEFAULT 0,
    direct_send_success INTEGER NOT NULL DEFAULT 0
);
INSERT INTO inbound_transactions (tx_id, source_public_key, amount, receiver_protocol, message, timestamp, cancelled, direct_send_success)
SELECT tx_id, source_public_key, amount, receiver_protocol, message, timestamp, cancelled, direct_send_success
FROM inbound_transactions_old;

DROP TABLE inbound_transactions_old;

ALTER TABLE outbound_transactions RENAME TO outbound_transactions_old;
CREATE TABLE outbound_transactions (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    destination_public_key BLOB NOT NULL,
    amount INTEGER NOT NULL,
    fee INTEGER NOT NULL,
    sender_protocol TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    cancelled INTEGER NOT NULL DEFAULT 0,
    direct_send_success INTEGER NOT NULL DEFAULT 0
);
INSERT INTO outbound_transactions (tx_id, destination_public_key, amount, fee, sender_protocol, message, timestamp, cancelled, direct_send_success)
SELECT tx_id, destination_public_key, amount, fee, sender_protocol, message, timestamp, cancelled, direct_send_success
FROM outbound_transactions_old;

DROP TABLE outbound_transactions_old;

PRAGMA foreign_keys=on;