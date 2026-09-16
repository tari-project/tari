ALTER TABLE stored_messages
    ADD body_hash TEXT;

CREATE UNIQUE INDEX uidx_stored_messages_body_hash ON stored_messages (body_hash);