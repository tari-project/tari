PRAGMA foreign_keys=OFF;

ALTER TABLE completed_transactions
    RENAME TO completed_transactions_old;

CREATE TABLE completed_transactions
(
    tx_id                           BIGINT NOT NULL PRIMARY KEY,
    source_public_key               BLOB NOT NULL,
    destination_public_key          BLOB NOT NULL,
    amount                          BIGINT NOT NULL,
    fee                             BIGINT NOT NULL,
    transaction_protocol            TEXT NOT NULL,
    status                          INTEGER NOT NULL,
    message                         TEXT NOT NULL,
    timestamp                       DATETIME NOT NULL,
    cancelled                       INTEGER NULL,
    direction                       INTEGER,
    coinbase_block_height           BIGINT,
    send_count                      INTEGER default 0 NOT NULL,
    last_send_timestamp             DATETIME,
    confirmations                   BIGINT default NULL,
    mined_height                    BIGINT,
    mined_in_block                  BLOB,
    transaction_signature_nonce     BLOB default 0 NOT NULL,
    transaction_signature_key       BLOB default 0 NOT NULL
);

INSERT INTO completed_transactions (tx_id, source_public_key, destination_public_key, amount, fee, transaction_protocol,
                                    status, message, timestamp, cancelled, direction, coinbase_block_height, send_count,
                                    last_send_timestamp, confirmations, mined_height, mined_in_block, transaction_signature_nonce,
                                    transaction_signature_key)
SELECT tx_id,
       source_public_key,
       destination_public_key,
       amount,
       fee,
       transaction_protocol,
       status,
       message,
       timestamp,
       CASE completed_transactions_old.valid --This flag was only ever used to signify an abandoned coinbase, we will do that in the cancelled reason enum now
            WHEN 0
                THEN 7 -- This is the value for AbandonedCoinbase
            ELSE
                NULLIF(cancelled, 0)
            END,
       direction,
       coinbase_block_height,
       send_count,
       last_send_timestamp,
       confirmations,
       mined_height,
       mined_in_block,
       transaction_signature_nonce,
       transaction_signature_key
FROM completed_transactions_old;

DROP TABLE completed_transactions_old;
PRAGMA foreign_keys=ON;