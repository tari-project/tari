// Copyright 2019. The Tari Project
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
//

use crate::{
    blocks::{block::Block, blockheader::BlockHeader},
    chain_storage::{
        error::ChainStorageError,
        transaction::{DbKey, DbTransaction, DbValue, MetadataKey, MetadataValue},
        ChainMetadata,
    },
    proof_of_work::Difficulty,
    transaction::{TransactionKernel, TransactionOutput},
    types::HashOutput,
};
use log::*;
use std::sync::{Arc, RwLock, RwLockReadGuard};

const LOG_TARGET: &str = "core::chain_storage::database";

pub enum MmrTree {
    Utxo,
    Kernel,
    RangeProof,
    Header,
}

pub trait BlockchainBackend: Send + Sync {
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError>;
    fn get(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError>;
    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError>;
    fn get_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError>;
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($self:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $self.db.get(&key) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::$key_var(k))) => Ok(Some(*k)),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

pub struct BlockChainDatabase<T>
where T: BlockchainBackend
{
    metadata: Arc<RwLock<ChainMetadata>>,
    db: Arc<T>,
}

impl<T> BlockChainDatabase<T>
where T: BlockchainBackend
{
    /// Reads the blockchain metadata (block height etc) from the underlying backend and returns it.
    fn read_metadata(db: &T) -> Result<ChainMetadata, ChainStorageError> {
        let height = match db.get(&DbKey::Metadata(MetadataKey::ChainHeight)) {
            Ok(Some(DbValue::Metadata(MetadataValue::ChainHeight(v)))) => v,
            Ok(None) => {
                warn!(
                    target: LOG_TARGET,
                    "The chain height entry is not present in the database. Assuming the database is empty."
                );
                0
            },
            Err(e) => return log_error(DbKey::Metadata(MetadataKey::ChainHeight), e),
            Ok(Some(other)) => return unexpected_result(DbKey::Metadata(MetadataKey::ChainHeight), other),
        };

        let work = match db.get(&DbKey::Metadata(MetadataKey::AccumulatedWork)) {
            Ok(Some(DbValue::Metadata(MetadataValue::AccumulatedWork(v)))) => v,
            Ok(None) => {
                warn!(
                    target: LOG_TARGET,
                    "The accumulated work entry is not present in the database. Assuming the database is empty."
                );
                0
            },
            Err(e) => return log_error(DbKey::Metadata(MetadataKey::AccumulatedWork), e),
            Ok(Some(other)) => return unexpected_result(DbKey::Metadata(MetadataKey::AccumulatedWork), other),
        };

        Ok(ChainMetadata {
            height_of_longest_chain: height,
            greatest_accumulated_work: work,
        })
    }

    /// Creates a new `BlockchainDatabase` using the provided backend.
    pub fn new(db: T) -> Result<Self, ChainStorageError> {
        let metadata = Self::read_metadata(&db)?;
        Ok(BlockChainDatabase {
            metadata: Arc::new(RwLock::new(metadata)),
            db: Arc::new(db),
        })
    }

    /// If a call to any metadata function fails, you can try and force a re-sync with this function. If the RWLock
    /// is poisoned because a write attempt failed, this function will replace the old lock with a new one with data
    /// freshly read from the underlying database. If this still fails, there's probably something badly wrong and
    /// the thread will panic.
    pub fn try_recover_metadata(&mut self) -> bool {
        if !self.metadata.is_poisoned() {
            // metadata is fine. Nothing to do here
            return false;
        }
        match BlockChainDatabase::read_metadata(self.db.as_ref()) {
            Ok(data) => {
                self.metadata = Arc::new(RwLock::new(data));
                true
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not read metadata from database. {}. We're going to panic here. Perhaps restarting will \
                     fix things",
                    e.to_string()
                );
                panic!("Blockchain metadata is compromised and the recovery attempt failed.")
            },
        }
    }

