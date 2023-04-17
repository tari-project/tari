CREATE TABLE messages (
    address    BLOB             NOT NULL,
    message_id BLOB PRIMARY KEY NOT NULL,
    body       BLOB             NOT NULL,
    stored_at  TIMESTAMP        NOT NULL,
    direction  INTEGER          NOT NULL
);

CREATE INDEX idx_messages_address ON messages (address);
