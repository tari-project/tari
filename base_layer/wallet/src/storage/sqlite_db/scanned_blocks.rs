// Copyright 2021. The Tari Project
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

use std::convert::TryFrom;

use chrono::{NaiveDateTime, Utc};
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};
use tari_common_types::types::FixedHash;
use tari_utilities::ByteArray;

use crate::{
    diesel::{BoolExpressionMethods, OptionalExtension},
    error::WalletStorageError,
    schema::scanned_blocks,
    utxo_scanner_service::service::ScannedBlock,
};

/// Number of recent blocks to keep without any pruning (i.e. the "hot" cache window).
const SCANNED_BLOCK_CACHE_SIZE: i64 = 720;
/// Boundary for the second tier: blocks between `tip - TIER2_BOUNDARY` and `tip - CACHE_SIZE`.
const TIER2_BOUNDARY: i64 = 10_000;
/// Boundary for the third tier: blocks between `tip - TIER3_BOUNDARY` and `tip - TIER2_BOUNDARY`.
const TIER3_BOUNDARY: i64 = 100_000;
/// Sparse interval for tier 2: keep 1 per TIER2_INTERVAL blocks.
const TIER2_INTERVAL: i64 = 100;
/// Sparse interval for tier 3: keep 1 per TIER3_INTERVAL blocks.
const TIER3_INTERVAL: i64 = 1_000;
/// Sparse interval for tier 4 (oldest blocks): keep 1 per TIER4_INTERVAL blocks.
const TIER4_INTERVAL: i64 = 5_000;

#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[diesel(table_name = scanned_blocks)]
pub struct ScannedBlockSql {
    header_hash: Vec<u8>,
    height: i64,
    num_outputs: Option<i64>,
    amount: Option<i64>,
    timestamp: NaiveDateTime,
}

impl ScannedBlockSql {
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<ScannedBlockSql>, WalletStorageError> {
        Ok(scanned_blocks::table
            .order(scanned_blocks::height.desc())
            .load::<ScannedBlockSql>(conn)?)
    }

    pub fn last_height(conn: &mut SqliteConnection) -> Result<Option<i64>, WalletStorageError> {
        let result = scanned_blocks::table
            .select(scanned_blocks::height)
            .order(scanned_blocks::height.desc())
            .first::<i64>(conn)
            .optional()?;
        Ok(result)
    }

    pub fn new(header_hash: Vec<u8>, height: i64) -> Self {
        Self {
            header_hash,
            height,
            amount: None,
            num_outputs: None,
            timestamp: Utc::now().naive_utc(),
        }
    }

    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::replace_into(scanned_blocks::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn clear_all(conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::delete(scanned_blocks::table).execute(conn)?;
        Ok(())
    }

    /// Clear Scanned Blocks from the given height and higher
    pub fn clear_from_and_higher(height: u64, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::delete(scanned_blocks::table.filter(scanned_blocks::height.ge(height as i64))).execute(conn)?;
        Ok(())
    }

    pub fn clear_before_height(
        height: u64,
        exclude_recovered: bool,
        conn: &mut SqliteConnection,
    ) -> Result<(), WalletStorageError> {
        let mut query = diesel::delete(scanned_blocks::table)
            .into_boxed()
            .filter(scanned_blocks::height.lt(height as i64));
        if exclude_recovered {
            query = query.filter(
                scanned_blocks::num_outputs
                    .is_null()
                    .or(scanned_blocks::num_outputs.eq(0)),
            );
        }

        query.execute(conn)?;
        Ok(())
    }

    /// Prune scanned blocks using tiered sparse retention.
    ///
    /// Retention tiers relative to `tip_height`:
    /// - `tip - SCANNED_BLOCK_CACHE_SIZE` to `tip`: keep all headers
    /// - `tip - TIER2_BOUNDARY` to `tip - SCANNED_BLOCK_CACHE_SIZE`: keep 1 per `TIER2_INTERVAL` blocks
    /// - `tip - TIER3_BOUNDARY` to `tip - TIER2_BOUNDARY`: keep 1 per `TIER3_INTERVAL` blocks
    /// - below `tip - TIER3_BOUNDARY`: keep 1 per `TIER4_INTERVAL` blocks
    ///
    /// Blocks with recovered outputs (`num_outputs > 0`) are always preserved.
    /// Each block is deleted in its own transaction to avoid size-limit issues.
    pub fn prune_sparse(
        tip_height: u64,
        exclude_recovered: bool,
        conn: &mut SqliteConnection,
    ) -> Result<usize, WalletStorageError> {
        let tip = tip_height as i64;
        let tier1_boundary = tip.saturating_sub(SCANNED_BLOCK_CACHE_SIZE);
        let tier2_boundary = tip.saturating_sub(TIER2_BOUNDARY);
        let tier3_boundary = tip.saturating_sub(TIER3_BOUNDARY);

        // Collect heights eligible for pruning.
        let candidates: Vec<i64> = scanned_blocks::table
            .select(scanned_blocks::height)
            .filter(scanned_blocks::height.lt(tier1_boundary))
            .order(scanned_blocks::height.asc())
            .load::<i64>(conn)?;

        let mut deleted = 0usize;
        for h in candidates {
            let interval = if h >= tier2_boundary {
                TIER2_INTERVAL
            } else if h >= tier3_boundary {
                TIER3_INTERVAL
            } else {
                TIER4_INTERVAL
            };

            // Keep blocks that land on the sparse interval boundary.
            if h % interval == 0 {
                continue;
            }

            // Process each deletion in a separate statement to avoid large transaction issues.
            let mut query = diesel::delete(scanned_blocks::table)
                .into_boxed()
                .filter(scanned_blocks::height.eq(h));

            if exclude_recovered {
                query = query.filter(
                    scanned_blocks::num_outputs
                        .is_null()
                        .or(scanned_blocks::num_outputs.eq(0)),
                );
            }

            deleted += query.execute(conn)?;
        }

        Ok(deleted)
    }
}

impl From<ScannedBlock> for ScannedBlockSql {
    fn from(sb: ScannedBlock) -> Self {
        Self {
            header_hash: sb.header_hash.to_vec(),
            height: sb.height as i64,
            amount: None,
            num_outputs: None,
            timestamp: sb.timestamp,
        }
    }
}

impl TryFrom<ScannedBlockSql> for ScannedBlock {
    type Error = String;

    fn try_from(sb: ScannedBlockSql) -> Result<Self, Self::Error> {
        Ok(Self {
            header_hash: FixedHash::try_from(sb.header_hash).map_err(|err| err.to_string())?,
            height: sb.height as u64,
            timestamp: sb.timestamp,
        })
    }
}
