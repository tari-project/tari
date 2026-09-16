CREATE TABLE dedup_cache (
    id INTEGER NOT NULL PRIMARY KEY,
    body_hash TEXT NOT NULL,
    sender_public_key TEXT NOT NULL,
    number_of_hits INT NOT NULL,
    stored_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_hit_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uidx_dedup_cache_body_hash ON dedup_cache (body_hash);
