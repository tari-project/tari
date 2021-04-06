-- This migration is part of a testnet reset and should not be used on db's with existing old data in them
-- thus this migration does not accommodate db's with existing rows.
ALTER TABLE outputs
    ADD COLUMN script BLOB NOT NULL;
ALTER TABLE outputs
    ADD COLUMN input_data BLOB NOT NULL;
ALTER TABLE outputs
    ADD COLUMN height INTEGER NOT NULL;
ALTER TABLE outputs
    ADD COLUMN script_private_key BLOB NOT NULL;
ALTER TABLE outputs
    ADD COLUMN script_offset_public_key BLOB NOT NULL;