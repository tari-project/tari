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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
    RwLock,
};

use tari_common_types::{chain_metadata::ChainMetadata, types::Commitment};
use tari_utilities::epoch_time::EpochTime;

use super::{
    traits::CandidateBlockValidator,
    BlockBodyValidator,
    HeaderChainLinkedValidator,
    InternalConsistencyValidator,
    TransactionValidator,
};
use crate::{
    blocks::{Block, BlockHeader, ChainBlock},
    chain_storage::BlockchainBackend,
    proof_of_work::{randomx_factory::RandomXFactory, AchievedTargetDifficulty, Difficulty},
    test_helpers::create_consensus_rules,
    transactions::transaction_components::Transaction,
    validation::{error::ValidationError, DifficultyCalculator, FinalHorizonStateValidation},
    OutputSmt,
};

#[derive(Clone)]
pub struct MockValidator {
    is_valid: Arc<AtomicBool>,
}

pub struct SharedFlag(Arc<AtomicBool>);

impl SharedFlag {
    pub fn set(&self, v: bool) {
        self.0.store(v, Ordering::SeqCst);
    }
}

impl MockValidator {
    pub fn new(is_valid: bool) -> Self {
        Self {
            is_valid: Arc::new(AtomicBool::new(is_valid)),
        }
    }

    pub fn shared_flag(&self) -> SharedFlag {
        SharedFlag(self.is_valid.clone())
    }
}

impl<B: BlockchainBackend> BlockBodyValidator<B> for MockValidator {
    fn validate_body(&self, _: &B, block: &Block, _: Arc<RwLock<OutputSmt>>) -> Result<Block, ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(block.clone())
        } else {
            Err(ValidationError::ConsensusError(
                "This mock validator always returns an error".to_string(),
            ))
        }
    }
}

impl<B: BlockchainBackend> CandidateBlockValidator<B> for MockValidator {
    fn validate_body_with_metadata(
        &self,
        _: &B,
        _: &ChainBlock,
        _: &ChainMetadata,
        _: Arc<RwLock<OutputSmt>>,
    ) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::ConsensusError(
                "This mock validator always returns an error".to_string(),
            ))
        }
    }
}

// #[async_trait]
impl InternalConsistencyValidator for MockValidator {
    fn validate_internal_consistency(&self, _item: &Block) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::ConsensusError(
                "This mock validator always returns an error".to_string(),
            ))
        }
    }
}

impl<B: BlockchainBackend> HeaderChainLinkedValidator<B> for MockValidator {
    fn validate(
        &self,
        db: &B,
        header: &BlockHeader,
        _: &BlockHeader,
        _: &[EpochTime],
        _: Option<Difficulty>,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            // this assumes consensus rules are the same as the test rules which is a little brittle
            let difficulty_calculator = DifficultyCalculator::new(create_consensus_rules(), RandomXFactory::default());
            let achieved_target_diff = difficulty_calculator.check_achieved_and_target_difficulty(db, header)?;
            Ok(achieved_target_diff)
        } else {
            Err(ValidationError::ConsensusError(
                "This mock validator always returns an error".to_string(),
            ))
        }
    }
}

impl TransactionValidator for MockValidator {
    fn validate(&self, _transaction: &Transaction) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::ConsensusError(
                "This mock validator always returns an error".to_string(),
            ))
        }
    }
}

impl<B: BlockchainBackend> FinalHorizonStateValidation<B> for MockValidator {
    fn validate(
        &self,
        _backend: &B,
        _height: u64,
        _total_utxo_sum: &Commitment,
        _total_kernel_sum: &Commitment,
        _total_burned_sum: &Commitment,
    ) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::ConsensusError(
                "This mock validator always returns an error".to_string(),
            ))
        }
    }
}
