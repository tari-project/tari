PRAGMA foreign_keys=OFF;
ALTER TABLE outputs
    RENAME TO outputs_old;
CREATE TABLE outputs (
    id           INTEGER NOT NULL PRIMARY KEY,
    commitment   BLOB    NULL,
    spending_key BLOB    NOT NULL,
    value        INTEGER NOT NULL,
    flags        INTEGER NOT NULL,
    maturity     INTEGER NOT NULL,
    status       INTEGER NOT NULL,
    tx_id        INTEGER NULL,
    hash         BLOB    NULL,
);
INSERT INTO outputs
SELECT *
FROM outputs_old;
DROP TABLE outputs_old;
PRAGMA foreign_keys=ON;
