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
    blocks::Block,
    chain_storage::{BlockchainBackend, ChainStorageError},
    consensus::emission::EmissionSchedule,
    proof_of_work::{DiffAdjManager, DiffAdjManagerError, Difficulty, DifficultyAdjustmentError, PowAlgorithm},
    transactions::tari_amount::MicroTari,
};
use derive_error::Error;
use std::sync::{Arc, RwLock, RwLockReadGuard};
use tari_crypto::tari_utilities::epoch_time::EpochTime;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum ConsensusManagerError {
    /// Difficulty adjustment encountered an error
    DifficultyAdjustmentError(DifficultyAdjustmentError),
    /// Difficulty adjustment manager encountered an error
    DifficultyAdjustmentManagerError(DiffAdjManagerError),
    /// Problem with the DB backend storage
    ChainStorageError(ChainStorageError),
    /// There is no blockchain to query
    EmptyBlockchain,
    /// RwLock access broken.
    #[error(non_std, no_from)]
    PoisonedAccess(String),
    /// No Diffuclty adjustment manager present
    MissingDifficultyAdjustmentManager,
}

/// This is the consensus manager struct. This manages all state-full consensus code.
/// The inside is wrapped inside of an ARC so that it can safely and cheaply be cloned.
/// The code is multi-thread safe and so only one instance is required. Inner objects are wrapped inside of RwLocks.
pub struct ConsensusManager<B>
where B: BlockchainBackend
{
    inner: Arc<ConsensusManagerInner<B>>,
}

impl<B> ConsensusManager<B>
where B: BlockchainBackend
{
    /// Get a pointer to the emission schedule
    pub fn emission_schedule(&self) -> &EmissionSchedule {
        &self.inner.emission_schedule
    }

    /// This moves over a difficulty adjustment manager to the ConsensusManager to control.
    pub fn set_diff_manager(&self, diff_manager: DiffAdjManager<B>) -> Result<(), ConsensusManagerError> {
        let mut lock = self
            .inner
            .diff_adj_manager
            .write()
            .map_err(|e| ConsensusManagerError::PoisonedAccess(e.to_string()))?;
        *lock = Some(diff_manager);
        Ok(())
    }

    /// This returns the difficulty adjustment manager back. This can safely be cloned as the Difficulty adjustment
    /// manager wraps an ARC in side of it.
    pub fn get_diff_manager(&self) -> Result<DiffAdjManager<B>, ConsensusManagerError> {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => Ok(v.clone()),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm at the chain tip.
    pub fn get_target_difficulty(&self, pow_algo: &PowAlgorithm) -> Result<Difficulty, ConsensusManagerError> {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_target_difficulty(pow_algo)
                .map_err(|e| ConsensusManagerError::DifficultyAdjustmentManagerError(e)),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm and provided height.
    pub fn get_target_difficulty_with_height(
        &self,
        pow_algo: &PowAlgorithm,
        height: u64,
    ) -> Result<Difficulty, ConsensusManagerError>
    {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_target_difficulty_at_height(pow_algo, height)
                .map_err(|e| ConsensusManagerError::DifficultyAdjustmentManagerError(e)),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the median timestamp of the past 11 blocks at the chain tip.
    pub fn get_median_timestamp(&self) -> Result<EpochTime, ConsensusManagerError> {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_median_timestamp()
                .map_err(|e| ConsensusManagerError::DifficultyAdjustmentManagerError(e)),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the median timestamp of the past 11 blocks at the provided height.
    pub fn get_median_timestamp_at_height(&self, height: u64) -> Result<EpochTime, ConsensusManagerError> {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_median_timestamp_at_height(height)
                .map_err(|e| ConsensusManagerError::DifficultyAdjustmentManagerError(e)),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Creates a total_coinbase offset containing all fees for the validation from block
    pub fn calculate_coinbase_and_fees(&self, block: &Block) -> MicroTari {
        let coinbase = self.emission_schedule().block_reward(block.header.height);
        coinbase + block.calculate_fees()
    }

    // Inner helper function to access to the difficulty adjustment manager
    fn access_diff_adj(&self) -> Result<RwLockReadGuard<Option<DiffAdjManager<B>>>, ConsensusManagerError> {
        self.inner
            .diff_adj_manager
            .read()
            .map_err(|e| ConsensusManagerError::PoisonedAccess(e.to_string()))
    }
}

impl<B> Default for ConsensusManager<B>
where B: BlockchainBackend
{
    fn default() -> Self {
        ConsensusManager {
            inner: Arc::new(ConsensusManagerInner::default()),
        }
    }
}

impl<B> Clone for ConsensusManager<B>
where B: BlockchainBackend
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// This is the used to control all consensus values.
struct ConsensusManagerInner<B>
where B: BlockchainBackend
{
    /// The emission schedule to use for coinbase rewards
    pub emission_schedule: EmissionSchedule,
    /// Difficulty adjustment manager for the blockchain
    pub diff_adj_manager: RwLock<Option<DiffAdjManager<B>>>,
}

impl<B> Default for ConsensusManagerInner<B>
where B: BlockchainBackend
{
    fn default() -> Self {
        ConsensusManagerInner {
            emission_schedule: EmissionSchedule::new(10_000_000.into(), 0.999, 100.into()),
            diff_adj_manager: RwLock::new(None),
        }
    }
}
