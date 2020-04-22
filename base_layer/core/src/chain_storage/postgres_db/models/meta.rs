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
    blocks,
    blocks::BlockHash,
    chain_storage::{
        postgres_db::{models::error::PostgresError, schema::*},
        DbKeyValuePair,
        DbValue,
    },
    transactions::types::{BlindingFactor, HashOutput},
};
use chrono::{NaiveDateTime, Utc};
use diesel::{self, expression::dsl, prelude::*, OptionalExtension};
use log::*;
use serde_json::Value;
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "b::c::storage::postgres:meta";

#[derive(Queryable)]
pub struct Metadata {
    id: i32,
    chain_height: Option<i64>,
    best_block: Option<String>,
    accumulated_work: Option<i64>,
    pruning_horizon: i64,
    updated_at: NaiveDateTime,
}

impl Metadata {
    /// This will fetch the current meta from the database
    pub fn fetch(key: &MetadataKey, conn: &PgConnection) -> Result<MetadataValue, PostgresError> {
        let row: Metadata = metadata::table
            .first(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

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

    /// This will fetch current meta form the database
    pub fn update(value: MetadataValue, conn: &PgConnection) -> Result<(), PostgresError> {
        let mut fields = MetadataFields::default();
        match &value {
            MetadataValue::ChainHeight(height) =>
            // TODO: Could lose some data here
            {
                fields.chain_height = Some(height.map(|i| i as i64))
            },
            MetadataValue::BestBlock(hash) => fields.best_block = Some(hash.as_ref().map(|h| h.to_hex())),
            MetadataValue::AccumulatedWork(diff) => fields.accumulated_work = Some(diff.map(|d| d.as_u64() as i64)),
            MetadataValue::PruningHorizon(horiz) => fields.pruning_horizon = Some(*horiz as i64),
        };
        diesel::update(metadata::table.filter(metadata::id.eq(0)))
            .set(fields)
            .execute(conn)
            .map_err(|e| PostgresError::CouldNotAdd(e.to_string()))?;

        Ok(())
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
