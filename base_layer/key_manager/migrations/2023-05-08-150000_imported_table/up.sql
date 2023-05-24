CREATE TABLE imported_keys (
    id                INTEGER PRIMARY KEY NOT NULL,
    private_key       BLOB UNIQUE         NOT NULL,
    public_key        TEXT                NOT NULL,
    timestamp         DATETIME            NOT NULL
);