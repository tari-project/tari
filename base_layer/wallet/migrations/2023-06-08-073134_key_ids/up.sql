-- Any old 'outputs' will not be valid due to the change in 'spending_key' and 'script_private_key' to
-- 'TEXT', so we drop and recreate the table.

DROP TABLE outputs;
CREATE TABLE outputs
(
    id                                      INTEGER PRIMARY KEY NOT NULL,
    commitment                              BLOB                NOT NULL,
    spending_key                            TEXT                NOT NULL,
    value                                   BIGINT              NOT NULL,
    output_type                             INTEGER             NOT NULL,
    maturity                                BIGINT              NOT NULL,
    status                                  INTEGER             NOT NULL,
    hash                                    BLOB                NOT NULL,
    script                                  BLOB                NOT NULL,
    input_data                              BLOB                NOT NULL,
    script_private_key                      TEXT                NOT NULL,
    script_lock_height                      UNSIGNED BIGINT     NOT NULL DEFAULT 0,
    sender_offset_public_key                BLOB                NOT NULL,
    metadata_signature_ephemeral_commitment BLOB                NOT NULL,
    metadata_signature_ephemeral_pubkey     BLOB                NOT NULL,
    metadata_signature_u_a                  BLOB                NOT NULL,
    metadata_signature_u_x                  BLOB                NOT NULL,
    metadata_signature_u_y                  BLOB                NOT NULL,
    mined_height                            UNSIGNED BIGINT     NULL,
    mined_in_block                          BLOB                NULL,
    mined_mmr_position                      BIGINT              NULL,
    marked_deleted_at_height                BIGINT              NULL,
    marked_deleted_in_block                 BLOB                NULL,
    received_in_tx_id                       BIGINT              NULL,
    spent_in_tx_id                          BIGINT              NULL,
    coinbase_block_height                   UNSIGNED BIGINT     NULL,
    coinbase_extra                          BLOB                NULL,
    features_json                           TEXT                NOT NULL DEFAULT '{}',
    spending_priority                       UNSIGNED INTEGER    NOT NULL DEFAULT 500,
    covenant                                BLOB                NOT NULL,
    mined_timestamp                         DATETIME            NULL,
    encrypted_data                          BLOB                NOT NULL,
    minimum_value_promise                   BIGINT              NOT NULL,
    source                                  INTEGER             NOT NULL DEFAULT 0,
    last_validation_timestamp               DATETIME            NULL,
    CONSTRAINT unique_commitment UNIQUE (commitment)
);

-- Any old 'completed_transactions' will not be valid due to the change in 'spending_key' and 'script_private_key' to
-- 'TEXT', so we drop and recreate the table.

DROP TABLE completed_transactions;
CREATE TABLE completed_transactions
(
    tx_id                       BIGINT PRIMARY KEY NOT NULL,
    source_address              BLOB               NOT NULL,
    destination_address         BLOB               NOT NULL,
    amount                      BIGINT             NOT NULL,
    fee                         BIGINT             NOT NULL,
    transaction_protocol        TEXT               NOT NULL,
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

-- Any old 'inbound_transactions' will not be valid due to the change in 'spending_key' and 'script_private_key' to
-- -- 'TEXT', so we drop and recreate the table.

DROP TABLE inbound_transactions;
CREATE TABLE inbound_transactions
(
    tx_id               BIGINT PRIMARY KEY NOT NULL,
    source_address      BLOB               NOT NULL,
    amount              BIGINT             NOT NULL,
    receiver_protocol   TEXT               NOT NULL,
    message             TEXT               NOT NULL,
    timestamp           DATETIME           NOT NULL,
    cancelled           INTEGER            NOT NULL,
    direct_send_success INTEGER DEFAULT 0  NOT NULL,
    send_count          INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp DATETIME           NULL
);

-- Any old 'outbound_transactions' will not be valid due to the change in 'spending_key' and 'script_private_key' to
-- -- 'TEXT', so we drop and recreate the table.

DROP TABLE outbound_transactions;
CREATE TABLE outbound_transactions
(
    tx_id               BIGINT PRIMARY KEY NOT NULL,
    destination_address BLOB               NOT NULL,
    amount              BIGINT             NOT NULL,
    fee                 BIGINT             NOT NULL,
    sender_protocol     TEXT               NOT NULL,
    message             TEXT               NOT NULL,
    timestamp           DATETIME           NOT NULL,
    cancelled           INTEGER DEFAULT 0  NOT NULL,
    direct_send_success INTEGER DEFAULT 0  NOT NULL,
    send_count          INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp DATETIME           NULL
);

-- Any old 'scanned_blocks' will not be valid due to the change in 'spending_key' and 'script_private_key' to
-- -- 'TEXT', so we drop and recreate the table.

DROP TABLE scanned_blocks;
CREATE TABLE scanned_blocks
(
    header_hash BLOB PRIMARY KEY NOT NULL,
    height      BIGINT           NOT NULL,
    num_outputs BIGINT           NULL,
    amount      BIGINT           NULL,
    timestamp   DATETIME         NOT NULL
);

-- Any old 'burnt_proofs' will not be valid due to the change in 'spending_key' and 'script_private_key' to
-- -- 'TEXT', so we drop and recreate the table.

DROP TABLE burnt_proofs;
CREATE TABLE burnt_proofs
(
    id                          INTEGER PRIMARY KEY NOT NULL,
    reciprocal_claim_public_key TEXT                NOT NULL,
    payload                     TEXT                NOT NULL,
    burned_at                   DATETIME            NOT NULL
);

-- Any old 'known_one_sided_payment_scripts' will not be valid due to the change in 'private_key' to
-- -- 'TEXT', so we drop and recreate the table.

DROP TABLE known_one_sided_payment_scripts;
CREATE TABLE known_one_sided_payment_scripts
(
    script_hash        BLOB PRIMARY KEY NOT NULL,
    private_key        TEXT             NOT NULL,
    script             BLOB             NOT NULL,
    input              BLOB             NOT NULL,
    script_lock_height UNSIGNED BIGINT  NOT NULL DEFAULT 0
);
