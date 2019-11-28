CREATE TABLE contacts (
    public_key BLOB PRIMARY KEY NOT NULL UNIQUE,
    alias TEXT NOT NULL
);