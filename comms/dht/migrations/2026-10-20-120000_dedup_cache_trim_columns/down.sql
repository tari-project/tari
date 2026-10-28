DROP INDEX IF EXISTS idx_dedup_cache_is_signed_last_hit_at;
DROP INDEX IF EXISTS idx_dedup_cache_keep_until;
ALTER TABLE dedup_cache DROP COLUMN is_signed;
ALTER TABLE dedup_cache DROP COLUMN keep_until;
