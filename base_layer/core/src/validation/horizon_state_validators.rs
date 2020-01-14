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

pub use crate::consensus::ConsensusManager;
use crate::{
    blocks::blockheader::{BlockHeader, BlockHeaderValidationError},
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    validation::{
        error::ValidationError,
        helpers::{check_achieved_difficulty, check_median_timestamp},
        traits::Validation,
    },
};

/// This validator check that the synced horizon state headers satisfies *all* consensus rules.
pub struct HorizonStateHeaderValidator<B: BlockchainBackend> {
    rules: ConsensusManager<B>,
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> HorizonStateHeaderValidator<B>
where B: BlockchainBackend
{
    pub fn new(rules: ConsensusManager<B>, db: BlockchainDatabase<B>) -> Self {
        Self { rules, db }
    }

    fn db(&self) -> Result<BlockchainDatabase<B>, ValidationError> {
        Ok(self.db.clone())
    }
}

impl<B: BlockchainBackend> Validation<BlockHeader, B> for HorizonStateHeaderValidator<B> {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Do the headers form a valid sequence and are they correctly chained?
    /// 1. Is the block header timestamp greater than the median timestamp?
    /// 1. Is the Proof of Work valid and is the achieved difficulty of this block >= the target difficulty for this
    /// block?
    fn validate(&self, block_header: &BlockHeader) -> Result<(), ValidationError> {
        check_header_sequence_and_chaining(block_header, self.db()?)?;
        check_median_timestamp(&block_header, block_header.height, self.rules.clone())?;
        check_achieved_difficulty(&block_header, block_header.height, self.rules.clone())?;

        Ok(())
    }
}

/// Check that the headers form a valid sequence and that the headers are correctly chained.
fn check_header_sequence_and_chaining<B: BlockchainBackend>(
    block_header: &BlockHeader,
    db: BlockchainDatabase<B>,
) -> Result<(), ValidationError>
{
    if block_header.height == 0 {
        if block_header.prev_hash != vec![0; 32] {
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::ChainedGenesisBlockHeader,
            ));
        }
        return Ok(());
    }

    match db.fetch_header_with_block_hash(block_header.prev_hash.clone()) {
        Ok(prev_block_header) => {
            if block_header.height != prev_block_header.height + 1 {
                return Err(ValidationError::BlockHeaderError(
                    BlockHeaderValidationError::InvalidChaining,
                ));
            }
        },
        Err(_) => {
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidChaining,
            ));
        },
    }

    Ok(())
}
