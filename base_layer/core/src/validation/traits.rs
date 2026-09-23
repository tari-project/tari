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
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
use tari_common_types::{chain_metadata::ChainMetadata, types::CompressedCommitment};
use tari_node_components::blocks::{Block, BlockHeader, ChainBlock};
use tari_transaction_components::transaction_components::Transaction;
use tari_utilities::epoch_time::EpochTime;

use crate::{
    chain_storage::BlockchainBackend,
    proof_of_work::{AchievedTargetDifficulty, AdjustedTarget},
    validation::{chain_context::HeaderChainContext, error::ValidationError},
};
/// A validator that determines if a block body is valid, assuming that the header has already been
/// validated
pub trait BlockBodyValidator<B>: Send + Sync {
    fn validate_body(&self, backend: &B, block: Block) -> Result<Block, ValidationError>;
}

/// A validator that validates a body after it has been determined to be a valid orphan
pub trait CandidateBlockValidator<B>: Send + Sync {
    fn validate_body_with_metadata(
        &self,
        backend: &B,
        block: &ChainBlock,
        metadata: &ChainMetadata,
    ) -> Result<(), ValidationError>;

    // This body-in-isolation validation is intended to validate the block body without any knowledge of consecutive
    // blocks that may exist. For example, it cannot validate that kernels are unique or that outputs have not been
    // spent already.
    fn validate_body_at_height(&self, backend: &B, block: &ChainBlock) -> Result<(), ValidationError>;
}

pub trait TransactionValidator: Send + Sync {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError>;
}

pub trait InternalConsistencyValidator: Send + Sync {
    fn validate_internal_consistency(&self, item: &Block) -> Result<(), ValidationError>;
}

/// What header validation found out about a header, beyond the fact that it is valid.
#[derive(Debug, Clone)]
pub struct ValidatedHeader {
    /// The difficulty the header achieved, and the target it had to clear.
    pub achieved_target: AchievedTargetDifficulty,
    /// The Monero RandomX seed the header is keyed by, for a merge mined header, and `None` for any other proof of
    /// work.
    ///
    /// This is handed back rather than left for the caller to work out because recovering it means parsing the
    /// header's PoW data, and `MoneroPowData::from_header` deliberately does an expensive job of it: it Borsh
    /// deserializes the structure and then re-serializes it to prove the encoding was canonical. A caller that
    /// tracks the seeds of a chain the database does not hold yet (see [`crate::validation::MoneroSeedHeights`])
    /// would otherwise pay for that a second time on every merge mined header of a sync.
    pub monero_seed: Option<Vec<u8>>,
}

pub trait HeaderChainLinkedValidator<B: BlockchainBackend>: Send + Sync {
    fn validate(
        &self,
        db: &B,
        header: &BlockHeader,
        prev_header: &BlockHeader,
        prev_timestamps: &[EpochTime],
        target_difficulty: Option<AdjustedTarget>,
        chain_context: HeaderChainContext<'_>,
    ) -> Result<ValidatedHeader, ValidationError>;
}

pub trait FinalHorizonStateValidation<B>: Send + Sync {
    fn validate(
        &self,
        backend: &B,
        height: u64,
        total_utxo_sum: &CompressedCommitment,
        total_kernel_sum: &CompressedCommitment,
        total_burned_sum: &CompressedCommitment,
    ) -> Result<(), ValidationError>;
}
