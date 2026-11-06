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

use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection, dsl, sql_types};
use log::*;
use tari_common_sqlite::{connection::DbConnection, error::StorageError};
use tari_comms::types::CommsPublicKey;
use tari_utilities::{
    epoch_time::EpochTime,
    hex::{Hex, to_hex},
};

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
/// An entry is kept at least this long after its last hit, so an unexpired message cannot be flushed out of the cache
/// and replayed as new.
const MIN_ENTRY_AGE: Duration = MESSAGE_VALIDITY_WINDOW.saturating_add(CLOCK_SLACK);
/// An entry is kept at most this long after its last hit, however far in the future the message says it expires.
const MAX_ENTRY_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// The cache is trimmed regardless of entry age above this many times its capacity.
const CEILING_CAPACITY_MULTIPLE: usize = 10;

/// Deletes up to `$2` entries whose `keep_until` has passed (`$1`), earliest first.
const DELETE_EXPIRED_SQL: &str = "DELETE FROM dedup_cache WHERE id IN (SELECT id FROM dedup_cache WHERE keep_until <= \
                                  $1 ORDER BY keep_until ASC LIMIT $2)";
/// Deletes up to `$2` of the least recently hit signed (`$1` true) or unsigned entries.
const DELETE_OLDEST_SQL: &str = "DELETE FROM dedup_cache WHERE id IN (SELECT id FROM dedup_cache WHERE is_signed = $1 \
                                 ORDER BY last_hit_at ASC LIMIT $2)";
/// Inserts a hit for body hash `$1` from `$2` at `$3`, or counts another hit on the existing entry, keeping the later
/// `keep_until` (`$4`). `$5` is whether the message is signed. Returns the hit count.
const UPSERT_HIT_SQL: &str = "INSERT INTO dedup_cache (body_hash, sender_public_key, number_of_hits, last_hit_at, \
                              keep_until, is_signed) VALUES ($1, $2, 1, $3, $4, $5) ON CONFLICT(body_hash) DO UPDATE \
                              SET sender_public_key = excluded.sender_public_key, number_of_hits = number_of_hits + \
                              1, last_hit_at = excluded.last_hit_at, keep_until = MAX(keep_until, \
                              excluded.keep_until) RETURNING number_of_hits";

#[derive(Clone)]
pub struct DedupCacheDatabase {
    connection: DbConnection,
    capacity: usize,
    /// Trim once every this many new entries, so the table stays below `ceiling + trim_every_inserts` rows.
    trim_every_inserts: usize,
    /// New entries inserted, shared by all clones.
    num_inserts: Arc<AtomicUsize>,
    /// The shortest time an entry is kept after a hit.
    min_entry_age: Duration,
    /// Set while a trim runs, so overlapping trims do not each delete the same excess.
    is_trimming: Arc<AtomicBool>,
    /// Set by a trim request; the running trim makes another pass until none is pending.
    trim_requested: Arc<AtomicBool>,
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
            trim_requested: Arc::new(AtomicBool::new(false)),
            ceiling_logged_at: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_min_entry_age(mut self, min_entry_age: Duration) -> Self {
        self.min_entry_age = min_entry_age;
        self
    }

