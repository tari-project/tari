CREATE TABLE outbound_transactions (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    destination_public_key BLOB NOT NULL,
    amount INTEGER NOT NULL,
    fee INTEGER NOT NULL,
    sender_protocol TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp DATETIME NOT NULL
);

CREATE TABLE inbound_transactions (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    source_public_key BLOB NOT NULL,
    amount INTEGER NOT NULL,
    receiver_protocol TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp DATETIME NOT NULL
);

CREATE TABLE completed_transactions (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    source_public_key BLOB NOT NULL,
    destination_public_key BLOB NOT NULL,
    amount INTEGER NOT NULL,
    fee INTEGER NOT NULL,
    transaction_protocol TEXT NOT NULL,
    status INTEGER NOT NULL,
    message TEXT NOT NULL,
    timestamp DATETIME NOT NULL
);