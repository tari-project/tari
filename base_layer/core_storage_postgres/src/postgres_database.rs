use crate::{error::PostgresChainStorageError, models, models::Metadata};
use diesel::{result::Error, Connection, PgConnection};
use digest::Digest;
use log::*;
use std::convert::TryInto;
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
use tari_crypto::tari_utilities::Hashable;
use tari_mmr::{Hash, MerkleCheckPoint, MerkleProof};

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
            DbKeyValuePair::BlockHeader(_, block_header) => {
                models::BlockHeader::insert_if_not_exists(&*block_header, conn)?
            },
            DbKeyValuePair::UnspentOutput(hash, output, update_mmr) => {
                // TODO: Not sure if we need to have the range proof in a different db
                if models::UnspentOutput::insert_if_not_exists(&*output, conn)? && update_mmr {
                    models::MerkleCheckpoint::add_node(MmrTree::Utxo, &hash, conn)?;
                    models::MerkleCheckpoint::add_node(MmrTree::RangeProof, &output.proof().hash(), conn)?;
                }
            },
            DbKeyValuePair::TransactionKernel(hash, kernel, update_mmr) => {
                if update_mmr {
                    models::MerkleCheckpoint::add_node(MmrTree::Kernel, &hash, conn)?;
                }
                models::TransactionKernel::insert(hash, *kernel, conn)?;
            },
            DbKeyValuePair::OrphanBlock(hash, block) => models::OrphanBlock::insert(&hash, &*block, conn)?,
        };

        Ok(())
    }

    fn delete(&self, key: DbKey) -> Result<(), PostgresChainStorageError> {
        let conn = self.get_conn()?;
        match key {
            DbKey::Metadata(_) => unimplemented!(),
            DbKey::BlockHeader(height) => models::BlockHeader::delete_at_height(height as i64, &conn),
            DbKey::BlockHash(hash) => {
                unimplemented!()
                // let result: Option<u64> = lmdb_get(&self.env, &self.block_hashes_db, &hash)?;
                // if let Some(k) = result {
                //     lmdb_delete(&txn, &self.block_hashes_db, &hash)?;
                //     lmdb_delete(&txn, &self.headers_db, &k)?;
                // }
            },
            DbKey::UnspentOutput(k) => {
                unimplemented!()
                // lmdb_delete(&txn, &self.utxos_db, &k)?;
                // lmdb_delete(&txn, &self.txos_hash_to_index_db, &k)?;
            },
            DbKey::SpentOutput(k) => {
                unimplemented!()
                // lmdb_delete(&txn, &self.stxos_db, &k)?;
                // lmdb_delete(&txn, &self.txos_hash_to_index_db, &k)?;
            },
            DbKey::TransactionKernel(k) => {
                unimplemented!()
                // lmdb_delete(&txn, &self.kernels_db, &k)?;
            },
            DbKey::OrphanBlock(hash) => models::OrphanBlock::delete(&hash, &conn),
        }
    }

    fn spend(&self, key: DbKey) -> Result<(), PostgresChainStorageError> {
        unimplemented!()
    }

    fn unspend(&self, key: DbKey) -> Result<(), PostgresChainStorageError> {
        unimplemented!()
    }

    fn create_mmr_checkpoint(&self, conn: &PgConnection, mmr_tree: MmrTree) -> Result<(), PostgresChainStorageError> {
        models::MerkleCheckpoint::save_current(mmr_tree, conn)
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
        debug!(target: LOG_TARGET, "Fetching:{:?}", key);

        match key {
            DbKey::Metadata(key) => Ok(Some(DbValue::Metadata(Metadata::fetch(key, &conn)?))),
            DbKey::BlockHeader(height) => Ok(match models::BlockHeader::fetch_by_height(*height as i64, &conn)? {
                Some(bh) => Some(bh.try_into_db_block_header()?),
                None => None,
            }),
            DbKey::BlockHash(key) => Ok(match models::BlockHeader::fetch_by_hash(key, &conn)? {
                Some(bh) => Some(bh.try_into_db_block_hash()?),
                None => None,
            }),
            DbKey::UnspentOutput(hash) => Ok(match models::UnspentOutput::fetch(hash, &conn)? {
                Some(out) => Some(out.try_into()?),
                None => None,
            }),
            DbKey::SpentOutput(_) => unimplemented!(),
            DbKey::TransactionKernel(_) => unimplemented!(),
            DbKey::OrphanBlock(hash) => Ok(match models::OrphanBlock::fetch(hash, &conn)? {
                Some(b) => Some(b.try_into()?),
                None => None,
            }),
        }
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        self.fetch(key).map(|s| s.is_some())
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

    fn for_each_orphan<F>(&self, mut f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, Block), ChainStorageError>),
    {
        let conn = self.get_conn()?;
        for orphan in models::OrphanBlock::find_all(&conn)? {
            f(Ok(orphan));
        }
        Ok(())
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
        let conn = self.get_conn()?;
        match models::BlockHeader::fetch_tip(&conn)? {
            Some(header) => Ok(Some(header.try_into()?)),
            None => Ok(None),
        }
    }

    fn range_proof_checkpoints_len(&self) -> Result<usize, ChainStorageError> {
        unimplemented!()
    }

    fn get_range_proof_checkpoints(&self, cp_index: usize) -> Result<Option<MerkleCheckPoint>, ChainStorageError> {
        unimplemented!()
    }

    fn curr_range_proof_checkpoint_get_added_position(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<usize>, ChainStorageError>
    {
        unimplemented!()
    }
}
