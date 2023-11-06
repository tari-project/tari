-- Any old 'completed_transactions' will not be valid due to the change in 'transaction_protocol' to
-- 'BLOB', so we drop and recreate the table.

DROP TABLE completed_transactions;
CREATE TABLE completed_transactions
(
    tx_id                       BIGINT PRIMARY KEY NOT NULL,
    source_address              BLOB               NOT NULL,
    destination_address         BLOB               NOT NULL,
    amount                      BIGINT             NOT NULL,
    fee                         BIGINT             NOT NULL,
    transaction_protocol        BLOB               NOT NULL,
    status                      INTEGER            NOT NULL,
    message                     TEXT               NOT NULL,
    timestamp                   DATETIME           NOT NULL,
    cancelled                   INTEGER            NULL,
    direction                   INTEGER            NULL,
    coinbase_block_height       BIGINT             NULL,
    send_count                  INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp         DATETIME           NULL,
    confirmations               BIGINT             NULL,
    mined_height                BIGINT             NULL,
    mined_in_block              BLOB               NULL,
    mined_timestamp             DATETIME           NULL,
    transaction_signature_nonce BLOB    DEFAULT 0  NOT NULL,
    transaction_signature_key   BLOB    DEFAULT 0  NOT NULL
);
