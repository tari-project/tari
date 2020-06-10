// Copyright 2020. The Tari Project
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
    chain_storage::postgres_db::{
        error::PostgresError,
        models::{block_headers::BlockHeader, tx_outputs::TxOutput},
        schema::*,
    },
    transactions::types::HashOutput,
};
use diesel::{self, prelude::*};
use tari_crypto::tari_utilities::hex::Hex;
use uuid::Uuid;

#[derive(Queryable, Identifiable)]
#[table_name = "spends"]
pub struct Spent {
    pub id: Uuid,
    pub spent_in_block: String,
    pub tx_output: String,
}

#[derive(Queryable, Insertable)]
#[table_name = "spends"]
struct NewSpent {
    pub spent_in_block: String,
    pub tx_output: String,
}

#[allow(clippy::ptr_arg)]
impl Spent {
    /// This will insert a transactional output if it does not exist.
    pub fn insert(spent_in_block: String, tx_output: String, conn: &PgConnection) -> Result<bool, PostgresError> {
        let row = NewSpent {
            spent_in_block,
            tx_output,
        };
        diesel::insert_into(spends::table)
            .values(&row)
            .execute(conn)
            .map_err(|e| PostgresError::CouldNotAdd(e.to_string()))?;

        Ok(true)
    }

    pub fn fetch_spent_output(hash: &HashOutput, conn: &PgConnection) -> Result<Option<TxOutput>, PostgresError> {
        let hex = hash.to_hex();
        let results: Vec<Spent> = spends::table
            .filter(spends::tx_output.eq(hex.clone()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;
        if results.is_empty() {
            return Ok(None);
        };
        let header = BlockHeader::fetch_by_hex(&results[0].spent_in_block, conn)?;
        if header.is_none() || header.unwrap().orphan {
            return Ok(None);
        };
        let mut results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::hash.eq(hex))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results.pop())
    }
}
