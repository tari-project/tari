DROP INDEX idx_messages_address;

ALTER TABLE messages DROP address;

ALTER TABLE messages ADD receiver_address BLOB NOT NULL;
ALTER TABLE messages ADD sender_address BLOB NOT NULL;

CREATE INDEX idx_messages_receiver_address ON messages (receiver_address);
CREATE INDEX idx_messages_sender_address ON messages (sender_address);
