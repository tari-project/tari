use crate::{error::PostgresChainStorageError, models, models::metadata::Metadata};
use diesel::{result::Error, Connection, PgConnection};
use digest::Digest;
use tari_core::{
    blocks::{Block, BlockHeader},
    chain_storage::{
        BlockchainBackend,
        ChainStorageError,
        DbKey,
        DbKeyValuePair,
        DbTransaction,
        DbValue,
        MmrTree,
        WriteOperation,
    },
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use tari_mmr::{Hash, MerkleCheckPoint, MerkleProof};
use log::*;


const LOG_TARGET: &str = "base_layer::core::storage::postgres";


pub fn create_postgres_database(database_url: String) -> PostgresDatabase {
    PostgresDatabase { database_url }
}

pub struct PostgresDatabase {
    database_url: String,
}

impl PostgresDatabase {
    fn get_conn(&self) -> Result<PgConnection, PostgresChainStorageError> {
        Ok(PgConnection::establish(&self.database_url)?)
    }

    fn insert(&self, conn: &PgConnection, record: DbKeyValuePair) -> Result<(), PostgresChainStorageError> {
        match record {
            DbKeyValuePair::Metadata(key, value) => {
                Metadata::update(value, conn)?;
            },
            DbKeyValuePair::BlockHeader(_, _) => {},
            DbKeyValuePair::UnspentOutput(_, _, _) => {},
            DbKeyValuePair::TransactionKernel(_, _, _) => {},
            DbKeyValuePair::OrphanBlock(_, _) => {},
        };

        Ok(())
    }

    fn delete(&self, key: DbKey) -> Result<(), PostgresChainStorageError> {
        unimplemented!()
    }

    fn spend(&self, key: DbKey) -> Result<(), PostgresChainStorageError> {
        unimplemented!()
    }

    fn unspend(&self, key: DbKey) -> Result<(), PostgresChainStorageError> {
        unimplemented!()
    }

    fn create_mmr_checkpoint(&self, conn: &PgConnection, mmr_tree: MmrTree) -> Result<(), PostgresChainStorageError> {
        models::merkle_checkpoints::MerkleCheckpoint::save_current(mmr_tree, conn)
    }

    fn rewind_mmr(&self, mmr_tree: MmrTree, height: usize) -> Result<(), PostgresChainStorageError> {
        unimplemented!()
    }
}

impl BlockchainBackend for PostgresDatabase {
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        let conn = self.get_conn()?;
        conn.transaction::<(), PostgresChainStorageError, _>(|| {
            for operation in tx.operations {
                debug!(target: LOG_TARGET, "Executing write operation:{}", operation);
                match operation {
                    WriteOperation::Insert(record) => self.insert(&conn, record)?,
                    WriteOperation::Delete(key) => self.delete(key)?,
                    WriteOperation::Spend(key) => self.spend(key)?,
                    WriteOperation::UnSpend(key) => self.unspend(key)?,
                    WriteOperation::CreateMmrCheckpoint(mmr) => self.create_mmr_checkpoint(&conn, mmr)?,
                    WriteOperation::RewindMmr(mmr, height) => self.rewind_mmr(mmr, height)?,
                };
            }

            Ok(())
        })?;

        Ok(())
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let conn = self.get_conn()?;
        match key {
            DbKey::Metadata(key) => Ok(Some(DbValue::Metadata(Metadata::fetch(key, &conn)?))),
            DbKey::BlockHeader(_) => unimplemented!(),
            DbKey::BlockHash(_) => unimplemented!(),
            DbKey::UnspentOutput(_) => unimplemented!(),
            DbKey::SpentOutput(_) => unimplemented!(),
            DbKey::TransactionKernel(_) => unimplemented!(),
            DbKey::OrphanBlock(_) => unimplemented!(),
        }
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        unimplemented!()
    }

    fn calculate_mmr_root(
        &self,
        tree: MmrTree,
        additions: Vec<HashOutput>,
        deletions: Vec<HashOutput>,
    ) -> Result<HashOutput, ChainStorageError>
    {
        unimplemented!()
    }

    fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Hash, bool), ChainStorageError> {
        unimplemented!()
    }

    fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, Block), ChainStorageError>),
    {
        unimplemented!()
    }

    fn for_each_kernel<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>),
    {
        unimplemented!()
    }

    fn for_each_header<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(u64, BlockHeader), ChainStorageError>),
    {
        unimplemented!()
    }

    fn for_each_utxo<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>),
    {
        unimplemented!()
    }

    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError> {
        unimplemented!()
    }
}