    /// Adds the body hash to the cache, returning the number of hits (inclusive) that have been recorded for this body
    /// hash. `expires` is the message's expiry and `is_signed` whether it carries a signature.
    pub fn add_msg_hash(
        &self,
        msg_hash: &[u8],
        public_key: &CommsPublicKey,
        expires: Option<EpochTime>,
        is_signed: bool,
    ) -> Result<u32, StorageError> {
        let keep_until = self.keep_until(Utc::now().naive_utc(), expires, is_signed);
        let hit_count =
            self.insert_body_hash_or_update_stats(&to_hex(msg_hash), &public_key.to_hex(), keep_until, is_signed)?;

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

    /// How long to keep an entry hit at `now`: until the message expires plus clock slack, but no less than the
    /// minimum and no more than the maximum entry age. An unsigned message's expiry is not authenticated, so it gets
    /// the minimum.
    fn keep_until(&self, now: NaiveDateTime, expires: Option<EpochTime>, is_signed: bool) -> NaiveDateTime {
        let earliest = add_duration(now, self.min_entry_age);
        let latest = add_duration(now, MAX_ENTRY_AGE);
        let expires = expires
            .filter(|_| is_signed)
            .and_then(|e| i64::try_from(e.as_u64()).ok())
            .and_then(|secs| DateTime::from_timestamp(secs, 0));
        match expires {
            Some(expires) => add_duration(expires.naive_utc(), CLOCK_SLACK).min(latest).max(earliest),
            None => earliest,
        }
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

    /// Trims the dedup cache. Entries whose `keep_until` has passed are removed, earliest first, down to capacity.
    /// If the cache is still above the ceiling (10x capacity), the least recently hit unsigned entries are removed down
    /// to the ceiling, then signed ones if that is not enough. Only one trim runs at a time: a trim requested while
    /// another runs returns at once, and the running one makes another pass.
    pub fn trim_entries(&self) -> Result<usize, StorageError> {
        self.trim_requested.store(true, Ordering::SeqCst);
        if self.is_trimming.swap(true, Ordering::SeqCst) {
            debug!(target: LOG_TARGET, "Message dedup cache trim already running, it will run again");
            return Ok(0);
        }
        self.run_requested_trims()
    }

    /// Runs trim passes while one is requested. The caller holds `is_trimming`; this releases it.
    fn run_requested_trims(&self) -> Result<usize, StorageError> {
        let mut num_removed = 0usize;
        loop {
            while self.trim_requested.swap(false, Ordering::SeqCst) {
                match self.trim_entries_at(Utc::now().naive_utc()) {
                    Ok(n) => num_removed = num_removed.saturating_add(n),
                    Err(err) => {
                        self.is_trimming.store(false, Ordering::SeqCst);
                        return Err(err);
                    },
                }
            }
            self.is_trimming.store(false, Ordering::SeqCst);
            // A request made after the last check, while the flag was still held, must not be lost
            if !self.trim_requested.load(Ordering::SeqCst) || self.is_trimming.swap(true, Ordering::SeqCst) {
                return Ok(num_removed);
            }
        }
    }

    fn trim_entries_at(&self, now: NaiveDateTime) -> Result<usize, StorageError> {
        let capacity = i64::try_from(self.capacity).unwrap_or(i64::MAX);
        let ceiling = i64::try_from(self.capacity.saturating_mul(CEILING_CAPACITY_MULTIPLE)).unwrap_or(i64::MAX);
        let msg_count = {
            let mut conn = self.connection.get_pooled_connection()?;
            dedup_cache::table
                .select(dsl::count(dedup_cache::id))
                .first::<i64>(&mut conn)?
        };

        let mut num_removed = 0;
        if msg_count > capacity {
            num_removed = self.delete_in_batches(msg_count.saturating_sub(capacity), |conn, limit| {
                diesel::sql_query(DELETE_EXPIRED_SQL)
                    .bind::<sql_types::Timestamp, _>(now)
                    .bind::<sql_types::BigInt, _>(limit)
                    .execute(conn)
            })?;
        }

        let remaining = msg_count.saturating_sub(num_removed);
        if remaining > ceiling {
            let mut excess = remaining.saturating_sub(ceiling);
            for is_signed in [false, true] {
                let n = self.delete_in_batches(excess, |conn, limit| {
                    diesel::sql_query(DELETE_OLDEST_SQL)
                        .bind::<sql_types::Bool, _>(is_signed)
                        .bind::<sql_types::BigInt, _>(limit)
                        .execute(conn)
                })?;
                num_removed = num_removed.saturating_add(n);
                excess = excess.saturating_sub(n);
            }
            self.log_ceiling_hit(remaining, ceiling);
        }
        debug!(
            target: LOG_TARGET,
            "Message dedup cache: count {msg_count}, capacity {capacity}, ceiling {ceiling}, removed {num_removed}"
        );
        Ok(usize::try_from(num_removed).unwrap_or(usize::MAX))
    }

    /// Removes up to `target` rows with `delete_batch`, at most `trim_every_inserts` per statement, each on a freshly
    /// pooled connection so other writers get the lock between batches. Returns the number removed.
    fn delete_in_batches<F>(&self, target: i64, delete_batch: F) -> Result<i64, StorageError>
    where F: Fn(&mut SqliteConnection, i64) -> diesel::QueryResult<usize> {
        let batch_size = i64::try_from(self.trim_every_inserts).unwrap_or(i64::MAX);
        let mut num_removed = 0i64;
        while num_removed < target {
            let mut conn = self.connection.get_pooled_connection()?;
            let n = delete_batch(&mut conn, batch_size.min(target.saturating_sub(num_removed)))?;
            if n == 0 {
                break;
            }
            num_removed = num_removed.saturating_add(i64::try_from(n).unwrap_or(i64::MAX));
        }
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
                "Message dedup cache held {msg_count} unexpired entries, above its ceiling of {ceiling}. Removed the \
                 least recently hit, unsigned first."
            );
        }
    }

