// Copyright 2020, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use chrono::{NaiveDateTime, Utc};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, dsl, result::DatabaseErrorKind, sql_types};
use log::*;
use tari_common_sqlite::{connection::DbConnection, error::StorageError};
use tari_comms::types::CommsPublicKey;
use tari_utilities::hex::{Hex, to_hex};

use crate::{actor::DEDUP_ERROR_LOG_INTERVAL, outbound::MESSAGE_VALIDITY_WINDOW, schema::dedup_cache};

const LOG_TARGET: &str = "comms::dht::dedup_cache";

#[allow(dead_code)]
#[derive(Queryable, PartialEq, Eq, Debug)]
struct DedupCacheEntry {
    body_hash: String,
    sender_public_ke: String,
    number_of_hit: i32,
    stored_at: NaiveDateTime,
    last_hit_at: NaiveDateTime,
}

/// The fewest new entries between two growth-triggered trims.
const MIN_TRIM_EVERY_INSERTS: usize = 100;
/// Allowance for clocks that disagree on a message's expiry.
const CLOCK_SLACK: Duration = Duration::from_secs(10 * 60);
/// Entries hit more recently than this are kept above capacity, so an unexpired message cannot be flushed out of the
/// cache and replayed as new.
const MIN_ENTRY_AGE: Duration = MESSAGE_VALIDITY_WINDOW.saturating_add(CLOCK_SLACK);
/// The cache is trimmed regardless of entry age above this many times its capacity.
const CEILING_CAPACITY_MULTIPLE: usize = 10;

#[derive(Clone)]
pub struct DedupCacheDatabase {
    connection: DbConnection,
    capacity: usize,
    /// Trim once every this many new entries, so the table stays below `capacity + trim_every_inserts` rows.
    trim_every_inserts: usize,
    /// New entries inserted, shared by all clones.
    num_inserts: Arc<AtomicUsize>,
    /// Entries younger than this are only trimmed above the ceiling.
    min_entry_age: Duration,
    /// Set while a trim runs, so overlapping trims do not each delete the same excess.
    is_trimming: Arc<AtomicBool>,
    /// When hitting the ceiling was last logged.
    ceiling_logged_at: Arc<Mutex<Option<Instant>>>,
}

