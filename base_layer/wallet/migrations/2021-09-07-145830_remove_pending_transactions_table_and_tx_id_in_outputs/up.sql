DROP TABLE IF EXISTS pending_transaction_outputs;

-- Remove tx_id column
PRAGMA foreign_keys=OFF;
ALTER TABLE outputs
    RENAME TO outputs_old;
CREATE TABLE outputs (
                         id                       INTEGER NOT NULL PRIMARY KEY, --auto inc,
                         commitment               BLOB    NULL,
                         spending_key             BLOB    NOT NULL,
                         value                    BIGINT  NOT NULL,
                         flags                    INTEGER NOT NULL,
                         maturity                 BIGINT  NOT NULL,
                         status                   INTEGER NOT NULL,
                         hash                     BLOB    NULL,
                         script                   BLOB    NOT NULL,
                         input_data               BLOB    NOT NULL,
                         script_private_key       BLOB    NOT NULL,
                         sender_offset_public_key BLOB    NOT NULL,
                         metadata_signature_nonce BLOB    NOT NULL,
                         metadata_signature_u_key BLOB    NOT NULL,
                         metadata_signature_v_key BLOB    NOT NULL,
                         mined_height             UNSIGNED BIGINT NULL,
                         mined_in_block           BLOB NULL,
                         mined_mmr_position       BIGINT NULL,
                         marked_deleted_at_height BIGINT,
                         marked_deleted_in_block  BLOB,
                         received_in_tx_id        BIGINT,
                         spent_in_tx_id           BIGINT,
                         coinbase_block_height    UNSIGNED BIGINT NULL,
                         CONSTRAINT unique_commitment UNIQUE (commitment)
);
PRAGMA foreign_keys=ON;

INSERT INTO outputs (id, commitment, spending_key, value, flags, maturity, status, hash, script, input_data,
                     script_private_key, sender_offset_public_key, metadata_signature_nonce, metadata_signature_u_key,
                     metadata_signature_v_key, mined_height, mined_in_block, mined_mmr_position, marked_deleted_at_height,
                     marked_deleted_in_block, received_in_tx_id, spent_in_tx_id)
SELECT id,
       commitment,
       spending_key,
       value,
       flags,
       maturity,
       status,
       hash,
       script,
       input_data,
       script_private_key,
       sender_offset_public_key,
       metadata_signature_nonce,
       metadata_signature_u_key,
       metadata_signature_v_key,
       mined_height,
       mined_in_block,
       mined_mmr_position,
       marked_deleted_at_height,
       marked_deleted_in_block,
       received_in_tx_id,
       spent_in_tx_id
FROM outputs_old;

DROP TABLE outputs_old;
PRAGMA foreign_keys=ON;


