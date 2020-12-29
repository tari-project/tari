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
    chain_storage::{BlockHeaderAccumulatedData, BlockHeaderAccumulatedDataBuilder, BlockchainBackend, ChainBlock},
    proof_of_work::sha3_difficulty,
    transactions::transaction::Transaction,
    validation::{
        error::ValidationError,
        CandidateBlockBodyValidation,
        FinalHeaderStateValidation,
        HeaderValidation,
        MempoolTransactionValidation,
        OrphanValidation,
        PostOrphanBodyValidation,
    },
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tari_crypto::tari_utilities::Hashable;

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

impl<B: BlockchainBackend> CandidateBlockBodyValidation<B> for MockValidator {
    fn validate_body(&self, _item: &ChainBlock, _db: &B) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::custom_error(
                "This mock validator always returns an error",
            ))
        }
    }
}

impl<B: BlockchainBackend> PostOrphanBodyValidation<B> for MockValidator {
    fn validate_body_for_valid_orphan(&self, _item: &ChainBlock, _db: &B) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::custom_error(
                "This mock validator always returns an error",
            ))
        }
    }
}

impl OrphanValidation for MockValidator {
    fn validate(&self, _item: &Block) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::custom_error(
                "This mock validator always returns an error",
            ))
        }
    }
}

impl<B: BlockchainBackend> HeaderValidation<B> for MockValidator {
    fn validate(
        &self,
        _db: &B,
        header: &BlockHeader,
        previous_data: &BlockHeaderAccumulatedData,
    ) -> Result<BlockHeaderAccumulatedDataBuilder, ValidationError>
    {
        if self.is_valid.load(Ordering::SeqCst) {
            let achieved = sha3_difficulty(header);
            let accum_data = BlockHeaderAccumulatedDataBuilder::default()
                .hash(header.hash())
                .target_difficulty(1.into())
                .achieved_difficulty(previous_data, header.pow_algo(), achieved)
                .total_kernel_offset(&previous_data.total_kernel_offset, &header.total_kernel_offset);

            Ok(accum_data)
        } else {
            Err(ValidationError::custom_error(
                "This mock validator always returns an error",
            ))
        }
    }
    // fn validate(&self, header: &BlockHeader, previous_header: &BlockHeader, previous_data:
    // &BlockHeaderAccumulatedData) -> Result<BlockHeaderAccumulatedDataBuilder, ValidationError> {
    //     unimplemented!()
    // }
}

impl MempoolTransactionValidation for MockValidator {
    fn validate(&self, _transaction: &Transaction) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::custom_error(
                "This mock validator always returns an error",
            ))
        }
    }
}

impl FinalHeaderStateValidation for MockValidator {
    fn validate(&self, _header: &BlockHeader) -> Result<(), ValidationError> {
        if self.is_valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ValidationError::custom_error(
                "This mock validator always returns an error",
            ))
        }
    }
}
