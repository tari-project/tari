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

#[cfg(feature = "base_node")]
use croaring::Bitmap;
use tari_common::configuration::Network;
#[cfg(feature = "base_node")]
use tari_common_types::types::{Commitment, PrivateKey};
#[cfg(feature = "base_node")]
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_crypto::errors::RangeProofError;
use tari_mmr::error::MerkleMountainRangeError;
use thiserror::Error;

#[cfg(feature = "base_node")]
use crate::{
    blocks::ChainBlock,
    consensus::chain_strength_comparer::{strongest_chain, ChainStrengthComparer},
    proof_of_work::PowAlgorithm,
    proof_of_work::TargetDifficultyWindow,
};
#[cfg(feature = "base_node")]
use crate::{
    chain_storage::calculate_validator_node_mr,
    transactions::{transaction_components::transaction_output::batch_verify_range_proofs, CryptoFactories},
    KernelMmr,
    MutableOutputMmr,
};
use crate::{
    consensus::{
        emission::{Emission, EmissionSchedule},
        ConsensusConstants,
        NetworkConsensus,
    },
    proof_of_work::DifficultyAdjustmentError,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{TransactionError, TransactionKernel},
    },
};

// This can be adjusted as required, but must be limited
#[cfg(feature = "base_node")]
pub const NOT_BEFORE_PROOF_BYTES_SIZE: usize = 2048usize;

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
    #[error("Genesis block is invalid: `{0}`")]
    InvalidGenesisBlock(String),
    #[error("Genesis block range proof is invalid: `{0}`")]
    InvalidGenesisBlockProof(#[from] RangeProofError),
    #[error("Genesis block transaction is invalid: `{0}`")]
    InvalidGenesisBlockTransaction(#[from] TransactionError),
    #[error("Genesis block MMR is invalid: `{0}`")]
    InvalidGenesisBlockMMR(#[from] MerkleMountainRangeError),
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
    pub fn get_genesis_block(&self) -> Result<ChainBlock, ConsensusManagerError> {
        use crate::blocks::genesis_block::get_genesis_block;
        let network = self.inner.network.as_network();
        let genesis_block = match network {
            Network::LocalNet => self
                .inner
                .gen_block
                .clone()
                .unwrap_or_else(|| get_genesis_block(network)),
            _ => get_genesis_block(network),
        };
        self.check_genesis_block(&genesis_block)?;
        Ok(genesis_block)
    }

    // Performs a manual validation of every aspect of the genesis block
    #[allow(clippy::too_many_lines)]
    #[cfg(feature = "base_node")]
    fn check_genesis_block(&self, block: &ChainBlock) -> Result<(), ConsensusManagerError> {
        // Check the not-before-proof
        if block.block().header.pow.pow_algo != PowAlgorithm::Sha3x {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block must use Sha3x PoW".to_string(),
            ));
        }
        if block.block().header.pow.pow_data.len() > NOT_BEFORE_PROOF_BYTES_SIZE {
            return Err(ConsensusManagerError::InvalidGenesisBlock(format!(
                "Genesis block PoW data is too large: expected {}, received {}",
                NOT_BEFORE_PROOF_BYTES_SIZE,
                block.block().header.pow.pow_data.len()
            )));
        }

        // Check transaction composition
        if !block.block().body.inputs().is_empty() {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block may not have inputs".to_string(),
            ));
        }
        if block.block().body.outputs().iter().any(|o| o.is_coinbase()) {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block may not have coinbase outputs".to_string(),
            ));
        }
        if block
            .block()
            .body
            .outputs()
            .iter()
            .any(|o| o.features.output_type.is_sidechain_type())
        {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block may not have sidechain outputs".to_string(),
            ));
        }
        if block.block().body.kernels().len() > 1 {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block may not have more than one kernel".to_string(),
            ));
        }
        if block
            .block()
            .body
            .kernels()
            .iter()
            .any(|k| k.features.is_coinbase() || k.features.is_burned())
        {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block may not have coinbase or burn kernels".to_string(),
            ));
        }

        // Check range proofs
        let factories = CryptoFactories::default();
        let outputs = block.block().body.outputs().iter().collect::<Vec<_>>();
        batch_verify_range_proofs(&factories.range_proof, &outputs)?;

        // Check the kernel signature
        for kernel in block.block().body.kernels() {
            kernel.verify_signature()?;
        }

        // Check the metadata signatures
        for o in block.block().body.outputs() {
            o.verify_metadata_signature()?;
        }

        // Check MMR sizes
        if block.block().body.kernels().len() as u64 != block.header().kernel_mmr_size {
            return Err(ConsensusManagerError::InvalidGenesisBlock(format!(
                "Genesis block kernel MMR size is invalid, expected {} got {}",
                block.block().body.kernels().len(),
                block.header().kernel_mmr_size
            )));
        }
        if block.block().body.outputs().len() as u64 != block.header().output_mmr_size {
            return Err(ConsensusManagerError::InvalidGenesisBlock(format!(
                "Genesis block output MMR size is invalid, expected {} got {}",
                block.block().body.outputs().len(),
                block.header().output_mmr_size
            )));
        }

        // Check the MMRs and MMR roots
        let mut kernel_mmr = KernelMmr::new(Vec::new());
        for kernel in block.block().body.kernels() {
            kernel_mmr.push(kernel.hash().to_vec())?;
        }
        if kernel_mmr.get_merkle_root()? != block.header().kernel_mr {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block kernel MMR root is invalid".to_string(),
            ));
        }
        let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create())?;
        for output in block.block().body.outputs() {
            output_mmr.push(output.hash().to_vec())?;
        }
        if output_mmr.get_merkle_root()? != block.header().output_mr {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block output MMR root is invalid".to_string(),
            ));
        }
        if calculate_validator_node_mr(&[]) != block.header().validator_node_mr {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Genesis block validator node MMR root is invalid".to_string(),
            ));
        }

        // Check the chain balance
        // See `ChainBalanceValidator::validate`
        let factories = CryptoFactories::default();
        let emission_h = {
            // See `ChainBalanceValidator::get_emission_commitment_at`
            let total_supply = self.get_total_emission_at(0) + self.consensus_constants(0).faucet_value();
            factories
                .commitment
                .commit_value(&PrivateKey::default(), total_supply.into())
        };
        let total_offset = {
            // See `ChainBalanceValidator::fetch_total_offset_commitment`
            let chain_header = block.to_chain_header();
            let offset = &chain_header.accumulated_data().total_kernel_offset;
            factories.commitment.commit(offset, &0u64.into())
        };
        let total_kernel_sum: Commitment = block.block().body.kernels().iter().map(|k| &k.excess).sum();
        let input = &(&emission_h + &total_kernel_sum) + &total_offset;
        let total_utxo_sum: Commitment = block.block().body.outputs().iter().map(|o| &o.commitment).sum();
        let total_burned_sum = Commitment::default();
        if (&total_utxo_sum + &total_burned_sum) != input {
            return Err(ConsensusManagerError::InvalidGenesisBlock(
                "Chain balance validation failed".to_string(),
            ));
        }

        Ok(())
    }

    /// Get a pointer to the emission schedule
    /// The height provided here, decides the emission curve to use. It swaps to the integer curve upon reaching
    /// 1_000_000_000
    pub fn emission_schedule(&self) -> &EmissionSchedule {
        &self.inner.emission
    }

    /// Gets the block emission for the height
    pub fn get_block_emission_at(&self, height: u64) -> MicroTari {
        self.emission_schedule().block_emission(height)
    }

    /// Get the emission reward at height
    /// Returns None if the total supply > u64::MAX
    pub fn get_total_emission_at(&self, height: u64) -> MicroTari {
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

    /// Create a new TargetDifficulty for the given proof of work using constants that are effective from the given
    /// height
    #[cfg(feature = "base_node")]
    pub(crate) fn new_target_difficulty(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<TargetDifficultyWindow, String> {
        use std::convert::TryFrom;
        let constants = self.consensus_constants(height);
        let block_window = constants.difficulty_block_window();

        TargetDifficultyWindow::new(
            usize::try_from(block_window).expect("difficulty block window exceeds usize::MAX"),
            constants.pow_target_block_interval(pow_algo),
        )
    }

    /// Creates a total_coinbase offset containing all fees for the validation from the height and kernel set
    pub fn calculate_coinbase_and_fees(&self, height: u64, kernels: &[TransactionKernel]) -> MicroTari {
        let coinbase = self.emission_schedule().block_emission(height);
        kernels.iter().fold(coinbase, |total, k| total + k.fee)
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
            self.consensus_constants[0].emission_tail,
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