    fn get_metadata(&self) -> Result<RwLockReadGuard<ChainMetadata>, ChainStorageError> {
        self.metadata.read().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get sa read lock on the blockchain metadata failed. {}",
                e.to_string()
            );
            ChainStorageError::AccessError("Read lock on blockchain metadata failed".into())
        })
    }

    /// Returns the height of the current longest chain. This method will only fail if there's a fairly serious
    /// synchronisation problem on the database. You can try calling [BlockchainDatabase::try_recover_metadata] in
    /// that case to re-sync the metadata; or else just exit the program.
    pub fn get_height(&self) -> Result<u64, ChainStorageError> {
        let metadata = self.get_metadata()?;
        Ok(metadata.height_of_longest_chain)
    }

    /// Returns the total accumulated work/difficulty of the longest chain.
    ///
    /// This method will only fail if there's a fairly serious synchronisation problem on the database. You can try
    /// calling [BlockchainDatabase::try_recover_metadata] in that case to re-sync the metadata; or else
    /// just exit the program.
    pub fn get_total_work(&self) -> Result<Difficulty, ChainStorageError> {
        let metadata = self.get_metadata()?;
        Ok(metadata.greatest_accumulated_work.into())
    }

    /// Returns the transaction kernel with the given hash.
    pub fn fetch_kernel(&self, hash: HashOutput) -> Result<Option<TransactionKernel>, ChainStorageError> {
        fetch!(self, hash, TransactionKernel)
    }

    /// Returns the block header at the given block height.
    pub fn fetch_header(&self, block_num: u64) -> Result<Option<BlockHeader>, ChainStorageError> {
        fetch!(self, block_num, BlockHeader)
    }

    /// Returns the UTXO with the given hash.
    pub fn fetch_utxo(&self, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
        fetch!(self, hash, UnspentOutput)
    }

    /// Returns the STXO with the given hash.
    pub fn fetch_stxo(&self, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
        fetch!(self, hash, SpentOutput)
    }

    /// Returns the orphan block with the given hash.
    pub fn fetch_orphan(&self, hash: HashOutput) -> Result<Option<Block>, ChainStorageError> {
        fetch!(self, hash, OrphanBlock)
    }

    /// Returns true if the given UTXO, represented by its hash exists in the UTXO set.
    pub fn is_utxo(&self, hash: HashOutput) -> Result<bool, ChainStorageError> {
        let key = DbKey::UnspentOutput(hash);
        self.db.contains(&key)
    }

    /// Calculate the Merklish root of the current UTXO set.
    pub fn get_utxo_root(&self) -> Result<HashOutput, ChainStorageError> {
        self.db.get_mmr_root(MmrTree::Utxo)
    }

    /// Calculate the Merklish root of the kernel set.
    pub fn get_kernel_root(&self) -> Result<HashOutput, ChainStorageError> {
        self.db.get_mmr_root(MmrTree::Kernel)
    }

    /// Calculate the Merklish root of the kernel set.
    pub fn get_header_root(&self) -> Result<HashOutput, ChainStorageError> {
        self.db.get_mmr_root(MmrTree::Header)
    }

    /// Calculate the Merklish root of the range proof set.
    pub fn get_range_proof_root(&self) -> Result<HashOutput, ChainStorageError> {
        self.db.get_mmr_root(MmrTree::RangeProof)
    }

    /// Atomically commit the provided transaction to the database backend. This function does not update the metadata.
    pub(crate) fn commit(&mut self, txn: DbTransaction) -> Result<(), ChainStorageError> {
        self.db.write(txn)
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ChainStorageError> {
    error!(
        target: LOG_TARGET,
        "Unexpected result for database query {}. Response: {}", req, res
    );
    Err(ChainStorageError::UnexpectedResult)
}

fn log_error<T>(req: DbKey, err: ChainStorageError) -> Result<T, ChainStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req,
        err.to_string()
    );
    Err(err)
}

impl<T> Clone for BlockChainDatabase<T>
where T: BlockchainBackend
{
    fn clone(&self) -> Self {
        BlockChainDatabase {
            metadata: self.metadata.clone(),
            db: self.db.clone(),
        }
    }
}
