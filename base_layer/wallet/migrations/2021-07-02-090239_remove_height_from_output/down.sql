PRAGMA foreign_keys=off;
ALTER TABLE outputs RENAME TO outputs_old;
CREATE TABLE outputs (
                         id INTEGER NOT NULL PRIMARY KEY,
                         commitment BLOB NOT NULL,
                         spending_key BLOB NOT NULL,
                         value INTEGER NOT NULL,
                         flags INTEGER NOT NULL,
                         maturity INTEGER NOT NULL,
                         status INTEGER NOT NULL,
                         tx_id INTEGER NULL,
                         hash BLOB NOT NULL,
                         script BLOB NOT NULL,
                         input_data BLOB NOT NULL,
                         height INTEGER NOT NULL,
                         script_private_key BLOB NOT NULL,
                         script_offset_public_key BLOB NOT NULL,
                         sender_metadata_signature_key BLOB NOT NULL,
                         sender_metadata_signature_nonce BLOB NOT NULL,
                         CONSTRAINT unique_commitment UNIQUE (commitment)
);

INSERT INTO outputs (id, commitment, spending_key, value, flags, maturity, status, tx_id, hash, script, input_data, height, script_private_key, script_offset_public_key, sender_metadata_signature_key, sender_metadata_signature_nonce)
SELECT id, commitment, spending_key, value, flags, maturity, status, tx_id, hash, script, input_data, 0, script_private_key, script_offset_public_key, sender_metadata_signature_key, sender_metadata_signature_nonce
FROM outputs_old;
DROP TABLE outputs_old;
PRAGMA foreign_keys=on;