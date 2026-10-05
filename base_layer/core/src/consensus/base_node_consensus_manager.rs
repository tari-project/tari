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
use tari_node_components::blocks::ChainBlock;
use tari_transaction_components::{
    MicroMinotari,
    consensus::{
        ConsensusConstants,
        ConsensusManager,
        ConsensusManagerBuilder,
        NetworkConsensus,
        emission::{Emission, EmissionSchedule},
    },
    tari_proof_of_work::PowAlgorithm,
    transaction_components::TransactionKernel,
};

use crate::{
    blocks::pre_mine::{get_pre_mine_items, pre_mine_spendable_at_height},
    consensus::chain_strength_comparer::{ChainStrengthComparer, strongest_chain},
    proof_of_work::TargetDifficultyWindow,
};

/// A simple struct to hold the maturity and effective height
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct MaturityTranche {
    pub maturity: u64,
    pub effective_from_height: u64,
}

/// Container struct for consensus rules. This can be cheaply cloned.
#[derive(Debug, Clone)]
pub struct BaseNodeConsensusManager {
    inner: Arc<BaseNodeConsensusManagerInner>,
}

impl BaseNodeConsensusManager {
    /// Start a builder for specified network
    pub fn builder(network: Network) -> BaseNodeConsensusManagerBuilder {
        BaseNodeConsensusManagerBuilder::new(network)
    }

    /// Returns the genesis block for the selected network.
    pub fn get_genesis_block(&self) -> ChainBlock {
        use crate::blocks::genesis_block::get_genesis_block;
        let network = self.inner.consensus_manager.network().as_network();
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
        self.inner.consensus_manager.emission()
    }

    /// Gets the block reward for the height
    pub fn get_block_reward_at(&self, height: u64) -> MicroMinotari {
        self.emission_schedule().block_reward(height)
    }

    /// Get the emission reward at height
    /// Returns None if the total supply > u64::MAX
    pub fn get_total_emission_at(&self, height: u64) -> MicroMinotari {
        self.inner.consensus_manager.emission().supply_at_block(height)
    }

    /// Get a reference to consensus constants that are effective from the given height
    pub fn consensus_constants(&self, height: u64) -> &ConsensusConstants {
        self.inner.consensus_manager.consensus_constants(height)
    }

    /// Get the vector of consensus constants applicable for all heights
    pub fn consensus_constants_vec(&self) -> &[ConsensusConstants] {
        self.inner.consensus_manager.consensus_constants_vec()
    }

    /// The height from which the GHSA-3qmx-q9pv-f3m4 proof of work rules are consensus on this network, or
    /// `u64::MAX` (`UNSCHEDULED_ACTIVATION_HEIGHT`) where the advisory's fork is not scheduled.
    ///
    /// Read off the constants vector, so there is no second place to keep in step with the activation entries.
    pub fn derived_monero_coinbase_activation_height(&self) -> u64 {
        ConsensusConstants::derived_monero_coinbase_activation_height(self.consensus_constants_vec())
    }

    /// The height from which the GHSA-3qmx-q9pv-f3m4 bipartite Cuckaroo verifier is consensus on this network, or
    /// `u64::MAX` (`UNSCHEDULED_ACTIVATION_HEIGHT`) where the advisory's fork is not scheduled.
    ///
    /// Read off the constants vector, so there is no second place to keep in step with the activation entries. The
    /// accumulated-data rebuild migration uses this as the height from and above which stored `target_difficulty`
    /// values must be recomputed and re-validated under the post-fork rules.
    pub fn bipartite_cuckaroo_activation_height(&self) -> u64 {
        ConsensusConstants::bipartite_cuckaroo_activation_height(self.consensus_constants_vec())
    }

    pub fn consensus_manager(&self) -> ConsensusManager {
        self.inner.consensus_manager.clone()
    }

