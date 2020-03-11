use crate::{
    error::{FetchError, HashesDontMatchError, InsertError, PostgresChainStorageError},
    schema::*,
};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use serde_json::Value;
use snafu::ResultExt;
use std::convert::{TryFrom, TryInto};
use tari_core::{
    blocks,
    blocks::BlockHash,
    chain_storage::{DbKeyValuePair, DbValue},
    transactions::types::BlindingFactor,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "block_headers"]
#[primary_key(hash)]
pub struct BlockHeader {
    hash: String,
    height: i64,
    version: i32,
    prev_hash: String,
    timestamp: i64,
    output_mmr: String,
    range_proof_mmr: String,
    kernel_mmr: String,
    total_kernel_offset: String,
    nonce: i64,
    proof_of_work: Value,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl BlockHeader {
    pub fn fetch_by_hash(
        hash: &Vec<u8>,
        conn: &PgConnection,
    ) -> Result<Option<BlockHeader>, PostgresChainStorageError>
    {
        let key = hash.to_hex();
        let mut results: Vec<BlockHeader> = block_headers::table
            .filter(block_headers::hash.eq(&key))
            .get_results(conn)
            .context(FetchError {
                key: hash.to_hex(),
                entity: "block header".to_string(),
            })?;

        Ok(results.pop())
    }

    pub fn insert(block_header: &blocks::BlockHeader, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        let row: BlockHeader = block_header.try_into()?;
        diesel::insert_into(block_headers::table)
            .values(row)
            .execute(conn)
            .context(InsertError {
                key: block_header.hash().to_hex(),
                entity: "block header".to_string(),
            })?;

        Ok(())
    }

    pub fn try_into_db_block_hash(self) -> Result<DbValue, PostgresChainStorageError> {
        let mut header = blocks::BlockHeader::default();
        header.version = self.version as u16;
        header.prev_hash = BlockHash::from_hex(&self.prev_hash)?;
        header.height = self.height as u64;
        header.timestamp = (self.timestamp as u64).into();
        header.kernel_mr = BlockHash::from_hex(&self.kernel_mmr)?;
        header.nonce = self.nonce as u64;
        header.output_mr = BlockHash::from_hex(&self.output_mmr)?;
        header.range_proof_mr = BlockHash::from_hex(&self.range_proof_mmr)?;
        header.total_kernel_offset = BlindingFactor::from_hex(&self.total_kernel_offset)?;
        header.pow = serde_json::from_value(self.proof_of_work)?;

        if header.hash() != BlockHash::from_hex(&self.hash)? {
            return HashesDontMatchError {
                actual_hash: header.hash().to_hex(),
                expected_hash: self.hash.clone(),
                entity: "block header".to_string(),
            }
            .fail();
        }

        Ok(DbValue::BlockHash(Box::new(header)))
    }
}

impl TryFrom<&blocks::BlockHeader> for BlockHeader {
    type Error = PostgresChainStorageError;

    fn try_from(value: &blocks::BlockHeader) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: value.hash().to_hex(),
            height: value.height as i64,
            version: value.version as i32,
            prev_hash: value.prev_hash.to_hex(),
            timestamp: value.timestamp.as_u64() as i64,
            output_mmr: value.output_mr.to_hex(),
            range_proof_mmr: value.range_proof_mr.to_hex(),
            kernel_mmr: value.kernel_mr.to_hex(),
            total_kernel_offset: value.total_kernel_offset.to_hex(),
            nonce: value.nonce as i64,
            proof_of_work: serde_json::to_value(&value.pow)?,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        })
    }
}
