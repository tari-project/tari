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

use crate::{
    schema::dedup_cache,
    storage::{DbConnection, StorageError},
};
use chrono::Utc;
use diesel::{dsl, result::DatabaseErrorKind, ExpressionMethods, QueryDsl, RunQueryDsl};
use log::*;
use tari_comms::types::CommsPublicKey;
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_utilities::hex;

const LOG_TARGET: &str = "comms::dht::dedup_cache";

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

    /// Inserts and returns Ok(true) if the item already existed and Ok(false) if it didn't, also updating hit stats
    pub async fn insert_body_hash_if_unique(
        &self,
        body_hash: Vec<u8>,
        public_key: CommsPublicKey,
    ) -> Result<bool, StorageError> {
        let body_hash = hex::to_hex(&body_hash.as_bytes());
        let public_key = public_key.to_hex();
        match self
            .insert_body_hash_or_update_stats(body_hash.clone(), public_key.clone())
            .await
        {
            Ok(val) => {
                if val == 0 {
                    warn!(
                        target: LOG_TARGET,
                        "Unable to insert new entry into message dedup cache"
                    );
                }
                Ok(false)
            },
            Err(e) => match e {
                StorageError::UniqueViolation(_) => Ok(true),
                _ => Err(e),
            },
        }
    }

    /// Trims the dedup cache to the configured limit by removing the oldest entries
    pub async fn truncate(&self) -> Result<usize, StorageError> {
        let capacity = self.capacity;
        self.connection
            .with_connection_async(move |conn| {
                let mut num_removed = 0;
                let msg_count = dedup_cache::table
                    .select(dsl::count(dedup_cache::id))
                    .first::<i64>(conn)? as usize;
                // Hysteresis added to minimize database impact
                if msg_count > capacity {
                    let remove_count = msg_count - capacity;
                    num_removed = diesel::delete(dedup_cache::table)
                        .filter(
                            dedup_cache::id.eq_any(
                                dedup_cache::table
                                    .select(dedup_cache::id)
                                    .order_by(dedup_cache::last_hit_at.asc())
                                    .limit(remove_count as i64)
                                    .get_results::<i32>(conn)?,
                            ),
                        )
                        .execute(conn)?;
                }
                debug!(
                    target: LOG_TARGET,
                    "Message dedup cache: count {}, capacity {}, removed {}", msg_count, capacity, num_removed,
                );
                Ok(num_removed)
            })
            .await
    }

    // Insert new row into the table or update existing row in an atomic fashion; more than one thread can access this
    // table at the same time.
    async fn insert_body_hash_or_update_stats(
        &self,
        body_hash: String,
        public_key: String,
    ) -> Result<usize, StorageError> {
        self.connection
            .with_connection_async(move |conn| {
                let insert_result = diesel::insert_into(dedup_cache::table)
                    .values((
                        dedup_cache::body_hash.eq(body_hash.clone()),
                        dedup_cache::sender_public_key.eq(public_key.clone()),
                        dedup_cache::number_of_hits.eq(1),
                        dedup_cache::last_hit_at.eq(Utc::now().naive_utc()),
                    ))
                    .execute(conn);
                match insert_result {
                    Ok(val) => Ok(val),
                    Err(diesel::result::Error::DatabaseError(kind, e_info)) => match kind {
                        DatabaseErrorKind::UniqueViolation => {
                            // Update hit stats for the message
                            let result =
                                diesel::update(dedup_cache::table.filter(dedup_cache::body_hash.eq(&body_hash)))
                                    .set((
                                        dedup_cache::sender_public_key.eq(public_key),
                                        dedup_cache::number_of_hits.eq(dedup_cache::number_of_hits + 1),
                                        dedup_cache::last_hit_at.eq(Utc::now().naive_utc()),
                                    ))
                                    .execute(conn);
                            match result {
                                Ok(_) => Err(StorageError::UniqueViolation(body_hash)),
                                Err(e) => Err(e.into()),
                            }
                        },
                        _ => Err(diesel::result::Error::DatabaseError(kind, e_info).into()),
                    },
                    Err(e) => Err(e.into()),
                }
            })
            .await
    }
}
