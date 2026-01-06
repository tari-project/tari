-- Remove kernel_excess and kernel_excess_sig columns from burn_proofs table
-- SQLite doesn't support DROP COLUMN directly, so we need to recreate the table

-- Create a temporary table without the kernel_excess columns
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
    value               BIGINT                            NULL
);

-- Copy data from the original table
INSERT INTO burn_proofs_temp (id, output_hash, commitment, burn_proof, kernel, kernel_merkle_proof, created_at, updated_at, encrypted_data, value)
SELECT id, output_hash, commitment, burn_proof, kernel, kernel_merkle_proof, created_at, updated_at, encrypted_data, value
FROM burn_proofs;

-- Drop the original table
DROP TABLE burn_proofs;

-- Rename the temp table to the original name
ALTER TABLE burn_proofs_temp RENAME TO burn_proofs;

-- Recreate indexes
CREATE INDEX idx_burn_proofs_output_hash ON burn_proofs (output_hash);
CREATE INDEX idx_burn_proofs_commitment ON burn_proofs (commitment);
