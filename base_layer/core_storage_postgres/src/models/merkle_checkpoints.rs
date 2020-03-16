use crate::{
    error::{MmrFetchError, MmrSaveError, PostgresChainStorageError, QueryError},
    schema::*,
};
use chrono::NaiveDateTime;
use croaring::Bitmap;
use diesel::prelude::*;
use snafu::ResultExt;
use std::{convert::TryFrom, default::Default};
use tari_core::{blocks::BlockHash, chain_storage::MmrTree};
use tari_crypto::tari_utilities::hex::Hex;
use tari_mmr::{Hash, MerkleCheckPoint};

#[derive(Queryable, Identifiable)]
#[table_name = "merkle_checkpoints"]
pub struct MerkleCheckpoint {
    pub id: i64,
    pub mmr_tree: String,
    pub is_current: bool,
    pub nodes_added: Vec<String>,
    pub nodes_deleted: Vec<u8>,
    pub rank: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable, Default)]
#[table_name = "merkle_checkpoints"]
pub struct NewMerkleCheckpoint {
    pub mmr_tree: String,
    pub is_current: bool,
    pub rank: i64,
    pub nodes_added: Vec<String>,
    pub nodes_deleted: Vec<u8>,
}

impl MerkleCheckpoint {
    pub fn fetch(
        mmr_tree: MmrTree,
        rank: i64,
        conn: &PgConnection,
    ) -> Result<Option<MerkleCheckpoint>, PostgresChainStorageError>
    {
        merkle_checkpoints::table
            .filter(merkle_checkpoints::mmr_tree.eq(mmr_tree.to_string()))
            .filter(merkle_checkpoints::rank.eq(rank))
            .get_result(conn)
            .optional()
            .context(MmrFetchError { mmr_tree })
    }

    pub fn fetch_or_create_current(
        mmr_tree: MmrTree,
        conn: &PgConnection,
    ) -> Result<MerkleCheckpoint, PostgresChainStorageError>
    {
        let row: Option<MerkleCheckpoint> = merkle_checkpoints::table
            .filter(merkle_checkpoints::mmr_tree.eq(mmr_tree.to_string()))
            .filter(merkle_checkpoints::is_current.eq(true))
            .get_result(conn)
            .optional()
            .context(MmrFetchError { mmr_tree })?;

        match row {
            Some(r) => Ok(r),
            None => MerkleCheckpoint::create(mmr_tree, true, conn),
        }
    }

    pub fn get_len(mmr_tree: MmrTree, conn: &PgConnection) -> Result<i64, PostgresChainStorageError> {
        let len = merkle_checkpoints::table
            .filter(merkle_checkpoints::mmr_tree.eq(mmr_tree.to_string()))
            .count()
            .first(conn)
            .context(QueryError {
                query: format!("Get checkpoint length for MMR {}", mmr_tree.to_string()),
            })?;
        Ok(len)
    }

    pub fn add_node(mmr_tree: MmrTree, hash: &Vec<u8>, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        let mut checkpoint = MerkleCheckpoint::fetch_or_create_current(mmr_tree, conn)?;
        checkpoint.nodes_added.push(hash.to_hex());
        diesel::update(merkle_checkpoints::table.filter(merkle_checkpoints::id.eq(checkpoint.id)))
            .set(merkle_checkpoints::nodes_added.eq(checkpoint.nodes_added))
            .execute(conn)
            .context(MmrSaveError {
                mmr_tree,
                action: "update".to_string(),
            })?;
        Ok(())
    }

    pub fn create(
        mmr_tree: MmrTree,
        is_current: bool,
        conn: &PgConnection,
    ) -> Result<MerkleCheckpoint, PostgresChainStorageError>
    {

        let new_row = NewMerkleCheckpoint {
            mmr_tree: mmr_tree.to_string(),
            is_current,
            nodes_deleted: Bitmap::create().serialize(),
            ..Default::default()
        };

        let result = diesel::insert_into(merkle_checkpoints::table)
            .values(new_row)
            .get_result(conn)
            .context(MmrSaveError {
                mmr_tree,
                action: "insert".to_string(),
            })?;

        Ok(result)
    }

    pub fn save_current(mmr_tree: MmrTree, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        let prev_checkpoint: Option<MerkleCheckpoint> = diesel::update(merkle_checkpoints::table)
            .filter(merkle_checkpoints::mmr_tree.eq(mmr_tree.to_string()))
            .filter(merkle_checkpoints::is_current.eq(true))
            .set((merkle_checkpoints::is_current.eq(false)))
            .get_result(conn)
            .optional()
            .context(MmrSaveError {
                mmr_tree,
                action: "update".to_string(),
            })?;

        let rank;
        if prev_checkpoint.is_none() {
            MerkleCheckpoint::create(mmr_tree, false, conn)?;

            rank = 0;
        } else {
            rank = prev_checkpoint.unwrap().rank;
        }

        let new_row = NewMerkleCheckpoint {
            mmr_tree: mmr_tree.to_string(),
            is_current: true,
            rank: rank + 1,
            nodes_deleted: Bitmap::create().serialize(),
            ..Default::default()
        };

        diesel::insert_into(merkle_checkpoints::table)
            .values(new_row)
            .execute(conn)?;

        Ok(())
    }
}

impl TryFrom<MerkleCheckpoint> for MerkleCheckPoint {
    type Error = PostgresChainStorageError;

    fn try_from(value: MerkleCheckpoint) -> Result<Self, Self::Error> {
        let mut result = Vec::<Hash>::new();
        for node in value.nodes_added {
            result.push(Hash::from_hex(node.as_str())?);
        }

        if value.nodes_deleted.is_empty() {
            Ok(Self::new(result, Bitmap::create()))
        } else {
            Ok(Self::new(result, Bitmap::deserialize(&value.nodes_deleted)))
        }
    }
}
