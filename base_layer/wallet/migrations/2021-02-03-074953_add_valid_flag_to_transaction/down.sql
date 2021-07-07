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
                                        coinbase_block_height INTEGER NULL,
                                        send_count INTEGER NOT NULL DEFAULT 0,
                                        last_send_timestamp DATETIME NULL,
);
INSERT INTO completed_transactions (tx_id, source_public_key, destination_public_key, amount, fee, transaction_protocol, status, message, timestamp, cancelled, direction, coinbase_block_height, send_count, last_send_timestamp)
SELECT tx_id, source_public_key, destination_public_key, amount, fee, transaction_protocol, status, message, timestamp, cancelled, direction, coinbase_block_height, send_count, last_send_timestamp
FROM completed_transactions_old;