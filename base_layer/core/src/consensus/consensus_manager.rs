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
    blocks::{
        genesis_block::{
            get_mainnet_block_hash,
            get_mainnet_genesis_block,
            get_rincewind_block_hash,
            get_rincewind_genesis_block,
        },
        Block,
    },
    chain_storage::{BlockchainBackend, ChainMetadata, ChainStorageError},
    consensus::{emission::EmissionSchedule, network::Network, ConsensusConstants},
    proof_of_work::{DiffAdjManager, DiffAdjManagerError, Difficulty, DifficultyAdjustmentError, PowAlgorithm},
    transactions::tari_amount::MicroTari,
};
use derive_error::Error;
use std::sync::{Arc, RwLock, RwLockReadGuard};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hash::Hashable};

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
    /// No Difficulty adjustment manager present
    MissingDifficultyAdjustmentManager,
}

/// This is the consensus manager struct. This manages all state-full consensus code.
/// The inside is wrapped inside of an ARC so that it can safely and cheaply be cloned.
/// The code is multi-thread safe and so only one instance is required. Inner objects are wrapped inside of RwLocks.
pub struct ConsensusManager {
    inner: Arc<ConsensusManagerInner>,
}

impl ConsensusManager {
    /// Returns the genesis block for the selected network.
    pub fn get_genesis_block(&self) -> Block {
        match self.inner.network {
            Network::MainNet => get_mainnet_genesis_block(),
            Network::Rincewind => get_rincewind_genesis_block(),
            Network::LocalNet => (self.inner.gen_block.clone().unwrap_or_else(get_rincewind_genesis_block)),
        }
    }

    /// Returns the genesis block hash for the selected network.
    pub fn get_genesis_block_hash(&self) -> Vec<u8> {
        match self.inner.network {
            Network::MainNet => get_mainnet_block_hash(),
            Network::Rincewind => get_rincewind_block_hash(),
            Network::LocalNet => (self.inner.gen_block.clone().unwrap_or_else(get_rincewind_genesis_block)).hash(),
        }
    }

    /// Get a pointer to the emission schedule
    pub fn emission_schedule(&self) -> &EmissionSchedule {
        &self.inner.emission
    }

    /// Get a pointer to the consensus constants
    pub fn consensus_constants(&self) -> &ConsensusConstants {
        &self.inner.consensus_constants
    }

