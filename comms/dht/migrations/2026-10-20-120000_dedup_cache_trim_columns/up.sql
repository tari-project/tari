-- Rows written before this migration keep the default 3 h validity window plus 10 min clock slack, and are treated as
-- signed so the ceiling trim does not prefer them.
ALTER TABLE dedup_cache ADD COLUMN keep_until TIMESTAMP NOT NULL DEFAULT '1970-01-01 00:00:00';
ALTER TABLE dedup_cache ADD COLUMN is_signed BOOLEAN NOT NULL DEFAULT 1;
UPDATE dedup_cache SET keep_until = strftime('%Y-%m-%d %H:%M:%f', last_hit_at, '+190 minutes');

CREATE INDEX idx_dedup_cache_keep_until ON dedup_cache (keep_until);
CREATE INDEX idx_dedup_cache_is_signed_last_hit_at ON dedup_cache (is_signed, last_hit_at);
