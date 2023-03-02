CREATE TABLE contacts (
    address     BLOB PRIMARY KEY NOT NULL UNIQUE,
    node_id     BLOB             NOT NULL UNIQUE,
    alias       TEXT             NOT NULL,
    last_seen   DATETIME         NULL,
    latency     INTEGER          NULL
);