    /// This moves over a difficulty adjustment manager to the ConsensusManager to control.
    pub fn set_diff_manager(&self, diff_manager: DiffAdjManager) -> Result<(), ConsensusManagerError> {
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
    pub fn get_diff_manager(&self) -> Result<DiffAdjManager, ConsensusManagerError> {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => Ok(v.clone()),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm at the chain tip.
    pub fn get_target_difficulty<B: BlockchainBackend>(
        &self,
        metadata: &ChainMetadata,
        db: &B,
        pow_algo: PowAlgorithm,
    ) -> Result<Difficulty, ConsensusManagerError>
    {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_target_difficulty(metadata, db, pow_algo)
                .map_err(ConsensusManagerError::DifficultyAdjustmentManagerError),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm and provided height.
    pub fn get_target_difficulty_with_height<B: BlockchainBackend>(
        &self,
        db: &B,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<Difficulty, ConsensusManagerError>
    {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_target_difficulty_at_height(db, pow_algo, height)
                .map_err(ConsensusManagerError::DifficultyAdjustmentManagerError),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the median timestamp of the past 11 blocks at the chain tip.
    pub fn get_median_timestamp<B: BlockchainBackend>(
        &self,
        metadata: &ChainMetadata,
        db: &B,
    ) -> Result<EpochTime, ConsensusManagerError>
    {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_median_timestamp(metadata, db)
                .map_err(ConsensusManagerError::DifficultyAdjustmentManagerError),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Returns the median timestamp of the past 11 blocks at the provided height.
    pub fn get_median_timestamp_at_height<B: BlockchainBackend>(
        &self,
        db: &B,
        height: u64,
    ) -> Result<EpochTime, ConsensusManagerError>
    {
        match self.access_diff_adj()?.as_ref() {
            Some(v) => v
                .get_median_timestamp_at_height(db, height)
                .map_err(ConsensusManagerError::DifficultyAdjustmentManagerError),
            None => Err(ConsensusManagerError::MissingDifficultyAdjustmentManager),
        }
    }

    /// Creates a total_coinbase offset containing all fees for the validation from block
    pub fn calculate_coinbase_and_fees(&self, block: &Block) -> MicroTari {
        let coinbase = self.emission_schedule().block_reward(block.header.height);
        coinbase + block.calculate_fees()
    }

    // Inner helper function to access to the difficulty adjustment manager
    fn access_diff_adj(&self) -> Result<RwLockReadGuard<Option<DiffAdjManager>>, ConsensusManagerError> {
        self.inner
            .diff_adj_manager
            .read()
            .map_err(|e| ConsensusManagerError::PoisonedAccess(e.to_string()))
    }

    /// This is the currently configured chain network.
    pub fn network(&self) -> Network {
        self.inner.network
    }
}

impl Clone for ConsensusManager {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// This is the used to control all consensus values.
struct ConsensusManagerInner {
    /// Difficulty adjustment manager for the blockchain
    pub diff_adj_manager: RwLock<Option<DiffAdjManager>>,
    /// This is the inner struct used to control all consensus values.
    pub consensus_constants: ConsensusConstants,

    /// The configured chain network.
    pub network: Network,
    /// The configuration for the emission schedule.
    pub emission: EmissionSchedule,
    /// This allows the user to set a custom Genesis block
    pub gen_block: Option<Block>,
}

/// Constructor for the consensus manager struct
pub struct ConsensusManagerBuilder {
    /// Difficulty adjustment manager for the blockchain
    pub diff_adj_manager: Option<DiffAdjManager>,
    /// This is the inner struct used to control all consensus values.
    pub consensus_constants: Option<ConsensusConstants>,
    /// The configured chain network.
    pub network: Network,
    /// This allows the user to set a custom Genesis block
    pub gen_block: Option<Block>,
}

impl ConsensusManagerBuilder {
    /// Creates a new ConsensusManagerBuilder with the specified network
    pub fn new(network: Network) -> Self {
        ConsensusManagerBuilder {
            diff_adj_manager: None,
            consensus_constants: None,
            network,
            gen_block: None,
        }
    }

    /// Adds in a custom consensus constants to be used
    pub fn with_consensus_constants(mut self, consensus_constants: ConsensusConstants) -> Self {
        self.consensus_constants = Some(consensus_constants);
        self
    }

    /// Adds in a difficulty adjustment manager to be used to be used
    pub fn with_difficulty_adjustment_manager(mut self, difficulty_adj: DiffAdjManager) -> Self {
        self.diff_adj_manager = Some(difficulty_adj);
        self
    }

    /// Adds in a custom block to be used. This will be overwritten if the network is anything else than localnet
    pub fn with_block(mut self, block: Block) -> Self {
        self.gen_block = Some(block);
        self
    }

    /// Builds a consensus manager
    #[allow(clippy::or_fun_call)]
    pub fn build(self) -> ConsensusManager {
        let consensus_constants = self
            .consensus_constants
            .unwrap_or(self.network.create_consensus_constants());
        let emission = EmissionSchedule::new(
            consensus_constants.emission_initial,
            consensus_constants.emission_decay,
            consensus_constants.emission_tail,
        );
        let inner = ConsensusManagerInner {
            diff_adj_manager: RwLock::new(self.diff_adj_manager),
            consensus_constants,
            network: self.network,
            emission,
            gen_block: self.gen_block,
        };
        ConsensusManager { inner: Arc::new(inner) }
    }
}
