-- Reverse the addition of rejection_reason column from up.sql.
-- SQLite does not support DROP COLUMN; recreate the table without the column.

-- 1. Rename current table to keep data safe
ALTER TABLE completed_transactions RENAME TO completed_transactions_old;

-- 2. Create table without rejection_reason
CREATE TABLE completed_transactions (
    tx_id                       BIGINT PRIMARY KEY NOT NULL,
    source_address              BLOB               NOT NULL,
    destination_address         BLOB               NOT NULL,
    amount                      BIGINT             NOT NULL,
    fee                         BIGINT             NOT NULL,
    transaction_protocol        BLOB               NOT NULL,
    status                      INTEGER            NOT NULL,
    timestamp                   DATETIME           NOT NULL,
    cancelled                   INTEGER            NULL,
    direction                   INTEGER            NULL,
    send_count                  INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp         DATETIME           NULL,
    confirmations               BIGINT             NULL,
    mined_height                BIGINT             NULL,
    mined_in_block              BLOB               NULL,
    mined_timestamp             DATETIME           NULL,
    transaction_signature_nonce BLOB    DEFAULT 0  NOT NULL,
    transaction_signature_key   BLOB    DEFAULT 0  NOT NULL,
    payment_id                  BLOB               NULL,
    sent_output_hashes          BLOB               NULL,
    received_output_hashes      BLOB               NULL,
    change_output_hashes        BLOB               NULL,
    user_payment_id             BLOB               NULL,
    lock_height                 BIGINT             NULL DEFAULT NULL
);

-- 3. Copy data from old table (excluding rejection_reason)
INSERT INTO completed_transactions SELECT
    tx_id, source_address, destination_address, amount, fee, transaction_protocol,
    status, timestamp, cancelled, direction, send_count, last_send_timestamp,
    confirmations, mined_height, mined_in_block, mined_timestamp,
    transaction_signature_nonce, transaction_signature_key, payment_id,
    sent_output_hashes, received_output_hashes, change_output_hashes, user_payment_id,
    lock_height
FROM completed_transactions_old;

-- 4. Drop the old table
DROP TABLE completed_transactions_old;
