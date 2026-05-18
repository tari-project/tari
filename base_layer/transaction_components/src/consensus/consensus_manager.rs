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
use thiserror::Error;

use crate::{
    MicroMinotari,
    consensus::{
        ConsensusConstants,
        NetworkConsensus,
        emission::{Emission, EmissionSchedule},
    },
    transaction_components::TransactionKernel,
};

/// A simple struct to hold the maturity and effective height
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct MaturityTranche {
    pub maturity: u64,
    pub effective_from_height: u64,
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ConsensusManagerError {
    // #[error("Difficulty adjustment encountered an error: `{0}`")]
    // DifficultyAdjustmentError(#[from] DifficultyAdjustmentError),
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
        let mut constants = self
            .inner
            .consensus_constants
            .first()
            .expect("Should always have at least one consensus constant");
        for c in &self.inner.consensus_constants {
            if c.effective_from_height() > height {
                break;
            }
            constants = c
        }
        constants
    }

    /// Get the vector of consensus constants applicable for all heights
    pub fn consensus_constants_vec(&self) -> &[ConsensusConstants] {
        &self.inner.consensus_constants
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
                    ));
                },
            }
        }

        Ok(total)
    }

    /// This is the currently configured chain network.
    pub fn network(&self) -> NetworkConsensus {
        self.inner.network
    }

    pub fn emission(&self) -> &EmissionSchedule {
        &self.inner.emission
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

    /// Get the total spendable pre-mine at the specified height
    pub fn total_pre_mine_in_genesis_block(&self) -> MicroMinotari {
        self.consensus_constants(0).pre_mine_value()
    }

    /// Get the total mined block rewards at the specified height (excluding pre-mine)
    pub fn block_rewards_mined_at_height(&self, height: u64) -> Result<MicroMinotari, String> {
        Ok(self.get_total_emission_at(height) - self.consensus_constants(height).pre_mine_value())
    }

    /// Get the total spendable block rewards circulation at the specified height (excluding pre-mine)
    pub fn block_rewards_spendable_at_height(&self, height: u64) -> Result<MicroMinotari, String> {
        // Example initial maturity schedule up to 3 weeks ( | height | (maturity) |):
        // | 0 -> 5040 - 1 | (720) |
        //                 | 5040 -> 10080 - 1 | (540) |
        //                                     | 10080 -> 15120 - 1 | (360) |
        //                                                          | 15120 -> | (180) |

        let maturity_tranches = self.get_maturity_tranches();

        let last_effective_tranche = maturity_tranches
            .iter()
            .filter(|v| v.effective_from_height <= height)
            .max_by_key(|v| v.effective_from_height)
            .ok_or_else(|| format!("Last effective maturity tranche for height {height} not found"))?;
        let last_effective_index = maturity_tranches
            .iter()
            .position(|v| v == last_effective_tranche)
            .ok_or_else(|| format!("Last effective maturity tranche index for height {height} not found"))?;
        let previous_effective_tranch = maturity_tranches
            .get(last_effective_index.saturating_sub(1))
            .expect("Index should exist")
            .clone();

        // We have to adjust the matured rewards at height to account for the effective from height of the last
        // effective tranche
        let emission_schedule = self.emission_schedule();
        let matured_rewards_at_height = if last_effective_tranche.maturity < previous_effective_tranch.maturity &&
            height < last_effective_tranche.effective_from_height + previous_effective_tranch.maturity
        {
            emission_schedule
                .supply_at_block(height.saturating_sub(previous_effective_tranch.maturity))
                .saturating_sub(self.consensus_constants(height).pre_mine_value())
        } else {
            emission_schedule
                .supply_at_block(height.saturating_sub(last_effective_tranche.maturity))
                .saturating_sub(self.consensus_constants(height).pre_mine_value())
        };

        Ok(matured_rewards_at_height)
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
}

/// Constructor for the consensus manager struct
pub struct ConsensusManagerBuilder {
    consensus_constants: Vec<ConsensusConstants>,
    pub network: NetworkConsensus,
}

impl ConsensusManagerBuilder {
    /// Creates a new ConsensusManagerBuilder with the specified network
    pub fn new(network: Network) -> Self {
        ConsensusManagerBuilder {
            consensus_constants: vec![],
            network: network.into(),
        }
    }

    /// Adds in a custom consensus constants to be used
    pub fn add_consensus_constants(mut self, consensus_constants: ConsensusConstants) -> Self {
        self.consensus_constants.push(consensus_constants);
        self
    }

    /// Builds a consensus manager
    pub fn build(mut self) -> ConsensusManager {
        // should not be allowed to set the gen block and have the network type anything else than LocalNet
        // If feature != base_node, gen_block is not available
        if self.consensus_constants.is_empty() {
            self.consensus_constants = self.network.create_consensus_constants();
        }
        let cc = self
            .consensus_constants
            .first()
            .expect("Consensus constants should not be empty");
        let emission = EmissionSchedule::new(
            cc.emission_initial,
            cc.emission_decay.clone(),
            cc.inflation_bips,
            cc.tail_epoch_length,
            cc.pre_mine_value(),
        );

        let inner = ConsensusManagerInner {
            consensus_constants: self.consensus_constants,
            network: self.network,
            emission,
        };
        ConsensusManager { inner: Arc::new(inner) }
    }
}
