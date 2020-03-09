use crate::{error::PostgresChainStorageError, schema::*};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use tari_core::chain_storage::MmrTree;
use std::default::Default;

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
