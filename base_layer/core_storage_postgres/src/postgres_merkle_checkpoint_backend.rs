use crate::{error::PostgresChainStorageError, models};
use diesel::prelude::*;
use std::convert::TryInto;
use tari_core::chain_storage::MmrTree;
use tari_mmr::{ArrayLike, MerkleCheckPoint};

pub struct PostgresMerkleCheckpointBackend {
    mmr_tree: MmrTree,
    database_url: String,
}

impl PostgresMerkleCheckpointBackend {
    pub(crate) fn new(mmr_tree: MmrTree, database_url: String) -> Self {
        Self { mmr_tree, database_url }
    }

    fn get_conn(&self) -> Result<PgConnection, PostgresChainStorageError> {
        Ok(PgConnection::establish(&self.database_url)?)
    }
}

impl ArrayLike for PostgresMerkleCheckpointBackend {
    type Error = PostgresChainStorageError;
    type Value = MerkleCheckPoint;

    fn len(&self) -> Result<usize, Self::Error> {
        let conn = self.get_conn()?;
        Ok(models::MerkleCheckpoint::get_len(self.mmr_tree, &conn)? as usize)
    }

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.len()? == 0)
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        // models::MerkleCheckpoint::create()
        unimplemented!()
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        Ok(
            match models::MerkleCheckpoint::fetch(self.mmr_tree, index as i64, &self.get_conn()?)? {
                Some(cp) => Some(cp.try_into()?),
                None => None,
            },
        )
    }

    fn get_or_panic(&self, index: usize) -> Self::Value {
        self.get(index).unwrap().unwrap()
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        unimplemented!()
    }
}