    /// Insert new row into the table or updates an existing row. Returns the number of hits for this body hash.
    fn insert_body_hash_or_update_stats(
        &self,
        body_hash: &str,
        public_key: &str,
        keep_until: NaiveDateTime,
        is_signed: bool,
    ) -> Result<u32, StorageError> {
        #[derive(QueryableByName)]
        struct Hits {
            #[diesel(sql_type = sql_types::Integer)]
            number_of_hits: i32,
        }

        let mut conn = self.connection.get_pooled_connection()?;
        // A single statement, so concurrent hits on the same hash each get their own count without holding a write
        // lock across several statements
        let rows = diesel::sql_query(UPSERT_HIT_SQL)
            .bind::<sql_types::Text, _>(body_hash)
            .bind::<sql_types::Text, _>(public_key)
            .bind::<sql_types::Timestamp, _>(Utc::now().naive_utc())
            .bind::<sql_types::Timestamp, _>(keep_until)
            .bind::<sql_types::Bool, _>(is_signed)
            .load::<Hits>(&mut conn)?;
        let hits = match rows.as_slice() {
            [row] => row.number_of_hits,
            _ => {
                return Err(StorageError::UnexpectedResult(format!(
                    "Expected exactly one dedup row to be upserted. Got {}",
                    rows.len()
                )));
            },
        };
        #[allow(clippy::cast_sign_loss)]
        Ok(hits as u32)
    }
}

fn add_duration(time: NaiveDateTime, duration: Duration) -> NaiveDateTime {
    chrono::Duration::from_std(duration)
        .ok()
        .and_then(|d| time.checked_add_signed(d))
        .unwrap_or(NaiveDateTime::MAX)
}

