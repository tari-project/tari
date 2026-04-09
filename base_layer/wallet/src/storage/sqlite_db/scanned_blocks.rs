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

    /// Apply a sparse storage schedule to scanned blocks, keeping:
    /// - All blocks within `tip - 720` to `tip`
    /// - Every 100th block from `tip - 10,000` to `tip - 720`
    /// - Every 1,000th block from `tip - 100,000` to `tip - 10,000`
    /// - Every 5,000th block from genesis to `tip - 100,000`
    /// Blocks containing recovered outputs (num_outputs > 0) are always preserved.
    pub fn apply_sparse_schedule(tip_height: u64, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        let blocks = Self::index(conn)?;
        for block in blocks {
            let height = block.height as u64;
            if let Some(outputs) = block.num_outputs {
                if outputs > 0 {
                    continue;
                }
            }
            if !Self::should_keep_at_height(height, tip_height) {
                diesel::delete(scanned_blocks::table.filter(scanned_blocks::height.eq(block.height))).execute(conn)?;
            }
        }
        Ok(())
    }

    /// Determine whether a block at `height` should be kept given the current `tip`.
    fn should_keep_at_height(height: u64, tip: u64) -> bool {
        let depth = tip.saturating_sub(height);
        if depth <= 720 {
            true
        } else if depth <= 10_000 {
            height % 100 == 0
        } else if depth <= 100_000 {
            height % 1_000 == 0
        } else {
            height % 5_000 == 0
        }
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
