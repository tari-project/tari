use crate::{error::PostgresChainStorageError, schema::*};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use tari_core::chain_storage::MmrTree;
use std::default::Default;
use tari_core::blocks::BlockHash;
use tari_crypto::tari_utilities::hex::Hex;

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

    pub fn fetch_current(mmr_tree: MmrTree, conn: &PgConnection) -> Result<MerkleCheckpoint, PostgresChainStorageError>{
        let row: MerkleCheckpoint = merkle_checkpoints::table.filter(merkle_checkpoints::mmr_tree.eq(mmr_tree.to_string()))
            .filter(merkle_checkpoints::is_current.eq(true))
            .get_result(conn)?;

        Ok(row)
    }

    pub fn add_node(mmr_tree: MmrTree, hash: &Vec<u8>, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        let mut checkpoint = MerkleCheckpoint::fetch_current(mmr_tree, conn)?;
        checkpoint.nodes_added.push(hash.to_hex());
        diesel::update(merkle_checkpoints::table.filter(merkle_checkpoints::id.eq(checkpoint.id))).set(
            merkle_checkpoints::nodes_added.eq(checkpoint.nodes_added)).execute(conn)?;
        Ok(())
    }

    pub fn save_current(mmr_tree: MmrTree, conn: &PgConnection) -> Result<(), PostgresChainStorageError> {
        let prev_checkpoint: Option<MerkleCheckpoint> = diesel::update(merkle_checkpoints::table)
            .filter(merkle_checkpoints::mmr_tree.eq(mmr_tree.to_string()))
            .filter(merkle_checkpoints::is_current.eq(true))
            .set((merkle_checkpoints::is_current.eq(false)))
            .get_result(conn)
            .optional()
            .map_err(|err| {
                PostgresChainStorageError::UpdateError(format!("Could not create merkle checkpoint:{}", err))
            })?;

        let rank;
        if prev_checkpoint.is_none() {
            let new_row = NewMerkleCheckpoint {
                mmr_tree: mmr_tree.to_string(),
                ..Default::default()
            };

            diesel::insert_into(merkle_checkpoints::table)
                .values(new_row)
                .execute(conn)?;

            rank = 0;
        } else {
            rank = prev_checkpoint.unwrap().rank;
        }

        let new_row = NewMerkleCheckpoint {
            mmr_tree: mmr_tree.to_string(),
            is_current: true,
            rank: rank + 1,
            ..Default::default()
        };

        diesel::insert_into(merkle_checkpoints::table)
            .values(new_row)
            .execute(conn)?;

        Ok(())
    }
}
