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
    blocks,
    blocks::BlockHash,
    chain_storage::{
        postgres_db::{models::error::PostgresError, schema::*},
        DbKeyValuePair,
        DbValue,
    },
    transactions::types::{BlindingFactor, HashOutput},
};
use chrono::{NaiveDateTime, Utc};
use diesel::{self, expression::dsl, prelude::*, OptionalExtension};
use log::*;
use serde_json::Value;
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "b::c::storage::postgres:block_headers";

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "kernels"]
#[primary_key(hash)]
pub struct Kernels {
    hash: String,
    features: i32,
    fee: i64,
    lock_height: i64,
    meta_info: Option<String>,
    linked_kernel: Option<String>,
    excess: String,
    excess_sig_nonce: Vec<u8>,
    excess_sig_sig: Vec<u8>,
    created_at: NaiveDateTime,
}

impl Kernels {
    /// This function will seach for a block header via hash, it will return orphan block headers as well.
    pub fn fetch_by_hash(hash: HashOutput, conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresError> {
        let mut results: Vec<Kernels> = kernels::table
            .filter(kernels::hash.eq(&hash))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results.pop())
    }

    /// This function will insert a new block header only if the block header does not exist.
    pub fn insert_if_not_exists(
        hash: HashOutput,
        kernel: transaction::TransactionKernel,
        conn: &PgConnection,
    ) -> Result<(), PostgresError>
    {
        if Kernels::fetch_by_hash(&hash, conn)?.is_some() {
            warn!(
                target: LOG_TARGET,
                "Tried to insert kernel with hash:{} but it already exists",
                hash.to_hex()
            );
            return Ok(());
        }

        let row: TransactionKernel = kernel.into();
        if row.hash != hash.to_hex() {
            return Err(PostgresError::Other("Kernel and kernel hash don't match".to_string()));
        }

        diesel::insert_into(transaction_kernels::table)
            .values(row)
            .execute(conn)
            .map_err(|e| PostgresError::CouldNotAdd(e.to_string()))?;

        Ok(())
    }

    pub fn delete_at_hash(hash: HashOutput, conn: &PgConnection) -> Result<(), PostgresError> {
        diesel::delete(kernels::table.filter(kernels::hash.eq(hash)))
            .execute(conn)
            .map_err(|e| PostgresError::CouldDelete(e.to_string()))?;
        Ok(())
    }
}

impl From<transaction::TransactionKernel> for TransactionKernel {
    fn from(value: transaction::TransactionKernel) -> Self {
        Self {
            hash: value.hash().to_hex(),
            features: value.features.bits() as i32,
            fee: value.fee.0 as i64,
            lock_height: value.lock_height as i64,
            meta_info: value.meta_info.map(|mi| mi.to_hex()),
            linked_kernel: value.linked_kernel.map(|lk| lk.to_hex()),
            excess: value.excess.to_hex(),
            excess_sig_nonce: value.excess_sig.get_public_nonce().to_vec(),
            excess_sig_sig: value.excess_sig.get_signature().to_vec(),
            created_at: Utc::now().naive_utc(),
        }
    }
}
