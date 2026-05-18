-- Any old 'outputs' will not be valid due to the change in 'spending_key' and 'script_private_key' to
-- 'TEXT', so we drop and recreate the table.

DROP TABLE outputs;
CREATE TABLE outputs
(
    id                                      INTEGER PRIMARY KEY NOT NULL,
    commitment                              BLOB                NOT NULL,
    rangeproof                              BLOB                NULL,
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
