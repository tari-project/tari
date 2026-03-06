-- Restore distance_to_self column to peers table

CREATE TABLE peers_new (
    peer_id BIGINT PRIMARY KEY NOT NULL,
    public_key TEXT NOT NULL,
    node_id TEXT NOT NULL,
    distance_to_self TEXT NOT NULL DEFAULT '',
    flags INTEGER NOT NULL,
    banned_until TIMESTAMP,
    banned_reason TEXT NOT NULL,
    features INTEGER NOT NULL,
    supported_protocols TEXT NOT NULL,
    added_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_agent TEXT NOT NULL,
    metadata BLOB,
    deleted_at TIMESTAMP,

    CONSTRAINT unique_public_key UNIQUE (public_key),
    CONSTRAINT unique_node_id UNIQUE (node_id)
);

INSERT INTO peers_new (peer_id, public_key, node_id, distance_to_self, flags, banned_until, banned_reason, features, supported_protocols, added_at, user_agent, metadata, deleted_at)
SELECT peer_id, public_key, node_id, '', flags, banned_until, banned_reason, features, supported_protocols, added_at, user_agent, metadata, deleted_at
FROM peers;

DROP TABLE peers;
ALTER TABLE peers_new RENAME TO peers;

CREATE INDEX idx_node_id ON peers (node_id);
CREATE INDEX idx_banned_until ON peers (banned_until);
CREATE INDEX idx_deleted_at ON peers (deleted_at);
CREATE INDEX idx_distance_to_self ON peers (distance_to_self);
