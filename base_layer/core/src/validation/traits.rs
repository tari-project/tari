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
    blocks::{Block, BlockHeader},
    chain_storage::{BlockchainBackend, ChainBlock},
    proof_of_work::AchievedTargetDifficulty,
    transactions::{transaction::Transaction, types::Commitment},
    validation::error::ValidationError,
};

/// A validator that determines if a block body is valid, assuming that the header has already been
/// validated
pub trait CandidateBlockBodyValidation<B: BlockchainBackend>: Send + Sync {
    fn validate_body(&self, block: &Block, backend: &B) -> Result<(), ValidationError>;
}

/// A validator that validates a body after it has been determined to be a valid orphan
pub trait PostOrphanBodyValidation<B>: Send + Sync {
    fn validate_body_for_valid_orphan(&self, block: &ChainBlock, backend: &B) -> Result<(), ValidationError>;
}

pub trait MempoolTransactionValidation: Send + Sync {
    fn validate(&self, transaction: &Transaction) -> Result<(), ValidationError>;
}

pub trait OrphanValidation: Send + Sync {
    fn validate(&self, item: &Block) -> Result<(), ValidationError>;
}

pub trait HeaderValidation<B: BlockchainBackend>: Send + Sync {
    fn validate(&self, db: &B, header: &BlockHeader) -> Result<AchievedTargetDifficulty, ValidationError>;
}

pub trait FinalHorizonStateValidation<B>: Send + Sync {
    fn validate(
        &self,
        height: u64,
        total_utxo_sum: &Commitment,
        total_kernel_sum: &Commitment,
        backend: &B,
    ) -> Result<(), ValidationError>;
}