impl DedupCacheDatabase {
    pub fn new(connection: DbConnection, capacity: usize) -> Self {
        let trim_every_inserts = (capacity / 10).max(MIN_TRIM_EVERY_INSERTS);
        debug!(
            target: LOG_TARGET,
            "Message dedup cache capacity initialized at {capacity}, trimmed every {trim_every_inserts} new entries",
        );
        Self {
            connection,
            capacity,
            trim_every_inserts,
            num_inserts: Arc::new(AtomicUsize::new(0)),
            min_entry_age: MIN_ENTRY_AGE,
            is_trimming: Arc::new(AtomicBool::new(false)),
            ceiling_logged_at: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_min_entry_age(mut self, min_entry_age: Duration) -> Self {
        self.min_entry_age = min_entry_age;
        self
    }

    /// Adds the body hash to the cache, returning the number of hits (inclusive) that have been recorded for this body
    /// hash
    pub fn add_msg_hash(&self, msg_hash: &[u8], public_key: &CommsPublicKey) -> Result<u32, StorageError> {
        let hit_count = self.insert_body_hash_or_update_stats(&to_hex(msg_hash), &public_key.to_hex())?;

        if hit_count == 0 {
            warn!(
                target: LOG_TARGET,
                "Unable to insert new entry into message dedup cache"
            );
        }
        if hit_count == 1 {
            let num_inserts = self.num_inserts.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
            if num_inserts.is_multiple_of(self.trim_every_inserts) &&
                let Err(err) = self.trim_entries()
            {
                warn!(target: LOG_TARGET, "Unable to trim message dedup cache: {err:?}");
            }
        }
        Ok(hit_count)
    }

    pub fn get_hit_count(&self, body_hash: &[u8]) -> Result<u32, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        let hit_count = dedup_cache::table
            .select(dedup_cache::number_of_hits)
            .filter(dedup_cache::body_hash.eq(&to_hex(body_hash)))
            .get_result::<i32>(&mut conn)
            .optional()?;

        #[allow(clippy::cast_sign_loss)]
        Ok(hit_count.unwrap_or(0) as u32)
    }

    /// Trims the dedup cache: entries older than the minimum entry age are removed, oldest first, down to capacity;
    /// above the ceiling (10x capacity) the oldest entries are removed down to the ceiling whatever their age. A trim
    /// requested while another is running is skipped.
    pub fn trim_entries(&self) -> Result<usize, StorageError> {
        if self.is_trimming.swap(true, Ordering::AcqRel) {
            debug!(target: LOG_TARGET, "Message dedup cache trim already running, skipping");
            return Ok(0);
        }
        let result = self.trim_entries_inner();
        self.is_trimming.store(false, Ordering::Release);
        result
    }

    fn trim_entries_inner(&self) -> Result<usize, StorageError> {
        let capacity = i64::try_from(self.capacity).unwrap_or(i64::MAX);
        let ceiling = i64::try_from(self.capacity.saturating_mul(CEILING_CAPACITY_MULTIPLE)).unwrap_or(i64::MAX);
        let mut conn = self.connection.get_pooled_connection()?;
        let msg_count = dedup_cache::table
            .select(dsl::count(dedup_cache::id))
            .first::<i64>(&mut conn)?;

        let mut num_removed = 0;
        if msg_count > ceiling {
            num_removed = diesel::sql_query(
                "DELETE FROM dedup_cache WHERE id IN (SELECT id FROM dedup_cache ORDER BY last_hit_at ASC LIMIT $1)",
            )
            .bind::<sql_types::BigInt, _>(msg_count.saturating_sub(ceiling))
            .execute(&mut conn)?;
            self.log_ceiling_hit(msg_count, ceiling);
        }

        let remaining = msg_count.saturating_sub(i64::try_from(num_removed).unwrap_or(i64::MAX));
        if remaining > capacity {
            let cutoff = chrono::Duration::from_std(self.min_entry_age)
                .ok()
                .and_then(|age| Utc::now().naive_utc().checked_sub_signed(age))
                .unwrap_or(NaiveDateTime::MIN);
            let num_old_removed = diesel::sql_query(
                "DELETE FROM dedup_cache WHERE id IN (SELECT id FROM dedup_cache WHERE last_hit_at <= $1 ORDER BY \
                 last_hit_at ASC LIMIT $2)",
            )
            .bind::<sql_types::Timestamp, _>(cutoff)
            .bind::<sql_types::BigInt, _>(remaining.saturating_sub(capacity))
            .execute(&mut conn)?;
            num_removed = num_removed.saturating_add(num_old_removed);
        }
        debug!(
            target: LOG_TARGET,
            "Message dedup cache: count {msg_count}, capacity {capacity}, ceiling {ceiling}, removed {num_removed}"
        );
        Ok(num_removed)
    }

    fn log_ceiling_hit(&self, msg_count: i64, ceiling: i64) {
        let mut logged_at = self.ceiling_logged_at.lock().unwrap_or_else(|e| e.into_inner());
        let should_log = match *logged_at {
            Some(at) => at.elapsed() >= DEDUP_ERROR_LOG_INTERVAL,
            None => true,
        };
        if should_log {
            *logged_at = Some(Instant::now());
            warn!(
                target: LOG_TARGET,
                "Message dedup cache held {msg_count} entries, above its ceiling of {ceiling}. Removed the oldest \
                 entries, including unexpired ones."
            );
        }
    }

    /// Insert new row into the table or updates an existing row. Returns the number of hits for this body hash.
    fn insert_body_hash_or_update_stats(&self, body_hash: &str, public_key: &str) -> Result<u32, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        let insert_result = diesel::insert_into(dedup_cache::table)
            .values((
                dedup_cache::body_hash.eq(&body_hash),
                dedup_cache::sender_public_key.eq(&public_key),
                dedup_cache::number_of_hits.eq(1),
                dedup_cache::last_hit_at.eq(Utc::now().naive_utc()),
            ))
            .execute(&mut conn);
        match insert_result {
            Ok(1) => Ok(1),
            Ok(n) => Err(StorageError::UnexpectedResult(format!(
                "Expected exactly one row to be inserted. Got {n}"
            ))),
            Err(diesel::result::Error::DatabaseError(kind, e_info)) => match kind {
                DatabaseErrorKind::UniqueViolation => {
                    // Update hit stats for the message
                    diesel::update(dedup_cache::table.filter(dedup_cache::body_hash.eq(&body_hash)))
                        .set((
                            dedup_cache::sender_public_key.eq(&public_key),
                            // Diesel column expression: builds SQL, not integer arithmetic
                            #[allow(clippy::arithmetic_side_effects)]
                            dedup_cache::number_of_hits.eq(dedup_cache::number_of_hits + 1),
                            dedup_cache::last_hit_at.eq(Utc::now().naive_utc()),
                        ))
                        .execute(&mut conn)?;

                    let hits = dedup_cache::table
                        .select(dedup_cache::number_of_hits)
                        .filter(dedup_cache::body_hash.eq(&body_hash))
                        .get_result::<i32>(&mut conn)?;
                    #[allow(clippy::cast_sign_loss)]
                    Ok(hits as u32)
                },
                _ => Err(diesel::result::Error::DatabaseError(kind, e_info).into()),
            },
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod test {
    use diesel::{QueryableByName, sql_types::Text};
    use tokio::task;

    use super::*;
    use crate::storage::MIGRATIONS;

    fn new_cache(capacity: usize) -> DedupCacheDatabase {
        let conn = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        DedupCacheDatabase::new(conn, capacity)
    }

    fn row_count(cache: &DedupCacheDatabase) -> i64 {
        let mut conn = cache.connection.get_pooled_connection().unwrap();
        dedup_cache::table
            .select(dsl::count(dedup_cache::id))
            .first::<i64>(&mut conn)
            .unwrap()
    }

    fn insert(cache: &DedupCacheDatabase, range: std::ops::Range<usize>) {
        for i in range {
            let num_hits = cache
                .add_msg_hash(&i.to_le_bytes(), &CommsPublicKey::default())
                .unwrap();
            assert_eq!(num_hits, 1);
        }
    }

    /// Makes entry `i` look last hit `age + (num_entries - i)` seconds ago, so lower `i` is older.
    // Overflow in test code panics, which is the desired failure mode for a test.
    #[allow(clippy::arithmetic_side_effects)]
    fn age_entries(cache: &DedupCacheDatabase, num_entries: usize, age: Duration) {
        let mut conn = cache.connection.get_pooled_connection().unwrap();
        let now = Utc::now().naive_utc();
        for i in 0..num_entries {
            let extra = Duration::from_secs(u64::try_from(num_entries - i).unwrap());
            let last_hit_at = now - chrono::Duration::from_std(age + extra).unwrap();
            diesel::update(dedup_cache::table.filter(dedup_cache::body_hash.eq(to_hex(&i.to_le_bytes()))))
                .set(dedup_cache::last_hit_at.eq(last_hit_at))
                .execute(&mut conn)
                .unwrap();
        }
    }

    fn contains(cache: &DedupCacheDatabase, i: usize) -> bool {
        cache.get_hit_count(&i.to_le_bytes()).unwrap() > 0
    }

    #[test]
    fn it_trims_as_it_grows() {
        let capacity = 10;
        let cache = new_cache(capacity);
        let n = cache.trim_every_inserts;
        assert_eq!(n, 100);
        let ceiling = capacity * CEILING_CAPACITY_MULTIPLE;
        let limit = i64::try_from(ceiling + n).unwrap();

        for i in 0..(ceiling + 2 * n) {
            insert(&cache, i..i + 1);
            assert!(row_count(&cache) <= limit);
        }
        assert!(row_count(&cache) <= limit);
    }

    #[test]
    fn it_keeps_young_entries_above_capacity() {
        let cache = new_cache(10);
        insert(&cache, 0..20);
        assert_eq!(cache.trim_entries().unwrap(), 0);
        assert_eq!(row_count(&cache), 20);
    }

    #[test]
    fn it_trims_old_entries_to_capacity() {
        let cache = new_cache(10);
        insert(&cache, 0..20);
        age_entries(&cache, 20, MIN_ENTRY_AGE + Duration::from_secs(60));
        assert_eq!(cache.trim_entries().unwrap(), 10);
        assert_eq!(row_count(&cache), 10);
        assert!((0..10).all(|i| !contains(&cache, i)));
        assert!((10..20).all(|i| contains(&cache, i)));
    }

    #[test]
    fn it_only_trims_old_entries_below_the_ceiling() {
        let cache = new_cache(10);
        insert(&cache, 0..20);
        // Entries 0..5 are old, the rest young
        age_entries(&cache, 5, MIN_ENTRY_AGE + Duration::from_secs(60));
        assert_eq!(cache.trim_entries().unwrap(), 5);
        assert!((0..5).all(|i| !contains(&cache, i)));
        assert_eq!(row_count(&cache), 15);
    }

    #[test]
    fn it_trims_young_entries_to_the_ceiling() {
        let capacity = 10;
        let ceiling = capacity * CEILING_CAPACITY_MULTIPLE;
        let cache = new_cache(capacity);
        // Insert directly so that the growth trim does not run first
        for i in 0..(ceiling + 20) {
            cache
                .insert_body_hash_or_update_stats(&to_hex(&i.to_le_bytes()), &CommsPublicKey::default().to_hex())
                .unwrap();
        }
        age_entries(&cache, ceiling + 20, Duration::ZERO);
        assert_eq!(cache.trim_entries().unwrap(), 20);
        assert_eq!(row_count(&cache), i64::try_from(ceiling).unwrap());
        assert!((0..20).all(|i| !contains(&cache, i)));
    }

    #[test]
    fn a_trim_is_skipped_while_another_runs() {
        let cache = new_cache(10).with_min_entry_age(Duration::ZERO);
        insert(&cache, 0..20);
        cache.is_trimming.store(true, Ordering::Release);
        assert_eq!(cache.trim_entries().unwrap(), 0);
        assert_eq!(row_count(&cache), 20);
        cache.is_trimming.store(false, Ordering::Release);
        assert_eq!(cache.trim_entries().unwrap(), 10);
        assert_eq!(row_count(&cache), 10);
    }

    #[test]
    fn trim_uses_last_hit_at_index() {
        #[derive(QueryableByName)]
        struct QueryPlanRow {
            #[diesel(sql_type = Text)]
            detail: String,
        }

        let cache = new_cache(10);
        let mut conn = cache.connection.get_pooled_connection().unwrap();
        for query in [
            "EXPLAIN QUERY PLAN SELECT id FROM dedup_cache ORDER BY last_hit_at ASC LIMIT 1",
            "EXPLAIN QUERY PLAN SELECT id FROM dedup_cache WHERE last_hit_at <= '2026-01-01 00:00:00' ORDER BY \
             last_hit_at ASC LIMIT 1",
        ] {
            let plan = diesel::sql_query(query).load::<QueryPlanRow>(&mut conn).unwrap();
            assert!(
                plan.iter()
                    .any(|row| row.detail.contains("idx_dedup_cache_last_hit_at")),
                "query plan does not use the index: {query}"
            );
        }
    }

    /// The DHT pipeline runs up to 100 inbound tasks at once, each inserting on a blocking thread. None of them may
    /// time out waiting for a pooled connection, because a dedup error drops the message.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_inserts_do_not_exhaust_the_pool() {
        let cache = new_cache(50_000);
        let mut tasks = Vec::new();
        for i in 0u32..100 {
            let cache = cache.clone();
            tasks.push(task::spawn_blocking(move || {
                cache.add_msg_hash(&i.to_le_bytes(), &CommsPublicKey::default())
            }));
        }
        for t in tasks {
            assert_eq!(t.await.unwrap().unwrap(), 1);
        }
        assert_eq!(row_count(&cache), 100);
    }
}
