-- This migration is part of a testnet reset and should not be used on db's with existing old data in them
-- thus this migration does not accommodate db's with existing rows.

PRAGMA foreign_keys=off;
DROP TABLE outputs;
CREATE TABLE outputs (
    id INTEGER NOT NULL PRIMARY KEY,
    commitment BLOB NOT NULL,
    spending_key BLOB NOT NULL,
    value INTEGER NOT NULL,
    flags INTEGER NOT NULL,
    maturity INTEGER NOT NULL,
    status INTEGER NOT NULL,
    tx_id INTEGER NULL,
    hash BLOB NOT NULL,
    script BLOB NOT NULL,
    input_data BLOB NOT NULL,
    height INTEGER NOT NULL,
    script_private_key BLOB NOT NULL,
    script_offset_public_key BLOB NOT NULL,
    CONSTRAINT unique_commitment UNIQUE (commitment)
);
PRAGMA foreign_keys=on;
