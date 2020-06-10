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
        postgres_db::{error::PostgresError, schema::*},
        DbValue,
    },
    transactions::types::BlindingFactor,
};
use diesel::{self, prelude::*, OptionalExtension};
use log::*;
use serde_json::Value;
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "b::c::storage::postgres:block_headers";

#[derive(Queryable, Identifiable, Insertable)]
#[table_name = "block_headers"]
#[primary_key(hash)]
pub struct BlockHeader {
    pub hash: String,
    pub height: i64,
    pub version: i32,
    pub prev_hash: String,
    pub time_stamp: i64,
    pub output_mmr: String,
    pub range_proof_mmr: String,
    pub kernel_mmr: String,
    pub total_kernel_offset: String,
    pub nonce: i64,
    pub proof_of_work: Value,
    pub orphan: bool,
}

impl BlockHeader {
    /// This function will search only the main chain for the block height and return the block header in question.
    /// Orphan block headers will not be returned
    pub fn fetch_by_height(height: i64, conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresError> {
        let mut results: Vec<BlockHeader> = block_headers::table
            .filter(block_headers::height.eq(height))
            .filter(block_headers::orphan.eq(false))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;
        Ok(results.pop())
    }

    /// This function will seach for a block header via hash, it will return orphan block headers as well.
    pub fn fetch_by_hash(hash: &Vec<u8>, conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresError> {
        let key = hash.to_hex();
        BlockHeader::fetch_by_hex(&key, conn)
    }

    /// This function will seach for a block header via hash, provided in string, it will return orphan block headers as
    /// well.
    pub fn fetch_by_hex(hash: &String, conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresError> {
        let mut results: Vec<BlockHeader> = block_headers::table
            .filter(block_headers::hash.eq(hash))
            .get_results(conn)
            .map_err(|e| PostgresError::NotFound(e.to_string()))?;

        Ok(results.pop())
    }

    /// This function will insert a new block header only if the block header does not exist.
    pub fn insert(block_header: &blocks::BlockHeader, orphan: bool, conn: &PgConnection) -> Result<(), PostgresError> {
        if BlockHeader::fetch_by_hash(&block_header.hash(), conn)?.is_some() {
            warn!(
                target: LOG_TARGET,
                "Tried to insert block header with hash:{} but it already exists",
                block_header.hash().to_hex()
            );
            return Ok(());
        }

        let mut row: BlockHeader = block_header.try_into()?;
        row.orphan = orphan;

        diesel::insert_into(block_headers::table)
            .values(row)
            .execute(conn)
            .map_err(|e| PostgresError::CouldNotAdd(e.to_string()))?;

        Ok(())
    }

    /// This will return the tip header of the main chain.
    pub fn fetch_tip(conn: &PgConnection) -> Result<Option<BlockHeader>, PostgresError> {
        Ok(block_headers::table
            .order_by(block_headers::height.desc())
            .filter(block_headers::orphan.eq(false))
            .first::<BlockHeader>(conn)
            .optional()
            .map_err(|e| PostgresError::NotFound(e.to_string()))?)
    }

    pub fn delete_at_hash(hash: &Vec<u8>, conn: &PgConnection) -> Result<(), PostgresError> {
        let key = hash.to_hex();
        diesel::delete(block_headers::table.filter(block_headers::hash.eq(key)))
            .execute(conn)
            .map_err(|e| PostgresError::CouldDelete(e.to_string()))?;
        Ok(())
    }

    pub fn delete_at_height(height: u64, conn: &PgConnection) -> Result<(), PostgresError> {
        diesel::delete(
            block_headers::table
                .filter(block_headers::height.eq(height as i64))
                .filter(block_headers::orphan.eq(false)),
        )
        .execute(conn)
        .map_err(|e| PostgresError::CouldDelete(e.to_string()))?;
        Ok(())
    }

    pub fn try_into_db_block_hash(self) -> Result<DbValue, PostgresError> {
        let header: blocks::BlockHeader = self.try_into()?;
        Ok(DbValue::BlockHash(Box::new(header)))
    }

    pub fn try_into_db_block_header(self) -> Result<DbValue, PostgresError> {
        let header: blocks::BlockHeader = self.try_into()?;
        Ok(DbValue::BlockHeader(Box::new(header)))
    }
}

impl TryFrom<&blocks::BlockHeader> for BlockHeader {
    type Error = PostgresError;

    fn try_from(value: &blocks::BlockHeader) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: value.hash().to_hex(),
            height: value.height as i64,
            version: value.version as i32,
            prev_hash: value.prev_hash.to_hex(),
            time_stamp: value.timestamp.as_u64() as i64,
            output_mmr: value.output_mr.to_hex(),
            range_proof_mmr: value.range_proof_mr.to_hex(),
            kernel_mmr: value.kernel_mr.to_hex(),
            total_kernel_offset: value.total_kernel_offset.to_hex(),
            nonce: value.nonce as i64,
            proof_of_work: serde_json::to_value(&value.pow)?,
            orphan: false,
        })
    }
}

impl TryFrom<BlockHeader> for blocks::BlockHeader {
    type Error = PostgresError;

    fn try_from(value: BlockHeader) -> Result<Self, Self::Error> {
        let mut header = blocks::BlockHeader::default();
        header.version = value.version as u16;
        header.prev_hash = BlockHash::from_hex(&value.prev_hash)?;
        header.height = value.height as u64;
        header.timestamp = (value.time_stamp as u64).into();
        header.kernel_mr = BlockHash::from_hex(&value.kernel_mmr)?;
        header.nonce = value.nonce as u64;
        header.output_mr = BlockHash::from_hex(&value.output_mmr)?;
        header.range_proof_mr = BlockHash::from_hex(&value.range_proof_mmr)?;
        header.total_kernel_offset = BlindingFactor::from_hex(&value.total_kernel_offset)?;
        header.pow = serde_json::from_value(value.proof_of_work)?;

        if header.hash() != BlockHash::from_hex(&value.hash)? {
            return Err(PostgresError::Other(
                "Retrieved block header does not match saved hash".to_string(),
            ));
        }

        Ok(header)
    }
}
