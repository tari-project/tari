-- Lossless address storage, dial-candidate network groups and peer garbage collection.
--
-- `address` holds the text form of a multiaddr, which is not lossless: a DNS name containing `/` reads back as a
-- different address or fails to parse at all. `address_bytes` holds the binary form, which is what is read back.
-- Existing rows are filled in from the text by the node on the next start (rows that do not parse are removed), as
-- the multiaddr encoding is not available in SQL. `address` is still written for display and debugging.
ALTER TABLE multi_addresses ADD COLUMN address_bytes BLOB NOT NULL DEFAULT X'';

-- The network group of the address (IPv4 /16, IPv6 /32, the onion address or the DNS name), computed by the node on
-- write, so that dial candidates can be capped per group in SQL.
ALTER TABLE multi_addresses ADD COLUMN netgroup TEXT NOT NULL DEFAULT '';

CREATE INDEX idx_address_bytes ON multi_addresses (address_bytes);
CREATE INDEX idx_netgroup ON multi_addresses (netgroup);
-- Garbage collection removes the oldest never-seen peers first.
CREATE INDEX idx_added_at ON peers (added_at);
