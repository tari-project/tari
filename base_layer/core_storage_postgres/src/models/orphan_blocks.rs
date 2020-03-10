use crate::schema::*;
use diesel::prelude::*;
use serde_json::Value;
use chrono::NaiveDateTime;
use crate::error::PostgresChainStorageError;
use tari_core::transactions::types::HashOutput;
use tari_crypto::tari_utilities::hex::Hex;
use std::convert::TryFrom;
use tari_core::blocks;
use tari_crypto::tari_utilities::Hashable;
use tari_core::chain_storage::DbValue;
use std::convert::TryInto;

#[derive(Queryable, Identifiable, Insertable)]
#[table_name ="orphan_blocks"]
#[primary_key(hash)]
pub struct OrphanBlock
{
    hash: String,
    header: Value,
    body: Value,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime
}

impl OrphanBlock {
    pub fn fetch(hash: &HashOutput, conn: &PgConnection) -> Result<Option<OrphanBlock>, PostgresChainStorageError>
    {
        let mut results: Vec<OrphanBlock> = orphan_blocks::table.filter(orphan_blocks::hash.eq(hash.to_hex())).get_results(conn).map_err(
            |err| PostgresChainStorageError::FetchError(format!("Could not fetch orphan block with hash:{}:{}", hash.to_hex(), err))
        )?;

        Ok(results.pop())
    }
}

impl TryFrom<OrphanBlock> for blocks::Block {
    type Error = PostgresChainStorageError;

    fn try_from(value: OrphanBlock) -> Result<Self, Self::Error> {
        let result = Self{
            header: serde_json::from_value(value.header)?,
            body: serde_json::from_value(value.body)?
        };
        if result.hash().to_hex() != value.hash {
            return Err(PostgresChainStorageError::FetchError(format!("Deserialized orphan block's hash did not match record's hash. {} != {}", result.hash().to_hex(), value.hash)))
        }
        Ok(result)
    }
}

impl TryFrom<OrphanBlock> for DbValue {
    type Error = PostgresChainStorageError;

    fn try_from(value: OrphanBlock) -> Result<Self, Self::Error> {
        let block:blocks::Block= value.try_into()?;

        Ok(DbValue::OrphanBlock(Box::new(block)))
    }
}