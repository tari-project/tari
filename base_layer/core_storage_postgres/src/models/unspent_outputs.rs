use crate::{
    error::{FetchError, HashesDontMatchError, InsertError, PostgresChainStorageError},
    models::MerkleCheckpoint,
    schema::*,
};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use log::*;
use snafu::ResultExt;
use std::convert::{TryFrom, TryInto};
use tari_core::{
    blocks::BlockHash,
    chain_storage::{DbValue, MmrTree},
    transactions::{
        transaction::{OutputFeatures, OutputFlags, TransactionOutput},
        types::{Commitment, HashOutput, RangeProof},
    },
};
use tari_crypto::tari_utilities::{hex::Hex, ByteArray, Hashable};

const LOG_TARGET: &str = "base_layer::core::storage::postgres:unspent_outputs";

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "unspent_outputs"]
#[primary_key(hash)]
pub struct UnspentOutput {
    pub hash: String,
    pub features_flags: i16,
    pub features_maturity: i64,
    pub commitment: String,
    pub proof: Vec<u8>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl UnspentOutput {
    pub fn insert_if_not_exists(
        output: &TransactionOutput,
        conn: &PgConnection,
    ) -> Result<bool, PostgresChainStorageError>
    {
        let hash = output.hash();

        if UnspentOutput::fetch(&hash, conn)?.is_some() {
            warn!(
                target: LOG_TARGET,
                "Tried to insert unspent output with hash:{} but it already exists",
                hash.to_hex()
            );

            return Ok(false);
        }

        let row: UnspentOutput = output.try_into()?;
        if row.hash != hash.to_hex() {
            HashesDontMatchError {
                entity: "unspent output",
                expected_hash: hash.to_hex(),
                actual_hash: row.hash.clone(),
            }
            .fail()?;
        }

        diesel::insert_into(unspent_outputs::table)
            .values(&row)
            .execute(conn)
            .context(InsertError {
                key: hash.to_hex(),
                entity: "unspent output",
            })?;

        Ok(true)
    }

    pub fn fetch(hash: &HashOutput, conn: &PgConnection) -> Result<Option<UnspentOutput>, PostgresChainStorageError> {
        let mut results: Vec<UnspentOutput> = unspent_outputs::table
            .filter(unspent_outputs::hash.eq(hash.to_hex()))
            .get_results(conn)
            .context(FetchError {
                key: hash.to_hex(),
                entity: "unspent output".to_string(),
            })?;

        Ok(results.pop())
    }
}

impl TryFrom<&TransactionOutput> for UnspentOutput {
    type Error = PostgresChainStorageError;

    fn try_from(value: &TransactionOutput) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: value.hash().to_hex(),

            features_flags: value.features.flags.bits() as i16,
            features_maturity: value.features.maturity as i64,
            commitment: value.commitment.to_hex(),
            proof: value.proof.0.clone(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        })
    }
}

impl TryFrom<UnspentOutput> for TransactionOutput {
    type Error = PostgresChainStorageError;

    fn try_from(value: UnspentOutput) -> Result<Self, Self::Error> {
        let result = Self {
            features: OutputFeatures {
                flags: OutputFlags::from_bits_truncate(value.features_flags as u8),
                maturity: value.features_maturity as u64,
            },
            commitment: Commitment::from_hex(&value.commitment)?,
            proof: RangeProof::from_bytes(&value.proof)?,
        };

        if result.hash().to_hex() != value.hash {
            HashesDontMatchError {
                entity: "unspent output",
                expected_hash: value.hash.clone(),
                actual_hash: result.hash().to_hex(),
            }
            .fail()?;
        }

        Ok(result)
    }
}

impl TryFrom<UnspentOutput> for DbValue {
    type Error = PostgresChainStorageError;

    fn try_from(value: UnspentOutput) -> Result<Self, Self::Error> {
        let result: TransactionOutput = value.try_into()?;
        Ok(DbValue::UnspentOutput(Box::new(result)))
    }
}
