-- Migration to add PayRef history tracking for reorgs

CREATE TABLE payref_history
(
    id              INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    output_hash     BLOB                             NOT NULL,
    payref          BLOB                             NOT NULL,
    tx_id           BIGINT                           NOT NULL,
    superseded_at   TIMESTAMP                        NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_payref_history_payref ON payref_history(payref);
CREATE INDEX IF NOT EXISTS idx_payref_history_tx_id ON payref_history(tx_id);