    /// Create a new TargetDifficulty for the given proof of work using constants that are effective from the given
    /// height
    pub(crate) fn new_target_difficulty(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<TargetDifficultyWindow, String> {
        let constants = self.consensus_constants(height);
        let block_window = constants.difficulty_block_window();

        let block_window_u =
            usize::try_from(block_window).map_err(|e| format!("difficulty block window exceeds usize::MAX: {e}"))?;

        TargetDifficultyWindow::new(block_window_u, constants.pow_target_block_interval(pow_algo))
    }

    /// Creates a total_coinbase offset containing all fees for the validation from the height and kernel set
    pub fn calculate_coinbase_and_fees(
        &self,
        height: u64,
        kernels: &[TransactionKernel],
    ) -> Result<MicroMinotari, String> {
        self.inner
            .consensus_manager
            .calculate_coinbase_and_fees(height, kernels)
    }

    /// Returns a ref to the chain strength comparer
    pub fn chain_strength_comparer(&self) -> &dyn ChainStrengthComparer {
        self.inner.chain_strength_comparer.as_ref()
    }

    /// This is the currently configured chain network.
    pub fn network(&self) -> NetworkConsensus {
        self.inner.consensus_manager.network()
    }

    /// Get the maturity tranches from the consensus manager
    pub fn get_maturity_tranches(&self) -> Vec<MaturityTranche> {
        self.consensus_constants_vec()
            .iter()
            .map(|c| MaturityTranche {
                maturity: c.coinbase_min_maturity(),
                effective_from_height: c.effective_from_height(),
            })
            .collect::<Vec<_>>()
    }

    /// Get the total spendable block rewards and pre-mine at the specified height
    pub fn total_tokens_spendable_at_height(&self, height: u64) -> Result<MicroMinotari, String> {
        let spendable_rewards = self.block_rewards_spendable_at_height(height)?;
        let spendable_pre_mine = self.pre_mine_spendable_at_height(height)?;
        spendable_rewards
            .checked_add(spendable_pre_mine)
            .ok_or_else(|| "total_tokens_spendable_at_height overflowed u128".to_string())
    }

    /// Get the total circulating block rewards and spendable pre-mine at the specified height
    pub fn total_tokens_circulating_at_height(&self, height: u64) -> Result<MicroMinotari, String> {
        let mined_rewards = self.block_rewards_mined_at_height(height)?;
        let spendable_pre_mine = self.pre_mine_spendable_at_height(height)?;
        mined_rewards
            .checked_add(spendable_pre_mine)
            .ok_or_else(|| "total_circulating_tokens_at_height overflowed u128".to_string())
    }

    /// Get the total spendable pre-mine at the specified height
    pub fn pre_mine_spendable_at_height(&self, height: u64) -> Result<MicroMinotari, String> {
        pre_mine_spendable_at_height(height, self.network().as_network())
    }

    /// Get the total spendable pre-mine at the specified height
    pub fn total_pre_mine_in_genesis_block(&self) -> MicroMinotari {
        self.consensus_constants(0).pre_mine_value()
    }

    /// Get the total pre-mine that is still time-locked at the specified height
    pub fn time_locked_pre_mine(&self, height: u64) -> Result<MicroMinotari, String> {
        Ok(self
            .total_pre_mine_in_genesis_block()
            .saturating_sub(self.pre_mine_spendable_at_height(height)?))
    }

    /// Get the total mined block rewards at the specified height (excluding pre-mine)
    pub fn block_rewards_mined_at_height(&self, height: u64) -> Result<MicroMinotari, String> {
        Ok(self
            .get_total_emission_at(height)
            .saturating_sub(self.consensus_constants(height).pre_mine_value()))
    }

    /// Get the total spendable block rewards circulation at the specified height (excluding pre-mine)
    pub fn block_rewards_spendable_at_height(&self, height: u64) -> Result<MicroMinotari, String> {
        let maturity_tranches = self.get_maturity_tranches();
        let spendable_height = self.spendable_emission_height(&maturity_tranches, height)?;
        Ok(self
            .emission_schedule()
            .supply_at_block(spendable_height)
            .saturating_sub(self.consensus_constants(height).pre_mine_value()))
    }

    /// The emission height whose supply is spendable at `height`, i.e. `height` less the coinbase maturity that
    /// applies to it.
    fn spendable_emission_height(&self, maturity_tranches: &[MaturityTranche], height: u64) -> Result<u64, String> {
        // Example initial maturity schedule up to 3 weeks ( | height | (maturity) |):
        // | 0 -> 5040 - 1 | (720) |
        //                 | 5040 -> 10080 - 1 | (540) |
        //                                     | 10080 -> 15120 - 1 | (360) |
        //                                                          | 15120 -> | (180) |

        // `get_maturity_tranches` maps the consensus constants vector one to one and in order, so the index of the
        // active constants entry is also the index of the active tranche. Going through
        // `ConsensusConstants::active_index_at_height` keeps this on the single authoritative definition of "active"
        // rather than adding another lookup that has to be kept in step, and it is exact where searching the tranche
        // vector by value was not: two constants entries can produce identical `MaturityTranche` values.
        let last_effective_index =
            ConsensusConstants::active_index_at_height(self.consensus_constants_vec(), height)
                .ok_or_else(|| format!("Last effective maturity tranche for height {height} not found"))?;
        let last_effective_tranche = maturity_tranches
            .get(last_effective_index)
            .ok_or_else(|| format!("Last effective maturity tranche for height {height} not found"))?;
        let previous_effective_tranch = maturity_tranches
            .get(last_effective_index.saturating_sub(1))
            .ok_or_else(|| format!("Last effective maturity tranche index for height {height} not found"))?;

        // We have to adjust the matured rewards at height to account for the effective from height of the last
        // effective tranche
        let spendable_height = if last_effective_tranche.maturity < previous_effective_tranch.maturity &&
            height <
                last_effective_tranche
                    .effective_from_height
                    .saturating_add(previous_effective_tranch.maturity)
        {
            height.saturating_sub(previous_effective_tranch.maturity)
        } else {
            height.saturating_sub(last_effective_tranche.maturity)
        };

        Ok(spendable_height)
    }

    /// Get the token values reported by the `GetTokensInCirculation` gRPC method for each of the given heights.
    ///
    /// The values are the same as those returned by the per-height functions above, but the emission schedule is
    /// walked once for all heights and the pre-mine schedule is built once, so the cost is bounded by the highest
    /// height plus the number of heights. `heights` must be sorted ascending and contain no duplicates; the results
    /// are returned in the same order.
    pub fn token_values_at_heights(&self, heights: &[u64]) -> Result<Vec<TokenValuesAtHeight>, String> {
        if heights.iter().zip(heights.iter().skip(1)).any(|(a, b)| a >= b) {
            return Err("Heights must be sorted ascending and unique".to_string());
        }

        // The emission heights we need the supply at: each height and its spendable emission height
        let maturity_tranches = self.get_maturity_tranches();
        let mut spendable_heights = Vec::with_capacity(heights.len());
        for &height in heights {
            spendable_heights.push(self.spendable_emission_height(&maturity_tranches, height)?);
        }
        let mut targets = heights.to_vec();
        targets.extend_from_slice(&spendable_heights);
        targets.sort_unstable();
        targets.dedup();

        // Walk the emission schedule once, recording the supply at every target height. A `None` from the iterator
        // (supply overflow, reachable on LocalNet) is ignored exactly as `EmissionSchedule::supply_at_block` does, so
        // the values stay identical to the per-height functions.
        let mut supplies = Vec::with_capacity(targets.len());
        let mut emission = self.emission_schedule().iter();
        for &target in &targets {
            while emission.block_height() < target {
                let _ignore = emission.next();
            }
            supplies.push(emission.supply());
        }
        let supply_at = |height: u64| -> Result<MicroMinotari, String> {
            let index = targets
                .binary_search(&height)
                .map_err(|_| format!("Emission supply at height {height} not found"))?;
            supplies
                .get(index)
                .copied()
                .ok_or_else(|| format!("Emission supply at height {height} not found"))
        };

        // Build the pre-mine schedule once, ordered by the height at which each item becomes spendable
        let mut pre_mine_items = get_pre_mine_items(self.network().as_network())?;
        pre_mine_items.sort_by_key(|item| item.original_maturity);
        let total_pre_mine = self.total_pre_mine_in_genesis_block();

        let mut results = Vec::with_capacity(heights.len());
        let mut pre_mine_index = 0;
        let mut spendable_pre_mine = MicroMinotari::zero();
        for (&height, &spendable_height) in heights.iter().zip(spendable_heights.iter()) {
            while let Some(item) = pre_mine_items.get(pre_mine_index) {
                if item.original_maturity > height {
                    break;
                }
                spendable_pre_mine = spendable_pre_mine
                    .checked_add(item.value)
                    .ok_or_else(|| "pre_mine_spendable_at_height overflowed u128".to_string())?;
                pre_mine_index = pre_mine_index.saturating_add(1);
            }

            let pre_mine_value = self.consensus_constants(height).pre_mine_value();
            let mined_rewards = supply_at(height)?.saturating_sub(pre_mine_value);
            let spendable_rewards = supply_at(spendable_height)?.saturating_sub(pre_mine_value);
            let circulating_supply = mined_rewards
                .checked_add(spendable_pre_mine)
                .ok_or_else(|| "total_circulating_tokens_at_height overflowed u128".to_string())?;
            let total_spendable = spendable_rewards
                .checked_add(spendable_pre_mine)
                .ok_or_else(|| "total_tokens_spendable_at_height overflowed u128".to_string())?;

            results.push(TokenValuesAtHeight {
                height,
                circulating_supply,
                mined_rewards,
                spendable_rewards,
                spendable_pre_mine,
                total_spendable,
                total_pre_mine,
                time_locked_pre_mine: total_pre_mine.saturating_sub(spendable_pre_mine),
            });
        }

        Ok(results)
    }
}

/// The token values at a single height, as reported by the `GetTokensInCirculation` gRPC method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenValuesAtHeight {
    pub height: u64,
    pub circulating_supply: MicroMinotari,
    pub mined_rewards: MicroMinotari,
    pub spendable_rewards: MicroMinotari,
    pub spendable_pre_mine: MicroMinotari,
    pub total_spendable: MicroMinotari,
    pub total_pre_mine: MicroMinotari,
    pub time_locked_pre_mine: MicroMinotari,
}

