-- Add kernel_excess (excess) and kernel_excess_sig (excess_sig) columns to burn_proofs table
ALTER TABLE burn_proofs
    ADD COLUMN kernel_excess BLOB NULL;

ALTER TABLE burn_proofs
    ADD COLUMN kernel_excess_sig BLOB NULL;
