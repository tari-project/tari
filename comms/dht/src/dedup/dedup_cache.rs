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

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use chrono::{NaiveDateTime, Utc};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, dsl, result::DatabaseErrorKind, sql_types};
use log::*;
use tari_common_sqlite::{connection::DbConnection, error::StorageError};
use tari_comms::types::CommsPublicKey;
use tari_utilities::hex::{Hex, to_hex};

use crate::schema::dedup_cache;

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

#[derive(Clone)]
pub struct DedupCacheDatabase {
    connection: DbConnection,
    capacity: usize,
    /// Trim once every this many new entries, so the table stays below `capacity + trim_every_inserts` rows.
    trim_every_inserts: usize,
    /// New entries inserted, shared by all clones.
    num_inserts: Arc<AtomicUsize>,
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
        }
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

    /// Trims the dedup cache to the configured limit by removing the oldest entries
    pub fn trim_entries(&self) -> Result<usize, StorageError> {
        #[allow(clippy::cast_possible_wrap)]
        let capacity = self.capacity as i64;
        let mut num_removed = 0;
        let mut conn = self.connection.get_pooled_connection()?;
        let msg_count = dedup_cache::table
            .select(dsl::count(dedup_cache::id))
            .first::<i64>(&mut conn)?;
        // Hysteresis added to minimize database impact
        if msg_count > capacity {
            let remove_count = msg_count.saturating_sub(capacity);
            num_removed = diesel::sql_query(
                "DELETE FROM dedup_cache WHERE id IN (SELECT id FROM dedup_cache ORDER BY last_hit_at ASC LIMIT $1)",
            )
            .bind::<sql_types::BigInt, _>(remove_count)
            .execute(&mut conn)?;
        }
        debug!(
            target: LOG_TARGET,
            "Message dedup cache: count {msg_count}, capacity {capacity}, removed {num_removed}"
        );
        Ok(num_removed)
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

    #[test]
    fn it_trims_as_it_grows() {
        let capacity = 1_000;
        let cache = new_cache(capacity);
        let n = cache.trim_every_inserts;
        assert_eq!(n, 100);

        for i in 0..(capacity + 2 * n) {
            let num_hits = cache
                .add_msg_hash(&i.to_le_bytes(), &CommsPublicKey::default())
                .unwrap();
            assert_eq!(num_hits, 1);
            assert!(row_count(&cache) <= i64::try_from(capacity + n).unwrap());
        }
        assert!(row_count(&cache) <= i64::try_from(capacity + n).unwrap());
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
        let plan = diesel::sql_query("EXPLAIN QUERY PLAN SELECT id FROM dedup_cache ORDER BY last_hit_at ASC LIMIT 1")
            .load::<QueryPlanRow>(&mut conn)
            .unwrap();
        assert!(
            plan.iter()
                .any(|row| row.detail.contains("idx_dedup_cache_last_hit_at")),
            "query plan does not use the index"
        );
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
