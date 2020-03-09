use chrono::{NaiveDateTime, Utc};
use serde_json::Value;
use crate::error::PostgresChainStorageError;
use diesel::prelude::*;
use crate::schema::*;
use tari_crypto::tari_utilities::hex::Hex;
use tari_core::chain_storage::{DbKeyValuePair, DbValue};
use tari_core::blocks;
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::Hashable;
use tari_core::blocks::BlockHash;
use tari_core::transactions::types::BlindingFactor;


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
    pub fn fetch_by_hash(hash: &Vec<u8>, conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresChainStorageError> {
        let key = hash.to_hex();
        let mut results: Vec<BlockHeader> = block_headers::table.filter(block_headers::hash.eq(&key)).get_results(conn).map_err(
            |err| PostgresChainStorageError::FetchError(format!("Could not fetch block header with hash:{}", &key)))?;

        Ok(results.pop())
    }

    pub fn insert(block_header: &blocks::BlockHeader, conn: &PgConnection) -> Result<(), PostgresChainStorageError>{
        let row:BlockHeader = block_header.try_into()?;
        diesel::insert_into(block_headers::table).values(row).execute(conn).map_err(|err|
        PostgresChainStorageError::InsertError(format!("Could not insert block header:{}", err))) ?;
        Ok(())
    }
}

impl TryFrom<&blocks::BlockHeader> for BlockHeader {
    type Error =PostgresChainStorageError;

    fn try_from(value: &blocks::BlockHeader) -> Result<Self, Self::Error> {
        Ok(Self{
            hash: value.hash().to_hex(),
            height: value.height as i64,
            version: value.version as i32,
            prev_hash:value.prev_hash.to_hex(),
            timestamp: value.timestamp.as_u64() as i64,
            output_mmr: value.output_mr.to_hex(),
            range_proof_mmr: value.range_proof_mr.to_hex(),
            kernel_mmr: value.kernel_mr.to_hex(),
            total_kernel_offset: value.total_kernel_offset.to_hex(),
            nonce: value.nonce as i64,
            proof_of_work: serde_json::to_value(&value.pow)?,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc()
        })
    }
}


impl TryFrom<BlockHeader> for DbValue {

    type Error = PostgresChainStorageError;

    fn try_from(bh: BlockHeader) -> Result<Self, Self::Error> {
        let mut header = blocks::BlockHeader::default();
        header.version = bh.version as u16;
        header.prev_hash = BlockHash::from_hex(&bh.prev_hash)?;
        header.height = bh.height as u64;
        header.timestamp = (bh.timestamp as u64).into();
        header.kernel_mr = BlockHash::from_hex(&bh.kernel_mmr)?;
        header.nonce = bh.nonce as u64;
        header.output_mr = BlockHash::from_hex(&bh.output_mmr)?;
        header.range_proof_mr = BlockHash::from_hex(&bh.range_proof_mmr)?;
        header.total_kernel_offset = BlindingFactor::from_hex(&bh.total_kernel_offset)?;
        header.pow = serde_json::from_value(bh.proof_of_work)?;

        if header.hash() != BlockHash::from_hex(&bh.hash)? {
            return Err(
                PostgresChainStorageError::FetchError("Block header hash does not match object hash. Data might be corrupted".to_string())
            );
        }

        Ok(DbValue::BlockHeader(Box::new(header)))
    }
}