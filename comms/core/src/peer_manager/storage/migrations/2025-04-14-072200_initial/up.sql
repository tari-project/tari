-- Table for NodeIdentity
CREATE TABLE node_identity (
    public_key TEXT PRIMARY KEY NOT NULL,
    node_id TEXT NOT NULL,
    features INTEGER NOT NULL
);

-- Table for Peer
CREATE TABLE peers (
   peer_id BIGINT PRIMARY KEY NOT NULL,
   public_key TEXT NOT NULL,
   node_id TEXT NOT NULL,
   distance_to_self TEXT NOT NULL,
   flags INTEGER NOT NULL,
   banned_until TIMESTAMP,
   banned_reason TEXT,
   features INTEGER NOT NULL,
   supported_protocols TEXT NOT NULL,
   added_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
   user_agent TEXT NOT NULL,
   metadata BLOB,
   deleted_at TIMESTAMP,

   CONSTRAINT unique_public_key UNIQUE (public_key),
   CONSTRAINT unique_node_id UNIQUE (node_id)
);
CREATE INDEX idx_node_id ON peers (node_id);
CREATE INDEX idx_banned_until ON peers (banned_until);
CREATE INDEX idx_deleted_at ON peers (deleted_at);
CREATE INDEX idx_distance_to_self ON peers (distance_to_self);

-- Table for MultiaddressesWithStats
CREATE TABLE multi_addresses (
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

     FOREIGN KEY (peer_id) REFERENCES peers (peer_id) ON DELETE CASCADE,
     -- We do not allow the same address to be associated more than one peer or the same peer more than once.
     CONSTRAINT unique_address UNIQUE (address)
);
CREATE INDEX idx_last_seen ON multi_addresses (last_seen);
CREATE INDEX idx_last_failed_reason ON multi_addresses (last_failed_reason);
CREATE INDEX idx_peer_id ON multi_addresses (peer_id);
