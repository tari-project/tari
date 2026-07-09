-- The L1 block height the burn was mined in, populated alongside the kernel merkle proof once the burn is
-- confirmed on-chain. Used to derive the L1 epoch (height / vn_epoch_length) exposed in the proof file and over gRPC.
ALTER TABLE burn_proofs
    ADD COLUMN mined_in_height BIGINT NULL;
