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
    chain_storage::{
        postgres_db::{error::PostgresError, schema::*},
        DbValue,
    },
    transactions::{
        transaction::{OutputFeatures, OutputFlags, TransactionOutput},
        types::{Commitment, HashOutput, RangeProof},
    },
};
use diesel::{self, prelude::*};
use log::*;
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::{byte_array::ByteArray, hex::Hex, Hashable};

const LOG_TARGET: &str = "b::c::storage::postgres:tx_outputs";

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "tx_outputs"]
#[primary_key(hash)]
pub struct TxOutput {
    hash: String,
    features_flags: i16,
    features_maturity: i64,
    commitment: String,
    proof: Option<Vec<u8>>,
    tx_output: Option<String>,
    spent: Option<String>,
}

impl TxOutput {
    pub fn insert_if_not_exists(output: &TransactionOutput, conn: &PgConnection) -> Result<bool, PostgresError> {
        let hash = output.hash();

        if TxOutput::fetch(&hash, conn)?.is_some() {
            warn!(
                target: LOG_TARGET,
                "Tried to insert transactional output with hash:{} but it already exists",
                hash.to_hex()
            );

            return Ok(false);
        }

        let row: TxOutput = output.try_into()?;
        if row.hash != hash.to_hex() {
            return Err(PostgresError::Other("tx and tx hash don't match".to_string()));
        }

        diesel::insert_into(tx_outputs::table)
            .values(&row)
            .execute(conn)
            .map_err(|e| PostgresError::CouldNotAdd(e.to_string()))?;

        Ok(true)
    }

    pub fn fetch(hash: &HashOutput, conn: &PgConnection) -> Result<Option<TxOutput>, PostgresError> {
        let mut results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::hash.eq(hash.to_hex()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results.pop())
    }

    pub fn fetch_output(hash: &HashOutput, conn: &PgConnection) -> Result<Vec<TxOutput>, PostgresError> {
        let mut results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::tx_output.eq(hash.to_hex()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results)
    }

    pub fn fetch_spent_output(hash: &HashOutput, conn: &PgConnection) -> Result<Vec<TxOutput>, PostgresError> {
        let mut results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::spent.eq(hash.to_hex()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results)
    }

    pub fn fetch_unspent_output(hash: &HashOutput, conn: &PgConnection) -> Result<Vec<TxOutput>, PostgresError> {
        let mut results: Vec<TxOutput> = tx_outputs::table
            .filter(tx_outputs::hash.eq(hash.to_hex()))
            .filter(tx_outputs::spent.eq("".to_string()))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results)
    }
}

impl TryFrom<&TransactionOutput> for TxOutput {
    type Error = PostgresError;

    fn try_from(value: &TransactionOutput) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: value.hash().to_hex(),

            features_flags: value.features.flags.bits() as i16,
            features_maturity: value.features.maturity as i64,
            commitment: value.commitment.to_hex(),
            proof: Some(value.proof.0.clone()),
            tx_output: None,
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
            proof: RangeProof::from_bytes(&value.proof.ok_or(PostgresError::Other("No proof found".to_string()))?)
                .map_err(|e| PostgresError::Other(e.to_string()))?,
        };

        if result.hash().to_hex() != value.hash {
            return Err(PostgresError::Other("tx and tx hash don't match".to_string()));
        }
        Ok(result)
    }
}
