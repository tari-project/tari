DROP INDEX IF EXISTS idx_added_at;
DROP INDEX IF EXISTS idx_netgroup;
DROP INDEX IF EXISTS idx_address_bytes;
ALTER TABLE multi_addresses DROP COLUMN netgroup;
ALTER TABLE multi_addresses DROP COLUMN address_bytes;
