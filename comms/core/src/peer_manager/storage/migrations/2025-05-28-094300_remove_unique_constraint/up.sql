-- Table for MultiaddressesWithStats

--   Create a new table without the unique constraint
CREATE TABLE multi_addresses_new (
    address_id INTEGER PRIMARY KEY AUTOINCREMENT,
    peer_id BIGINT NOT NULL,
    address TEXT NOT NULL,
    last_seen TIMESTAMP,
    connection_attempts INTEGER,
    avg_initial_dial_time BIGINT,
    initial_dial_time_sample_count INTEGER,
    avg_latency BIGINT,
    latency_sample_count INTEGER,
    last_attempted TIMESTAMP,
    last_failed_reason TEXT,
    quality_score INTEGER,
    source TEXT NOT NULL,

    FOREIGN KEY (peer_id) REFERENCES peers (peer_id) ON DELETE CASCADE
);

--   Copy data from the old table
INSERT INTO multi_addresses_new (
    address_id,
    peer_id,
    address,
    last_seen,
    connection_attempts,
    avg_initial_dial_time,
    initial_dial_time_sample_count,
    avg_latency,
    latency_sample_count,
    last_attempted,
    last_failed_reason,
    quality_score,
    source
)
SELECT
    address_id,
    peer_id,
    address,
    last_seen,
    connection_attempts,
    avg_initial_dial_time,
    initial_dial_time_sample_count,
    avg_latency,
    latency_sample_count,
    last_attempted,
    last_failed_reason,
    quality_score,
    source
FROM multi_addresses;

--   Drop the old table
DROP TABLE multi_addresses;
ALTER TABLE multi_addresses_new RENAME TO multi_addresses;

CREATE INDEX idx_last_seen ON multi_addresses (last_seen);
CREATE INDEX idx_last_failed_reason ON multi_addresses (last_failed_reason);
CREATE INDEX idx_peer_id ON multi_addresses (peer_id);
