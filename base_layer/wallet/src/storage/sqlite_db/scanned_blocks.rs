// Copyright 2021. The Taiji Project
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
use taiji_common_types::types::FixedHash;
use taiji_core::transactions::taiji_amount::MicroMinotaiji;
use tari_utilities::ByteArray;

use crate::{
    diesel::BoolExpressionMethods,
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

    pub fn new(header_hash: Vec<u8>, height: i64) -> Self {
        Self {
            header_hash,
            height,
            num_outputs: None,
            amount: None,
            timestamp: Utc::now().naive_utc(),
        }
    }

    pub fn new_with_amount(header_hash: Vec<u8>, height: i64, num_outputs: i64, amount: i64) -> Self {
        Self {
            header_hash,
            height,
            num_outputs: Some(num_outputs),
            amount: Some(amount),
            timestamp: Utc::now().naive_utc(),
        }
    }

    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::insert_into(scanned_blocks::table)
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
}

impl From<ScannedBlock> for ScannedBlockSql {
    fn from(sb: ScannedBlock) -> Self {
        Self {
            header_hash: sb.header_hash.to_vec(),
            height: sb.height as i64,
            num_outputs: sb.num_outputs.map(|n| n as i64),
            amount: sb.amount.map(|a| a.as_u64() as i64),
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
            num_outputs: sb.num_outputs.map(|n| n as u64),
            amount: sb.amount.map(|a| MicroMinotaiji::from(a as u64)),
            timestamp: sb.timestamp,
        })
    }
}
