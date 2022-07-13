CREATE TABLE known_stealth_addresses (
    stealth_address_hash BLOB PRIMARY KEY NOT NULL,
    scanning_private_key BLOB             NOT NULL,
    spending_private_key BLOB             NOT NULL
);
