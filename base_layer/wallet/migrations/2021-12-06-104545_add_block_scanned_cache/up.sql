CREATE TABLE scanned_blocks (
    header_hash BLOB PRIMARY KEY NOT NULL,
    height      BIGINT           NOT NULL,
    num_outputs BIGINT           NULL,
    amount      BIGINT           NULL,
    timestamp   DATETIME         NOT NULL
);
