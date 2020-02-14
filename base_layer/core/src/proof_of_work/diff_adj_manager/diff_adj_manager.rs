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

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    proof_of_work::{
        diff_adj_manager::{diff_adj_storage::DiffAdjStorage, error::DiffAdjManagerError},
        Difficulty,
        PowAlgorithm,
    },
};
use std::sync::{Arc, RwLock};
use tari_crypto::tari_utilities::epoch_time::EpochTime;

/// The DiffAdjManager is used to calculate the current target difficulty based on PoW recorded in the latest blocks of
/// the current best chain.
pub struct DiffAdjManager<T>
where T: BlockchainBackend
{
    diff_adj_storage: Arc<RwLock<DiffAdjStorage<T>>>,
}

impl<T> DiffAdjManager<T>
where T: BlockchainBackend
{
    /// Constructs a new DiffAdjManager with access to the blockchain db.
    pub fn new(blockchain_db: BlockchainDatabase<T>) -> Result<Self, DiffAdjManagerError> {
        Ok(Self {
            diff_adj_storage: Arc::new(RwLock::new(DiffAdjStorage::new(blockchain_db))),
        })
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm at the chain tip.
    pub fn get_target_difficulty(&self, pow_algo: &PowAlgorithm) -> Result<Difficulty, DiffAdjManagerError> {
        self.diff_adj_storage
            .write()
            .map_err(|_| DiffAdjManagerError::PoisonedAccess)?
            .get_target_difficulty(pow_algo)
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm and provided height.
    pub fn get_target_difficulty_at_height(
        &self,
        pow_algo: &PowAlgorithm,
        height: u64,
    ) -> Result<Difficulty, DiffAdjManagerError>
    {
        self.diff_adj_storage
            .write()
            .map_err(|_| DiffAdjManagerError::PoisonedAccess)?
            .get_target_difficulty_at_height(pow_algo, height)
    }

    /// Returns the median timestamp of the past 11 blocks at the chain tip.
    pub fn get_median_timestamp(&self) -> Result<EpochTime, DiffAdjManagerError> {
        self.diff_adj_storage
            .write()
            .map_err(|_| DiffAdjManagerError::PoisonedAccess)?
            .get_median_timestamp()
    }

    /// Returns the median timestamp of the past 11 blocks at the provided height.
    pub fn get_median_timestamp_at_height(&self, height: u64) -> Result<EpochTime, DiffAdjManagerError> {
        self.diff_adj_storage
            .write()
            .map_err(|_| DiffAdjManagerError::PoisonedAccess)?
            .get_median_timestamp_at_height(height)
    }
}

impl<T> Clone for DiffAdjManager<T>
where T: BlockchainBackend
{
    fn clone(&self) -> Self {
        Self {
            diff_adj_storage: self.diff_adj_storage.clone(),
        }
    }
}
