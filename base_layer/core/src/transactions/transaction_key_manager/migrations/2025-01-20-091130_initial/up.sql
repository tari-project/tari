CREATE TABLE key_manager_states (
    id                INTEGER PRIMARY KEY NOT NULL,
    branch_seed       TEXT UNIQUE         NOT NULL,
    primary_key_index BLOB                NOT NULL,
    timestamp         DATETIME            NOT NULL
);

CREATE TABLE imported_keys (
    id                INTEGER PRIMARY KEY NOT NULL,
    private_key       BLOB UNIQUE         NOT NULL,
    public_key        TEXT                NOT NULL,
    timestamp         DATETIME            NOT NULL
);