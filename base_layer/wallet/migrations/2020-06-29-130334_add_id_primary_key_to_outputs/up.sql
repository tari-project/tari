PRAGMA foreign_keys=off;
ALTER TABLE outputs RENAME TO outputs_old;
CREATE TABLE outputs (
                         id INTEGER NOT NULL PRIMARY KEY,
                         commitment BLOB NULL DEFAULT NULL,
                         spending_key BLOB NOT NULL,
                         value INTEGER NOT NULL,
                         flags INTEGER NOT NULL,
                         maturity INTEGER NOT NULL,
                         status INTEGER NOT NULL,
                         tx_id INTEGER NULL,
                         hash BLOB NULL DEFAULT NULL
);
INSERT INTO outputs (spending_key, value, flags, maturity, status, tx_id, hash) SELECT spending_key, value, flags, maturity, status, tx_id, hash FROM outputs_old;
DROP TABLE outputs_old;
PRAGMA foreign_keys=on;
