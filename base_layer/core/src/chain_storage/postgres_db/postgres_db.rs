use crate::{
    blocks::{Block, BlockHeader},
    chain_storage::{
        blockchain_database::BlockchainBackend,
        db_transaction::{DbKey, DbTransaction, DbValue, MmrTree, WriteOperation},
        error::ChainStorageError,
        postgres_db::{error::PostgresError, models},
        ChainMetadata,
        DbKeyValuePair,
    },
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use diesel::{Connection, PgConnection};
// use digest::Digest;
use log::*;
use std::convert::TryInto;
use tari_mmr::{MerkleCheckPoint, MerkleProof};

const LOG_TARGET: &str = "b::c::storage::postgres:postgres_db";

pub struct PostgresDatabase {
    database_url: String,
}

impl PostgresDatabase {
    /// This creates a new Postgres database
    pub fn new(database_url: String) -> Self {
        Self { database_url }
    }

    fn get_conn(&self) -> Result<PgConnection, PostgresError> {
        Ok(PgConnection::establish(&self.database_url)?)
    }

    fn insert(&self, conn: &PgConnection, record: DbKeyValuePair) -> Result<(), PostgresError> {
        match record {
            DbKeyValuePair::Metadata(_key, value) => {
                models::Metadata::update(value, conn)?;
            },
            DbKeyValuePair::BlockHeader(_, block_header) => models::BlockHeader::insert(&*block_header, false, conn)?,
            DbKeyValuePair::UnspentOutput(_hash, _output, _update_mmr) => unimplemented!(),
            DbKeyValuePair::TransactionKernel(_hash, _kernel, _update_mmr) => unimplemented!(),
            DbKeyValuePair::OrphanBlock(_hash, _block) => unimplemented!(),
        };

        Ok(())
    }

    fn delete(&self, key: DbKey) -> Result<(), PostgresError> {
        let conn = self.get_conn()?;
        match key {
            DbKey::Metadata(_) => Ok(()), // no-op,
            DbKey::BlockHeader(height) => {
                models::BlockHeader::delete_at_height(height, &conn)?;
                // Todo we need to delete the tx_outputs that where created in this block
                unimplemented!()
            },
            DbKey::BlockHash(hash) => {
                models::BlockHeader::delete_at_hash(&hash, &conn)?;
                // Todo we need to delete the tx_outputs that where created in this block
                unimplemented!()
            },
            DbKey::UnspentOutput(_k) => unimplemented!(),
            DbKey::SpentOutput(_k) => unimplemented!(),
            DbKey::TransactionKernel(_k) => unimplemented!(),
            DbKey::OrphanBlock(_hash) => unimplemented!(),
        }
    }

    fn spend(&self, _key: DbKey) -> Result<(), PostgresError> {
        unimplemented!()
    }

    fn unspend(&self, _key: DbKey) -> Result<(), PostgresError> {
        unimplemented!()
    }
}

impl BlockchainBackend for PostgresDatabase
// where D: Digest + Send + Sync
{
    fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        let conn = self.get_conn()?;
        conn.transaction::<(), PostgresError, _>(|| {
            for operation in tx.operations {
                debug!(target: LOG_TARGET, "Executing write operation:{}", operation);
                match operation {
                    WriteOperation::Insert(record) => self.insert(&conn, record)?,
                    WriteOperation::Delete(key) => self.delete(key)?,
                    WriteOperation::Spend(key) => self.spend(key)?,
                    WriteOperation::UnSpend(key) => self.unspend(key)?,
                    WriteOperation::CreateMmrCheckpoint(_mmr) => unimplemented!(),
                    WriteOperation::RewindMmr(_mmr, _height) => unimplemented!(),
                };
            }

            Ok(())
        })?;

        Ok(())
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let conn = self.get_conn()?;
        debug!(target: LOG_TARGET, "Fetching:{:?}", key);

        match key {
            DbKey::Metadata(key) => Ok(Some(DbValue::Metadata(models::Metadata::fetch(key, &conn)?))),
            DbKey::BlockHeader(height) => Ok(match models::BlockHeader::fetch_by_height(*height as i64, &conn)? {
                Some(val) => Some(val.try_into_db_block_header()?),
                None => None,
            }),
            DbKey::BlockHash(key) => Ok(match models::BlockHeader::fetch_by_hash(key, &conn)? {
                Some(val) => Some(val.try_into_db_block_hash()?),
                None => None,
            }),
            DbKey::UnspentOutput(hash) => Ok(match models::TxOutput::fetch_unspent_output(hash, &conn)? {
                Some(val) => Some(val.try_into()?),
                None => None,
            }),
            DbKey::SpentOutput(hash) => Ok(match models::TxOutput::fetch_spent_output(hash, &conn)? {
                Some(val) => Some(val.try_into()?),
                None => None,
            }),
            DbKey::TransactionKernel(hash) => Ok(match models::Kernels::fetch_by_hash(hash, &conn)? {
                Some(val) => Some(val.try_into_db_tx_kernel()?),
                None => None,
            }),
            DbKey::OrphanBlock(_hash) => unimplemented!(),
        }
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        self.fetch(key).map(|s| s.is_some())
    }

    fn fetch_mmr_root(&self, _tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_only_root(&self, _tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        unimplemented!()
    }

    fn calculate_mmr_root(
        &self,
        _tree: MmrTree,
        _additions: Vec<HashOutput>,
        _deletions: Vec<HashOutput>,
    ) -> Result<HashOutput, ChainStorageError>
    {
        unimplemented!()
    }

    fn fetch_mmr_proof(&self, _tree: MmrTree, _pos: usize) -> Result<MerkleProof, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_checkpoint(&self, _tree: MmrTree, _height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        unimplemented!()
    }

    fn fetch_mmr_node(&self, _tree: MmrTree, _pos: u32) -> Result<(Vec<u8>, bool), ChainStorageError> {
        unimplemented!()
    }

    fn for_each_orphan<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, Block), ChainStorageError>),
    {
        // let conn = self.get_conn()?;
        // for orphan in models::OrphanBlock::find_all(&conn)? {
        //     f(Ok(orphan));
        // }
        // Ok(())
        unimplemented!()
    }

    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError> {
        unimplemented!()
    }

    fn for_each_kernel<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>),
    {
        unimplemented!()
    }

    fn for_each_header<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(u64, BlockHeader), ChainStorageError>),
    {
        unimplemented!()
    }

    fn for_each_utxo<F>(&self, _f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>),
    {
        unimplemented!()
    }

    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError> {
        let conn = self.get_conn()?;
        match models::BlockHeader::fetch_tip(&conn)? {
            Some(header) => Ok(Some(header.try_into()?)),
            None => Ok(None),
        }
    }

    /// Returns the metadata of the chain.
    fn fetch_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        let conn = self.get_conn()?;
        Ok(models::Metadata::fetch_meta(&conn)?)
    }
}