#[cfg(test)]
// Overflow in test code panics, which is the desired failure mode for a test.
#[allow(clippy::arithmetic_side_effects)]
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

    fn add(cache: &DedupCacheDatabase, i: usize, expires: Option<EpochTime>, is_signed: bool) -> u32 {
        cache
            .add_msg_hash(&i.to_le_bytes(), &CommsPublicKey::default(), expires, is_signed)
            .unwrap()
    }

    fn insert(cache: &DedupCacheDatabase, range: std::ops::Range<usize>) {
        for i in range {
            assert_eq!(add(cache, i, None, true), 1);
        }
    }

    /// Inserts without the growth trim.
    fn insert_raw(cache: &DedupCacheDatabase, i: usize, is_signed: bool) {
        let keep_until = cache.keep_until(Utc::now().naive_utc(), None, is_signed);
        cache
            .insert_body_hash_or_update_stats(
                &to_hex(&i.to_le_bytes()),
                &CommsPublicKey::default().to_hex(),
                keep_until,
                is_signed,
            )
            .unwrap();
    }

    /// Makes entry `i` look last hit `num_entries - i` seconds ago, so lower `i` is less recently hit.
    fn order_entries(cache: &DedupCacheDatabase, num_entries: usize) {
        let mut conn = cache.connection.get_pooled_connection().unwrap();
        let now = Utc::now().naive_utc();
        for i in 0..num_entries {
            let ago = chrono::Duration::seconds(i64::try_from(num_entries - i).unwrap());
            diesel::update(dedup_cache::table.filter(dedup_cache::body_hash.eq(to_hex(&i.to_le_bytes()))))
                .set(dedup_cache::last_hit_at.eq(now - ago))
                .execute(&mut conn)
                .unwrap();
        }
    }

    fn contains(cache: &DedupCacheDatabase, i: usize) -> bool {
        cache.get_hit_count(&i.to_le_bytes()).unwrap() > 0
    }

    fn keep_until_of(cache: &DedupCacheDatabase, i: usize) -> NaiveDateTime {
        let mut conn = cache.connection.get_pooled_connection().unwrap();
        dedup_cache::table
            .select(dedup_cache::keep_until)
            .filter(dedup_cache::body_hash.eq(to_hex(&i.to_le_bytes())))
            .first(&mut conn)
            .unwrap()
    }

    fn after(duration: Duration) -> NaiveDateTime {
        add_duration(Utc::now().naive_utc(), duration)
    }

    fn expires_in(duration: Duration) -> Option<EpochTime> {
        Some(EpochTime::from_secs_since_epoch(
            EpochTime::now().as_u64() + duration.as_secs(),
        ))
    }

    const AFTER_MIN_AGE: Duration = MIN_ENTRY_AGE.saturating_add(Duration::from_secs(60));

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
    fn it_keeps_unexpired_entries_above_capacity() {
        let cache = new_cache(10);
        insert(&cache, 0..20);
        assert_eq!(cache.trim_entries().unwrap(), 0);
        assert_eq!(row_count(&cache), 20);
    }

    #[test]
    fn it_trims_expired_entries_to_capacity() {
        let cache = new_cache(10);
        insert(&cache, 0..20);
        assert_eq!(cache.trim_entries_at(after(AFTER_MIN_AGE)).unwrap(), 10);
        assert_eq!(row_count(&cache), 10);
        assert!((0..10).all(|i| !contains(&cache, i)));
        assert!((10..20).all(|i| contains(&cache, i)));
    }

    #[test]
    fn it_keeps_entries_until_the_message_expires() {
        let cache = new_cache(1);
        add(&cache, 0, None, true);
        add(&cache, 1, expires_in(Duration::from_secs(10 * 60 * 60)), true);
        add(&cache, 2, None, true);
        assert_eq!(cache.trim_entries_at(after(AFTER_MIN_AGE)).unwrap(), 2);
        assert!(contains(&cache, 1));
        assert_eq!(row_count(&cache), 1);
    }

    #[test]
    fn a_far_future_expiry_is_clamped() {
        let cache = new_cache(1);
        let before = after(MAX_ENTRY_AGE);
        add(&cache, 0, expires_in(Duration::from_secs(100 * 60 * 60)), true);
        let keep_until = keep_until_of(&cache, 0);
        assert!(keep_until >= before);
        assert!(keep_until <= after(MAX_ENTRY_AGE));
    }

    #[test]
    fn an_unsigned_expiry_is_ignored() {
        let cache = new_cache(10);
        let far = expires_in(Duration::from_secs(20 * 60 * 60));
        let before = after(MIN_ENTRY_AGE);
        add(&cache, 0, far, false);
        assert!(keep_until_of(&cache, 0) >= before);
        assert!(keep_until_of(&cache, 0) <= after(MIN_ENTRY_AGE));
        // A hit with a far-future expiry does not raise it either
        let first = keep_until_of(&cache, 0);
        assert_eq!(add(&cache, 0, far, false), 2);
        assert!(keep_until_of(&cache, 0) <= after(MIN_ENTRY_AGE));
        assert!(keep_until_of(&cache, 0) < add_duration(first, Duration::from_secs(60 * 60)));
    }

    #[test]
    fn a_trim_removes_more_than_one_batch_in_order() {
        let cache = new_cache(10);
        let n = cache.trim_every_inserts;
        // 2.5 batches of expired entries above capacity, then the same above the ceiling
        let num_entries = 10 + 2 * n + n / 2;
        for i in 0..num_entries {
            insert_raw(&cache, i, true);
        }
        assert_eq!(cache.trim_entries_at(after(AFTER_MIN_AGE)).unwrap(), num_entries - 10);
        assert!((0..num_entries - 10).all(|i| !contains(&cache, i)));
        assert!((num_entries - 10..num_entries).all(|i| contains(&cache, i)));

        let cache = new_cache(10);
        let ceiling = 10 * CEILING_CAPACITY_MULTIPLE;
        let num_entries = ceiling + 2 * n + n / 2;
        // Three in four entries are unsigned, enough to cover the excess, which is more than one batch
        for i in 0..num_entries {
            insert_raw(&cache, i, i % 4 == 3);
        }
        order_entries(&cache, num_entries);
        let excess = num_entries - ceiling;
        assert_eq!(cache.trim_entries().unwrap(), excess);
        assert_eq!(row_count(&cache), i64::try_from(ceiling).unwrap());
        let unsigned = (0..num_entries).filter(|i| i % 4 != 3).collect::<Vec<_>>();
        assert!(unsigned.len() > excess && excess > n);
        assert!(unsigned.iter().take(excess).all(|i| !contains(&cache, *i)));
        assert!(unsigned.iter().skip(excess).all(|i| contains(&cache, *i)));
        assert!((0..num_entries).filter(|i| i % 4 == 3).all(|i| contains(&cache, i)));
    }

    #[test]
    fn a_hit_keeps_the_later_keep_until() {
        let cache = new_cache(10);
        add(&cache, 0, expires_in(Duration::from_secs(10 * 60 * 60)), true);
        let first = keep_until_of(&cache, 0);
        assert_eq!(add(&cache, 0, None, true), 2);
        assert_eq!(keep_until_of(&cache, 0), first);
        assert_eq!(add(&cache, 0, expires_in(Duration::from_secs(20 * 60 * 60)), true), 3);
        assert!(keep_until_of(&cache, 0) > first);
    }

    #[test]
    fn it_trims_unsigned_entries_first_above_the_ceiling() {
        let capacity = 1;
        let ceiling = capacity * CEILING_CAPACITY_MULTIPLE;
        let cache = new_cache(capacity);
        // Even entries are unsigned (8 of them), odd ones signed (7)
        for i in 0..15 {
            insert_raw(&cache, i, i % 2 == 1);
        }
        order_entries(&cache, 15);
        assert_eq!(cache.trim_entries().unwrap(), 5);
        assert_eq!(row_count(&cache), i64::try_from(ceiling).unwrap());
        assert!((0..15).filter(|i| i % 2 == 1).all(|i| contains(&cache, i)));
        // The 5 least recently hit unsigned entries are gone
        assert!([0, 2, 4, 6, 8].iter().all(|i| !contains(&cache, *i)));
        assert!([10, 12, 14].iter().all(|i| contains(&cache, *i)));
    }

    #[test]
    fn it_trims_signed_entries_when_unsigned_ones_are_not_enough() {
        let capacity = 1;
        let ceiling = capacity * CEILING_CAPACITY_MULTIPLE;
        let cache = new_cache(capacity);
        // Entries 0 and 1 are unsigned, the rest signed
        for i in 0..15 {
            insert_raw(&cache, i, i >= 2);
        }
        order_entries(&cache, 15);
        assert_eq!(cache.trim_entries().unwrap(), 5);
        assert_eq!(row_count(&cache), i64::try_from(ceiling).unwrap());
        assert!((0..5).all(|i| !contains(&cache, i)));
        assert!((5..15).all(|i| contains(&cache, i)));
    }

    #[test]
    fn a_trim_requested_during_a_trim_runs_another_pass() {
        let cache = new_cache(10).with_min_entry_age(Duration::ZERO);
        insert(&cache, 0..20);
        // Another trim is running: the request returns at once and leaves the rows to it
        cache.is_trimming.store(true, Ordering::SeqCst);
        assert_eq!(cache.trim_entries().unwrap(), 0);
        assert_eq!(row_count(&cache), 20);
        // The running trim, finishing its own pass, sees the request and runs another
        assert_eq!(cache.run_requested_trims().unwrap(), 10);
        assert_eq!(row_count(&cache), 10);
        assert!(!cache.is_trimming.load(Ordering::SeqCst));
        assert!(!cache.trim_requested.load(Ordering::SeqCst));
    }

    /// Concurrent hits on one hash each get their own count, so `dedup_allowed_message_occurrences` copies get through
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_hits_get_distinct_counts() {
        let cache = new_cache(50_000);
        let num_copies = 20;
        let mut tasks = Vec::new();
        for _ in 0..num_copies {
            let cache = cache.clone();
            tasks.push(task::spawn_blocking(move || add(&cache, 0, None, true)));
        }
        let mut counts = Vec::new();
        for t in tasks {
            counts.push(t.await.unwrap());
        }
        counts.sort_unstable();
        assert_eq!(counts, (1..=num_copies).collect::<Vec<u32>>());
    }

    #[test]
    fn trims_use_an_index() {
        #[derive(QueryableByName)]
        struct QueryPlanRow {
            #[diesel(sql_type = Text)]
            detail: String,
        }

        let cache = new_cache(10);
        let mut conn = cache.connection.get_pooled_connection().unwrap();
        let expired_plan = diesel::sql_query(format!("EXPLAIN QUERY PLAN {DELETE_EXPIRED_SQL}"))
            .bind::<sql_types::Timestamp, _>(Utc::now().naive_utc())
            .bind::<sql_types::BigInt, _>(1)
            .load::<QueryPlanRow>(&mut conn)
            .unwrap();
        let oldest_plan = diesel::sql_query(format!("EXPLAIN QUERY PLAN {DELETE_OLDEST_SQL}"))
            .bind::<sql_types::Bool, _>(false)
            .bind::<sql_types::BigInt, _>(1)
            .load::<QueryPlanRow>(&mut conn)
            .unwrap();
        for (plan, index) in [
            (expired_plan, "idx_dedup_cache_keep_until"),
            (oldest_plan, "idx_dedup_cache_is_signed_last_hit_at"),
        ] {
            let details = plan.iter().map(|row| row.detail.as_str()).collect::<Vec<_>>();
            assert!(
                details.iter().any(|d| d.contains(index)),
                "query plan does not use {index}: {details:?}"
            );
            assert!(
                !details.iter().any(|d| d.contains("TEMP B-TREE")),
                "query plan sorts without an index: {details:?}"
            );
        }
    }

    /// The DHT pipeline runs up to 100 inbound tasks at once, each inserting on a blocking thread. None of them may
    /// time out waiting for a pooled connection, because a dedup error drops the message.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_inserts_do_not_exhaust_the_pool() {
        let cache = new_cache(50_000);
        let mut tasks = Vec::new();
        for i in 0usize..100 {
            let cache = cache.clone();
            tasks.push(task::spawn_blocking(move || add(&cache, i, None, true)));
        }
        for t in tasks {
            assert_eq!(t.await.unwrap(), 1);
        }
        assert_eq!(row_count(&cache), 100);
    }
}
