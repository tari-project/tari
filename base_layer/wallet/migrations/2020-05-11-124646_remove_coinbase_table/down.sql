CREATE TABLE coinbase_transactions (
    tx_id INTEGER PRIMARY KEY NOT NULL,
    amount INTEGER NOT NULL,
    commitment BLOB NOT NULL,
    timestamp DATETIME NOT NULL
);