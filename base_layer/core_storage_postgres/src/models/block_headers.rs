use crate::{
    error::{DeleteError, FetchError, HashesDontMatchError, InsertError, PostgresChainStorageError, QueryError},
    schema::*,
};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use log::*;
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

const LOG_TARGET: &str = "base_layer::core::storage::postgres:block_headers";

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
    pub fn fetch_by_height(height: i64, conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresChainStorageError> {
        let mut results: Vec<BlockHeader> = block_headers::table
            .filter(block_headers::height.eq(height))
            .get_results(conn)
            .context(FetchError {
                key: height.to_string(),
                entity: "block header".to_string(),
            })?;

        Ok(results.pop())
    }

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
                key: key.clone(),
                entity: "block header".to_string(),
            })?;

        Ok(results.pop())
    }

    pub fn insert_if_not_exists(
        block_header: &blocks::BlockHeader,
        conn: &PgConnection,
    ) -> Result<(), PostgresChainStorageError>
    {
        if BlockHeader::fetch_by_hash(&block_header.hash(), conn)?.is_some() {
            warn!(
                target: LOG_TARGET,
                "Tried to insert block header with hash:{} but it already exists",
                block_header.hash().to_hex()
            );
            return Ok(());
        }

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

    pub fn fetch_tip(conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresChainStorageError> {
        block_headers::table
            .order_by(block_headers::height.desc())
            .first(conn)
            .optional()
            .context(QueryError {
                query: "Get latest header",
            })
    }

    pub fn delete_at_height(height: i64, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        diesel::delete(block_headers::table.filter(block_headers::height.eq(height)))
            .execute(conn)
            .context(DeleteError {
                key: height.to_string(),
                entity: "block header",
            })?;
        Ok(())
    }

    pub fn try_into_db_block_hash(self) -> Result<DbValue, PostgresChainStorageError> {
        let header: blocks::BlockHeader = self.try_into()?;
        Ok(DbValue::BlockHash(Box::new(header)))
    }

    pub fn try_into_db_block_header(self) -> Result<DbValue, PostgresChainStorageError> {
        let header: blocks::BlockHeader = self.try_into()?;
        Ok(DbValue::BlockHeader(Box::new(header)))
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

impl TryFrom<BlockHeader> for blocks::BlockHeader {
    type Error = PostgresChainStorageError;

    fn try_from(value: BlockHeader) -> Result<Self, Self::Error> {
        let mut header = blocks::BlockHeader::default();
        header.version = value.version as u16;
        header.prev_hash = BlockHash::from_hex(&value.prev_hash)?;
        header.height = value.height as u64;
        header.timestamp = (value.timestamp as u64).into();
        header.kernel_mr = BlockHash::from_hex(&value.kernel_mmr)?;
        header.nonce = value.nonce as u64;
        header.output_mr = BlockHash::from_hex(&value.output_mmr)?;
        header.range_proof_mr = BlockHash::from_hex(&value.range_proof_mmr)?;
        header.total_kernel_offset = BlindingFactor::from_hex(&value.total_kernel_offset)?;
        header.pow = serde_json::from_value(value.proof_of_work)?;

        if header.hash() != BlockHash::from_hex(&value.hash)? {
            return HashesDontMatchError {
                actual_hash: header.hash().to_hex(),
                expected_hash: value.hash.clone(),
                entity: "block header".to_string(),
            }
            .fail();
        }

        Ok(header)
    }
}
