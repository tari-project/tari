-- CREATE TABLE sent_messages (
--    id TEXT PRIMARY KEY NOT NULL,
--    source_pub_key TEXT NOT NULL,
--    dest_pub_key TEXT NOT NULL,
--    message TEXT  NOT NULL,
--    timestamp DATETIME NOT NULL,
--    acknowledged INTEGER NOT NULL DEFAULT 0,
--    is_read INTEGER NOT NULL DEFAULT 0,
--    FOREIGN KEY(dest_pub_key) REFERENCES contacts(pub_key)
-- );

-- CREATE TABLE received_messages (
--     id BLOB PRIMARY KEY NOT NULL,
--     source_pub_key TEXT NOT NULL,
--     dest_pub_key TEXT NOT NULL,
--     message TEXT  NOT NULL,
--     timestamp DATETIME NOT NULL
-- );

-- CREATE TABLE settings (
--     pub_key TEXT PRIMARY KEY NOT NULL,
--     screen_name TEXT NOT NULL
-- )


