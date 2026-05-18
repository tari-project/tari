DROP INDEX uidx_stored_messages_body_hash;

ALTER TABLE stored_messages
    DROP COLUMN body_hash;