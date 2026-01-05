-- Migration to add PayRef tables

CREATE TABLE payrefs
(
    output_hash         BLOB    PRIMARY KEY NOT NULL,
    payref              BLOB               NOT NULL,
    tx_id               BIGINT             NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_payrefs_in_payref ON payrefs(payref);
CREATE INDEX IF NOT EXISTS idx_payrefs_in_tx_id ON payrefs(tx_id);