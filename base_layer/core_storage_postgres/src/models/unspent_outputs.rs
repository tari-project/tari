use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use tari_core::blocks::BlockHash;
use crate::error::PostgresChainStorageError;
use tari_core::transactions::transaction::TransactionOutput;
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::Hashable;
use tari_crypto::tari_utilities::hex::Hex;
use crate::schema::*;
use tari_core::chain_storage::MmrTree;
use crate::models::MerkleCheckpoint;

#[derive(Queryable, Identifiable, Insertable)]
#[table_name="unspent_outputs"]
#[primary_key(hash)]
pub struct UnspentOutput {
    pub hash: String,
    pub features_flags: i32,
    pub features_maturity: i64,
    pub commitment: String,
    pub proof: Vec<u8>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime
}

impl UnspentOutput{
    pub fn insert(output: TransactionOutput, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        let hash = output.hash();

        let row :UnspentOutput = output.try_into()?;

        diesel::insert_into(unspent_outputs::table).values(&row).execute(conn).map_err(|err|
                                                                                           PostgresChainStorageError::InsertError(format!(
                                                                                               "Could not insert unspent output:{}", err)))?;

        Ok(())
    }
}

impl TryFrom<TransactionOutput> for UnspentOutput {
    type Error = PostgresChainStorageError;

    fn try_from(value: TransactionOutput) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: value.hash().to_hex(),

            features_flags: value.features.flags.bits() as i32,
            features_maturity: value.features.maturity as i64,
            commitment: value.commitment.to_hex(),
            proof: value.proof.0,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc()
        })
    }
}