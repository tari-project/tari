DROP INDEX idx_messages_address;

ALTER TABLE messages DROP address;

ALTER TABLE messages ADD to_address BLOB NOT NULL;
ALTER TABLE messages ADD from_address BLOB NOT NULL;

CREATE INDEX idx_messages_to_address ON messages (to_address);
CREATE INDEX idx_messages_from_address ON messages (from_address);
