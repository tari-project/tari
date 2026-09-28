-- The burn claim proof now proves the burn output against the block's `block_output_mr` instead of proving the kernel
-- in the kernel MMR. Stored kernel proofs are not converted.
ALTER TABLE burn_proofs
    RENAME COLUMN kernel_merkle_proof TO burn_output_proof;
UPDATE burn_proofs
SET burn_output_proof = NULL;
