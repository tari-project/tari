use crate::{
    error::{FetchError, HashesDontMatchError, InsertError, PostgresChainStorageError, QueryError},
    schema::*,
};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use snafu::ResultExt;
use std::convert::{TryFrom, TryInto};
use tari_core::{
    blocks,
    blocks::{Block, BlockHeader},
    chain_storage::DbValue,
    transactions::types::HashOutput,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "orphan_blocks"]
#[primary_key(hash)]
pub struct OrphanBlock {
    hash: String,
    header: Value,
    body: Value,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl OrphanBlock {
    pub fn fetch(hash: &HashOutput, conn: &PgConnection) -> Result<Option<OrphanBlock>, PostgresChainStorageError> {
        let mut results: Vec<OrphanBlock> = orphan_blocks::table
            .filter(orphan_blocks::hash.eq(hash.to_hex()))
            .get_results(conn)
            .context(FetchError {
                key: hash.to_hex(),
                entity: "orphan block".to_string(),
            })?;

        Ok(results.pop())
    }

    pub fn insert(hash: &HashOutput, block: &Block, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        let row: OrphanBlock = block.into();
        if row.hash != hash.to_hex() {
            return HashesDontMatchError {
                entity: "orphan block".to_string(),
                expected_hash: hash.to_hex(),
                actual_hash: row.hash,
            }
            .fail();
        }

        diesel::insert_into(orphan_blocks::table)
            .values(&row)
            .execute(conn)
            .context(InsertError {
                entity: "orphan block".to_string(),
                key: row.hash.to_string(),
            })?;
        Ok(())
    }

    pub fn find_all(conn: &PgConnection) -> Result<Vec<(HashOutput, blocks::Block)>, PostgresChainStorageError> {


        let orphans: Vec<OrphanBlock> = orphan_blocks::table
            .order_by(orphan_blocks::created_at.desc())
            .load(conn)
            .context(QueryError {
                query: "find all orphan blocks",
            })?;

        let mut result = vec![];
        for orphan in orphans {
            result.push((
                HashOutput::from_hex(&orphan.hash)?,
                orphan.try_into()?,

            ));
        }

        Ok(result)
    }
}

impl From<&blocks::Block> for OrphanBlock {
    fn from(value: &blocks::Block) -> Self {
        Self {
            hash: value.hash().to_hex(),
            header: json!(value.header),
            body: json!(value.body),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        }
    }
}

impl TryFrom<OrphanBlock> for blocks::Block {
    type Error = PostgresChainStorageError;

    fn try_from(value: OrphanBlock) -> Result<Self, Self::Error> {
        let result = Self {
            header: serde_json::from_value(value.header)?,
            body: serde_json::from_value(value.body)?,
        };
        if result.hash().to_hex() != value.hash {
            return HashesDontMatchError {
                entity: "orphan block".to_string(),
                actual_hash: result.hash().to_hex(),
                expected_hash: value.hash.clone(),
            }
            .fail();
        }
        Ok(result)
    }
}

impl TryFrom<OrphanBlock> for DbValue {
    type Error = PostgresChainStorageError;

    fn try_from(value: OrphanBlock) -> Result<Self, Self::Error> {
        let block: blocks::Block = value.try_into()?;

        Ok(DbValue::OrphanBlock(Box::new(block)))
    }
}
