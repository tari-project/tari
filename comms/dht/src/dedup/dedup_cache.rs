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

use chrono::{NaiveDateTime, Utc};
use diesel::{dsl, result::DatabaseErrorKind, sql_types, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl};
use log::*;
use tari_comms::types::CommsPublicKey;
use tari_crypto::tari_utilities::hex::Hex;

use crate::{
    schema::dedup_cache,
    storage::{DbConnection, StorageError},
};

const LOG_TARGET: &str = "comms::dht::dedup_cache";

#[derive(Queryable, PartialEq, Debug)]
struct DedupCacheEntry {
    body_hash: String,
    sender_public_ke: String,
    number_of_hit: i32,
    stored_at: NaiveDateTime,
    last_hit_at: NaiveDateTime,
}

#[derive(Clone)]
pub struct DedupCacheDatabase {
    connection: DbConnection,
    capacity: usize,
}

impl DedupCacheDatabase {
    pub fn new(connection: DbConnection, capacity: usize) -> Self {
        debug!(
            target: LOG_TARGET,
            "Message dedup cache capacity initialized at {}", capacity,
        );
        Self { connection, capacity }
    }

    /// Adds the body hash to the cache, returning the number of hits (inclusive) that have been recorded for this body
    /// hash
    pub fn add_body_hash(&self, body_hash: Vec<u8>, public_key: CommsPublicKey) -> Result<u32, StorageError> {
        let hit_count = self.insert_body_hash_or_update_stats(body_hash.to_hex(), public_key.to_hex())?;

        if hit_count == 0 {
            warn!(
                target: LOG_TARGET,
                "Unable to insert new entry into message dedup cache"
            );
        }
        Ok(hit_count)
    }

    pub fn get_hit_count(&self, body_hash: Vec<u8>) -> Result<u32, StorageError> {
        let conn = self.connection.get_pooled_connection()?;
        let hit_count = dedup_cache::table
            .select(dedup_cache::number_of_hits)
            .filter(dedup_cache::body_hash.eq(&body_hash.to_hex()))
            .get_result::<i32>(&conn)
            .optional()?;

        Ok(hit_count.unwrap_or(0) as u32)
    }

    /// Trims the dedup cache to the configured limit by removing the oldest entries
    pub fn trim_entries(&self) -> Result<usize, StorageError> {
        let capacity = self.capacity as i64;
        let mut num_removed = 0;
        let conn = self.connection.get_pooled_connection()?;
        let msg_count = dedup_cache::table
            .select(dsl::count(dedup_cache::id))
            .first::<i64>(&conn)?;
        // Hysteresis added to minimize database impact
        if msg_count > capacity {
            let remove_count = msg_count - capacity;
            num_removed = diesel::sql_query(
                "DELETE FROM dedup_cache WHERE id IN (SELECT id FROM dedup_cache ORDER BY last_hit_at ASC LIMIT $1)",
            )
            .bind::<sql_types::BigInt, _>(remove_count)
            .execute(&conn)?;
        }
        debug!(
            target: LOG_TARGET,
            "Message dedup cache: count {}, capacity {}, removed {}", msg_count, capacity, num_removed,
        );
        Ok(num_removed)
    }

    /// Insert new row into the table or updates an existing row. Returns the number of hits for this body hash.
    fn insert_body_hash_or_update_stats(&self, body_hash: String, public_key: String) -> Result<u32, StorageError> {
        let conn = self.connection.get_pooled_connection()?;
        let insert_result = diesel::insert_into(dedup_cache::table)
            .values((
                dedup_cache::body_hash.eq(&body_hash),
                dedup_cache::sender_public_key.eq(&public_key),
                dedup_cache::number_of_hits.eq(1),
                dedup_cache::last_hit_at.eq(Utc::now().naive_utc()),
            ))
            .execute(&conn);
        match insert_result {
            Ok(1) => Ok(1),
            Ok(n) => Err(StorageError::UnexpectedResult(format!(
                "Expected exactly one row to be inserted. Got {}",
                n
            ))),
            Err(diesel::result::Error::DatabaseError(kind, e_info)) => match kind {
                DatabaseErrorKind::UniqueViolation => {
                    // Update hit stats for the message
                    diesel::update(dedup_cache::table.filter(dedup_cache::body_hash.eq(&body_hash)))
                        .set((
                            dedup_cache::sender_public_key.eq(&public_key),
                            dedup_cache::number_of_hits.eq(dedup_cache::number_of_hits + 1),
                            dedup_cache::last_hit_at.eq(Utc::now().naive_utc()),
                        ))
                        .execute(&conn)?;

                    let hits = dedup_cache::table
                        .select(dedup_cache::number_of_hits)
                        .filter(dedup_cache::body_hash.eq(&body_hash))
                        .get_result::<i32>(&conn)?;

                    Ok(hits as u32)
                },
                _ => Err(diesel::result::Error::DatabaseError(kind, e_info).into()),
            },
            Err(e) => Err(e.into()),
        }
    }
}
