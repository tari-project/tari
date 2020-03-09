use crate::{error::PostgresChainStorageError, schema::metadata, types::BlockHash};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use std::fs::metadata;
use tari_core::{
    chain_storage::{ChainStorageError, DbKey, DbValue, MetadataKey, MetadataValue},
    proof_of_work::Difficulty,
};
use tari_crypto::tari_utilities::hex::Hex;

#[derive(Queryable)]
pub struct Metadata {
    id: i32,
    chain_height: Option<i64>,
    best_block: Option<String>,
    accumulated_work: Option<i64>,
    pruning_horizon: i64,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl Metadata {
    pub fn fetch(key: &MetadataKey, conn: &PgConnection) -> Result<MetadataValue, PostgresChainStorageError> {
        let row: Metadata = metadata::table.first(conn).map_err(|err| {
            PostgresChainStorageError::FetchError(format!("Could not fetch metadata:{}", err.to_string()))
        })?;

        let value = match key {
            MetadataKey::ChainHeight => MetadataValue::ChainHeight(row.chain_height.map(|ch| ch as u64)),
            MetadataKey::BestBlock => MetadataValue::BestBlock(match row.best_block {
                Some(b) => Some(BlockHash::from_hex(&b)?),
                None => None,
            }),
            MetadataKey::AccumulatedWork => {
                MetadataValue::AccumulatedWork(row.accumulated_work.map(|w| (w as u64).into()))
            },
            MetadataKey::PruningHorizon => MetadataValue::PruningHorizon(row.pruning_horizon as u64),
        };
        Ok(value)
    }

    pub fn update(value: MetadataValue, conn: &PgConnection) -> Result<Metadata, PostgresChainStorageError> {
        let mut fields = MetadataFields::default();
        match value {
            MetadataValue::ChainHeight(height) =>
            // TODO: Could lose some data here
            {
                fields.chain_height = Some(height.map(|i| i as i64))
            },
            MetadataValue::BestBlock(hash) => fields.best_block = Some(hash.map(|h| h.to_hex())),
            MetadataValue::AccumulatedWork(diff) => fields.accumulated_work = Some(diff.map(|d| d.as_u64() as i64)),
            MetadataValue::PruningHorizon(horiz) => fields.pruning_horizon = Some(horiz as i64),
        };
        diesel::update(metadata::table.filter(metadata::id.eq(0)))
            .set(fields)
            .get_result(conn)
            .map_err(|err| {
                PostgresChainStorageError::UpdateError(format!("Could not update metadata: {}", err.to_string()))
            })
    }
}

#[derive(AsChangeset, Default)]
#[table_name = "metadata"]
struct MetadataFields {
    chain_height: Option<Option<i64>>,
    best_block: Option<Option<String>>,
    accumulated_work: Option<Option<i64>>,
    pruning_horizon: Option<i64>,
}
