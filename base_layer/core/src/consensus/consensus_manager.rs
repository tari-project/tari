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
            get_ridcully_block_hash,
            get_ridcully_genesis_block,
            get_stibbons_block_hash,
            get_stibbons_genesis_block,
        },
        Block,
    },
    chain_storage::{ChainBlock, ChainStorageError},
    consensus::{
        chain_strength_comparer::{strongest_chain, ChainStrengthComparer},
        emission::{Emission, EmissionSchedule},
        network::Network,
        ConsensusConstants,
    },
    proof_of_work::{DifficultyAdjustmentError, PowAlgorithm, TargetDifficultyWindow},
    transactions::tari_amount::MicroTari,
};
use std::{convert::TryFrom, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsensusManagerError {
    #[error("Difficulty adjustment encountered an error: `{0}`")]
    DifficultyAdjustmentError(#[from] DifficultyAdjustmentError),
    #[error("Problem with the DB backend storage: `{0}`")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("There is no blockchain to query")]
    EmptyBlockchain,
    #[error("RwLock access broken: `{0}`")]
    PoisonedAccess(String),
    #[error("No Difficulty adjustment manager present")]
    MissingDifficultyAdjustmentManager,
}

/// Container struct for consensus rules. This can be cheaply cloned.
#[derive(Debug, Clone)]
pub struct ConsensusManager {
    inner: Arc<ConsensusManagerInner>,
}

impl ConsensusManager {
    pub fn builder(network: Network) -> ConsensusManagerBuilder {
        ConsensusManagerBuilder::new(network)
    }

    /// Returns the genesis block for the selected network.
    pub fn get_genesis_block(&self) -> ChainBlock {
        match self.inner.network {
            Network::MainNet => get_mainnet_genesis_block(),
            Network::Ridcully => get_ridcully_genesis_block(),
            Network::Stibbons => get_stibbons_genesis_block(),
            Network::LocalNet => self.inner.gen_block.clone().unwrap_or_else(get_stibbons_genesis_block),
        }
    }

    /// Returns the genesis block hash for the selected network.
    #[deprecated]
    pub fn get_genesis_block_hash(&self) -> Vec<u8> {
        match self.inner.network {
            Network::MainNet => get_mainnet_block_hash(),
            Network::Ridcully => get_ridcully_block_hash(),
            Network::Stibbons => get_stibbons_block_hash(),
            Network::LocalNet => get_ridcully_block_hash(),
        }
    }

    /// Get a pointer to the emission schedule
    /// The height provided here, decides the emission curve to use. It swaps to the integer curve upon reaching
    /// 1_000_000_000
    pub fn emission_schedule(&self) -> &dyn Emission {
        &self.inner.emission
    }

    pub fn get_block_reward_at(&self, height: u64) -> MicroTari {
        self.emission_schedule().block_reward(height)
    }

    // Get the emission reward at height
    pub fn get_total_emission_at(&self, height: u64) -> MicroTari {
        self.inner.emission.supply_at_block(height)
    }

    /// Get a reference to consensus constants that are effective from the given height
    pub fn consensus_constants(&self, height: u64) -> &ConsensusConstants {
        let mut constants = &self.inner.consensus_constants[0];
        for c in self.inner.consensus_constants.iter() {
            if c.effective_from_height() > height {
                break;
            }
            constants = &c
        }
        constants
    }

    // This finds the max block window time in all the consensus constants
    pub fn max_block_window(&self) -> u64 {
        let mut block_window = self.inner.consensus_constants[0].get_difficulty_block_window();
        for c in self.inner.consensus_constants.iter() {
            if c.get_difficulty_block_window() > block_window {
                block_window = c.get_difficulty_block_window();
            }
        }
        block_window
    }

    /// Create a new TargetDifficulty for the given proof of work using constants that are effective from the given
    /// height
    pub(crate) fn new_target_difficulty(&self, pow_algo: PowAlgorithm, height: u64) -> TargetDifficultyWindow {
        let constants = self.consensus_constants(height);
        let block_window = constants.get_difficulty_block_window();

        TargetDifficultyWindow::new(
            usize::try_from(block_window).expect("difficulty block window exceeds usize::MAX"),
            constants.get_diff_target_block_interval(pow_algo),
            constants.min_pow_difficulty(pow_algo),
            constants.max_pow_difficulty(pow_algo),
            constants.get_difficulty_max_block_interval(pow_algo),
        )
    }

    /// Creates a total_coinbase offset containing all fees for the validation from block
    pub fn calculate_coinbase_and_fees(&self, block: &Block) -> MicroTari {
        let coinbase = self.emission_schedule().block_reward(block.header.height);
        coinbase + block.calculate_fees()
    }

    pub fn chain_strength_comparer(&self) -> &dyn ChainStrengthComparer {
        self.inner.chain_strength_comparer.as_ref()
    }

    /// This is the currently configured chain network.
    pub fn network(&self) -> Network {
        self.inner.network
    }
}

/// This is the used to control all consensus values.
#[derive(Debug)]
struct ConsensusManagerInner {
    /// This is the inner struct used to control all consensus values.
    pub consensus_constants: Vec<ConsensusConstants>,
    /// The configured chain network.
    pub network: Network,
    /// The configuration for the emission schedule for integer only.
    pub emission: EmissionSchedule,
    /// This allows the user to set a custom Genesis block
    pub gen_block: Option<ChainBlock>,
    /// The comparer used to determine which chain is stronger for reorgs.
    pub chain_strength_comparer: Box<dyn ChainStrengthComparer + Send + Sync>,
}

/// Constructor for the consensus manager struct
pub struct ConsensusManagerBuilder {
    consensus_constants: Vec<ConsensusConstants>,
    network: Network,
    gen_block: Option<ChainBlock>,
    chain_strength_comparer: Option<Box<dyn ChainStrengthComparer + Send + Sync>>,
}

impl ConsensusManagerBuilder {
    /// Creates a new ConsensusManagerBuilder with the specified network
    pub fn new(network: Network) -> Self {
        ConsensusManagerBuilder {
            consensus_constants: vec![],
            network,
            gen_block: None,
            chain_strength_comparer: None,
        }
    }

    /// Adds in a custom consensus constants to be used
    pub fn with_consensus_constants(mut self, consensus_constants: ConsensusConstants) -> Self {
        self.consensus_constants.push(consensus_constants);
        self
    }

    /// Adds in a custom block to be used. This will be overwritten if the network is anything else than localnet
    pub fn with_block(mut self, block: ChainBlock) -> Self {
        self.gen_block = Some(block);
        self
    }

    pub fn on_ties(mut self, chain_strength_comparer: Box<dyn ChainStrengthComparer + Send + Sync>) -> Self {
        self.chain_strength_comparer = Some(chain_strength_comparer);
        self
    }

    /// Builds a consensus manager
    pub fn build(mut self) -> ConsensusManager {
        if self.consensus_constants.is_empty() {
            self.consensus_constants = self.network.create_consensus_constants();
        }
        // TODO: Check that constants is not empty

        let emission = EmissionSchedule::new(
            self.consensus_constants[0].emission_initial,
            &self.consensus_constants[0].emission_decay,
            self.consensus_constants[0].emission_tail,
        );
        let inner = ConsensusManagerInner {
            consensus_constants: self.consensus_constants,
            network: self.network,
            emission,
            gen_block: self.gen_block,
            chain_strength_comparer: self.chain_strength_comparer.unwrap_or_else(|| {
                strongest_chain()
                    .by_accumulated_difficulty()
                    .then()
                    .by_height()
                    .then()
                    .by_monero_difficulty()
                    .then()
                    .by_blake_difficulty()
                    .build()
            }),
        };
        ConsensusManager { inner: Arc::new(inner) }
    }
}
