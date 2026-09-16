-- SQLite does not support DROP COLUMN on the versions we target, so recreate the table without mined_in_height.
CREATE TABLE burn_proofs_temp
(
    id                  INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    output_hash         BLOB                              NOT NULL,
    commitment          BLOB                              NOT NULL,
    burn_proof          BLOB                              NOT NULL,
    kernel              BLOB                              NOT NULL,
    kernel_merkle_proof BLOB                              NULL,
    created_at          DATETIME                          NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          DATETIME                          NOT NULL DEFAULT CURRENT_TIMESTAMP,
    encrypted_data      BLOB                              NULL,
    value               BIGINT                            NULL,
    kernel_excess       BLOB                              NULL,
    kernel_excess_sig   BLOB                              NULL
);

INSERT INTO burn_proofs_temp (id, output_hash, commitment, burn_proof, kernel, kernel_merkle_proof, created_at, updated_at, encrypted_data, value, kernel_excess, kernel_excess_sig)
SELECT id, output_hash, commitment, burn_proof, kernel, kernel_merkle_proof, created_at, updated_at, encrypted_data, value, kernel_excess, kernel_excess_sig
FROM burn_proofs;

DROP TABLE burn_proofs;

ALTER TABLE burn_proofs_temp RENAME TO burn_proofs;

CREATE INDEX idx_burn_proofs_output_hash ON burn_proofs (output_hash);
CREATE INDEX idx_burn_proofs_commitment ON burn_proofs (commitment);
