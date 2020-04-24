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
    transactions::{
        transaction::{OutputFeatures, OutputFlags, TransactionOutput},
        types::{Commitment, HashOutput, RangeProof},
    },
};
use diesel::{self, prelude::*};
use std::convert::TryFrom;
use tari_crypto::tari_utilities::{byte_array::ByteArray, hex::Hex, Hashable};

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "tx_outputs"]
#[primary_key(hash)]
pub struct TxOutput {
    hash: String,
    features_flags: i16,
    features_maturity: i64,
    commitment: String,
    proof: Option<Vec<u8>>,
    created_in_block: String,
    spent: Option<String>,
}
#[allow(clippy::ptr_arg)]
impl TxOutput {
    /// This will insert a transactional output if it does not exist.
    pub fn insert(output: &TransactionOutput, block: String, conn: &PgConnection) -> Result<bool, PostgresError> {
        let hash = output.hash();

        let row: TxOutput = TxOutput::from_transaction_output(output, block)?;
        if row.hash != hash.to_hex() {
            return Err(PostgresError::Other("tx and tx hash don't match".to_string()));
        }

        diesel::insert_into(tx_outputs::table)
            .values(&row)
            .execute(conn)
            .map_err(|e| PostgresError::CouldNotAdd(e.to_string()))?;

        Ok(true)
    }

    /// This will fetch a transactional output via a hash
    pub fn fetch(hash: &HashOutput, conn: &PgConnection) -> Result<Option<TxOutput>, PostgresError> {
        let mut results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::hash.eq(hash.to_hex()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results.pop())
    }

    /// This will fetch all block outputs of the given block
    pub fn fetch_block_outputs(hash: &HashOutput, conn: &PgConnection) -> Result<Vec<TxOutput>, PostgresError> {
        let results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::created_in_block.eq(hash.to_hex()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results)
    }

    /// This will fetch all block inputs of the given block
    pub fn fetch_block_inputs(hash: &HashOutput, conn: &PgConnection) -> Result<Vec<TxOutput>, PostgresError> {
        let mut results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::spent.eq(hash.to_hex()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results)
    }

    // This will a transactional output only if the output is unspent
    pub fn fetch_unspent_output(hash: &HashOutput, conn: &PgConnection) -> Result<Vec<TxOutput>, PostgresError> {
        let results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::hash.eq(hash.to_hex()))
            .filter(tx_outputs::spent.is_null())
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results)
    }

    /// Creates a TxOutput from the provided TransactionalOutput
    pub fn from_transaction_output(output: &TransactionOutput, block: String) -> Result<TxOutput, PostgresError> {
        Ok(TxOutput {
            hash: output.hash().to_hex(),

            features_flags: output.features.flags.bits() as i16,
            features_maturity: output.features.maturity as i64,
            commitment: output.commitment.to_hex(),
            proof: Some(output.proof.0.clone()),
            created_in_block: block,
            spent: None,
        })
    }
}

impl TryFrom<TxOutput> for TransactionOutput {
    type Error = PostgresError;

    fn try_from(value: TxOutput) -> Result<Self, Self::Error> {
        let result = Self {
            features: OutputFeatures {
                flags: OutputFlags::from_bits_truncate(value.features_flags as u8),
                maturity: value.features_maturity as u64,
            },
            commitment: Commitment::from_hex(&value.commitment)?,
            proof: RangeProof::from_bytes(
                &value
                    .proof
                    .ok_or_else(|| PostgresError::Other("No proof found".to_string()))?,
            )
            .map_err(|e| PostgresError::Other(e.to_string()))?,
        };

        if result.hash().to_hex() != value.hash {
            return Err(PostgresError::Other("tx and tx hash don't match".to_string()));
        }
        Ok(result)
    }
}
