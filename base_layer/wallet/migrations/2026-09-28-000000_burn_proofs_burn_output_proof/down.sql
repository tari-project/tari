ALTER TABLE burn_proofs
    RENAME COLUMN burn_output_proof TO kernel_merkle_proof;
UPDATE burn_proofs
SET kernel_merkle_proof = NULL;