/// This is the used to control all consensus values.
#[derive(Debug)]
struct BaseNodeConsensusManagerInner {
    pub consensus_manager: ConsensusManager,

    pub gen_block: Option<ChainBlock>,
    /// The comparer used to determine which chain is stronger for reorgs.
    pub chain_strength_comparer: Box<dyn ChainStrengthComparer + Send + Sync>,
}

/// Constructor for the consensus manager struct
pub struct BaseNodeConsensusManagerBuilder {
    consensus_manager_builder: ConsensusManagerBuilder,
    /// This is can only used be used if the network is localnet
    gen_block: Option<ChainBlock>,
    chain_strength_comparer: Option<Box<dyn ChainStrengthComparer + Send + Sync>>,
}

impl BaseNodeConsensusManagerBuilder {
    /// Creates a new ConsensusManagerBuilder with the specified network
    pub fn new(network: Network) -> Self {
        BaseNodeConsensusManagerBuilder {
            consensus_manager_builder: ConsensusManagerBuilder::new(network),
            gen_block: None,
            chain_strength_comparer: None,
        }
    }

    /// Adds in a custom consensus constants to be used
    pub fn add_consensus_constants(mut self, consensus_constants: ConsensusConstants) -> Self {
        self.consensus_manager_builder = self
            .consensus_manager_builder
            .add_consensus_constants(consensus_constants);
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
    pub fn build(self) -> Result<BaseNodeConsensusManager, BaseConsensusBuilderError> {
        // should not be allowed to set the gen block and have the network type anything else than LocalNet
        // If feature != base_node, gen_block is not available
        if self.consensus_manager_builder.network.as_network() != Network::LocalNet && self.gen_block.is_some() {
            return Err(BaseConsensusBuilderError::CannotSetGenesisBlock);
        }

        let consensus_manager = self.consensus_manager_builder.build();

        let inner = BaseNodeConsensusManagerInner {
            consensus_manager,
            gen_block: self.gen_block,
            chain_strength_comparer: self.chain_strength_comparer.unwrap_or_else(|| {
                strongest_chain()
                    .by_accumulated_difficulty()
                    .then()
                    .by_height()
                    .then()
                    .by_tari_randomx_difficulty()
                    .then()
                    .by_monero_randomx_difficulty()
                    .then()
                    .by_sha3x_difficulty()
                    .then()
                    .by_cuckaroo_cycle_difficulty()
                    .build()
            }),
        };
        Ok(BaseNodeConsensusManager { inner: Arc::new(inner) })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BaseConsensusBuilderError {
    #[error("Cannot set a genesis block with a network other than LocalNet")]
    CannotSetGenesisBlock,
}

#[cfg(test)]
mod test {
    /// The average amount of blocks per day based on the target block time
    pub const BLOCKS_PER_DAY: u64 = 24 * 60 / 2;
    use std::str::FromStr;

    use tari_transaction_components::consensus::consensus_constants::MAINNET_PRE_MINE_VALUE;

    use super::*;

    #[test]
    fn test_supply_at_block() {
        let network = Network::MainNet;
        let consensus_manager = BaseNodeConsensusManager::builder(network).build().unwrap();
        for (height, mined, spendable, pre_mine, total) in [
            (
                0,
                MicroMinotari::from_str("        0.000000 T"), // mined
                MicroMinotari::from_str("        0.000000 T"), // spendable
                MicroMinotari::from_str("756000002.000000 T"), // pre_mine
                MicroMinotari::from_str("756000002.000000 T"), // total
            ),
            (
                1000,
                MicroMinotari::from_str(" 13946753.809464 T"), // mined
                MicroMinotari::from_str("  3906326.802521 T"), // spendable
                MicroMinotari::from_str("756000002.000000 T"), // pre_mine
                MicroMinotari::from_str("759906328.802521 T"), // total
            ),
            (
                10000,
                MicroMinotari::from_str("138917413.875832 T"), // mined
                MicroMinotari::from_str("131447021.355866 T"), // spendable
                MicroMinotari::from_str("756000002.000000 T"), // pre_mine
                MicroMinotari::from_str("887447023.355866 T"), // total
            ),
            (
                180 * BLOCKS_PER_DAY,
                MicroMinotari::from_str("1709098961.342784 T"), // mined
                MicroMinotari::from_str("1706857672.130454 T"), // spendable
                MicroMinotari::from_str(" 867125003.916666 T"), // pre_mine
                MicroMinotari::from_str("2573982676.047120 T"), // total
            ),
            (
                (180 + 20) * BLOCKS_PER_DAY,
                MicroMinotari::from_str("1887258043.208972 T"), // mined
                MicroMinotari::from_str("1885044943.492867 T"), // spendable
                MicroMinotari::from_str(" 867125003.916666 T"), // pre_mine
                MicroMinotari::from_str("2752169947.409533 T"), // total
            ),
            (
                365 * BLOCKS_PER_DAY,
                MicroMinotari::from_str("3274120131.965798 T"), // mined
                MicroMinotari::from_str("3272126467.754857 T"), // spendable
                MicroMinotari::from_str("1652875003.416662 T"), // pre_mine
                MicroMinotari::from_str("4925001471.171519 T"), // total
            ),
            (
                (365 + 20) * BLOCKS_PER_DAY,
                MicroMinotari::from_str("3432595650.489607 T"), // mined
                MicroMinotari::from_str("3430627060.613596 T"), // spendable
                MicroMinotari::from_str("1652875003.416662 T"), // pre_mine
                MicroMinotari::from_str("5083502064.030258 T"), // total
            ),
            (
                (365 + 200) * BLOCKS_PER_DAY,
                MicroMinotari::from_str("4772127517.495734 T"), // mined
                MicroMinotari::from_str("4770370867.355004 T"), // spendable
                MicroMinotari::from_str("2946125002.916658 T"), // pre_mine
                MicroMinotari::from_str("7716495870.271662 T"), // total
            ),
        ] {
            let mined = mined.unwrap();
            let spendable = spendable.unwrap();
            let pre_mine = pre_mine.unwrap();
            let total = total.unwrap();

            let mined_rewards = consensus_manager.block_rewards_mined_at_height(height).unwrap();
            let spendable_rewards = consensus_manager.block_rewards_spendable_at_height(height).unwrap();
            let total_spendable = consensus_manager.total_tokens_spendable_at_height(height).unwrap();
            let pre_mine_spendable = consensus_manager.pre_mine_spendable_at_height(height).unwrap();
            let circulating_supply = consensus_manager.total_tokens_circulating_at_height(height).unwrap();
            let total_pre_mine = consensus_manager.total_pre_mine_in_genesis_block();
            let time_locked_pre_mine = consensus_manager.time_locked_pre_mine(height).unwrap();

            assert_eq!(mined_rewards, mined);
            assert_eq!(spendable_rewards, spendable);
            assert_eq!(pre_mine_spendable, pre_mine);
            assert_eq!(total_spendable, total);
            assert_eq!(circulating_supply, mined + pre_mine);
            assert_eq!(total_pre_mine, MAINNET_PRE_MINE_VALUE);
            assert_eq!(time_locked_pre_mine, MAINNET_PRE_MINE_VALUE - pre_mine);
        }
    }

    #[test]
    fn token_values_at_heights_matches_per_height_functions() {
        for network in [Network::MainNet, Network::NextNet, Network::LocalNet] {
            let consensus_manager = BaseNodeConsensusManager::builder(network).build().unwrap();

            let mut heights = vec![
                0,
                1,
                2,
                1000,
                10000,
                180 * BLOCKS_PER_DAY,
                (180 + 20) * BLOCKS_PER_DAY,
                365 * BLOCKS_PER_DAY,
                (365 + 20) * BLOCKS_PER_DAY,
                399_999,
                400_000,
            ];
            // Either side of every maturity tranche boundary
            let tranches = consensus_manager.get_maturity_tranches();
            for (previous, tranche) in tranches.iter().zip(tranches.iter().skip(1)) {
                for boundary in [
                    tranche.effective_from_height,
                    tranche.effective_from_height.saturating_add(previous.maturity),
                ] {
                    heights.extend([boundary.saturating_sub(1), boundary, boundary.saturating_add(1)]);
                }
            }
            heights.retain(|h| *h <= 400_000);
            heights.sort_unstable();
            heights.dedup();

            let values = consensus_manager.token_values_at_heights(&heights).unwrap();
            assert_eq!(values.len(), heights.len());
            for (value, &height) in values.iter().zip(heights.iter()) {
                assert_eq!(value.height, height);
                assert_eq!(
                    value.circulating_supply,
                    consensus_manager.total_tokens_circulating_at_height(height).unwrap(),
                    "{network} circulating_supply at {height}"
                );
                assert_eq!(
                    value.mined_rewards,
                    consensus_manager.block_rewards_mined_at_height(height).unwrap(),
                    "{network} mined_rewards at {height}"
                );
                assert_eq!(
                    value.spendable_rewards,
                    consensus_manager.block_rewards_spendable_at_height(height).unwrap(),
                    "{network} spendable_rewards at {height}"
                );
                assert_eq!(
                    value.spendable_pre_mine,
                    consensus_manager.pre_mine_spendable_at_height(height).unwrap(),
                    "{network} spendable_pre_mine at {height}"
                );
                assert_eq!(
                    value.total_spendable,
                    consensus_manager.total_tokens_spendable_at_height(height).unwrap(),
                    "{network} total_spendable at {height}"
                );
                assert_eq!(
                    value.total_pre_mine,
                    consensus_manager.total_pre_mine_in_genesis_block(),
                    "{network} total_pre_mine at {height}"
                );
                assert_eq!(
                    value.time_locked_pre_mine,
                    consensus_manager.time_locked_pre_mine(height).unwrap(),
                    "{network} time_locked_pre_mine at {height}"
                );
            }
        }
    }

    #[test]
    fn token_values_at_heights_rejects_unsorted_or_duplicate_heights() {
        let consensus_manager = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        assert!(consensus_manager.token_values_at_heights(&[]).unwrap().is_empty());
        assert!(consensus_manager.token_values_at_heights(&[2, 1]).is_err());
        assert!(consensus_manager.token_values_at_heights(&[1, 1]).is_err());
    }
}
