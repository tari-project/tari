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

use std::sync::Arc;

use tari_common::configuration::Network;
use tari_common_types::epoch::VnEpoch;
use thiserror::Error;

#[cfg(feature = "base_node")]
use crate::{
    blocks::ChainBlock,
    consensus::chain_strength_comparer::{strongest_chain, ChainStrengthComparer},
    proof_of_work::PowAlgorithm,
    proof_of_work::TargetDifficultyWindow,
};
use crate::{
    consensus::{
        emission::{Emission, EmissionSchedule},
        ConsensusConstants,
        NetworkConsensus,
    },
    proof_of_work::DifficultyAdjustmentError,
    transactions::{tari_amount::MicroMinotari, transaction_components::TransactionKernel},
};

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ConsensusManagerError {
    #[error("Difficulty adjustment encountered an error: `{0}`")]
    DifficultyAdjustmentError(#[from] DifficultyAdjustmentError),
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
    /// Start a builder for specified network
    pub fn builder(network: Network) -> ConsensusManagerBuilder {
        ConsensusManagerBuilder::new(network)
    }

    /// Returns the genesis block for the selected network.
    #[cfg(feature = "base_node")]
    pub fn get_genesis_block(&self) -> ChainBlock {
        use crate::blocks::genesis_block::get_genesis_block;
        let network = self.inner.network.as_network();
        match network {
            Network::LocalNet => self
                .inner
                .gen_block
                .clone()
                .unwrap_or_else(|| get_genesis_block(network)),
            _ => get_genesis_block(network),
        }
    }

    /// Get a reference to the emission parameters
    pub fn emission_schedule(&self) -> &EmissionSchedule {
        &self.inner.emission
    }

    /// Gets the block reward for the height
    pub fn get_block_reward_at(&self, height: u64) -> MicroMinotari {
        self.emission_schedule().block_reward(height)
    }

    /// Get the emission reward at height
    /// Returns None if the total supply > u64::MAX
    pub fn get_total_emission_at(&self, height: u64) -> MicroMinotari {
        self.inner.emission.supply_at_block(height)
    }

    /// Get a reference to consensus constants that are effective from the given height
    pub fn consensus_constants(&self, height: u64) -> &ConsensusConstants {
        let mut constants = &self.inner.consensus_constants[0];
        for c in &self.inner.consensus_constants {
            if c.effective_from_height() > height {
                break;
            }
            constants = c
        }
        constants
    }

    /// Returns the current epoch number as calculated from the given height
    pub fn block_height_to_epoch(&self, height: u64) -> VnEpoch {
        let mut epoch = 0;
        let mut leftover_height = 0;
        let mut active_effective_height = 0;
        let mut active_epoch_length = self.inner.consensus_constants[0].epoch_length();
        for c in &self.inner.consensus_constants[1..] {
            if c.effective_from_height() > height {
                break;
            }
            epoch += (c.effective_from_height() - active_effective_height + leftover_height) / active_epoch_length;
            leftover_height = std::cmp::min(
                c.epoch_length(),
                (c.effective_from_height() - active_effective_height + leftover_height) % active_epoch_length,
            );
            active_effective_height = c.effective_from_height();
            active_epoch_length = c.epoch_length();
        }
        epoch += (height - active_effective_height + leftover_height) / active_epoch_length;
        VnEpoch(epoch)
    }

    /// Returns the block height of the start of the given epoch number
    pub fn epoch_to_block_height(&self, epoch: VnEpoch) -> u64 {
        let mut cur_epoch = 0;
        let mut leftover_height = 0;
        let mut active_effective_height = 0;
        let mut active_epoch_length = self.inner.consensus_constants[0].epoch_length();
        for c in &self.inner.consensus_constants[1..] {
            if cur_epoch + (c.effective_from_height() - active_effective_height + leftover_height) / active_epoch_length >
                epoch.as_u64()
            {
                break;
            }
            cur_epoch += (c.effective_from_height() - active_effective_height + leftover_height) / active_epoch_length;
            leftover_height = std::cmp::min(
                c.epoch_length(),
                (c.effective_from_height() - active_effective_height + leftover_height) % active_epoch_length,
            );
            active_effective_height = c.effective_from_height();
            active_epoch_length = c.epoch_length();
        }
        (epoch.as_u64() - cur_epoch) * active_epoch_length + active_effective_height - leftover_height
    }

    /// Create a new TargetDifficulty for the given proof of work using constants that are effective from the given
    /// height
    #[cfg(feature = "base_node")]
    pub(crate) fn new_target_difficulty(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<TargetDifficultyWindow, String> {
        let constants = self.consensus_constants(height);
        let block_window = constants.difficulty_block_window();

        let block_window_u =
            usize::try_from(block_window).map_err(|e| format!("difficulty block window exceeds usize::MAX: {}", e))?;

        TargetDifficultyWindow::new(block_window_u, constants.pow_target_block_interval(pow_algo))
    }

    /// Creates a total_coinbase offset containing all fees for the validation from the height and kernel set
    pub fn calculate_coinbase_and_fees(
        &self,
        height: u64,
        kernels: &[TransactionKernel],
    ) -> Result<MicroMinotari, String> {
        let mut total = self.emission_schedule().block_reward(height);

        for kernel in kernels {
            match total.checked_add(kernel.fee) {
                Some(t) => total = t,
                None => {
                    return Err(format!(
                        "Coinbase total ({}) + fee ({}) exceeds max transactions allowance",
                        total, kernel.fee
                    ))
                },
            }
        }

        Ok(total)
    }

    /// Returns a ref to the chain strength comparer
    #[cfg(feature = "base_node")]
    pub fn chain_strength_comparer(&self) -> &dyn ChainStrengthComparer {
        self.inner.chain_strength_comparer.as_ref()
    }

    /// This is the currently configured chain network.
    pub fn network(&self) -> NetworkConsensus {
        self.inner.network
    }
}

/// This is the used to control all consensus values.
#[derive(Debug)]
struct ConsensusManagerInner {
    /// This is the inner struct used to control all consensus values.
    pub consensus_constants: Vec<ConsensusConstants>,
    /// The configured chain network.
    pub network: NetworkConsensus,
    /// The configuration for the emission schedule for integer only.
    pub emission: EmissionSchedule,
    /// This allows the user to set a custom Genesis block
    #[cfg(feature = "base_node")]
    pub gen_block: Option<ChainBlock>,
    #[cfg(feature = "base_node")]
    /// The comparer used to determine which chain is stronger for reorgs.
    pub chain_strength_comparer: Box<dyn ChainStrengthComparer + Send + Sync>,
}

/// Constructor for the consensus manager struct
pub struct ConsensusManagerBuilder {
    consensus_constants: Vec<ConsensusConstants>,
    network: NetworkConsensus,
    /// This is can only used be used if the network is localnet
    #[cfg(feature = "base_node")]
    gen_block: Option<ChainBlock>,
    #[cfg(feature = "base_node")]
    chain_strength_comparer: Option<Box<dyn ChainStrengthComparer + Send + Sync>>,
}

impl ConsensusManagerBuilder {
    /// Creates a new ConsensusManagerBuilder with the specified network
    pub fn new(network: Network) -> Self {
        ConsensusManagerBuilder {
            consensus_constants: vec![],
            network: network.into(),
            #[cfg(feature = "base_node")]
            gen_block: None,
            #[cfg(feature = "base_node")]
            chain_strength_comparer: None,
        }
    }

    /// Adds in a custom consensus constants to be used
    pub fn add_consensus_constants(mut self, consensus_constants: ConsensusConstants) -> Self {
        self.consensus_constants.push(consensus_constants);
        self
    }

    /// Adds in a custom block to be used. This will be overwritten if the network is anything else than localnet
    #[cfg(feature = "base_node")]
    pub fn with_block(mut self, block: ChainBlock) -> Self {
        self.gen_block = Some(block);
        self
    }

    #[cfg(feature = "base_node")]
    pub fn on_ties(mut self, chain_strength_comparer: Box<dyn ChainStrengthComparer + Send + Sync>) -> Self {
        self.chain_strength_comparer = Some(chain_strength_comparer);
        self
    }

    /// Builds a consensus manager
    pub fn build(mut self) -> Result<ConsensusManager, ConsensusBuilderError> {
        // should not be allowed to set the gen block and have the network type anything else than LocalNet
        // If feature != base_node, gen_block is not available
        #[cfg(feature = "base_node")]
        if self.network.as_network() != Network::LocalNet && self.gen_block.is_some() {
            return Err(ConsensusBuilderError::CannotSetGenesisBlock);
        }

        if self.consensus_constants.is_empty() {
            self.consensus_constants = self.network.create_consensus_constants();
        }

        let emission = EmissionSchedule::new(
            self.consensus_constants[0].emission_initial,
            self.consensus_constants[0].emission_decay,
            self.consensus_constants[0].inflation_bips,
            self.consensus_constants[0].tail_epoch_length,
            self.consensus_constants[0].faucet_value(),
        );

        let inner = ConsensusManagerInner {
            consensus_constants: self.consensus_constants,
            network: self.network,
            emission,
            #[cfg(feature = "base_node")]
            gen_block: self.gen_block,
            #[cfg(feature = "base_node")]
            chain_strength_comparer: self.chain_strength_comparer.unwrap_or_else(|| {
                strongest_chain()
                    .by_accumulated_difficulty()
                    .then()
                    .by_height()
                    .then()
                    .by_randomx_difficulty()
                    .then()
                    .by_sha3x_difficulty()
                    .build()
            }),
        };
        Ok(ConsensusManager { inner: Arc::new(inner) })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConsensusBuilderError {
    #[error("Cannot set a genesis block with a network other than LocalNet")]
    CannotSetGenesisBlock,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::ConsensusConstantsBuilder;

    fn create_manager() -> ConsensusManager {
        ConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_height(0)
                    .with_vn_epoch_length(15)
                    .build(),
            )
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_height(100)
                    .with_vn_epoch_length(6)
                    .build(),
            )
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_height(200)
                    .with_vn_epoch_length(8)
                    .build(),
            )
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_height(300)
                    .with_vn_epoch_length(13)
                    .build(),
            )
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_height(400)
                    .with_vn_epoch_length(17)
                    .build(),
            )
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_height(500)
                    .with_vn_epoch_length(7)
                    .build(),
            )
            .build()
            .unwrap()
    }

    #[test]
    fn test_epoch_to_height_and_back() {
        let manager = create_manager();
        assert_eq!(manager.block_height_to_epoch(99), VnEpoch(6)); // The next epoch should change at 105
        assert_eq!(manager.block_height_to_epoch(100), VnEpoch(7)); // But with the new length the epoch should change right away
        assert_eq!(manager.block_height_to_epoch(199), VnEpoch(23)); // The next epoch should change at 202
        assert_eq!(manager.block_height_to_epoch(202), VnEpoch(23)); // But we have new length with size +2 so the epoch change will happen at 204
        assert_eq!(manager.block_height_to_epoch(204), VnEpoch(24));
        // Now test couple more back and forth
        for epoch in 0..=100 {
            assert_eq!(
                manager.block_height_to_epoch(manager.epoch_to_block_height(VnEpoch(epoch))),
                VnEpoch(epoch)
            );
        }
    }

    #[test]
    fn test_epoch_is_non_decreasing() {
        let manager = create_manager();
        let mut epoch = manager.block_height_to_epoch(0).as_u64();
        for height in 0..600 {
            assert!(manager.block_height_to_epoch(height).as_u64() >= epoch);
            epoch = manager.block_height_to_epoch(height).as_u64();
        }
    }
}
