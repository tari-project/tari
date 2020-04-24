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
    chain_storage::postgres_db::{error::PostgresError, schema::*},
    transactions::{transaction, types::HashOutput},
};
use chrono::{NaiveDateTime, Utc};
use diesel::{self, prelude::*};
use tari_crypto::tari_utilities::{byte_array::ByteArray, hex::Hex, Hashable};

const LOG_TARGET: &str = "b::c::storage::postgres:kernels";

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "kernels"]
#[primary_key(hash)]
pub struct Kernels {
    hash: String,
    features: i16,
    fee: i64,
    lock_height: i64,
    meta_info: Option<String>,
    linked_kernel: Option<String>,
    excess: String,
    excess_sig_nonce: Vec<u8>,
    excess_sig_sig: Vec<u8>,
    block_hash: String,
    created_at: NaiveDateTime,
}

impl Kernels {
    /// This function will seach for a kernel via hash.
    pub fn fetch_by_hash(hash: &HashOutput, conn: &PgConnection) -> Result<Option<Kernels>, PostgresError> {
        let hex_hash = hash.to_hex();
        let mut results: Vec<Kernels> = kernels::table
            .filter(kernels::hash.eq(&hex_hash))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results.pop())
    }

    /// This function will insert a new kernel only if the kernel does not exist.
    pub fn insert(
        hash: HashOutput,
        block: String,
        kernel: transaction::TransactionKernel,
        conn: &PgConnection,
    ) -> Result<(), PostgresError>
    {
        let row: Kernels = Kernels::from_transaction_kernel(kernel, block)?;
        if row.hash != hash.to_hex() {
            return Err(PostgresError::Other("Kernel and kernel hash don't match".to_string()));
        }

        diesel::insert_into(kernels::table)
            .values(row)
            .execute(conn)
            .map_err(|e| PostgresError::CouldNotAdd(e.to_string()))?;

        Ok(())
    }

    /// This function will delete the  kernel with the provided hash
    pub fn delete_at_hash(hash: HashOutput, conn: &PgConnection) -> Result<(), PostgresError> {
        let hash_key = hash.to_hex();
        diesel::delete(kernels::table.filter(kernels::hash.eq(hash_key)))
            .execute(conn)
            .map_err(|e| PostgresError::CouldDelete(e.to_string()))?;
        Ok(())
    }

    fn from_transaction_kernel(value: transaction::TransactionKernel, block: String) -> Result<Kernels, PostgresError> {
        Ok(Kernels {
            hash: value.hash().to_hex(),
            features: value.features.bits() as i16,
            fee: value.fee.0 as i64,
            lock_height: value.lock_height as i64,
            meta_info: value.meta_info.map(|mi| mi.to_hex()),
            linked_kernel: value.linked_kernel.map(|lk| lk.to_hex()),
            excess: value.excess.to_hex(),
            excess_sig_nonce: value.excess_sig.get_public_nonce().to_vec(),
            excess_sig_sig: value.excess_sig.get_signature().to_vec(),
            block_hash: block,
            created_at: Utc::now().naive_utc(),
        })
    }
}
