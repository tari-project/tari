-- We assume that no burn proofs exist in the database yet (there is no use case anyway), so we can drop and recreate the table.
DROP TABLE IF EXISTS burnt_proofs;

CREATE TABLE burn_proofs
(
    id                  INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    output_hash         BLOB                              NOT NULL,
    commitment          BLOB                              NOT NULL,
    -- The burn proof is stored as an encrypted binary blob
    burn_proof          BLOB                              NOT NULL,
    kernel              BLOB                              NOT NULL,
    -- The kernel merkle proof populated after the burn proof is confirmed on-chain
    kernel_merkle_proof BLOB                              NULL,
    created_at          DATETIME                          NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          DATETIME                          NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_burn_proofs_output_hash ON burn_proofs (output_hash);
CREATE INDEX idx_burn_proofs_commitment ON burn_proofs (commitment);