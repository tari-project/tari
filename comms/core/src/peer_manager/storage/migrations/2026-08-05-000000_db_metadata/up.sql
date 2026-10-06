-- Key/value store for bookkeeping that does not belong to a peer.
--
-- The first user is the address classifier version. Whether an address is externally routable is
-- decided in Rust (`is_external_address`), not in SQL, so a classifier change cannot be applied by
-- an `UPDATE` here the way `2025-07-19-085200_external_flag` did. Recording the applied version
-- instead lets the node recompute the stored `is_external` flags exactly once after an upgrade.
CREATE TABLE db_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
